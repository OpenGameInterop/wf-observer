use crate::raw::{Client, ClientError, Subscription, Topic, TypedData, decode_snapshot, types};
use warframe_model::PlayerSnapshot;

/// Local player name and stable account identity.
pub struct PlayerTopic;

impl Topic for PlayerTopic {
    const GAME_ID: &'static str = "warframe";
    const PROVIDER_ID: &'static str = "opengameinterop.warframe";
    const NAME: &'static str = "warframe.player";
    const SCHEMA_VERSION: u32 = 1;
}

impl crate::raw::SnapshotTopic for PlayerTopic {
    type Snapshot = PlayerSnapshot;
}

/// Decodes player information after checking topic, schema, game and account identity.
///
/// # Errors
/// Returns identity or payload validation errors.
pub fn decode_player(
    envelope: types::DataEnvelope,
) -> Result<TypedData<PlayerSnapshot>, ClientError> {
    if envelope.metadata.source.game_id != "warframe" {
        return Err(ClientError::WrongTopic);
    }
    decode_snapshot::<PlayerTopic>(envelope)
}

impl Client {
    /// Queries one session's cached player information without creating demand.
    ///
    /// # Errors
    /// Returns request, transport or payload validation errors.
    pub async fn player_snapshot(
        &self,
        session: &types::SessionRef,
    ) -> Result<TypedData<PlayerSnapshot>, ClientError> {
        decode_player(self.snapshot(session, &PlayerTopic::topic()).await?)
    }

    /// Listens to player information, sharing existing SDK feeds.
    /// Decode snapshots from the listener's state with [`decode_player`].
    ///
    /// # Errors
    /// Returns transport or subscription rejection errors.
    pub async fn subscribe_player(
        &self,
        sessions: types::SessionSelector,
    ) -> Result<Subscription, ClientError> {
        self.subscribe(types::Subscribe {
            sessions,
            topics: vec![PlayerTopic::topic()],
        })
        .await
    }
}
