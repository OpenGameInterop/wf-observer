use crate::raw::{Client, ClientError, Subscription, Topic, TypedData, decode_event, types};
use warframe_model::ChatEvent;

/// Newly observed chat messages and explicit source-continuity gaps.
pub struct ChatTopic;

impl Topic for ChatTopic {
    const GAME_ID: &'static str = "warframe";
    const PROVIDER_ID: &'static str = "opengameinterop.warframe";
    const NAME: &'static str = "warframe.chat";
    const SCHEMA_VERSION: u32 = 1;
}

impl crate::raw::EventTopic for ChatTopic {
    type Event = ChatEvent;
}

/// Decodes a chat event after checking topic, schema, game and payload.
/// Account identity belongs to the payload; generation and ordering to the envelope.
///
/// # Errors
/// Returns identity or payload validation errors, including invalid clock times.
pub fn decode_chat(envelope: types::EventEnvelope) -> Result<TypedData<ChatEvent>, ClientError> {
    if envelope.metadata.source.game_id != "warframe" {
        return Err(ClientError::WrongTopic);
    }
    decode_event::<ChatTopic>(envelope)
}

impl Client {
    /// Subscribes to new chat events. Starting/resuming acquisition baselines retained messages.
    /// Events preserve each channel's native order. State reports health;
    /// buffered events expire when their session or generation ends.
    ///
    /// # Errors
    /// Returns transport or subscription rejection errors.
    pub async fn subscribe_chat(
        &self,
        sessions: types::SessionSelector,
    ) -> Result<Subscription, ClientError> {
        self.subscribe(types::Subscribe {
            sessions,
            topics: vec![ChatTopic::topic()],
        })
        .await
    }
}
