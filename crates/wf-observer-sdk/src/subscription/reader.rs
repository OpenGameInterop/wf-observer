use iroh::endpoint::{RecvStream, SendStream};
use irpc::util::{AsyncReadVarintExt as _, AsyncWriteVarintExt as _};
use protocol::v1 as wire;
use std::collections::BTreeMap;

use super::validation::Validation;
use crate::raw::ClientError;

type Frame = std::io::Result<Result<wire::SubscriptionItem, wire::RequestError>>;

/// Validated wire frames for one upstream feed.
pub(super) struct Reader {
    send: SendStream,
    recv: RecvStream,
    validation: Validation,
    baselines: BTreeMap<(wire::SessionRef, wire::TopicRef), wire::DataEnvelope>,
    pending_ack: Option<wire::SnapshotAck>,
    deltas: bool,
}

pub(super) enum ReadError {
    Client(ClientError),
    Resync(&'static str),
}

impl From<ClientError> for ReadError {
    fn from(error: ClientError) -> Self {
        Self::Client(error)
    }
}

impl From<ReadError> for ClientError {
    fn from(error: ReadError) -> Self {
        match error {
            ReadError::Client(error) => error,
            ReadError::Resync(message) => Self::protocol(message),
        }
    }
}

impl Reader {
    pub(super) fn new(
        send: SendStream,
        recv: RecvStream,
        selection: wire::Subscribe,
        deltas: bool,
    ) -> Self {
        Self {
            send,
            recv,
            validation: Validation::new(selection),
            baselines: BTreeMap::new(),
            pending_ack: None,
            deltas,
        }
    }

    /// The feed owns this read loop; cancelling the feed drops the stream.
    pub(super) async fn next(&mut self) -> Result<wire::SubscriptionItem, ReadError> {
        let limit = usize::try_from(irpc::rpc::MAX_MESSAGE_SIZE).unwrap_or(usize::MAX);
        let frame = self.recv.read_length_prefixed(limit).await;
        let mut item = decode(frame, self.validation.begun())?;
        let acknowledged = matches!(item, wire::SubscriptionItem::Snapshot(_));
        if let wire::SubscriptionItem::Snapshot(frame) = item {
            let key = (
                frame.metadata.source.session.clone(),
                frame.metadata.source.topic.clone(),
            );
            item = frame
                .reconstruct(self.baselines.get(&key))
                .map_err(ReadError::Resync)?;
            let bytes =
                postcard::experimental::serialized_size(&Ok::<_, wire::RequestError>(&item))
                    .map_err(ClientError::transport)?;
            if bytes > limit {
                return Err(ReadError::Resync(
                    "reconstructed snapshot exceeds frame limit",
                ));
            }
        }
        self.validation.accept(&item)?;
        let topic = match &item {
            wire::SubscriptionItem::Topic(topic) => Some(topic),
            wire::SubscriptionItem::Update(update) => match &update.update {
                wire::SubscriptionUpdate::TopicChanged(topic) => Some(topic),
                wire::SubscriptionUpdate::TopicReset { source, .. } => {
                    self.baselines
                        .remove(&(source.session.clone(), source.topic.clone()));
                    None
                }
                wire::SubscriptionUpdate::SessionEnded { session, .. } => {
                    self.baselines.retain(|(key, _), _| key != session);
                    None
                }
                _ => None,
            },
            _ => None,
        };
        if let Some(topic) = topic {
            let key = (topic.source.session.clone(), topic.source.topic.clone());
            if let Some(snapshot) = &topic.snapshot {
                if acknowledged && self.deltas {
                    self.pending_ack = Some(wire::SnapshotAck {
                        metadata: snapshot.metadata.clone(),
                        hash: snapshot.payload.hash(),
                    });
                    self.baselines.insert(key, snapshot.clone());
                }
            } else {
                self.baselines.remove(&key);
            }
        }
        Ok(item)
    }

    /// Called only after the feed has installed and published the reconstructed state.
    pub(super) async fn acknowledge(&mut self) -> Result<(), ReadError> {
        if let Some(ack) = self.pending_ack.take() {
            n0_future::time::timeout(
                std::time::Duration::from_secs(5),
                self.send.write_length_prefixed(ack),
            )
            .await
            .map_err(ClientError::transport)?
            .map_err(ClientError::transport)?;
        }
        Ok(())
    }
}

fn decode(frame: Frame, begun: bool) -> Result<wire::SubscriptionItem, ClientError> {
    match frame {
        Ok(Ok(item)) => Ok(item),
        Ok(Err(error)) if !begun => Err(ClientError::Request(error)),
        Ok(Err(_)) => Err(ClientError::protocol("request error after Begin")),
        Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => {
            Err(ClientError::protocol("stream ended without Closed"))
        }
        Err(error) => Err(ClientError::transport(error)),
    }
}
