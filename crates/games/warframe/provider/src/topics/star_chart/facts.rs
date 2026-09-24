//! Executable 6ab2e96d-02cd8000: profile Missions serializer, completion updater,
//! and the mission-based mastery calculator agree on this record layout.
use provider_sdk::memory::{ObjectOffset, Rva};

pub(super) const VECTOR: ObjectOffset = ObjectOffset::new(0xff48);
pub(super) const RECORD_BYTES: u32 = 0x30;
pub(super) const TAG: ObjectOffset = ObjectOffset::new(0);
pub(super) const COMPLETIONS: ObjectOffset = ObjectOffset::new(4);
pub(super) const TIER: ObjectOffset = ObjectOffset::new(8);
pub(super) const GETTER: Rva = Rva::new(0x006f_ace0);
pub(super) const SERIALIZER_REFERENCE: Rva = Rva::new(0x003d_3777);
pub(super) const MISSIONS_NAME: Rva = Rva::new(0x0227_11b0);
