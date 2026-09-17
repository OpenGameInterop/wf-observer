use crate::raw::{Client, ClientError, Subscription, Topic, TypedData, decode_snapshot, types};
use warframe_model::RelicRewardsSnapshot;

/// Current relic reward picker, published as full replacements.
pub struct RelicRewardsTopic;

impl Topic for RelicRewardsTopic {
    const GAME_ID: &'static str = "warframe";
    const PROVIDER_ID: &'static str = "opengameinterop.warframe";
    const NAME: &'static str = "warframe.relic_rewards";
    const SCHEMA_VERSION: u32 = 1;
}

impl crate::raw::SnapshotTopic for RelicRewardsTopic {
    type Snapshot = RelicRewardsSnapshot;
}

/// Checks topic/schema, game identity and the complete topic payload.
///
/// # Errors
///
/// Returns identity or model validation errors.
pub fn decode_relic_rewards(
    envelope: types::DataEnvelope,
) -> Result<TypedData<RelicRewardsSnapshot>, ClientError> {
    if envelope.metadata.source.game_id != "warframe" {
        return Err(ClientError::WrongTopic);
    }
    decode_snapshot::<RelicRewardsTopic>(envelope)
}

impl Client {
    /// Queries one session's cached state. Does not start sampling.
    ///
    /// # Errors
    ///
    /// Returns snapshot request, identity or model validation errors.
    pub async fn relic_rewards_snapshot(
        &self,
        session: &types::SessionRef,
    ) -> Result<TypedData<RelicRewardsSnapshot>, ClientError> {
        decode_relic_rewards(self.snapshot(session, &RelicRewardsTopic::topic()).await?)
    }

    /// Listens to relic rewards, sharing existing SDK feeds.
    /// Decode snapshots from the listener's state with [`decode_relic_rewards`].
    ///
    /// # Errors
    ///
    /// Returns connection, protocol or subscription rejection errors.
    pub async fn subscribe_relic_rewards(
        &self,
        sessions: types::SessionSelector,
    ) -> Result<Subscription, ClientError> {
        self.subscribe(types::Subscribe {
            sessions,
            topics: vec![RelicRewardsTopic::topic()],
        })
        .await
    }
}
