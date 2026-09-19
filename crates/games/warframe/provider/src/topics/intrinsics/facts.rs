//! Executable 6a85c6b0-02cef000: mPlayerSkills serializer and rank update path.
use provider_sdk::memory::{ObjectOffset, Rva};

/// Two u32 skill-point balances followed by ten u32 skill ranks. Rank index zero
/// is `LPS_NONE`; indices 1..=5 are Railjack, 6..=9 are Drifter.
pub(super) const BLOCK: ObjectOffset = ObjectOffset::new(0x0001_66b4);
pub(super) const BLOCK_BYTES: usize = 48;
pub(super) const RANKS: ObjectOffset = ObjectOffset::new(0x0001_66bc);
pub(super) const POINT_SCALE: u32 = 1000;
pub(super) const SERIALIZER_FIELD: Rva = Rva::new(0x00af_43de);
pub(super) const SERIALIZER: Rva = Rva::new(0x0164_bfb0);
pub(super) const RANK_UPDATE: Rva = Rva::new(0x00e5_811d);
pub(super) const POOL_UPDATE: Rva = Rva::new(0x00e5_819d);
pub(super) const GROUP_CLASSIFIER: Rva = Rva::new(0x010c_a510);
pub(super) const GROUP_TABLE: Rva = Rva::new(0x010c_a544);

/// Serialized enum order, independently named by the executable registration.
pub(super) const SKILL_NAMES: &[(u32, &str)] = &[
    (0x0226_9ac8, "LPS_NONE"),
    (0x0226_9ad8, "LPS_PILOTING"),
    (0x0226_9b08, "LPS_GUNNERY"),
    (0x0226_9b38, "LPS_TACTICAL"),
    (0x0226_9b48, "LPS_ENGINEERING"),
    (0x0226_9b58, "LPS_COMMAND"),
    (0x0226_9b68, "LPS_DRIFT_COMBAT"),
    (0x0226_9b80, "LPS_DRIFT_RIDING"),
    (0x0226_9b98, "LPS_DRIFT_OPPORTUNITY"),
    (0x0226_9bb0, "LPS_DRIFT_ENDURANCE"),
];
