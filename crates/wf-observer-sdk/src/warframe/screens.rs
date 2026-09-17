use crate::raw::{Client, ClientError, Subscription, Topic, TypedData, decode_snapshot, types};
use warframe_model::ScreensSnapshot;

/// Visible interface movies, published as full replacements.
pub struct ScreensTopic;

impl Topic for ScreensTopic {
    const GAME_ID: &'static str = "warframe";
    const PROVIDER_ID: &'static str = "opengameinterop.warframe";
    const NAME: &'static str = "warframe.screens";
    const SCHEMA_VERSION: u32 = 1;
}

impl crate::raw::SnapshotTopic for ScreensTopic {
    type Snapshot = ScreensSnapshot;
}

/// Checks topic/schema, game identity and the complete topic payload.
///
/// # Errors
///
/// Returns identity or model validation errors.
pub fn decode_screens(
    envelope: types::DataEnvelope,
) -> Result<TypedData<ScreensSnapshot>, ClientError> {
    if envelope.metadata.source.game_id != "warframe" {
        return Err(ClientError::WrongTopic);
    }
    decode_snapshot::<ScreensTopic>(envelope)
}

impl Client {
    /// Queries one session's cached state. Does not start sampling.
    ///
    /// # Errors
    ///
    /// Returns snapshot request, identity or model validation errors.
    pub async fn screens_snapshot(
        &self,
        session: &types::SessionRef,
    ) -> Result<TypedData<ScreensSnapshot>, ClientError> {
        decode_screens(self.snapshot(session, &ScreensTopic::topic()).await?)
    }

    /// Listens to screens, sharing existing SDK feeds.
    /// Decode snapshots from the listener's state with [`decode_screens`].
    ///
    /// # Errors
    ///
    /// Returns connection, protocol or subscription rejection errors.
    pub async fn subscribe_screens(
        &self,
        sessions: types::SessionSelector,
    ) -> Result<Subscription, ClientError> {
        self.subscribe(types::Subscribe {
            sessions,
            topics: vec![ScreensTopic::topic()],
        })
        .await
    }
}
