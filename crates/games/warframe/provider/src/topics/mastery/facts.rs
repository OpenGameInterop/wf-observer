//! Mastery evidence for executable 6ab2e96d-02cd8000.
//!
//! Located from the calculation log string and its non-item-pool caller.
//! These codecs are specific to mastery; inventory quantities use another codec.
use crate::scalar::{AddressEncodedScalarFacts, AddressEncodedU16Facts};
use provider_sdk::memory::{ObjectOffset, Rva};

#[derive(Debug, ..Copy)]
pub(super) struct MasteryFacts {
    pub(super) non_item_xp: [ObjectOffset; 3],
    pub(super) non_item_xp_codec: AddressEncodedScalarFacts,
    pub(super) vector: ObjectOffset,
    pub(super) record_bytes: u32,
    pub(super) item_type: ObjectOffset,
    pub(super) xp: AddressEncodedScalarFacts,
    pub(super) item_xp_dirty: ObjectOffset,
    pub(super) item_xp: ObjectOffset,
    pub(super) rank: ObjectOffset,
    pub(super) rank_codec: AddressEncodedU16Facts,
    pub(super) calculation_name_reference: Rva,
    pub(super) calculation_name: Rva,
    pub(super) calculation: Rva,
    pub(super) calculation_reader: Rva,
    pub(super) non_item_xp_reader: Rva,
    pub(super) rank_reader: Rva,
    pub(super) serializer: Rva,
}

pub(super) const MASTERY: MasteryFacts = MasteryFacts {
    non_item_xp: [
        ObjectOffset::new(0xe4c0),
        ObjectOffset::new(0xe4c8),
        ObjectOffset::new(0xe4d0),
    ],
    non_item_xp_codec: AddressEncodedScalarFacts {
        check: ObjectOffset::new(0),
        stored: ObjectOffset::new(4),
        rotate_left: 17,
        address_shift: 3,
        value_xor: 0x8637_d1b6,
        check_xor: 0x2c67_b217,
    },
    vector: ObjectOffset::new(0xf0),
    record_bytes: 16,
    item_type: ObjectOffset::new(0),
    xp: AddressEncodedScalarFacts {
        check: ObjectOffset::new(8),
        stored: ObjectOffset::new(12),
        rotate_left: 17,
        address_shift: 3,
        value_xor: 0x8637_d1b6,
        check_xor: 0x2c67_b217,
    },
    item_xp_dirty: ObjectOffset::new(0xea8),
    item_xp: ObjectOffset::new(0xeac),
    rank: ObjectOffset::new(0x40c),
    rank_codec: AddressEncodedU16Facts {
        check: ObjectOffset::new(0),
        stored: ObjectOffset::new(2),
        rotate_left: 1,
        address_shift: 3,
        value_xor: 0x810b,
        check_xor: 0xe85e,
    },
    calculation_name_reference: Rva::new(0x007d_da95),
    calculation_name: Rva::new(0x023f_1c10),
    calculation: Rva::new(0x007d_da30),
    calculation_reader: Rva::new(0x007d_da5c),
    non_item_xp_reader: Rva::new(0x00a0_41d0),
    rank_reader: Rva::new(0x007d_dbcc),
    serializer: Rva::new(0x01bd_47bf),
};
