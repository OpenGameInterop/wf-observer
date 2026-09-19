use provider_sdk::memory::ReadError;
use warframe_model::{ChatChannel, ChatDirection, ChatMessage, ChatTime};

use super::acquisition::Entry;
use crate::native_string::player_name;

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
    } else if key.starts_with("#G_") {
        ChatChannel::Region
    } else if key.starts_with("#S") {
        ChatChannel::Squad
    } else if key.starts_with("#C") {
        ChatChannel::Clan
    } else if key == "#D" || key.starts_with("#D_") {
        ChatChannel::DesignCouncil
    } else if participants(key).is_some() {
        ChatChannel::Direct
    } else {
        // Alliance and other unverified native forms remain Other until validated.
        ChatChannel::Other
    }
}

// The native writer stores sender + ',' + recipient and accepts either order
// when finding an existing private channel. Public keys retain their leading '#'.
fn participants(key: &str) -> Option<(&str, &str)> {
    if key.starts_with('#') {
        return None;
    }
    let (first, second) = key.split_once(',')?;
    let (first, second) = (player_name(first), player_name(second));
    (!first.is_empty() && !second.is_empty() && !second.contains(',')).then_some((first, second))
}

pub(super) fn message(entry: &Entry, key: &str, id: &str, local: Option<&str>) -> ChatMessage {
    let sender = player_name(&entry.sender);
    let local = local.map(player_name).filter(|name| !name.is_empty());
    let direction = if entry.sender.is_empty() {
        // The native notification path supplies an empty author. Role 3 alone
        // also describes player operators and cannot establish System direction.
        ChatDirection::System
    } else if sender.is_empty() {
        ChatDirection::Unknown
    } else if let Some(local) = local {
        if sender.eq_ignore_ascii_case(local) {
            ChatDirection::Outgoing
        } else {
            ChatDirection::Incoming
        }
    } else {
        ChatDirection::Unknown
    };
    let peer = local.and_then(|local| {
        let (first, second) = participants(key)?;
        match (
            first.eq_ignore_ascii_case(local),
            second.eq_ignore_ascii_case(local),
        ) {
            (true, false) => Some(second.to_owned()),
            (false, true) => Some(first.to_owned()),
            _ => None,
        }
    });
    ChatMessage {
        channel: channel(key),
        sender: (!sender.is_empty()).then(|| sender.to_owned()),
        text: entry.text.clone(),
        game_time: entry.game_time,
        direction,
        conversation_id: id.to_owned(),
        peer,
    }
}

#[cfg(test)]
mod tests {
    use super::super::acquisition::Links;
    use super::*;

    fn entry(sender: &str) -> Entry {
        Entry {
            address: 0x10000,
            links: Links {
                next: 0,
                previous: 0,
            },
            role: 3,
            sender: sender.into(),
            text: "<original markup>".into(),
            game_time: ChatTime::new(12, 34),
        }
    }

    #[test]
    fn native_channel_keys_do_not_classify_players_by_their_initials() {
        for key in ["Alice,Me", "Sam,Me", "Fred,Me", "Me,Someone"] {
            assert_eq!(channel(key), ChatChannel::Direct);
        }
        for key in ["", "Alice", "FPlayer", "Me,", ",Me", "Me,Other,Third"] {
            assert_eq!(channel(key), ChatChannel::Other);
            assert_eq!(message(&entry("Me"), key, "c1", Some("Me")).peer, None);
        }
    }

    #[test]
    fn authorship_and_peers_use_normalized_names_not_roles_or_text() {
        let incoming = message(
            &entry("Alice\u{e000}"),
            "Me\u{e001},Alice\u{e000}",
            "c1",
            Some("me"),
        );
        assert_eq!(incoming.direction, ChatDirection::Incoming);
        assert_eq!(incoming.peer.as_deref(), Some("Alice"));
        assert_eq!(incoming.sender.as_deref(), Some("Alice"));
        assert_eq!(incoming.text, "<original markup>");
        let outgoing = message(&entry("ME\u{e001}"), "Alice,Me", "c1", Some("Me"));
        assert_eq!(outgoing.direction, ChatDirection::Outgoing);
        assert_eq!(outgoing.peer.as_deref(), Some("Alice"));
        assert_eq!(outgoing.sender.as_deref(), Some("ME"));
        for (key, local) in [
            ("Alice,Me", None),
            ("Alice,Me", Some("Stranger")),
            ("Me,ME", Some("Me")),
        ] {
            assert_eq!(message(&entry("Alice"), key, "c1", local).peer, None);
        }
        let public = message(&entry("Me"), "#G_EN", "c1", Some("Me"));
        assert_eq!(public.direction, ChatDirection::Outgoing);
        assert_eq!(public.peer, None);
        assert_eq!(
            message(&entry("Alice"), "Alice,Me", "c1", None).direction,
            ChatDirection::Unknown
        );
        assert_eq!(
            message(&entry(""), "", "c1", None).direction,
            ChatDirection::System
        );
        assert_eq!(
            message(&entry("\u{e000}"), "", "c1", Some("Me")).direction,
            ChatDirection::Unknown
        );
    }
}
