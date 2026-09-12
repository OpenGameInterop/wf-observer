use std::mem::size_of;

use super::account::{DecodeError, decode_account_id, decode_account_string_header};
use derive_more::{Error, From};
use displaydoc::Display;
use memory_reader::ProcessMemory;
use provider_sdk::memory::{ObjectOffset, ReadError, Rva, TargetReader, read_stable};
use warframe_model::AccountId;
use zeroize::Zeroizing;

use crate::target::READ_LIMITS;

use super::{
    LoginIdentity, ObjectIdentity,
    facts::{ActiveProfileFacts, LOGIN, LoginRootFacts, ProfileManagerFacts},
};

const LOGGED_IN_MANAGER_STATE: u32 = 4;
const MAX_PROFILE_CONTROLS: usize = 16;
const PROFILE_VECTOR_BYTES: usize = MAX_PROFILE_CONTROLS * size_of::<u64>();

/// Result of login-root acquisition; repeated resolutions must agree before use.
#[derive(Clone, Debug, ..Eq)]
pub(crate) enum LoginResolution {
    Absent,
    Present(LoginIdentity),
}

/// Failure while validating or resolving the login-owned object graph.
#[derive(Debug, Display, Error, From)]
pub(crate) enum ResolveError {
    /// bounded target read failed: {0}
    #[from]
    Read(ReadError),
    /// target value decoding failed: {0}
    #[from]
    Decode(DecodeError),
    /// bundled layout validation failed at {0}
    Unsupported(#[error(not(source))] &'static str),
    /// target root contradicts the selected compatibility facts at {0}
    Contradiction(#[error(not(source))] &'static str),
    /// active profile selection is ambiguous
    AmbiguousProfile,
}

pub(crate) fn resolve_login(
    memory: &mut (impl ProcessMemory + ?Sized),
    module_base: u64,
    image_size: u32,
) -> Result<LoginResolution, ResolveError> {
    let mut reader = TargetReader::new(memory, module_base, image_size, READ_LIMITS)?;
    // Compare the ownership identities (or Absent) from two full traversals,
    // not every intermediate byte.
    read_stable(
        || resolve_once(&mut reader, &LOGIN),
        || ResolveError::from(ReadError::changed("login roots")),
    )
}

pub(crate) fn validate_login_layout(
    memory: &mut (impl ProcessMemory + ?Sized),
    module_base: u64,
    image_size: u32,
) -> Result<(), ResolveError> {
    let mut reader = TargetReader::new(memory, module_base, image_size, READ_LIMITS)?;
    validate_layout(&mut reader, &LOGIN)
}

fn resolve_once(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
    facts: &LoginRootFacts,
) -> Result<LoginResolution, ResolveError> {
    let manager_root = reader.module_address(facts.manager.root, size_of::<u64>())?;
    let manager_control = reader.read_u64(manager_root)?;
    let sentinel = reader.module_address(facts.shared_null_sentinel, 1)?;
    let Some((manager_control, manager)) = read_control_target(reader, manager_control, sentinel)?
    else {
        return Ok(LoginResolution::Absent);
    };
    // Readable memory alone does not identify a manager. Require the expected
    // vtable and function entries before interpreting its state and profile list.
    let manager_vtable = reader.read_u64(manager.get())?;
    reader.require_module_pointer(
        manager_vtable,
        facts.manager.vtable,
        "profile manager vtable",
    )?;
    reader.require_vtable_slot_le64(
        manager.get(),
        facts.manager.lookup_slot,
        facts.manager.lookup,
        "profile manager lookup slot",
    )?;
    reader.require_vtable_slot_le64(
        manager.get(),
        facts.manager.logged_in_slot,
        facts.manager.logged_in,
        "profile manager login slot",
    )?;
    if reader.read_object_u32(manager.get(), facts.manager.state)? != LOGGED_IN_MANAGER_STATE {
        return Ok(LoginResolution::Absent);
    }

    let vector = read_profile_vector(reader, manager.get(), &facts.manager)?;
    if vector.is_empty() {
        return Ok(LoginResolution::Absent);
    }
    let (profile_control, profile) = select_profile(reader, &vector, &facts.profile)?;
    let account_id = read_account_id(reader, profile.get(), facts.profile.account)?;

    reader.require_vtable_slot_le64(
        profile.get(),
        facts.profile_data.slot,
        facts.profile_data.getter,
        "profile-data getter slot",
    )?;
    let profile_data_control_value =
        reader.read_object_u64(profile.get(), facts.profile_data.field)?;
    let Some((profile_data_control, profile_data)) =
        read_control_target(reader, profile_data_control_value, sentinel)?
    else {
        return Ok(LoginResolution::Absent);
    };
    let profile_data_vtable = reader.read_u64(profile_data.get())?;
    reader.require_module_pointer(
        profile_data_vtable,
        facts.profile_data.vtable,
        "profile-data vtable",
    )?;
    Ok(LoginResolution::Present(LoginIdentity {
        account_id,
        manager_control,
        manager,
        profile_control,
        profile,
        profile_data_control,
        profile_data,
    }))
}

fn read_profile_vector(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
    manager: u64,
    facts: &ProfileManagerFacts,
) -> Result<Zeroizing<Vec<u8>>, ResolveError> {
    let pointer = reader.read_object_u64(manager, facts.vector)?;
    let bytes = reader.read_object_u32(manager, facts.vector_size)?;
    let capacity = reader.read_object_u32(manager, facts.vector_capacity)?;
    let bytes =
        usize::try_from(bytes).map_err(|_| ResolveError::Contradiction("profile vector length"))?;
    let capacity = usize::try_from(capacity)
        .map_err(|_| ResolveError::Contradiction("profile vector capacity"))?;
    if bytes > capacity
        || capacity > PROFILE_VECTOR_BYTES
        || !bytes.is_multiple_of(size_of::<u64>())
        || !capacity.is_multiple_of(size_of::<u64>())
        || (capacity != 0 && pointer == 0)
    {
        return Err(ResolveError::Contradiction("profile vector header"));
    }
    if pointer != 0 {
        object_identity(pointer, "profile vector allocation")?;
    }
    let mut contents = Zeroizing::new(vec![0_u8; bytes]);
    if !contents.is_empty() {
        reader.read_at(pointer, contents.as_mut_slice())?;
    }
    Ok(contents)
}

fn select_profile(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
    controls: &[u8],
    facts: &ActiveProfileFacts,
) -> Result<(ObjectIdentity, ObjectIdentity), ResolveError> {
    let mut selected = None;
    for bytes in controls.as_chunks::<8>().0 {
        let control_value = u64::from_le_bytes(*bytes);
        if control_value == 0 {
            continue;
        }
        let control = object_identity(control_value, "profile control")?;
        let profile_value = reader.read_u64(control.get())?;
        if profile_value == 0 {
            continue;
        }
        let profile = object_identity(profile_value, "profile object")?;
        // The supported layout selects selector 0 with a nonzero login byte;
        // vector order alone does not identify the active account.
        if reader.read_object_u64(profile.get(), facts.selector)? != 0
            || reader.read_object_u8(profile.get(), facts.logged_in)? == 0
        {
            continue;
        }
        let profile_vtable = reader.read_u64(profile.get())?;
        reader.require_module_pointer(profile_vtable, facts.vtable, "profile vtable")?;
        reader.require_vtable_slot_le64(
            profile.get(),
            facts.identity_slot,
            facts.identity_getter,
            "profile identity slot",
        )?;
        reader.require_vtable_slot_le64(
            profile.get(),
            facts.account_slot,
            facts.account_getter,
            "profile account slot",
        )?;
        // Do not choose arbitrarily if two entries satisfy the selection rules.
        if selected.replace((control, profile)).is_some() {
            return Err(ResolveError::AmbiguousProfile);
        }
    }
    // A logged-in manager with a nonempty list but no matching profile is a
    // contradiction, unlike the explicit no-data states handled by resolve_once.
    selected.ok_or(ResolveError::Contradiction("active profile selection"))
}

fn read_account_id(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
    profile: u64,
    offset: ObjectOffset,
) -> Result<AccountId, ResolveError> {
    let address = reader.object_address(profile, offset, 16)?;
    let mut first_header = [0_u8; 16];
    reader.read_at(address, &mut first_header)?;
    let storage = decode_account_string_header(&first_header)?;
    object_identity(storage, "account identifier storage")?;
    let mut bytes = Zeroizing::new([0_u8; 24]);
    reader.read_at(storage, bytes.as_mut())?;
    // Reject a changed storage pointer/header while fetching the characters.
    // In-place account-content changes are also compared by resolve_login's pair.
    let mut second_header = [0_u8; 16];
    reader.read_at(address, &mut second_header)?;
    if first_header != second_header {
        return Err(ReadError::changed("account identifier").into());
    }
    decode_account_id(&bytes).map_err(ResolveError::from)
}

fn read_control_target(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
    control: u64,
    sentinel: u64,
) -> Result<Option<(ObjectIdentity, ObjectIdentity)>, ResolveError> {
    if control == 0 || control == sentinel {
        return Ok(None);
    }
    let control = object_identity(control, "shared ownership control")?;
    let target = reader.read_u64(control.get())?;
    if target == 0 {
        return Ok(None);
    }
    Ok(Some((
        control,
        object_identity(target, "shared ownership target")?,
    )))
}

fn object_identity(value: u64, location: &'static str) -> Result<ObjectIdentity, ResolveError> {
    ObjectIdentity::from_address(value).ok_or(ResolveError::Contradiction(location))
}

fn validate_layout(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
    facts: &LoginRootFacts,
) -> Result<(), ResolveError> {
    validate_manager_getter(reader, &facts.manager)?;
    validate_member_getter(
        reader,
        facts.profile.identity_getter,
        0x8b,
        facts.profile.selector,
        "profile identity getter",
    )?;
    validate_member_getter(
        reader,
        facts.profile.account_getter,
        0x8d,
        facts.profile.account,
        "profile account getter",
    )?;
    validate_manager_functions(reader, &facts.manager)?;
    validate_dereferenced_member_getter(
        reader,
        facts.profile_data.getter,
        facts.profile_data.field,
        "profile-data getter",
    )
}

fn validate_dereferenced_member_getter(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
    function: Rva,
    offset: ObjectOffset,
    location: &'static str,
) -> Result<(), ResolveError> {
    let mut code = [0_u8; 11];
    reader.read_module(function, &mut code)?;
    if code[..3] != [0x48, 0x8b, 0x81]
        || code[3..7] != offset.get().to_le_bytes()
        || code[7..] != [0x48, 0x8b, 0x00, 0xc3]
    {
        return Err(ResolveError::Unsupported(location));
    }
    Ok(())
}

fn validate_manager_getter(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
    facts: &ProfileManagerFacts,
) -> Result<(), ResolveError> {
    let code = reader.read_module_array::<11>(facts.getter)?;
    if code[..3] != [0x48, 0x8b, 0x05] || code[7..] != [0x48, 0x8b, 0x00, 0xc3] {
        return Err(ResolveError::Unsupported("profile manager getter"));
    }
    let target = facts
        .getter
        .rip_target(7, &code[3..7])
        .ok_or(ResolveError::Unsupported("profile manager getter"))?;
    if target == facts.root {
        Ok(())
    } else {
        Err(ResolveError::Unsupported("profile manager root"))
    }
}

fn validate_member_getter(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
    function: Rva,
    operation: u8,
    offset: ObjectOffset,
    location: &'static str,
) -> Result<(), ResolveError> {
    let mut code = [0_u8; 8];
    reader.read_module(function, &mut code)?;
    if code[..3] != [0x48, operation, 0x81]
        || code[3..7] != offset.get().to_le_bytes()
        || code[7] != 0xc3
    {
        return Err(ResolveError::Unsupported(location));
    }
    Ok(())
}

fn validate_manager_functions(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
    facts: &ProfileManagerFacts,
) -> Result<(), ResolveError> {
    let mut lookup = [0_u8; 31];
    reader.read_module(facts.lookup, &mut lookup)?;
    if lookup[15..18] != [0x48, 0x8b, 0x99]
        || lookup[18..22] != facts.vector.get().to_le_bytes()
        || lookup[25..27] != [0x8b, 0x81]
        || lookup[27..31] != facts.vector_size.get().to_le_bytes()
    {
        return Err(ResolveError::Unsupported("profile manager lookup"));
    }

    let mut logged_in = [0_u8; 11];
    reader.read_module(facts.logged_in, &mut logged_in)?;
    if logged_in[..2] != [0x83, 0xb9]
        || logged_in[2..6] != facts.state.get().to_le_bytes()
        || logged_in[6..] != [0x04, 0x0f, 0x94, 0xc0, 0xc3]
    {
        return Err(ResolveError::Unsupported("profile manager login predicate"));
    }
    Ok(())
}
