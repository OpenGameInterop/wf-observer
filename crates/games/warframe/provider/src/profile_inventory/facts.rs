//! Embedded inventory ownership and commit markers shared by inventory and mastery.

use provider_sdk::memory::{ObjectOffset, Rva};

#[derive(Clone, Debug, ..Eq)]
pub(crate) struct InventoryOwnerFacts {
    /// Instructions checked for returning profile-data address + `offset`, without dereferencing.
    pub(crate) getter: Rva,
    /// Embedded inventory's byte offset within profile data, not a pointer field.
    pub(crate) offset: ObjectOffset,
}

/// Profile-wide coherence, independent of the embedded inventory's location.
#[derive(Clone, Debug, ..Eq)]
pub(crate) struct ProfileCommitFacts {
    /// One byte relative to profile data; nonzero means rebuilding, so reject the sample.
    pub(crate) force_update: ObjectOffset,
    /// 24 bytes relative to profile data, compared before/after reading all families.
    /// A change rejects the sample; we do not interpret the individual tokens.
    pub(crate) sync_tokens: ObjectOffset,
    pub(crate) commit: InventoryCommitFacts,
}

pub(crate) const INVENTORY_OWNER: InventoryOwnerFacts = InventoryOwnerFacts {
    getter: Rva::new(0x0148_fde0),
    offset: ObjectOffset::new(0xd608),
};

pub(crate) const PROFILE_COMMIT: ProfileCommitFacts = ProfileCommitFacts {
    force_update: ObjectOffset::new(0x0001_1ff8),
    sync_tokens: ObjectOffset::new(0xffe0),
    commit: InventoryCommitFacts {
        constructor_vtable: Rva::new(0x003e_fc20),
        force_update_init: Rva::new(0x003f_0bbc),
        force_update_read: Rva::new(0x01b1_2a89),
        sync_tokens_init: Rva::new(0x003f_06e5),
        sync_tokens_compare: Rva::new(0x0141_d6af),
        compare_bytes: Rva::new(0x01fc_5670),
    },
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
