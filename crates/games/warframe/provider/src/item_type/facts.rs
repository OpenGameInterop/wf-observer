//! Descriptor layout and executable references used by the item resolver.

use provider_sdk::memory::{ObjectOffset, Rva};

use crate::string_pool::facts::{STRINGS, StringPoolFacts};

/// Native item references point to type descriptors, not directly to item paths.
/// Descriptor-relative offsets locate tokens whose resolved prefix and suffix form the path.
#[derive(Debug, ..Copy, ..Eq)]
pub(crate) struct ItemTypeFacts {
    pub(crate) leaf_vtable: Rva,
    /// Constructor instructions checked for the vtable and descriptor field offsets.
    pub(crate) leaf_constructor: Rva,
    /// Pointer to an object whose first u32 is the prefix token; null means no prefix.
    pub(crate) prefix_object: ObjectOffset,
    /// Constructor-only layout check; the path reader does not traverse this parent.
    pub(crate) parent: ObjectOffset,
    pub(crate) suffix_token: ObjectOffset,
    /// Instructions checked for reading the prefix token and suffix token as a pair.
    pub(crate) type_pair_builder: Rva,
    pub(crate) strings: StringPoolFacts,
}

pub(crate) const ITEM_TYPES: ItemTypeFacts = ItemTypeFacts {
    leaf_vtable: Rva::new(0x0203_fba8),
    leaf_constructor: Rva::new(0x0119_77c0),
    prefix_object: ObjectOffset::new(0x10),
    parent: ObjectOffset::new(0x18),
    suffix_token: ObjectOffset::new(0x2c),
    type_pair_builder: Rva::new(0x015a_3770),
    strings: STRINGS,
};
