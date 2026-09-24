//! Profile-data fields and executable references for `warframe.currencies`.

use provider_sdk::memory::{ObjectOffset, Rva};

use crate::scalar::AddressEncodedScalarFacts;

pub(super) const CODEC: AddressEncodedScalarFacts = AddressEncodedScalarFacts {
    check: ObjectOffset::new(0),
    stored: ObjectOffset::new(4),
    rotate_left: 19,
    address_shift: 3,
    value_xor: 0xac7e_8740,
    check_xor: 0x9c08_4a47,
};

#[derive(..Copy)]
pub(super) struct BalanceFacts {
    /// Offset of the integrity/stored word pair within profile data.
    pub(super) field: ObjectOffset,
    /// Instructions checked for this field's offset and scalar encoding.
    pub(super) reader: Rva,
}

/// Endo's update path decodes and rewrites its protected field.
pub(super) const ENDO: BalanceFacts = BalanceFacts {
    field: ObjectOffset::new(0xd9e0),
    reader: Rva::new(0x0135_4146),
};

#[derive(..Copy)]
pub(super) struct EncodedNameFacts {
    pub(super) text: &'static str,
    /// RIP-relative instruction selecting the encoded name object.
    pub(super) reference: Rva,
    pub(super) object: Rva,
    /// Instruction loading the name's 64-bit XOR key.
    pub(super) key_reference: Rva,
    pub(super) key: u64,
}

#[derive(..Copy)]
pub(super) struct WalletFieldFacts {
    pub(super) balance: BalanceFacts,
    /// Game serialization name identifying the balance, not a string-pool token.
    pub(super) name: EncodedNameFacts,
}

/// Credits expands the codec inline in the wallet serializer.
pub(super) const CREDITS: WalletFieldFacts = WalletFieldFacts {
    balance: BalanceFacts {
        field: ObjectOffset::new(0xd9f0),
        reader: Rva::new(0x0070_1a22),
    },
    name: EncodedNameFacts {
        text: "RegularCredits",
        reference: Rva::new(0x0070_1a33),
        object: Rva::new(0x0285_7170),
        key_reference: Rva::new(0x0070_1a3d),
        key: 0x8bd7_61f9_775e_fe44,
    },
};

/// Platinum readers select the field, then call the shared serializer 12 bytes later.
pub(super) const TRADABLE_PLATINUM: WalletFieldFacts = WalletFieldFacts {
    balance: BalanceFacts {
        field: ObjectOffset::new(0xd9f8),
        reader: Rva::new(0x0070_1ad0),
    },
    name: EncodedNameFacts {
        text: "mInventory.mPremiumCredits",
        reference: Rva::new(0x0070_1ac4),
        object: Rva::new(0x0285_7190),
        key_reference: Rva::new(0x0070_1a7d),
        key: 0x3d35_20aa_8753_0034,
    },
};

pub(super) const NON_TRADABLE_PLATINUM: WalletFieldFacts = WalletFieldFacts {
    balance: BalanceFacts {
        field: ObjectOffset::new(0xda00),
        reader: Rva::new(0x0070_1afc),
    },
    name: EncodedNameFacts {
        text: "mInventory.mPremiumCreditsFree",
        reference: Rva::new(0x0070_1aeb),
        object: Rva::new(0x0285_71c0),
        key_reference: Rva::new(0x0070_1ae1),
        key: 0x15e4_0003_0f4d_012c,
    },
};

/// Both Platinum fields use this serializer's protected-scalar encoding.
pub(super) const PLATINUM_SERIALIZER: Rva = Rva::new(0x007e_51d0);
