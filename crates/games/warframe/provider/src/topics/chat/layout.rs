use provider_sdk::memory::ReadError;
use warframe_model::{ChatChannel, ChatTime};

pub(super) fn time(text: &str) -> Result<Option<ChatTime>, ReadError> {
    if text.is_empty() {
        return Ok(None);
    }
    let bytes = text.as_bytes();
    if let [b'[', h1, h2, b':', m1, m2, b']', b' '] = bytes
        && [h1, h2, m1, m2].iter().all(|b| b.is_ascii_digit())
    {
        return ChatTime::new((h1 - b'0') * 10 + h2 - b'0', (m1 - b'0') * 10 + m2 - b'0')
            .map(Some)
            .ok_or_else(|| ReadError::invalid("chat clock time"));
    }
    Err(ReadError::invalid("chat timestamp representation"))
}

pub(super) fn channel(key: &str) -> ChatChannel {
    if key.starts_with("#Q_") {
        ChatChannel::QuestionsAndAnswers
    } else if key.starts_with("#R_") {
        ChatChannel::Recruiting
    } else if key.starts_with("#T_") {
        ChatChannel::Trade
    } else if key.starts_with("#H_") {
        ChatChannel::Hub
    } else if key.starts_with("G_") {
        ChatChannel::Region
    } else {
        match key.as_bytes().first() {
            Some(b'F') if key.len() > 1 => ChatChannel::Direct,
            Some(b'S') => ChatChannel::Squad,
            Some(b'C') => ChatChannel::Clan,
            Some(b'A') => ChatChannel::Alliance,
            Some(b'D') => ChatChannel::DesignCouncil,
            _ => ChatChannel::Other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_text_has_no_invented_date_or_seconds() -> Result<(), ReadError> {
        assert_eq!(time("[23:59] ")?, ChatTime::new(23, 59));
        assert_eq!(time("")?, None);
        for bad in ["[24:00] ", "[01:60] ", "[1:00] ", "[01:00:00] ", "[ab:00] "] {
            assert!(time(bad).is_err());
        }
        Ok(())
    }
}
