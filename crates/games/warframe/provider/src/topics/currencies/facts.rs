//! Profile-data fields and executable references for `warframe.currencies`.

use provider_sdk::memory::{ObjectOffset, Rva};

use crate::scalar::AddressEncodedScalarFacts;

pub(super) const CODEC: AddressEncodedScalarFacts = AddressEncodedScalarFacts {
    check: ObjectOffset::new(0),
    stored: ObjectOffset::new(4),
    rotate_left: 19,
    address_shift: 3,
    value_xor: 0xc551_98a3,
    check_xor: 0xad84_b2ea,
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
    field: ObjectOffset::new(0xd998),
    reader: Rva::new(0x00f8_b32f),
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
        field: ObjectOffset::new(0xd9a8),
        reader: Rva::new(0x0126_e102),
    },
    name: EncodedNameFacts {
        text: "RegularCredits",
        reference: Rva::new(0x0126_e113),
        object: Rva::new(0x0286_dfe0),
        key_reference: Rva::new(0x0126_e11d),
        key: 0x8bb1_c1a5_d63b_e26c,
    },
};

/// Platinum readers select the field, then call the shared serializer 12 bytes later.
pub(super) const TRADABLE_PLATINUM: WalletFieldFacts = WalletFieldFacts {
    balance: BalanceFacts {
        field: ObjectOffset::new(0xd9b0),
        reader: Rva::new(0x0126_e1b0),
    },
    name: EncodedNameFacts {
        text: "mInventory.mPremiumCredits",
        reference: Rva::new(0x0126_e1a4),
        object: Rva::new(0x0286_e000),
        key_reference: Rva::new(0x0126_e15d),
        key: 0x3d0f_8056_e62f_e45c,
    },
};

pub(super) const NON_TRADABLE_PLATINUM: WalletFieldFacts = WalletFieldFacts {
    balance: BalanceFacts {
        field: ObjectOffset::new(0xd9b8),
        reader: Rva::new(0x0126_e1dc),
    },
    name: EncodedNameFacts {
        text: "mInventory.mPremiumCreditsFree",
        reference: Rva::new(0x0126_e1cb),
        object: Rva::new(0x0286_e030),
        key_reference: Rva::new(0x0126_e1c1),
        key: 0x15be_5faf_6e29_e554,
    },
};

/// Both Platinum fields use this serializer's protected-scalar encoding.
pub(super) const PLATINUM_SERIALIZER: Rva = Rva::new(0x00ab_2f20);
