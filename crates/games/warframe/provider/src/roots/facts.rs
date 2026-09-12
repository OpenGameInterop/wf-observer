//! Login and profile ownership layout; see [`super`] for the pointer traversal.

use provider_sdk::memory::{ObjectOffset, Rva, VtableSlot};

#[derive(Clone, Debug, ..Eq)]
pub(crate) struct LoginRootFacts {
    /// Non-null placeholder for an absent control object; do not dereference it.
    pub(crate) shared_null_sentinel: Rva,
    pub(crate) manager: ProfileManagerFacts,
    pub(crate) profile: ActiveProfileFacts,
    pub(crate) profile_data: ProfileDataFacts,
}

pub(crate) const LOGIN: LoginRootFacts = LoginRootFacts {
    shared_null_sentinel: Rva::new(0x0272_f3b0),
    manager: MANAGER,
    profile: PROFILE,
    profile_data: PROFILE_DATA,
};

/// All object offsets below are relative to the manager object.
/// Its profile vector contains control pointers, each pointing to a profile pointer.
#[derive(Clone, Debug, ..Eq)]
pub(crate) struct ProfileManagerFacts {
    /// Global storage holding a control pointer, whose first pointer is the manager.
    pub(crate) root: Rva,
    /// Instructions checked for the root reference and the extra dereference.
    pub(crate) getter: Rva,
    pub(crate) vtable: Rva,
    /// Instructions checked for the profile-vector pointer and length offsets.
    pub(crate) lookup: Rva,
    /// Instructions checked for the state offset and comparison against logged-in state 4.
    pub(crate) logged_in: Rva,
    /// Entries in the manager's vtable required to point to `lookup` and `logged_in`.
    pub(crate) lookup_slot: VtableSlot,
    pub(crate) logged_in_slot: VtableSlot,
    pub(crate) state: ObjectOffset,
    pub(crate) vector: ObjectOffset,
    /// Vector length and capacity are byte counts, not numbers of profiles.
    pub(crate) vector_size: ObjectOffset,
    pub(crate) vector_capacity: ObjectOffset,
}

pub(crate) const MANAGER: ProfileManagerFacts = ProfileManagerFacts {
    root: Rva::new(0x027a_53c0),
    getter: Rva::new(0x0029_cc30),
    vtable: Rva::new(0x0214_ba70),
    lookup: Rva::new(0x0020_9260),
    logged_in: Rva::new(0x00f7_a940),
    lookup_slot: VtableSlot::new(0xb8),
    logged_in_slot: VtableSlot::new(0x148),
    state: ObjectOffset::new(0x168),
    vector: ObjectOffset::new(0x230),
    vector_size: ObjectOffset::new(0x238),
    vector_capacity: ObjectOffset::new(0x23c),
};

/// All object offsets below are relative to a candidate profile object.
/// Select exactly one with selector 0 and a nonzero logged-in byte.
#[derive(Clone, Debug, ..Eq)]
pub(crate) struct ActiveProfileFacts {
    pub(crate) vtable: Rva,
    /// Checks that the game reads `selector` at the expected offset.
    pub(crate) identity_getter: Rva,
    /// Checks that the game takes the address of the `account` string header.
    pub(crate) account_getter: Rva,
    /// Entries in the profile's vtable required to point to the two getters above.
    pub(crate) identity_slot: VtableSlot,
    pub(crate) account_slot: VtableSlot,
    pub(crate) selector: ObjectOffset,
    pub(crate) logged_in: ObjectOffset,
    /// Native string header, not the account characters themselves; follow its storage pointer.
    pub(crate) account: ObjectOffset,
}

pub(crate) const PROFILE: ActiveProfileFacts = ActiveProfileFacts {
    vtable: Rva::new(0x0215_1328),
    identity_getter: Rva::new(0x0024_4ee0),
    account_getter: Rva::new(0x0056_ee90),
    identity_slot: VtableSlot::new(0x18),
    account_slot: VtableSlot::new(0x28),
    selector: ObjectOffset::new(0x1d8),
    logged_in: ObjectOffset::new(0x1e1),
    account: ObjectOffset::new(0x118),
};

#[derive(Clone, Debug, ..Eq)]
pub(crate) struct ProfileDataFacts {
    /// Entry in the active profile's vtable required to point to `getter`.
    pub(crate) slot: VtableSlot,
    /// Instructions checked for `field` and the extra control-to-data dereference.
    pub(crate) getter: Rva,
    /// Offset within the active profile holding a control pointer;
    /// the control object's first pointer leads to profile data.
    pub(crate) field: ObjectOffset,
    /// Expected vtable of the resulting profile-data object, not the active profile.
    pub(crate) vtable: Rva,
}

pub(crate) const PROFILE_DATA: ProfileDataFacts = ProfileDataFacts {
    slot: VtableSlot::new(0x390),
    getter: Rva::new(0x008e_1ff0),
    field: ObjectOffset::new(0x208),
    vtable: Rva::new(0x0237_e5c0),
};
