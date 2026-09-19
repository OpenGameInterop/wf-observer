//! Retained node completion credit for Normal and Steel Path.
use crate::raw::{Client, ClientError, Subscription, Topic, TypedData, decode_snapshot, types};
use warframe_model::StarChartSnapshot;

/// Retained completion counts and Steel Path completion flags by node tag.
pub struct StarChartTopic;

impl Topic for StarChartTopic {
    const GAME_ID: &'static str = "warframe";
    const PROVIDER_ID: &'static str = "opengameinterop.warframe";
    const NAME: &'static str = "warframe.star_chart";
    const SCHEMA_VERSION: u32 = 1;
}

impl crate::raw::SnapshotTopic for StarChartTopic {
    type Snapshot = StarChartSnapshot;
}

/// Checks topic/schema, game identity and the complete Star Chart payload.
///
/// # Errors
///
/// Returns identity or model validation errors.
pub fn decode_star_chart(
    envelope: types::DataEnvelope,
) -> Result<TypedData<StarChartSnapshot>, ClientError> {
    if envelope.metadata.source.game_id != "warframe" {
        return Err(ClientError::WrongTopic);
    }
    decode_snapshot::<StarChartTopic>(envelope)
}

impl Client {
    /// Queries one session's cached progression. Does not start sampling.
    ///
    /// # Errors
    ///
    /// Returns snapshot request, identity or model validation errors.
    pub async fn star_chart_snapshot(
        &self,
        session: &types::SessionRef,
    ) -> Result<TypedData<StarChartSnapshot>, ClientError> {
        decode_star_chart(self.snapshot(session, &StarChartTopic::topic()).await?)
    }

    /// Listens to Star Chart progress, sharing existing SDK feeds.
    /// Decode snapshots from the listener's state with [`decode_star_chart`].
    ///
    /// # Errors
    ///
    /// Returns connection, protocol or subscription rejection errors.
    pub async fn subscribe_star_chart(
        &self,
        sessions: types::SessionSelector,
    ) -> Result<Subscription, ClientError> {
        self.subscribe(types::Subscribe {
            sessions,
            topics: vec![StarChartTopic::topic()],
        })
        .await
    }
}
