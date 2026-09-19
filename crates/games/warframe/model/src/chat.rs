//! Best-effort live chat messages and continuity updates.

use crate::AccountId;

/// Game-local clock time at minute precision, without a date or UTC offset.
#[boltffi::data]
#[derive(Debug, ..Copy, ..Eq, ..Serde)]
#[serde(try_from = "UncheckedTime")]
pub struct ChatTime {
    pub hour: u8,
    pub minute: u8,
}

#[derive(serde::Deserialize)]
struct UncheckedTime {
    hour: u8,
    minute: u8,
}

impl ChatTime {
    /// Returns a clock time only for hours 0–23 and minutes 0–59.
    #[must_use]
    pub const fn new(hour: u8, minute: u8) -> Option<Self> {
        if hour < 24 && minute < 60 {
            Some(Self { hour, minute })
        } else {
            None
        }
    }
}

impl TryFrom<UncheckedTime> for ChatTime {
    type Error = &'static str;

    fn try_from(value: UncheckedTime) -> Result<Self, Self::Error> {
        Self::new(value.hour, value.minute).ok_or("invalid chat clock time")
    }
}

/// Channel category, not a private conversation identifier.
#[boltffi::data]
#[derive(Debug, ..Copy, ..Eq, ..Serde)]
pub enum ChatChannel {
    Direct,
    Squad,
    Clan,
    Alliance,
    Region,
    QuestionsAndAnswers,
    Recruiting,
    Trade,
    DesignCouncil,
    Hub,
    Other,
}

/// Authorship relative to the account in the event envelope.
#[boltffi::data]
#[derive(Debug, ..Copy, ..Eq, ..Serde)]
pub enum ChatDirection {
    /// Another player authored the message.
    Incoming,
    /// The observed player authored the message.
    Outgoing,
    /// The game generated the message.
    System,
    /// The provider could not establish authorship reliably.
    Unknown,
}

/// One newly observed message. Text retains game markup; render it as untrusted text.
#[boltffi::data]
#[derive(Debug, Clone, ..Eq, ..Serde)]
pub struct ChatMessage {
    pub channel: ChatChannel,
    /// Author name without its platform-icon suffix; absent for system or unknown authors.
    pub sender: Option<String>,
    pub text: String,
    /// The game's local hour/minute, not the observer's acquisition time.
    pub game_time: Option<ChatTime>,
    pub direction: ChatDirection,
    /// Opaque tracked-channel identity, scoped to the envelope's session, account and generation.
    /// Stable across directions and lost positions; may change after removal or acquisition reset.
    /// Public channels also have an ID. Its format has no consumer-visible meaning.
    #[serde(deserialize_with = "conversation_id")]
    pub conversation_id: String,
    /// The other participant in a direct conversation, independent of direction.
    /// Absent for public channels or when the participant cannot be established reliably.
    pub peer: Option<String>,
}

fn conversation_id<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    let value = <String as serde::Deserialize>::deserialize(deserializer)?;
    if value.is_empty() {
        return Err(serde::de::Error::custom("empty chat conversation ID"));
    }
    Ok(value)
}

#[boltffi::data]
#[derive(Debug, Clone, ..Eq, ..Serde)]
pub enum ChatUpdate {
    Message {
        #[serde(rename = "message")]
        value: ChatMessage,
    },
    /// A tracked channel lost its prior position. Retained history is skipped and
    /// delivery resumes with subsequently observed messages. No recovery is required.
    Gap { channel: ChatChannel },
}

/// One chat update belonging to an explicit account; ordering is in the envelope.
#[derive(Debug, Clone, ..Eq, ..Serde)]
pub struct ChatEvent {
    pub account_id: AccountId,
    pub update: ChatUpdate,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversation_id_must_not_be_empty() {
        let mut wire = serde_json::json!({
            "channel": "Direct",
            "sender": "Someone",
            "text": "hello",
            "direction": "Incoming",
            "conversation_id": "opaque-conversation",
            "peer": "Someone",
        });
        assert!(serde_json::from_value::<ChatMessage>(wire.clone()).is_ok());
        wire["conversation_id"] = serde_json::json!("");
        assert!(serde_json::from_value::<ChatMessage>(wire).is_err());
    }
}
