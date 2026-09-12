//! Chat messages and continuity updates.

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

/// One retained game message. Text may contain game markup; render it as untrusted text.
#[boltffi::data]
#[derive(Debug, Clone, ..Eq, ..Serde)]
pub struct ChatMessage {
    pub channel: ChatChannel,
    /// Player name without its platform-icon suffix; absent for system messages.
    pub sender: Option<String>,
    pub text: String,
    /// The game's local hour/minute, not the observer's acquisition time.
    pub game_time: Option<ChatTime>,
}

#[boltffi::data]
#[derive(Debug, Clone, ..Eq, ..Serde)]
pub enum ChatUpdate {
    Message {
        #[serde(rename = "message")]
        value: ChatMessage,
    },
    /// The prior position is no longer retained, or a channel appeared without a baseline.
    /// Following messages may overlap previously observed history.
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
    fn time_requires_valid_hours_and_minutes() -> Result<(), Box<dyn std::error::Error>> {
        for (hour, minute) in [(0, 0), (23, 59)] {
            let time = ChatTime::new(hour, minute).ok_or("valid time")?;
            assert_eq!(
                serde_json::from_value::<ChatTime>(serde_json::to_value(time)?)?,
                time
            );
        }
        for bad in [
            serde_json::json!({"hour":24,"minute":0}),
            serde_json::json!({"hour":0,"minute":60}),
            serde_json::json!({"hour":-1,"minute":0}),
            serde_json::json!({"hour":1}),
            serde_json::json!("12:34"),
        ] {
            assert!(serde_json::from_value::<ChatTime>(bad).is_err());
        }
        Ok(())
    }
}
