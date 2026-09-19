//! Purchased Intrinsic ranks and whole unspent points for Railjack and Drifter.
use crate::raw::{Client, ClientError, Subscription, Topic, TypedData, decode_snapshot, types};
use warframe_model::IntrinsicsSnapshot;

/// Purchased ranks and whole unspent points, independently of mastery credit.
pub struct IntrinsicsTopic;

impl Topic for IntrinsicsTopic {
    const GAME_ID: &'static str = "warframe";
    const PROVIDER_ID: &'static str = "opengameinterop.warframe";
    const NAME: &'static str = "warframe.intrinsics";
    const SCHEMA_VERSION: u32 = 1;
}

impl crate::raw::SnapshotTopic for IntrinsicsTopic {
    type Snapshot = IntrinsicsSnapshot;
}

/// Checks topic/schema, game identity and the complete intrinsics payload.
///
/// # Errors
///
/// Returns identity or model validation errors.
pub fn decode_intrinsics(
    envelope: types::DataEnvelope,
) -> Result<TypedData<IntrinsicsSnapshot>, ClientError> {
    if envelope.metadata.source.game_id != "warframe" {
        return Err(ClientError::WrongTopic);
    }
    decode_snapshot::<IntrinsicsTopic>(envelope)
}

impl Client {
    /// Queries one session's cached progression. Does not start sampling.
    ///
    /// # Errors
    ///
    /// Returns snapshot request, identity or model validation errors.
    pub async fn intrinsics_snapshot(
        &self,
        session: &types::SessionRef,
    ) -> Result<TypedData<IntrinsicsSnapshot>, ClientError> {
        decode_intrinsics(self.snapshot(session, &IntrinsicsTopic::topic()).await?)
    }

    /// Listens to intrinsics, sharing existing SDK feeds.
    /// Decode snapshots from the listener's state with [`decode_intrinsics`].
    ///
    /// # Errors
    ///
    /// Returns connection, protocol or subscription rejection errors.
    pub async fn subscribe_intrinsics(
        &self,
        sessions: types::SessionSelector,
    ) -> Result<Subscription, ClientError> {
        self.subscribe(types::Subscribe {
            sessions,
            topics: vec![IntrinsicsTopic::topic()],
        })
        .await
    }
}
