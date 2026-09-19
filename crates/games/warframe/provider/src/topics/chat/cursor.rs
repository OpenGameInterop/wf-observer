use std::collections::BTreeMap;

use provider_sdk::memory::ReadError;
use warframe_model::ChatUpdate;

use super::{
    acquisition::{Entry, History},
    facts::MAX_EVENTS,
    layout,
};

#[derive(Clone)]
struct Position {
    channel_address: u64,
    conversation_id: String,
    anchor: Option<Entry>,
}

/// Last published positions and opaque IDs, independent of public enrichment.
#[derive(Clone, Default)]
pub(crate) struct ChatCursor {
    positions: Option<BTreeMap<String, Position>>,
    next_conversation: u64,
}

impl ChatCursor {
    /// Produces tentative positions and IDs. Commit only after publication succeeds.
    pub(crate) fn prepare(
        &self,
        history: &History,
        local: Option<&str>,
    ) -> Result<(Self, Vec<ChatUpdate>, bool), ReadError> {
        let mut positions = BTreeMap::new();
        let mut next_conversation = self.next_conversation;
        let mut updates = Vec::new();
        let mut more = false;
        // Valid acquisition has at most MAX_EVENTS channels, giving each a turn.
        let allowance = MAX_EVENTS / history.channels.len().max(1);
        for channel in &history.channels {
            let previous = self
                .positions
                .as_ref()
                .and_then(|positions| positions.get(&channel.key))
                .filter(|position| position.channel_address == channel.address);
            let mut position = if let Some(previous) = previous {
                previous.clone()
            } else {
                next_conversation = next_conversation
                    .checked_add(1)
                    .ok_or_else(|| ReadError::limit("chat conversation IDs"))?;
                Position {
                    channel_address: channel.address,
                    conversation_id: format!("c{next_conversation}"),
                    anchor: None,
                }
            };
            if self.positions.is_none() {
                // Only initial acquisition baselines a newly observed conversation.
                position.anchor = channel.entries.last().cloned();
                positions.insert(channel.key.clone(), position);
                continue;
            }
            let start = if let Some(anchor) = &position.anchor {
                if let Some(index) = channel
                    .entries
                    .iter()
                    .position(|entry| entry.same_occurrence(anchor))
                {
                    index + 1
                } else {
                    // Lost position in an existing instance: skip retained entries.
                    position.anchor = channel.entries.last().cloned();
                    updates.push(ChatUpdate::Gap {
                        channel: layout::channel(&channel.key),
                    });
                    positions.insert(channel.key.clone(), position);
                    continue;
                }
            } else {
                0
            };
            let pending = &channel.entries[start..];
            for entry in pending.iter().take(allowance) {
                updates.push(ChatUpdate::Message {
                    value: layout::message(entry, &channel.key, &position.conversation_id, local),
                });
                position.anchor = Some(entry.clone());
            }
            more |= pending.len() > allowance;
            positions.insert(channel.key.clone(), position);
        }
        Ok((
            Self {
                positions: Some(positions),
                next_conversation,
            },
            updates,
            more,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::super::acquisition::{Channel, Links};
    use super::*;
    use warframe_model::{ChatDirection, ChatMessage, ChatTime};

    fn channel(key: &str, address: u64, ids: impl IntoIterator<Item = u64>) -> Channel {
        Channel {
            address,
            links: Links {
                next: 0,
                previous: 0,
            },
            key: key.into(),
            entry_links: Links {
                next: 0,
                previous: 0,
            },
            entries: ids
                .into_iter()
                .map(|id| Entry {
                    address: id * 0x100,
                    links: Links {
                        next: 0,
                        previous: id.saturating_sub(1) * 0x100,
                    },
                    role: 0,
                    sender: "Alice\u{e000}".into(),
                    text: "same text".into(),
                    game_time: ChatTime::new(12, 34),
                })
                .collect(),
        }
    }

    fn history(ids: impl IntoIterator<Item = u64>) -> History {
        History {
            channels: vec![channel("Alice,Me", 0x10000, ids)],
        }
    }

    fn messages(updates: &[ChatUpdate]) -> Vec<&ChatMessage> {
        updates
            .iter()
            .filter_map(|update| match update {
                ChatUpdate::Message { value } => Some(value),
                ChatUpdate::Gap { .. } => None,
            })
            .collect()
    }

    #[test]
    fn initial_history_is_skipped_and_identical_occurrences_are_preserved() -> Result<(), ReadError>
    {
        let (cursor, events, more) = ChatCursor::default().prepare(&history([1]), Some("Me"))?;
        assert!(events.is_empty() && !more);
        let appended = history([1, 2, 3]);
        let (accepted, events, more) = cursor.prepare(&appended, Some("Me"))?;
        assert_eq!(messages(&events).len(), 2);
        assert_eq!(messages(&events)[0], messages(&events)[1]);
        assert!(!more);
        assert_eq!(cursor.prepare(&appended, Some("Me"))?.1, events);
        assert!(accepted.prepare(&appended, Some("Me"))?.1.is_empty());
        assert!(
            ChatCursor::default()
                .prepare(&appended, Some("Me"))?
                .1
                .is_empty()
        );
        Ok(())
    }

    #[test]
    fn new_conversations_emit_first_whispers_with_stable_and_distinct_ids() -> Result<(), ReadError>
    {
        let empty = History { channels: vec![] };
        let (cursor, _, _) = ChatCursor::default().prepare(&empty, Some("Me"))?;
        let mut history = History {
            channels: vec![
                channel("Alice,Me", 0x10000, [1, 2]),
                channel("Me,Sam", 0x20000, [3]),
            ],
        };
        history.channels[0].entries[1].sender = "ME\u{e001}".into();
        let (_, events, _) = cursor.prepare(&history, Some("Me"))?;
        let messages = messages(&events);
        assert_eq!(messages.len(), 3);
        assert!(!messages[0].conversation_id.is_empty());
        assert_eq!(messages[0].conversation_id, messages[1].conversation_id);
        assert_ne!(messages[0].conversation_id, messages[2].conversation_id);
        assert_eq!(cursor.prepare(&history, Some("Me"))?.1, events);
        Ok(())
    }

    #[test]
    fn lost_positions_skip_retained_entries_but_keep_the_conversation_id() -> Result<(), ReadError>
    {
        let (cursor, _, _) = ChatCursor::default().prepare(&history([]), None)?;
        let (cursor, first, _) = cursor.prepare(&history([1]), None)?;
        let (cursor, lost, _) = cursor.prepare(&history([2, 3]), None)?;
        assert!(matches!(lost.as_slice(), [ChatUpdate::Gap { .. }]));
        let (cursor, next, _) = cursor.prepare(&history([2, 3, 4]), Some("Me"))?;
        assert_eq!(messages(&next).len(), 1);
        assert_eq!(
            messages(&first)[0].conversation_id,
            messages(&next)[0].conversation_id
        );
        let mut recycled = history([4]);
        recycled.channels[0].entries[0].text = "replacement".into();
        assert!(matches!(
            cursor.prepare(&recycled, None)?.1.as_slice(),
            [ChatUpdate::Gap { .. }]
        ));
        Ok(())
    }

    #[test]
    fn enrichment_changes_do_not_change_raw_occurrence_identity() -> Result<(), ReadError> {
        let (cursor, _, _) = ChatCursor::default().prepare(&history([]), None)?;
        let (cursor, first, _) = cursor.prepare(&history([1]), None)?;
        assert_eq!(messages(&first)[0].direction, ChatDirection::Unknown);
        assert_eq!(messages(&first)[0].peer, None);
        assert!(cursor.prepare(&history([1]), Some("Me"))?.1.is_empty());
        let (_, next, _) = cursor.prepare(&history([1, 2]), Some("Me"))?;
        assert_eq!(messages(&next).len(), 1);
        assert_eq!(messages(&next)[0].direction, ChatDirection::Incoming);
        assert_eq!(
            messages(&next)[0].conversation_id,
            messages(&first)[0].conversation_id
        );
        Ok(())
    }

    #[test]
    fn recreated_channels_receive_new_ids_even_with_reused_entry_addresses() -> Result<(), ReadError>
    {
        let (cursor, _, _) = ChatCursor::default().prepare(&history([]), None)?;
        let (cursor, first, _) = cursor.prepare(&history([1]), None)?;
        let mut recreated = history([1]);
        recreated.channels[0].address += 0x10000;
        let (_, next, _) = cursor.prepare(&recreated, None)?;
        assert_eq!(messages(&next).len(), 1);
        assert_ne!(
            messages(&first)[0].conversation_id,
            messages(&next)[0].conversation_id
        );
        let (removed, _, _) = cursor.prepare(&History { channels: vec![] }, None)?;
        let (_, next, _) = removed.prepare(&history([1]), None)?;
        assert_eq!(messages(&next).len(), 1);
        assert_ne!(
            messages(&first)[0].conversation_id,
            messages(&next)[0].conversation_id
        );
        Ok(())
    }

    #[test]
    fn busy_public_channels_cannot_starve_whispers_and_batches_stay_bounded()
    -> Result<(), ReadError> {
        let empty = History { channels: vec![] };
        let (mut cursor, _, _) = ChatCursor::default().prepare(&empty, Some("Me"))?;
        let history = History {
            channels: vec![
                channel("#G_EN", 0x10000, 1..101),
                channel("#T_EN", 0x20000, 101..201),
                channel("Alice,Me", 0x30000, [201, 202]),
            ],
        };
        let mut total = 0;
        loop {
            let (next, events, more) = cursor.prepare(&history, Some("Me"))?;
            assert!(events.len() <= MAX_EVENTS);
            if total == 0 {
                assert_eq!(
                    messages(&events)
                        .iter()
                        .filter(|message| message.peer.is_some())
                        .count(),
                    2
                );
            }
            total += messages(&events).len();
            cursor = next;
            if !more {
                break;
            }
        }
        assert_eq!(total, 202);
        Ok(())
    }
}
