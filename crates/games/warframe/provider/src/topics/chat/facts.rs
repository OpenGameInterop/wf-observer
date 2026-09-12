use provider_sdk::memory::{ObjectOffset, Rva, VtableSlot};

/// Profile-owned channel histories and the instructions that populate them.
pub(crate) struct ChatFacts {
    pub(crate) handler: Rva,
    pub(crate) handler_slot: VtableSlot,
    pub(crate) insert_call: Rva,
    pub(crate) insert: Rva,
    /// The channel-list sentinel is embedded in profile data.
    pub(crate) channels: ObjectOffset,
    pub(crate) channels_reference: Rva,
    pub(crate) channel_name: ObjectOffset,
    pub(crate) channel_name_reference: Rva,
    pub(crate) channel_entries: ObjectOffset,
    pub(crate) channel_entries_reference: Rva,
    /// Entry nodes start with next/previous pointers and a pool-owner pointer.
    pub(crate) sender: ObjectOffset,
    pub(crate) text: ObjectOffset,
    pub(crate) timestamp: ObjectOffset,
    pub(crate) flags: ObjectOffset,
    pub(crate) entry_fields: Rva,
    pub(crate) string_copy: Rva,
    pub(crate) retention_check: Rva,
}

pub(crate) const CHAT: ChatFacts = ChatFacts {
    handler: Rva::new(0x00c2_3eb0),
    handler_slot: VtableSlot::new(0x248),
    insert_call: Rva::new(0x00c2_4426),
    insert: Rva::new(0x00c1_3dd0),
    channels: ObjectOffset::new(0x11b10),
    channels_reference: Rva::new(0x00c1_3ffd),
    channel_name: ObjectOffset::new(0x18),
    channel_name_reference: Rva::new(0x00c1_4020),
    channel_entries: ObjectOffset::new(0x28),
    channel_entries_reference: Rva::new(0x00c1_5112),
    sender: ObjectOffset::new(0x18),
    text: ObjectOffset::new(0x28),
    timestamp: ObjectOffset::new(0x38),
    flags: ObjectOffset::new(0x48),
    entry_fields: Rva::new(0x00c1_5142),
    string_copy: Rva::new(0x004b_9060),
    retention_check: Rva::new(0x00c1_51da),
};

pub(super) const MAX_CHANNELS: usize = 32;
pub(super) const MAX_ENTRIES: usize = 100;
pub(super) const MAX_TOTAL_ENTRIES: usize = 1000;
pub(super) const MAX_STRING_BYTES: usize = 512 * 1024;
pub(super) const MAX_MESSAGE_BYTES: usize = 4096;
/// Messages emitted per chat poll.
pub(super) const MAX_EVENTS: usize = 32;
