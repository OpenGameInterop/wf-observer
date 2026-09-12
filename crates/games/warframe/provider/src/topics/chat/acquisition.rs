use std::collections::BTreeSet;

use memory_reader::ProcessMemory;
use provider_sdk::memory::{ReadError, TargetReader, read_stable};
use warframe_model::ChatMessage;

use super::{
    facts::{
        CHAT, MAX_CHANNELS, MAX_ENTRIES, MAX_MESSAGE_BYTES, MAX_STRING_BYTES, MAX_TOTAL_ENTRIES,
    },
    layout,
};
use crate::{
    native_string::{player_name, read_string},
    roots::LoginIdentity,
    target::READ_LIMITS,
};

#[derive(Clone, ..Eq)]
pub(crate) struct History {
    pub(super) channels: Vec<Channel>,
}

#[derive(Clone, ..Eq)]
pub(super) struct Channel {
    pub(super) address: u64,
    pub(super) links: Links,
    pub(super) key: String,
    pub(super) entry_links: Links,
    pub(super) entries: Vec<Entry>,
}

/// Private identity includes contents because native node addresses are recycled.
#[derive(Clone, ..Eq)]
pub(super) struct Entry {
    pub(super) address: u64,
    pub(super) links: Links,
    pub(super) flags: u32,
    pub(super) message: ChatMessage,
}

#[derive(Debug, ..Copy, ..Eq)]
pub(super) struct Links {
    pub(super) next: u64,
    pub(super) previous: u64,
}

pub(crate) fn read_chat(
    memory: &mut (impl ProcessMemory + ?Sized),
    base: u64,
    size: u32,
    login: &LoginIdentity,
    cached: Option<&History>,
) -> Result<Option<History>, ReadError> {
    let mut reader = TargetReader::new(memory, base, size, READ_LIMITS)?;
    reader.require_vtable_slot_le64(
        login.profile_data.get(),
        CHAT.handler_slot,
        CHAT.handler,
        "chat message handler",
    )?;
    let sentinel = reader.object_address(login.profile_data.get(), CHAT.channels, 16)?;
    let first = headers(&mut reader, sentinel)?;
    if let Some(cached) = cached
        && same_headers(&first, &cached.channels)
    {
        let mut unchanged = true;
        for channel in &cached.channels {
            if let Some(tail) = channel.entries.last() {
                unchanged &= read_entry(&mut reader, tail.address, &channel.key)? == *tail;
            }
        }
        if unchanged && same_headers(&headers(&mut reader, sentinel)?, &first) {
            return Ok(None);
        }
    }
    read_stable(
        || {
            let mut channels = headers(&mut reader, sentinel)?;
            let mut count = 0;
            let mut bytes = 0;
            for channel in &mut channels {
                let sentinel = reader.object_address(channel.address, CHAT.channel_entries, 16)?;
                let mut current = channel.entry_links.next;
                let mut previous = sentinel;
                let mut seen = BTreeSet::new();
                bytes += channel.key.len();
                while current != sentinel {
                    if channel.entries.len() >= MAX_ENTRIES || count >= MAX_TOTAL_ENTRIES {
                        return Err(ReadError::limit("chat entries"));
                    }
                    if !seen.insert(current) {
                        return Err(ReadError::invalid("chat entry cycle"));
                    }
                    let entry = read_entry(&mut reader, current, &channel.key)?;
                    if entry.links.previous != previous {
                        return Err(ReadError::changed("chat entry links"));
                    }
                    bytes += entry.message.text.len()
                        + entry.message.sender.as_ref().map_or(0, String::len);
                    if bytes > MAX_STRING_BYTES {
                        return Err(ReadError::limit("chat strings"));
                    }
                    previous = current;
                    current = entry.links.next;
                    channel.entries.push(entry);
                    count += 1;
                }
                if previous != channel.entry_links.previous {
                    return Err(ReadError::changed("chat entry tail"));
                }
            }
            if !same_headers(&headers(&mut reader, sentinel)?, &channels) {
                return Err(ReadError::changed("chat channel headers"));
            }
            Ok(History { channels })
        },
        || ReadError::changed("chat history"),
    )
    .map(Some)
}

fn links(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
    address: u64,
) -> Result<Links, ReadError> {
    if address == 0 || !address.is_multiple_of(8) {
        return Err(ReadError::invalid("chat node pointer"));
    }
    let bytes = reader.read_array::<16>(address)?;
    Ok(Links {
        next: u64::from_le_bytes(bytes.as_chunks::<8>().0[0]),
        previous: u64::from_le_bytes(bytes.as_chunks::<8>().0[1]),
    })
}

fn headers(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
    sentinel: u64,
) -> Result<Vec<Channel>, ReadError> {
    let head = links(reader, sentinel)?;
    let mut current = head.next;
    let mut previous = sentinel;
    let mut channels = Vec::new();
    let mut seen = BTreeSet::new();
    let mut keys = BTreeSet::new();
    while current != sentinel {
        if channels.len() >= MAX_CHANNELS {
            return Err(ReadError::limit("chat channels"));
        }
        if !seen.insert(current) {
            return Err(ReadError::invalid("chat channel cycle"));
        }
        let node = links(reader, current)?;
        if node.previous != previous {
            return Err(ReadError::changed("chat channel links"));
        }
        let key = read_string(reader, current, CHAT.channel_name, 128)?;
        if key.is_empty() || !keys.insert(key.clone()) {
            return Err(ReadError::invalid("chat channel name"));
        }
        let entries = reader.object_address(current, CHAT.channel_entries, 16)?;
        channels.push(Channel {
            address: current,
            links: node,
            key,
            entry_links: links(reader, entries)?,
            entries: Vec::new(),
        });
        previous = current;
        current = node.next;
    }
    if head.previous != previous || head != links(reader, sentinel)? {
        return Err(ReadError::changed("chat channel head"));
    }
    Ok(channels)
}

fn same_headers(first: &[Channel], second: &[Channel]) -> bool {
    first.len() == second.len()
        && first.iter().zip(second).all(|(a, b)| {
            a.address == b.address
                && a.links == b.links
                && a.key == b.key
                && a.entry_links == b.entry_links
        })
}

fn read_entry(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
    address: u64,
    key: &str,
) -> Result<Entry, ReadError> {
    let first = links(reader, address)?;
    let sender = read_string(reader, address, CHAT.sender, 128)?;
    let text = read_string(reader, address, CHAT.text, MAX_MESSAGE_BYTES)?;
    let time = read_string(reader, address, CHAT.timestamp, 64)?;
    let flags = reader.read_object_u32(address, CHAT.flags)?;
    if first != links(reader, address)? {
        return Err(ReadError::changed("chat entry"));
    }
    let sender = player_name(&sender);
    Ok(Entry {
        address,
        links: first,
        flags,
        message: ChatMessage {
            channel: layout::channel(key),
            sender: (!sender.is_empty()).then(|| sender.to_owned()),
            text,
            game_time: layout::time(&time)?,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use memory_reader::{AccessError, MemoryModule, Target};

    struct Record([u8; 80]);

    impl ProcessMemory for Record {
        fn target(&self) -> &Target {
            unreachable!("record-only test")
        }
        fn verify(&mut self) -> Result<(), AccessError> {
            unreachable!("record-only test")
        }
        fn modules(&mut self) -> Result<Vec<MemoryModule>, AccessError> {
            unreachable!("record-only test")
        }
        fn read_into(&mut self, address: u64, output: &mut [u8]) -> Result<(), AccessError> {
            let start = address
                .checked_sub(0x10000)
                .and_then(|at| usize::try_from(at).ok());
            let bytes = start
                .and_then(|at| self.0.get(at..at.checked_add(output.len())?))
                .ok_or(AccessError::InvalidReadRange {
                    address,
                    length: output.len(),
                })?;
            output.copy_from_slice(bytes);
            Ok(())
        }
    }

    #[test]
    fn copied_entry_fields_preserve_text_and_validate_clock_and_strings() -> Result<(), ReadError> {
        let mut record = Record([0; 80]);
        for (at, text) in [(24, "Player\u{e000}"), (40, "<text>"), (56, "[01:02] ")] {
            record.0[at..at + text.len()].copy_from_slice(text.as_bytes());
            record.0[at + 15] =
                u8::try_from(15 - text.len()).map_err(|_| ReadError::invalid("test string"))?;
        }
        let read = |record: &mut Record| {
            read_entry(
                &mut TargetReader::new(record, 0x10000, 80, READ_LIMITS)?,
                0x10000,
                "S",
            )
        };
        let entry = read(&mut record)?;
        assert_eq!(entry.message.sender.as_deref(), Some("Player"));
        assert_eq!(entry.message.text, "<text>");
        assert_eq!(entry.message.game_time, warframe_model::ChatTime::new(1, 2));
        record.0[57] = b'9';
        assert!(read(&mut record).is_err());
        record.0[71] = 15; // System message with no clock.
        assert_eq!(read(&mut record)?.message.game_time, None);
        record.0[55] = 0xff;
        record.0[48..52].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(
            read(&mut record),
            Err(ReadError::LimitExceeded { .. })
        ));
        Ok(())
    }
}
