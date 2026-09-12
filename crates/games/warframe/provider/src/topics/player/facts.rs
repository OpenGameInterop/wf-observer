use provider_sdk::memory::{ObjectOffset, Rva, VtableSlot};

/// The active profile's name getter chooses a preferred name, falling back when empty.
pub(crate) struct PlayerFacts {
    pub(crate) getter: Rva,
    pub(crate) slot: VtableSlot,
    /// Native string fields in the same active profile that owns the account ID.
    pub(crate) fallback: ObjectOffset,
    pub(crate) preferred: ObjectOffset,
}

pub(crate) const PLAYER: PlayerFacts = PlayerFacts {
    getter: Rva::new(0x008b_9520),
    slot: VtableSlot::new(8),
    fallback: ObjectOffset::new(0x40),
    preferred: ObjectOffset::new(0x50),
};
