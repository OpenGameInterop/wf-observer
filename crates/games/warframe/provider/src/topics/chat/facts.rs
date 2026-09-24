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
    pub(crate) public_prefix_reference: Rva,
    pub(crate) private_key_reference: Rva,
    pub(crate) append_separator: Rva,
    pub(crate) append_recipient: Rva,
    pub(crate) channel_entries: ObjectOffset,
    pub(crate) channel_entries_reference: Rva,
    /// Entry nodes start with next/previous pointers and a pool-owner pointer.
    pub(crate) sender: ObjectOffset,
    pub(crate) text: ObjectOffset,
    pub(crate) timestamp: ObjectOffset,
    /// IRC user role (normal/guide/moderator/operator), not message direction.
    pub(crate) role: ObjectOffset,
    pub(crate) entry_fields: Rva,
    pub(crate) string_copy: Rva,
    pub(crate) retention_check: Rva,
}

pub(crate) const CHAT: ChatFacts = ChatFacts {
    handler: Rva::new(0x00f0_f100),
    handler_slot: VtableSlot::new(0x248),
    insert_call: Rva::new(0x00f0_f62c),
    insert: Rva::new(0x00ba_a300),
    channels: ObjectOffset::new(0x11ea8),
    channels_reference: Rva::new(0x00ba_a4eb),
    channel_name: ObjectOffset::new(0x18),
    channel_name_reference: Rva::new(0x00ba_a500),
    // Insert preserves '#' channels; private keys join sender, ',' and recipient.
    public_prefix_reference: Rva::new(0x00ba_a3b8),
    private_key_reference: Rva::new(0x00ba_a98d),
    append_separator: Rva::new(0x0015_a550),
    append_recipient: Rva::new(0x0174_3080),
    channel_entries: ObjectOffset::new(0x28),
    channel_entries_reference: Rva::new(0x00ba_b41a),
    sender: ObjectOffset::new(0x18),
    text: ObjectOffset::new(0x28),
    timestamp: ObjectOffset::new(0x38),
    role: ObjectOffset::new(0x48),
    entry_fields: Rva::new(0x00ba_b440),
    string_copy: Rva::new(0x0133_0fc0),
    retention_check: Rva::new(0x00ba_b4ce),
};

pub(super) const MAX_CHANNELS: usize = 32;
pub(super) const MAX_ENTRIES: usize = 100;
pub(super) const MAX_TOTAL_ENTRIES: usize = 1000;
pub(super) const MAX_STRING_BYTES: usize = 512 * 1024;
pub(super) const MAX_MESSAGE_BYTES: usize = 4096;
/// Messages emitted per chat poll.
pub(super) const MAX_EVENTS: usize = 32;
