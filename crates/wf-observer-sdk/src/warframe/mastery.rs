use crate::raw::{Client, ClientError, Subscription, Topic, TypedData, decode_snapshot, types};
use warframe_model::MasterySnapshot;

/// Completed rank, mastery point totals and retained per-item raw affinity.
pub struct MasteryTopic;

impl Topic for MasteryTopic {
    const GAME_ID: &'static str = "warframe";
    const PROVIDER_ID: &'static str = "opengameinterop.warframe";
    const NAME: &'static str = "warframe.mastery";
    const SCHEMA_VERSION: u32 = 1;
}

impl crate::raw::SnapshotTopic for MasteryTopic {
    type Snapshot = MasterySnapshot;
}

/// Checks topic/schema, game identity and the complete mastery payload.
///
/// # Errors
///
/// Returns identity or model validation errors.
pub fn decode_mastery(
    envelope: types::DataEnvelope,
) -> Result<TypedData<MasterySnapshot>, ClientError> {
    if envelope.metadata.source.game_id != "warframe" {
        return Err(ClientError::WrongTopic);
    }
    decode_snapshot::<MasteryTopic>(envelope)
}

impl Client {
    /// Queries one session's cached progression. Does not start sampling.
    ///
    /// # Errors
    ///
    /// Returns snapshot request, identity or model validation errors.
    pub async fn mastery_snapshot(
        &self,
        session: &types::SessionRef,
    ) -> Result<TypedData<MasterySnapshot>, ClientError> {
        decode_mastery(self.snapshot(session, &MasteryTopic::topic()).await?)
    }

    /// Listens to mastery, sharing existing SDK feeds.
    /// Decode snapshots from the listener's state with [`decode_mastery`].
    ///
    /// # Errors
    ///
    /// Returns connection, protocol or subscription rejection errors.
    pub async fn subscribe_mastery(
        &self,
        sessions: types::SessionSelector,
    ) -> Result<Subscription, ClientError> {
        self.subscribe(types::Subscribe {
            sessions,
            topics: vec![MasteryTopic::topic()],
        })
        .await
    }
}
