//! Executable 6ab2e96d-02cd8000: mPlayerSkills serializer and rank update path.
use provider_sdk::memory::{ObjectOffset, Rva};

/// Two u32 skill-point balances followed by ten u32 skill ranks. Rank index zero
/// is `LPS_NONE`; indices 1..=5 are Railjack, 6..=9 are Drifter.
pub(super) const BLOCK: ObjectOffset = ObjectOffset::new(0x0001_707c);
pub(super) const BLOCK_BYTES: usize = 48;
pub(super) const RANKS: ObjectOffset = ObjectOffset::new(0x0001_7084);
pub(super) const POINT_SCALE: u32 = 1000;
pub(super) const SERIALIZER_FIELD: Rva = Rva::new(0x003d_69c4);
pub(super) const SERIALIZER: Rva = Rva::new(0x0160_d5b0);
pub(super) const RANK_UPDATE: Rva = Rva::new(0x00ac_5ecd);
pub(super) const POOL_UPDATE: Rva = Rva::new(0x00ac_5f4d);
pub(super) const GROUP_CLASSIFIER: Rva = Rva::new(0x00fd_2ac0);
pub(super) const GROUP_TABLE: Rva = Rva::new(0x00fd_2af4);

/// Serialized enum order, independently named by the executable registration.
pub(super) const SKILL_NAMES: &[(u32, &str)] = &[
    (0x0224_0a28, "LPS_NONE"),
    (0x0224_0a38, "LPS_PILOTING"),
    (0x0224_0a48, "LPS_GUNNERY"),
    (0x0224_0a70, "LPS_TACTICAL"),
    (0x0224_0a98, "LPS_ENGINEERING"),
    (0x0224_0aa8, "LPS_COMMAND"),
    (0x0224_0ab8, "LPS_DRIFT_COMBAT"),
    (0x0224_0af0, "LPS_DRIFT_RIDING"),
    (0x0224_0b28, "LPS_DRIFT_OPPORTUNITY"),
    (0x0224_0b40, "LPS_DRIFT_ENDURANCE"),
];
