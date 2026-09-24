//! Private world identities. These addresses never cross the provider boundary.

use memory_reader::ProcessMemory;
use provider_sdk::memory::{ObjectOffset, ReadError, Rva, TargetReader, VtableSlot};

use crate::roots::{ACCOUNT, ObjectIdentity};

pub(crate) const CLIENT_ROOT: Rva = Rva::new(0x0270_8d80);
const CLIENT_STORAGE: Rva = Rva::new(0x0270_8cd0);
const CLIENT_INSTALLER: Rva = Rva::new(0x0001_f7a7);
const CLIENT_VTABLE: Rva = Rva::new(0x0219_7448);
const CONTEXT_VTABLE: Rva = Rva::new(0x0219_bbc0);
const REGION_VTABLE: Rva = Rva::new(0x0219_f8e8);
const RULES_GETTER: Rva = Rva::new(0x004c_7600);
pub(crate) const MISSION_VTABLE: Rva = Rva::new(0x0231_b9c8);
const MISSION_REFERENCE: Rva = Rva::new(0x0122_234c);

#[derive(Debug, ..Copy, ..Eq)]
pub(crate) struct SharedObject {
    control: ObjectIdentity,
    pub(crate) object: ObjectIdentity,
}

#[derive(Debug, Default, ..Copy, ..Eq)]
pub(crate) struct WorldIdentity {
    pub(crate) client: Option<SharedObject>,
    context: Option<SharedObject>,
    region: Option<SharedObject>,
    pub(crate) rules: Option<SharedObject>,
}

pub(crate) fn shared(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
    control: u64,
) -> Result<Option<SharedObject>, ReadError> {
    if control == 0 || control == reader.module_address(ACCOUNT.shared_null_sentinel, 1)? {
        return Ok(None);
    }
    let control = identity(control)?;
    let object = reader.read_u64(control.get())?;
    if object == 0 {
        return Ok(None);
    }
    Ok(Some(SharedObject {
        control,
        object: identity(object)?,
    }))
}

pub(crate) fn shared_field(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
    owner: SharedObject,
    offset: ObjectOffset,
) -> Result<Option<SharedObject>, ReadError> {
    let control = reader.read_object_u64(owner.object.get(), offset)?;
    shared(reader, control)
}

pub(crate) fn identity(address: u64) -> Result<ObjectIdentity, ReadError> {
    ObjectIdentity::from_address(address).ok_or(ReadError::invalid("world object alignment"))
}

pub(crate) fn require_vtable(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
    object: SharedObject,
    vtable: Rva,
) -> Result<(), ReadError> {
    let actual = reader.read_u64(object.object.get())?;
    reader.require_module_pointer(actual, vtable, "world object vtable")
}

pub(crate) fn client(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
) -> Result<Option<SharedObject>, ReadError> {
    let control = u64::from_le_bytes(reader.read_module_array(CLIENT_ROOT)?);
    let client = shared(reader, control)?;
    if let Some(client) = client {
        require_vtable(reader, client, CLIENT_VTABLE)?;
    }
    Ok(client)
}

/// Missing roots are observed transitions. Invalid/unreadable roots are errors.
pub(crate) fn resolve(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
) -> Result<WorldIdentity, ReadError> {
    let mut world = WorldIdentity {
        client: client(reader)?,
        ..WorldIdentity::default()
    };
    if let Some(client) = world.client {
        world.context = shared_field(reader, client, ObjectOffset::new(0x788))?;
    }
    if let Some(context) = world.context {
        require_vtable(reader, context, CONTEXT_VTABLE)?;
        world.region = shared_field(reader, context, ObjectOffset::new(0x120))?;
    }
    if let Some(region) = world.region {
        require_vtable(reader, region, REGION_VTABLE)?;
        reader.require_vtable_slot_le64(
            region.object.get(),
            VtableSlot::new(0x300),
            RULES_GETTER,
            "region rules getter",
        )?;
        world.rules = shared_field(reader, region, ObjectOffset::new(0x240))?;
    }
    Ok(world)
}

pub(crate) fn validate_client(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
) -> Result<(), ReadError> {
    let code = reader.read_module_array::<12>(CLIENT_INSTALLER)?;
    if code[..8] != [0xb9, 2, 0, 0, 0, 0x48, 0x8d, 5]
        || CLIENT_INSTALLER.rip_target(12, &code[8..]) != Some(CLIENT_STORAGE)
        || CLIENT_STORAGE.checked_add(0xb0) != Some(CLIENT_ROOT)
    {
        return Err(ReadError::layout("client root installer"));
    }
    Ok(())
}

pub(crate) fn validate_rules(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
) -> Result<(), ReadError> {
    if reader.read_module_array::<11>(RULES_GETTER)?
        != [0x48, 0x8b, 0x81, 0x40, 2, 0, 0, 0x48, 0x8b, 0, 0xc3]
    {
        return Err(ReadError::layout("region rules field"));
    }
    let code = reader.read_module_array::<10>(MISSION_REFERENCE)?;
    if code[..3] != [0x48, 0x8d, 5]
        || MISSION_REFERENCE.rip_target(7, &code[3..7]) != Some(MISSION_VTABLE)
        || code[7..] != [0x48, 0x89, 6]
    {
        return Err(ReadError::layout("mission rules constructor"));
    }
    Ok(())
}
