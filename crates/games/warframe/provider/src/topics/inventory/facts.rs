//! Layout and executable references for `warframe.inventory`.

use provider_sdk::memory::{ObjectOffset, Rva};
use warframe_model::InventoryFamily;

use crate::scalar::AddressEncodedScalarFacts;

#[derive(Clone, Debug, ..Eq)]
pub(crate) struct InventoryFacts {
    pub(crate) families: &'static [InventoryFamilyFacts],
}

pub(crate) const INVENTORY: InventoryFacts = InventoryFacts { families: FAMILIES };

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
    serializer: Rva::new(0x01bf_1c80),
    count: InventoryRecordCountFacts::Single {
        stride_reader: Rva::new(0x01bf_1ce6),
        item_type_reader: Rva::new(0x0051_6d09),
    },
};

const SKINS: InventoryRecordFacts = InventoryRecordFacts {
    bytes: 80,
    item_type: ObjectOffset::new(0x0),
    serializer: Rva::new(0x0120_de20),
    count: InventoryRecordCountFacts::Plain {
        count: ObjectOffset::new(0x38),
        stride_reader: Rva::new(0x0120_df2a),
        item_type_reader: Rva::new(0x004f_925f),
        count_reader: Rva::new(0x004f_9337),
    },
};

const UPGRADES: InventoryRecordFacts = InventoryRecordFacts {
    bytes: 80,
    item_type: ObjectOffset::new(0x0),
    serializer: Rva::new(0x01bf_17f0),
    count: InventoryRecordCountFacts::Plain {
        count: ObjectOffset::new(0x38),
        stride_reader: Rva::new(0x01bf_195f),
        item_type_reader: Rva::new(0x004f_925f),
        count_reader: Rva::new(0x004f_9337),
    },
};

const CONSUMABLES: InventoryRecordFacts = InventoryRecordFacts {
    bytes: 16,
    item_type: ObjectOffset::new(0x0),
    serializer: Rva::new(0x01bf_10d0),
    count: InventoryRecordCountFacts::Plain {
        count: ObjectOffset::new(0x8),
        stride_reader: Rva::new(0x01bf_119d),
        item_type_reader: Rva::new(0x006e_477b),
        count_reader: Rva::new(0x006e_4799),
    },
};

const BEASTS: InventoryRecordFacts = InventoryRecordFacts {
    bytes: 528,
    item_type: ObjectOffset::new(0x8),
    serializer: Rva::new(0x0168_1ef0),
    count: InventoryRecordCountFacts::Single {
        stride_reader: Rva::new(0x0168_1fd4),
        item_type_reader: Rva::new(0x0051_6d09),
    },
};

const VESSELS: InventoryRecordFacts = InventoryRecordFacts {
    bytes: 1248,
    item_type: ObjectOffset::new(0x8),
    serializer: Rva::new(0x01bf_19d0),
    count: InventoryRecordCountFacts::Single {
        stride_reader: Rva::new(0x01bf_1ac7),
        item_type_reader: Rva::new(0x0051_6d09),
    },
};

const WRECKAGE: InventoryRecordFacts = InventoryRecordFacts {
    bytes: 304,
    item_type: ObjectOffset::new(0x8),
    serializer: Rva::new(0x013e_cfc0),
    count: InventoryRecordCountFacts::Single {
        stride_reader: Rva::new(0x013e_d0a4),
        item_type_reader: Rva::new(0x0051_6d09),
    },
};

const STACKS: InventoryRecordFacts = InventoryRecordFacts {
    bytes: 16,
    item_type: ObjectOffset::new(0x0),
    serializer: Rva::new(0x01bf_12e0),
    count: InventoryRecordCountFacts::AddressEncoded {
        codec: AddressEncodedScalarFacts {
            check: ObjectOffset::new(0x8),
            stored: ObjectOffset::new(0xc),
            rotate_left: 0x13,
            address_shift: 0x3,
            value_xor: 0xac7e_8740,
            check_xor: 0x9c08_4a47,
        },
        stride_reader: Rva::new(0x01bf_130c),
        codec_reader: Rva::new(0x01bf_1381),
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
    named(InventoryFamily::LongGuns, "LongGuns", 0x0, 0x013f_6515, 0x013f_6523, 0x0234_e890, EQUIPMENT),
    named(InventoryFamily::Pistols, "Pistols", 0x10, 0x013f_6532, 0x013f_653f, 0x0234_e8b0, EQUIPMENT),
    named(InventoryFamily::Warframes, "Suits", 0x20, 0x013f_654e, 0x013f_655b, 0x0224_82b4, EQUIPMENT),
    named(InventoryFamily::Melee, "Melee", 0x30, 0x013f_656a, 0x013f_6577, 0x0224_82ac, EQUIPMENT),
    direct(InventoryFamily::WeaponSkins, 0x50, 0x013f_6aad, 0x013f_6ab7, SKINS),
    named(InventoryFamily::RawUpgrades, "RawUpgrades", 0x60, 0x013f_68d7, 0x013f_68e4, 0x023f_1ca0, UPGRADES),
    direct(InventoryFamily::Consumables, 0xb0, 0x013f_68f3, 0x013f_690d, CONSUMABLES),
    named(InventoryFamily::MiscItems, "MiscItems", 0xd0, 0x013f_6912, 0x013f_6922, 0x0219_cb40, STACKS),
    named(InventoryFamily::Sentinels, "Sentinels", 0x100, 0x013f_6586, 0x013f_6596, 0x0234_e8b8, EQUIPMENT),
    named(InventoryFamily::SentinelWeapons, "SentinelWeapons", 0x110, 0x013f_65a5, 0x013f_65b5, 0x0234_e8c8, EQUIPMENT),
    named(InventoryFamily::KubrowPets, "KubrowPets", 0x178, 0x013f_66b8, 0x013f_6696, 0x0234_e8d8, BEASTS),
    named(InventoryFamily::SpaceSuits, "SpaceSuits", 0x198, 0x013f_65c4, 0x013f_65d4, 0x0234_e8e8, EQUIPMENT),
    named(InventoryFamily::SpaceGuns, "SpaceGuns", 0x1a8, 0x013f_65e3, 0x013f_65f3, 0x0234_e8f8, EQUIPMENT),
    named(InventoryFamily::SpaceMelee, "SpaceMelee", 0x1b8, 0x013f_6602, 0x013f_6612, 0x0234_e908, EQUIPMENT),
    named(InventoryFamily::Scoops, "Scoops", 0x208, 0x013f_6621, 0x013f_6631, 0x0234_e914, EQUIPMENT),
    named(InventoryFamily::FusionBundles, "FusionBundles", 0x228, 0x013f_69eb, 0x013f_69fb, 0x023f_1cf8, STACKS),
    named(InventoryFamily::ShipDecorations, "ShipDecorations", 0x1d8, 0x013f_69ad, 0x013f_69bd, 0x0235_2ea0, STACKS),
    named(InventoryFamily::EmailItems, "EmailItems", 0x1e8, 0x013f_698e, 0x013f_699e, 0x023f_1cd8, STACKS),
    named(InventoryFamily::FoundToday, "FoundToday", 0x1c8, 0x013f_69cc, 0x013f_69dc, 0x023f_1ce8, STACKS),
    named(InventoryFamily::LevelKeys, "LevelKeys", 0x120, 0x013f_696f, 0x013f_697f, 0x023f_1cc8, STACKS),
    named(InventoryFamily::Recipes, "Recipes", 0xe0, 0x013f_6950, 0x013f_6960, 0x023f_1cc0, STACKS),
    named(InventoryFamily::OperatorAmps, "OperatorAmps", 0x258, 0x013f_6640, 0x013f_6650, 0x0234_e8a0, EQUIPMENT),
    named(InventoryFamily::SpecialItems, "SpecialItems", 0x268, 0x013f_665f, 0x013f_666f, 0x0234_e920, EQUIPMENT),
    named(InventoryFamily::MoaPets, "MoaPets", 0x278, 0x013f_66c7, 0x013f_66d7, 0x0234_e930, EQUIPMENT),
    named(InventoryFamily::Hoverboards, "Hoverboards", 0x288, 0x013f_67de, 0x013f_67ee, 0x0234_e938, EQUIPMENT),
    named(InventoryFamily::CrewShips, "CrewShips", 0x298, 0x013f_67fd, 0x013f_680d, 0x0234_ea20, VESSELS),
    named(InventoryFamily::CrewShipWeapons, "CrewShipWeapons", 0x2f8, 0x013f_681c, 0x013f_682c, 0x023f_1c78, EQUIPMENT),
    named(InventoryFamily::CrewShipSalvagedWeapons, "CrewShipSalvagedWeapons", 0x308, 0x013f_686b, 0x013f_6849, 0x023f_1c88, WRECKAGE),
    named(InventoryFamily::DrifterGuns, "DrifterGuns", 0x2a8, 0x013f_6705, 0x013f_6715, 0x023f_1c60, EQUIPMENT),
    named(InventoryFamily::DrifterMelee, "DrifterMelee", 0x2b8, 0x013f_6724, 0x013f_6734, 0x0234_e988, EQUIPMENT),
    named(InventoryFamily::Gadgets, "Gadgets", 0x2c8, 0x013f_6743, 0x013f_6753, 0x0234_e998, EQUIPMENT),
    named(InventoryFamily::Horses, "Horses", 0x2d8, 0x013f_66e6, 0x013f_66f6, 0x0234_e97c, EQUIPMENT),
    named(InventoryFamily::CrewShipRawSalvage, "CrewShipRawSalvage", 0x358, 0x013f_6a29, 0x013f_6a39, 0x023f_1d08, STACKS),
    named(InventoryFamily::DataKnives, "DataKnives", 0x368, 0x013f_687a, 0x013f_688a, 0x0234_e948, EQUIPMENT),
    named(InventoryFamily::MechSuits, "MechSuits", 0x378, 0x013f_6899, 0x013f_68a9, 0x0234_e958, EQUIPMENT),
    named(InventoryFamily::CrewShipHarnesses, "CrewShipHarnesses", 0x388, 0x013f_68b8, 0x013f_68c8, 0x0234_e968, EQUIPMENT),
    named(InventoryFamily::Motorcycles, "Motorcycles", 0x398, 0x013f_6762, 0x013f_6772, 0x0234_e9a0, EQUIPMENT),
    named(InventoryFamily::OperatorSuits, "OperatorSuits", 0x3a8, 0x013f_6781, 0x013f_6791, 0x0234_e9b0, EQUIPMENT),
    named(InventoryFamily::Antiques, "Antiques", 0x3b8, 0x013f_67a0, 0x013f_67b0, 0x0234_e9c0, EQUIPMENT),
    named(InventoryFamily::BonusMiscItems, "BonusMiscItems", 0xc68, 0x013f_6931, 0x013f_6941, 0x023f_1cb0, STACKS),
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
