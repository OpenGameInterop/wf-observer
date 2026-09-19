//! Mastery evidence for executable 6a85c6b0-02cef000.
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
        ObjectOffset::new(0xe300),
        ObjectOffset::new(0xe308),
        ObjectOffset::new(0xe310),
    ],
    non_item_xp_codec: AddressEncodedScalarFacts {
        check: ObjectOffset::new(0),
        stored: ObjectOffset::new(4),
        rotate_left: 30,
        address_shift: 3,
        value_xor: 0x635b_f253,
        check_xor: 0xe19c_9bbd,
    },
    vector: ObjectOffset::new(0xf0),
    record_bytes: 16,
    item_type: ObjectOffset::new(0),
    xp: AddressEncodedScalarFacts {
        check: ObjectOffset::new(8),
        stored: ObjectOffset::new(12),
        rotate_left: 30,
        address_shift: 3,
        value_xor: 0x635b_f253,
        check_xor: 0xe19c_9bbd,
    },
    item_xp_dirty: ObjectOffset::new(0xd20),
    item_xp: ObjectOffset::new(0xd24),
    rank: ObjectOffset::new(0x3fc),
    rank_codec: AddressEncodedU16Facts {
        check: ObjectOffset::new(0),
        stored: ObjectOffset::new(2),
        rotate_left: 5,
        address_shift: 3,
        value_xor: 0x1575,
        check_xor: 0xb80e,
    },
    calculation_name_reference: Rva::new(0x011f_b165),
    calculation_name: Rva::new(0x0242_3170),
    calculation: Rva::new(0x011f_b100),
    calculation_reader: Rva::new(0x011f_b12c),
    non_item_xp_reader: Rva::new(0x002f_8fb0),
    rank_reader: Rva::new(0x011f_b29c),
    serializer: Rva::new(0x01c1_cfff),
};
