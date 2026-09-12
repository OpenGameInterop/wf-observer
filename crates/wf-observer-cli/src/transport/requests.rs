//! Framing and dispatch over independent streams on one connection.
//! Each stream keeps its own send handle so partial-write cancellation can reset
//! it explicitly; no game/provider/memory object is reachable here.

use anyhow::{Context as _, ensure};
use iroh::endpoint::{RecvStream, SendStream};
use irpc::util::{AsyncReadVarintExt as _, AsyncWriteVarintExt as _};
use protocol::v1 as wire;
use serde::Serialize;
use std::time::Duration;

use super::connection::{REJECTED, Streams};
use crate::service::ServiceView;
use std::sync::Arc;
use tokio::sync::OwnedSemaphorePermit;

const IO_DEADLINE: Duration = Duration::from_secs(5);

pub(super) async fn serve(
    view: ServiceView,
    message_bytes: u32,
    mut send: SendStream,
    mut recv: RecvStream,
    permits: (OwnedSemaphorePermit, OwnedSemaphorePermit),
    streams: Arc<Streams>,
) {
    let result = async {
        let mut permits = Some(permits);
        // Accept streams independently: a stalled request header cannot block ping
        // or another reader's bootstrap on this connection.
        let request = tokio::time::timeout(
            IO_DEADLINE,
            recv.read_length_prefixed::<wire::ObserverProtocolV1>(message_bytes as usize),
        )
        .await??;
        // All current operations are request-only; there is no upload stream.
        let _ = recv.stop(0_u32.into());
        dispatch(
            &view,
            message_bytes,
            request,
            &mut send,
            &mut permits,
            &streams,
        )
        .await?;
        send.finish()?;
        anyhow::Ok(())
    }
    .await;
    if let Err(error) = result {
        // Never append Closed after cancelling a partially written frame.
        let _ = send.reset(REJECTED.into());
        let _ = recv.stop(REJECTED.into());
        tracing::debug!(%error, "RPC stream ended without graceful completion");
    }
}

async fn dispatch(
    view: &ServiceView,
    message_bytes: u32,
    request: wire::ObserverProtocolV1,
    send: &mut SendStream,
    permits: &mut Option<(OwnedSemaphorePermit, OwnedSemaphorePermit)>,
    streams: &Arc<Streams>,
) -> anyhow::Result<()> {
    match request {
        wire::ObserverProtocolV1::Ping(_) => write(send, &wire::Pong, message_bytes).await,
        wire::ObserverProtocolV1::GetCatalog(_) => {
            write(send, &view.catalog(), message_bytes).await
        }
        wire::ObserverProtocolV1::GetStatus(_) => write(send, &view.status(), message_bytes).await,
        wire::ObserverProtocolV1::GetSnapshot(request) => {
            write(send, &view.snapshot(&request), message_bytes).await
        }
        wire::ObserverProtocolV1::Subscribe(request) => {
            let (_stream, mut subscription) = match streams.open().and_then(|slot| {
                view.subscribe(request)
                    .map(|subscription| (slot, subscription))
            }) {
                Ok(admitted) => admitted,
                Err(error) => {
                    return write(
                        send,
                        &Err::<wire::SubscriptionItem, _>(error),
                        message_bytes,
                    )
                    .await;
                }
            };
            drop(permits.take());
            loop {
                let item = tokio::select! {
                    biased;
                    _ = send.stopped() => return Ok(()),
                    item = subscription.next() => item.context("subscription ended without Closed")?,
                };
                let terminal = matches!(item, wire::SubscriptionItem::Closed(_));
                write(send, &Ok::<_, wire::RequestError>(item), message_bytes).await?;
                if terminal {
                    return Ok(());
                }
            }
        }
    }
}

async fn write<T: Serialize>(
    send: &mut SendStream,
    value: &T,
    message_bytes: u32,
) -> anyhow::Result<()> {
    ensure!(
        postcard::experimental::serialized_size(value)? <= message_bytes as usize,
        "response exceeds frame limit"
    );
    tokio::time::timeout(IO_DEADLINE, send.write_length_prefixed(value)).await??;
    Ok(())
}
