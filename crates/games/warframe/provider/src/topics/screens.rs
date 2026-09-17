//! One bounded, account-independent source shared by all screen consumers.

use memory_reader::ProcessMemory;
use provider_sdk::memory::{ObjectOffset, ReadError, Rva, TargetReader, read_stable};
use std::collections::BTreeSet;
use warframe_model::{Screen, ScreensSnapshot};

use crate::world::{self, SharedObject};

const OVERLAY_VTABLE: Rva = Rva::new(0x0222_2300);
const FLASH_VTABLE: Rva = Rva::new(0x0222_1518);
const MOVIE_VTABLE: Rva = Rva::new(0x0222_00f8);
const VISIBILITY: Rva = Rva::new(0x0143_d6e0);
const VECTOR: ObjectOffset = ObjectOffset::new(0xd0);
const MAX_MOVIES: usize = 128;

/// The visibility method reads all three flags from the movie itself. The
/// shared object at +0x140 is its owner, not the base of the hidden flag.
pub(crate) fn validate(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
) -> Result<(), ReadError> {
    world::validate_client(reader)?;
    let slot = MOVIE_VTABLE
        .checked_add(0x770)
        .ok_or(ReadError::overflow("movie visibility method"))?;
    let method = u64::from_le_bytes(reader.read_module_array(slot)?);
    reader.require_module_pointer(method, VISIBILITY, "movie visibility method")?;
    if reader.read_module_array::<46>(VISIBILITY)?
        != [
            0x80, 0xb9, 0x40, 0x54, 0, 0, 0, 0x75, 0x22, 0x80, 0xb9, 0x18, 1, 0, 0, 0, 0x75, 0x0d,
            0x48, 0x8b, 0x81, 0x40, 1, 0, 0, 0x48, 0x83, 0x38, 0, 0x74, 0x0c, 0x80, 0xb9, 0xfa, 0,
            0, 0, 0, 0x74, 3, 0xb0, 1, 0xc3, 0x32, 0xc0, 0xc3,
        ]
    {
        return Err(ReadError::layout("movie visibility fields"));
    }
    Ok(())
}

#[derive(Debug, Default, ..Copy, ..Eq)]
pub(crate) struct UiIdentity {
    pub(crate) client: Option<SharedObject>,
    overlay: Option<SharedObject>,
    flash: Option<SharedObject>,
}

#[derive(Debug, Clone, ..Eq)]
pub(crate) struct ScreenSample {
    pub(crate) owner: UiIdentity,
    pub(crate) snapshot: ScreensSnapshot,
    visible_movies: Vec<(SharedObject, SharedObject, Screen)>,
}

pub(crate) fn read(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
) -> Result<ScreenSample, ReadError> {
    read_stable(
        || read_once(reader),
        || ReadError::changed("visible screens"),
    )
}

fn resolve_ui(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
) -> Result<UiIdentity, ReadError> {
    let mut ui = UiIdentity {
        client: world::client(reader)?,
        ..UiIdentity::default()
    };
    if let Some(client) = ui.client {
        ui.overlay = world::shared_field(reader, client, ObjectOffset::new(0x5e0))?;
    }
    if let Some(overlay) = ui.overlay {
        world::require_vtable(reader, overlay, OVERLAY_VTABLE)?;
        ui.flash = world::shared_field(reader, overlay, ObjectOffset::new(0x48))?;
    }
    if let Some(flash) = ui.flash {
        world::require_vtable(reader, flash, FLASH_VTABLE)?;
    }
    Ok(ui)
}

#[allow(
    clippy::case_sensitive_file_extension_comparisons,
    reason = "Warframe asset identifiers are case-sensitive, not filesystem paths"
)]
fn read_once(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
) -> Result<ScreenSample, ReadError> {
    let owner = resolve_ui(reader)?;
    let mut visible_movies = Vec::new();
    if let Some(flash) = owner.flash {
        let header = reader.read_object_array::<16>(flash.object.get(), VECTOR)?;
        let pointer = u64::from_le_bytes(header.as_chunks::<8>().0[0]);
        let length = u32::from_le_bytes(header.as_chunks::<4>().0[2]) as usize;
        let capacity = u32::from_le_bytes(header.as_chunks::<4>().0[3]) as usize;
        if length > capacity
            || capacity > MAX_MOVIES * 8
            || !length.is_multiple_of(8)
            || !capacity.is_multiple_of(8)
            || (capacity != 0 && pointer == 0)
        {
            return Err(ReadError::invalid("screen vector bounds"));
        }
        if pointer != 0 {
            world::identity(pointer)?;
        }
        let mut controls = vec![0; length];
        reader.read_at(pointer, &mut controls)?;
        for control in controls.as_chunks::<8>().0 {
            let Some(movie) = world::shared(reader, u64::from_le_bytes(*control))? else {
                continue;
            };
            world::require_vtable(reader, movie, MOVIE_VTABLE)?;
            let address = movie.object.get();
            if !flag(reader, address, 0xfa)? || flag(reader, address, 0x118)? {
                continue;
            }
            let Some(movie_owner) = world::shared_field(reader, movie, ObjectOffset::new(0x140))?
            else {
                continue;
            };
            if flag(reader, address, 0x5440)? {
                continue;
            }
            let path = reader.read_object_u64(address, ObjectOffset::new(0x120))?;
            let bytes = reader.read_array::<256>(path)?;
            let end = bytes
                .iter()
                .position(|byte| *byte == 0)
                .ok_or(ReadError::limit("screen asset path"))?;
            let path = std::str::from_utf8(&bytes[..end])
                .map_err(|_| ReadError::invalid("screen asset UTF-8"))?;
            if !path.starts_with("/Lotus/Interface/") || !path.ends_with(".swf") {
                return Err(ReadError::invalid("screen asset path"));
            }
            visible_movies.push((movie, movie_owner, Screen::from_asset_path(path.to_owned())));
        }
        if reader.read_object_array::<16>(flash.object.get(), VECTOR)? != header {
            return Err(ReadError::changed("screen vector"));
        }
    }
    if resolve_ui(reader)? != owner {
        return Err(ReadError::changed("screen owner"));
    }
    let screens = visible_movies
        .iter()
        .map(|(_, _, screen)| screen.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    Ok(ScreenSample {
        owner,
        snapshot: ScreensSnapshot { screens },
        visible_movies,
    })
}

fn flag(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
    object: u64,
    offset: u32,
) -> Result<bool, ReadError> {
    match reader.read_object_u8(object, ObjectOffset::new(offset))? {
        0 => Ok(false),
        1 => Ok(true),
        value => {
            tracing::debug!(offset, value, "invalid screen visibility flag");
            Err(ReadError::invalid("screen visibility flag"))
        }
    }
}
