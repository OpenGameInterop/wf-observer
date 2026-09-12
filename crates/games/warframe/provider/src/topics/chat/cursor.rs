use std::collections::BTreeMap;
use warframe_model::ChatUpdate;

use super::{
    acquisition::{Entry, History},
    facts::MAX_EVENTS,
    layout,
};

#[derive(Clone, Default)]
struct Position {
    anchor: Option<Entry>,
    gap: bool,
}

/// Last published positions, independent of the latest acquired history.
#[derive(Clone, Default)]
pub(crate) struct ChatCursor {
    positions: Option<BTreeMap<String, Position>>,
}

impl ChatCursor {
    /// Produces a tentative cursor. Keep `self` until the host accepts this batch.
    pub(crate) fn prepare(&self, history: &History) -> (Self, Vec<ChatUpdate>, bool) {
        let mut positions = BTreeMap::new();
        let mut updates = Vec::new();
        let mut more = false;
        // Each channel gets a share so a busy public channel cannot starve whispers.
        let allowance = MAX_EVENTS / history.channels.len().max(1);
        for channel in &history.channels {
            let Some(previous) = &self.positions else {
                positions.insert(
                    channel.key.clone(),
                    Position {
                        anchor: channel.entries.last().cloned(),
                        gap: false,
                    },
                );
                continue;
            };
            let mut position = previous.get(&channel.key).cloned().unwrap_or(Position {
                anchor: None,
                gap: true,
            });
            let start = if let Some(anchor) = &position.anchor {
                let index = channel.entries.iter().position(|entry| {
                    entry.address == anchor.address
                        && entry.links.previous == anchor.links.previous
                        && entry.flags == anchor.flags
                        && entry.message == anchor.message
                });
                if let Some(index) = index {
                    index + 1
                } else {
                    position.anchor = None;
                    position.gap = true;
                    0
                }
            } else {
                0
            };
            let mut remaining = allowance;
            if position.gap && remaining > 0 {
                updates.push(ChatUpdate::Gap {
                    channel: layout::channel(&channel.key),
                });
                position.gap = false;
                remaining -= 1;
            }
            let pending = &channel.entries[start..];
            for entry in pending.iter().take(remaining) {
                updates.push(ChatUpdate::Message {
                    value: entry.message.clone(),
                });
                position.anchor = Some(entry.clone());
            }
            more |= position.gap || pending.len() > remaining;
            positions.insert(channel.key.clone(), position);
        }
        (
            Self {
                positions: Some(positions),
            },
            updates,
            more,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::super::acquisition::{Channel, Links};
    use super::*;
    use warframe_model::{ChatChannel, ChatMessage, ChatTime};

    fn history(ids: impl IntoIterator<Item = u64>) -> History {
        let entries = ids
            .into_iter()
            .map(|id| Entry {
                address: id * 0x100,
                links: Links {
                    next: 0,
                    previous: id.saturating_sub(1) * 0x100,
                },
                flags: 0,
                message: ChatMessage {
                    channel: ChatChannel::Squad,
                    sender: Some("Player".into()),
                    text: "same text".into(),
                    game_time: ChatTime::new(12, 34),
                },
            })
            .collect();
        History {
            channels: vec![Channel {
                address: 0x10000,
                links: Links {
                    next: 0,
                    previous: 0,
                },
                key: "S".into(),
                entry_links: Links {
                    next: 0,
                    previous: 0,
                },
                entries,
            }],
        }
    }

    #[test]
    fn baseline_repeats_and_rejected_batches_do_not_lose_or_replay_messages() {
        let (cursor, events, more) = ChatCursor::default().prepare(&history([1]));
        assert!(events.is_empty() && !more);
        let (accepted, events, more) = cursor.prepare(&history([1, 2, 3]));
        assert_eq!(events.len(), 2); // Equal text is still two separate messages.
        assert!(!more);
        assert_eq!(cursor.prepare(&history([1, 2, 3])).1, events); // Discard: retry unchanged position.
        assert!(accepted.prepare(&history([1, 2, 3])).1.is_empty());
        assert!(
            ChatCursor::default()
                .prepare(&history([1, 2, 3]))
                .1
                .is_empty()
        ); // New generation.
    }

    #[test]
    fn missing_or_recycled_anchors_report_gaps_and_large_bursts_are_bounded() {
        let (cursor, _, _) = ChatCursor::default().prepare(&history([1]));
        let mut recycled = history([1]);
        recycled.channels[0].entries[0].message.text = "replacement".into();
        assert!(matches!(
            cursor.prepare(&recycled).1.first(),
            Some(ChatUpdate::Gap { .. })
        ));
        let (mut cursor, events, _) = cursor.prepare(&history([]));
        assert!(matches!(events.as_slice(), [ChatUpdate::Gap { .. }]));
        let burst = history(10..110);
        let mut messages = 0;
        loop {
            let (next, events, more) = cursor.prepare(&burst);
            assert!(events.len() <= MAX_EVENTS);
            messages += events
                .iter()
                .filter(|event| matches!(event, ChatUpdate::Message { .. }))
                .count();
            cursor = next;
            if !more {
                break;
            }
        }
        assert_eq!(messages, 100);
    }
}
