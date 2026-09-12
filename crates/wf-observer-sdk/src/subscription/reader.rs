use iroh::endpoint::RecvStream;
use irpc::util::AsyncReadVarintExt as _;
use protocol::v1 as wire;

use super::validation::Validation;
use crate::raw::ClientError;

type Frame = std::io::Result<Result<wire::SubscriptionItem, wire::RequestError>>;

/// Validated wire frames for one upstream feed.
pub(super) struct Reader {
    recv: RecvStream,
    validation: Validation,
}

impl Reader {
    pub(super) fn new(recv: RecvStream, selection: wire::Subscribe) -> Self {
        Self {
            recv,
            validation: Validation::new(selection),
        }
    }

    /// The feed owns this read loop; cancelling the feed drops the stream.
    pub(super) async fn next(&mut self) -> Result<wire::SubscriptionItem, ClientError> {
        let limit = usize::try_from(irpc::rpc::MAX_MESSAGE_SIZE).unwrap_or(usize::MAX);
        let frame = self.recv.read_length_prefixed(limit).await;
        let item = decode(frame, self.validation.begun())?;
        self.validation.accept(&item)?;
        Ok(item)
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
