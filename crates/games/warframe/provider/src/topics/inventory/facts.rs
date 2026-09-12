//! Layout and executable references for `warframe.inventory`.

use provider_sdk::memory::{ObjectOffset, Rva};
use warframe_model::InventoryFamily;

use crate::scalar::AddressEncodedScalarFacts;

#[derive(Clone, Debug, ..Eq)]
pub(crate) struct InventoryFacts {
    /// Instructions checked for returning profile-data address + `offset`, without dereferencing.
    pub(crate) getter: Rva,
    /// Embedded inventory's byte offset within profile data, not a pointer field.
    pub(crate) offset: ObjectOffset,
    /// One byte relative to profile data; nonzero means rebuilding, so reject the sample.
    pub(crate) force_update: ObjectOffset,
    /// 24 bytes relative to profile data, compared before/after reading all families.
    /// A change rejects the sample; we do not interpret the individual tokens.
    pub(crate) sync_tokens: ObjectOffset,
    pub(crate) commit: InventoryCommitFacts,
    pub(crate) families: &'static [InventoryFamilyFacts],
}

pub(crate) const INVENTORY: InventoryFacts = InventoryFacts {
    getter: Rva::new(0x00f6_f880),
    offset: ObjectOffset::new(0xd5d0),
    force_update: ObjectOffset::new(0x0001_1c60),
    sync_tokens: ObjectOffset::new(0xfdc0),
    commit: InventoryCommitFacts {
        constructor_vtable: Rva::new(0x0091_3ac4),
        force_update_init: Rva::new(0x0091_4a70),
        force_update_read: Rva::new(0x01b6_90c9),
        sync_tokens_init: Rva::new(0x0091_458e),
        sync_tokens_compare: Rva::new(0x0192_2dbf),
        compare_bytes: Rva::new(0x01ff_8f20),
    },
    families: FAMILIES,
};

/// Code references tying the coherence fields to profile data. The constructor
/// installs its vtable and initializes the byte plus two adjacent 12-byte fields;
/// the readers establish the byte access and the fields' comparison width.
#[derive(Debug, ..Copy, ..Eq)]
pub(crate) struct InventoryCommitFacts {
    pub(crate) constructor_vtable: Rva,
    pub(crate) force_update_init: Rva,
    pub(crate) force_update_read: Rva,
    pub(crate) sync_tokens_init: Rva,
    pub(crate) sync_tokens_compare: Rva,
    pub(crate) compare_bytes: Rva,
}

#[derive(Debug, ..Copy, ..Eq)]
pub(crate) struct InventoryRecordFacts {
    /// Distance between adjacent records in a family's vector payload.
    pub(crate) bytes: u32,
    /// Record-relative offset of the pointer to an item type descriptor.
    pub(crate) item_type: ObjectOffset,
    /// Expected target of the family's game-side serialization call; binds the
    /// family's registration to this record layout. Not our JSON serializer.
    pub(crate) serializer: Rva,
    pub(crate) count: InventoryRecordCountFacts,
}

/// Instruction RVAs below support the record layout: `stride_reader` checks
/// record spacing, `item_type_reader` checks access to the item-type field,
/// and `count_reader` checks the plain-count field offset.
#[derive(Debug, ..Copy, ..Eq)]
pub(crate) enum InventoryRecordCountFacts {
    /// One vector record is one owned instance.
    Single {
        stride_reader: Rva,
        item_type_reader: Rva,
    },
    /// One vector record is a protected stack count.
    AddressEncoded {
        codec: AddressEncodedScalarFacts,
        stride_reader: Rva,
        /// Instruction window checked for the stored-word offset, shifts and XOR constants.
        codec_reader: Rva,
    },
    /// One vector record carries a plain unsigned stack count.
    Plain {
        /// Record-relative offset of an unsigned 32-bit quantity.
        count: ObjectOffset,
        stride_reader: Rva,
        item_type_reader: Rva,
        count_reader: Rva,
    },
}

const EQUIPMENT: InventoryRecordFacts = InventoryRecordFacts {
    bytes: 304,
    item_type: ObjectOffset::new(0x8),
    serializer: Rva::new(0x01c3_95b0),
    count: InventoryRecordCountFacts::Single {
        stride_reader: Rva::new(0x01c3_9616),
        item_type_reader: Rva::new(0x0081_eadb),
    },
};

const SKINS: InventoryRecordFacts = InventoryRecordFacts {
    bytes: 80,
    item_type: ObjectOffset::new(0x0),
    serializer: Rva::new(0x010a_c390),
    count: InventoryRecordCountFacts::Plain {
        count: ObjectOffset::new(0x38),
        stride_reader: Rva::new(0x010a_c498),
        item_type_reader: Rva::new(0x0027_db7f),
        count_reader: Rva::new(0x0027_dc57),
    },
};

const UPGRADES: InventoryRecordFacts = InventoryRecordFacts {
    bytes: 80,
    item_type: ObjectOffset::new(0x0),
    serializer: Rva::new(0x01c3_9120),
    count: InventoryRecordCountFacts::Plain {
        count: ObjectOffset::new(0x38),
        stride_reader: Rva::new(0x01c3_928f),
        item_type_reader: Rva::new(0x0027_db7f),
        count_reader: Rva::new(0x0027_dc57),
    },
};

const CONSUMABLES: InventoryRecordFacts = InventoryRecordFacts {
    bytes: 16,
    item_type: ObjectOffset::new(0x0),
    serializer: Rva::new(0x01c3_8a10),
    count: InventoryRecordCountFacts::Plain {
        count: ObjectOffset::new(0x8),
        stride_reader: Rva::new(0x01c3_8add),
        item_type_reader: Rva::new(0x009e_d28b),
        count_reader: Rva::new(0x009e_d2a9),
    },
};

const BEASTS: InventoryRecordFacts = InventoryRecordFacts {
    bytes: 528,
    item_type: ObjectOffset::new(0x8),
    serializer: Rva::new(0x0137_8990),
    count: InventoryRecordCountFacts::Single {
        stride_reader: Rva::new(0x0137_8a73),
        item_type_reader: Rva::new(0x0081_eadb),
    },
};

const VESSELS: InventoryRecordFacts = InventoryRecordFacts {
    bytes: 1248,
    item_type: ObjectOffset::new(0x8),
    serializer: Rva::new(0x01c3_9300),
    count: InventoryRecordCountFacts::Single {
        stride_reader: Rva::new(0x01c3_93f7),
        item_type_reader: Rva::new(0x0081_eadb),
    },
};

const WRECKAGE: InventoryRecordFacts = InventoryRecordFacts {
    bytes: 304,
    item_type: ObjectOffset::new(0x8),
    serializer: Rva::new(0x0180_55a0),
    count: InventoryRecordCountFacts::Single {
        stride_reader: Rva::new(0x0180_5683),
        item_type_reader: Rva::new(0x0081_eadb),
    },
};

const STACKS: InventoryRecordFacts = InventoryRecordFacts {
    bytes: 16,
    item_type: ObjectOffset::new(0x0),
    serializer: Rva::new(0x01c3_8c20),
    count: InventoryRecordCountFacts::AddressEncoded {
        codec: AddressEncodedScalarFacts {
            check: ObjectOffset::new(0x8),
            stored: ObjectOffset::new(0xc),
            rotate_left: 0x13,
            address_shift: 0x3,
            value_xor: 0xc551_98a3,
            check_xor: 0xad84_b2ea,
        },
        stride_reader: Rva::new(0x01c3_8c4c),
        codec_reader: Rva::new(0x01c3_8cc1),
    },
};

#[derive(Debug, ..Copy, ..Eq)]
pub(crate) enum InventoryRegistrationFacts {
    /// A serialized field name and its serializer call identify the family.
    Named {
        /// Game serialization name, which may differ from our enum (e.g. "Suits").
        field: &'static str,
        /// Instructions checked for referring to `name` and locating the serializer call.
        name_reference: Rva,
        /// NUL-terminated field-name bytes in the executable.
        name: Rva,
    },
    /// The vector selection is followed by a direct call to its serializer.
    Direct { serializer_call: Rva },
}

#[derive(Debug, ..Copy, ..Eq)]
pub(crate) struct InventoryFamilyFacts {
    pub(crate) family: InventoryFamily,
    /// Offset within the embedded inventory to a 16-byte vector header:
    /// payload pointer, u32 byte length, u32 byte capacity.
    pub(crate) vector: ObjectOffset,
    /// Instruction that selects this vector from the inventory object.
    pub(crate) vector_reference: Rva,
    pub(crate) registration: InventoryRegistrationFacts,
    pub(crate) record: InventoryRecordFacts,
}

// Both helpers describe game code we inspect.
// `named` checks the game's field name as well as the vector offset and serializer target.
// `direct` checks the vector offset and a nearby serializer call, without field-name evidence.
#[rustfmt::skip]
const FAMILIES: &[InventoryFamilyFacts] = &[
    named(InventoryFamily::LongGuns, "LongGuns", 0x0, 0x0103_2605, 0x0103_2613, 0x0238_0500, EQUIPMENT),
    named(InventoryFamily::Pistols, "Pistols", 0x10, 0x0103_2622, 0x0103_262f, 0x0238_0520, EQUIPMENT),
    named(InventoryFamily::Warframes, "Suits", 0x20, 0x0103_263e, 0x0103_264b, 0x0227_0cac, EQUIPMENT),
    named(InventoryFamily::Melee, "Melee", 0x30, 0x0103_265a, 0x0103_2667, 0x0227_0ca4, EQUIPMENT),
    direct(InventoryFamily::WeaponSkins, 0x50, 0x0103_2b63, 0x0103_2b6a, SKINS),
    named(InventoryFamily::RawUpgrades, "RawUpgrades", 0x60, 0x0103_29c7, 0x0103_29d4, 0x0242_3200, UPGRADES),
    direct(InventoryFamily::Consumables, 0xb0, 0x0103_29e3, 0x0103_29fd, CONSUMABLES),
    named(InventoryFamily::MiscItems, "MiscItems", 0xd0, 0x0103_2a02, 0x0103_2a12, 0x021c_fc40, STACKS),
    named(InventoryFamily::Sentinels, "Sentinels", 0x100, 0x0103_2676, 0x0103_2686, 0x0238_0528, EQUIPMENT),
    named(InventoryFamily::SentinelWeapons, "SentinelWeapons", 0x110, 0x0103_2695, 0x0103_26a5, 0x0238_0538, EQUIPMENT),
    named(InventoryFamily::KubrowPets, "KubrowPets", 0x178, 0x0103_27a8, 0x0103_2786, 0x0238_0548, BEASTS),
    named(InventoryFamily::SpaceSuits, "SpaceSuits", 0x198, 0x0103_26b4, 0x0103_26c4, 0x0238_0558, EQUIPMENT),
    named(InventoryFamily::SpaceGuns, "SpaceGuns", 0x1a8, 0x0103_26d3, 0x0103_26e3, 0x0238_0568, EQUIPMENT),
    named(InventoryFamily::SpaceMelee, "SpaceMelee", 0x1b8, 0x0103_26f2, 0x0103_2702, 0x0238_0578, EQUIPMENT),
    named(InventoryFamily::Scoops, "Scoops", 0x208, 0x0103_2711, 0x0103_2721, 0x0238_0584, EQUIPMENT),
    named(InventoryFamily::FusionBundles, "FusionBundles", 0x228, 0x0103_2adb, 0x0103_2aeb, 0x0242_3258, STACKS),
    named(InventoryFamily::ShipDecorations, "ShipDecorations", 0x1d8, 0x0103_2a9d, 0x0103_2aad, 0x0238_4aa0, STACKS),
    named(InventoryFamily::EmailItems, "EmailItems", 0x1e8, 0x0103_2a7e, 0x0103_2a8e, 0x0242_3238, STACKS),
    named(InventoryFamily::FoundToday, "FoundToday", 0x1c8, 0x0103_2abc, 0x0103_2acc, 0x0242_3248, STACKS),
    named(InventoryFamily::LevelKeys, "LevelKeys", 0x120, 0x0103_2a5f, 0x0103_2a6f, 0x0242_3228, STACKS),
    named(InventoryFamily::Recipes, "Recipes", 0xe0, 0x0103_2a40, 0x0103_2a50, 0x0242_3220, STACKS),
    named(InventoryFamily::OperatorAmps, "OperatorAmps", 0x258, 0x0103_2730, 0x0103_2740, 0x0238_0510, EQUIPMENT),
    named(InventoryFamily::SpecialItems, "SpecialItems", 0x268, 0x0103_274f, 0x0103_275f, 0x0238_0590, EQUIPMENT),
    named(InventoryFamily::MoaPets, "MoaPets", 0x278, 0x0103_27b7, 0x0103_27c7, 0x0238_05a0, EQUIPMENT),
    named(InventoryFamily::Hoverboards, "Hoverboards", 0x288, 0x0103_28ce, 0x0103_28de, 0x0238_05a8, EQUIPMENT),
    named(InventoryFamily::CrewShips, "CrewShips", 0x298, 0x0103_28ed, 0x0103_28fd, 0x0238_0690, VESSELS),
    named(InventoryFamily::CrewShipWeapons, "CrewShipWeapons", 0x2f8, 0x0103_290c, 0x0103_291c, 0x0242_31d8, EQUIPMENT),
    named(InventoryFamily::CrewShipSalvagedWeapons, "CrewShipSalvagedWeapons", 0x308, 0x0103_295b, 0x0103_2939, 0x0242_31e8, WRECKAGE),
    named(InventoryFamily::DrifterGuns, "DrifterGuns", 0x2a8, 0x0103_27f5, 0x0103_2805, 0x0242_31c0, EQUIPMENT),
    named(InventoryFamily::DrifterMelee, "DrifterMelee", 0x2b8, 0x0103_2814, 0x0103_2824, 0x0238_05f8, EQUIPMENT),
    named(InventoryFamily::Gadgets, "Gadgets", 0x2c8, 0x0103_2833, 0x0103_2843, 0x0238_0608, EQUIPMENT),
    named(InventoryFamily::Horses, "Horses", 0x2d8, 0x0103_27d6, 0x0103_27e6, 0x0238_05ec, EQUIPMENT),
    named(InventoryFamily::CrewShipRawSalvage, "CrewShipRawSalvage", 0x358, 0x0103_2b19, 0x0103_2b29, 0x0242_3268, STACKS),
    named(InventoryFamily::DataKnives, "DataKnives", 0x368, 0x0103_296a, 0x0103_297a, 0x0238_05b8, EQUIPMENT),
    named(InventoryFamily::MechSuits, "MechSuits", 0x378, 0x0103_2989, 0x0103_2999, 0x0238_05c8, EQUIPMENT),
    named(InventoryFamily::CrewShipHarnesses, "CrewShipHarnesses", 0x388, 0x0103_29a8, 0x0103_29b8, 0x0238_05d8, EQUIPMENT),
    named(InventoryFamily::Motorcycles, "Motorcycles", 0x398, 0x0103_2852, 0x0103_2862, 0x0238_0610, EQUIPMENT),
    named(InventoryFamily::OperatorSuits, "OperatorSuits", 0x3a8, 0x0103_2871, 0x0103_2881, 0x0238_0620, EQUIPMENT),
    named(InventoryFamily::Antiques, "Antiques", 0x3b8, 0x0103_2890, 0x0103_28a0, 0x0238_0630, EQUIPMENT),
    named(InventoryFamily::BonusMiscItems, "BonusMiscItems", 0xae0, 0x0103_2a21, 0x0103_2a31, 0x0242_3210, STACKS),
];

const fn named(
    family: InventoryFamily,
    field: &'static str,
    vector: u32,
    vector_reference: u32,
    name_reference: u32,
    name: u32,
    record: InventoryRecordFacts,
) -> InventoryFamilyFacts {
    InventoryFamilyFacts {
        family,
        vector: ObjectOffset::new(vector),
        vector_reference: Rva::new(vector_reference),
        registration: InventoryRegistrationFacts::Named {
            field,
            name_reference: Rva::new(name_reference),
            name: Rva::new(name),
        },
        record,
    }
}

const fn direct(
    family: InventoryFamily,
    vector: u32,
    vector_reference: u32,
    serializer_call: u32,
    record: InventoryRecordFacts,
) -> InventoryFamilyFacts {
    InventoryFamilyFacts {
        family,
        vector: ObjectOffset::new(vector),
        vector_reference: Rva::new(vector_reference),
        registration: InventoryRegistrationFacts::Direct {
            serializer_call: Rva::new(serializer_call),
        },
        record,
    }
}
