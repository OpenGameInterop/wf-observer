use super::{facts::MASTERY, layout::decode_records};
use crate::{
    item_type::{ItemTypeCache, ItemTypeError, facts::ITEM_TYPES},
    profile_inventory::{INVENTORY_OWNER, VECTOR_HEADER_BYTES, read_commit_state, read_vector},
    roots::LoginIdentity,
    scalar::AddressEncodedScalarFacts,
    string_pool::StringTokenCache,
    target::READ_LIMITS,
};
use memory_reader::ProcessMemory;
use provider_sdk::memory::{ObjectOffset, ReadError, RecordView, TargetReader};
use std::collections::BTreeMap;
use warframe_model::{MasteryItemProgress, MasterySnapshot};

#[derive(Debug, displaydoc::Display, derive_more::Error, derive_more::From)]
pub(crate) enum MasteryError {
    /// bounded mastery read failed: {0}
    #[from]
    Read(ReadError),
    /// mastery item identity failed validation: {0}
    #[from]
    ItemType(ItemTypeError),
    /// mastery model failed validation: {0}
    #[from]
    InvalidModel(warframe_model::InvalidMastery),
}

pub(crate) fn read_mastery(
    memory: &mut (impl ProcessMemory + ?Sized),
    module_base: u64,
    image_size: u32,
    login: &LoginIdentity,
    items: &mut ItemTypeCache,
    strings: &mut StringTokenCache,
) -> Result<MasterySnapshot, MasteryError> {
    let mut reader = TargetReader::new(memory, module_base, image_size, READ_LIMITS)?;
    let profile = login.profile_data.get();
    let inventory = reader.object_address(profile, INVENTORY_OWNER.offset, VECTOR_HEADER_BYTES)?;
    let commit = read_commit_state(&mut reader, profile)?;
    let before = read_progression(&mut reader, profile, inventory)?;
    let (address, payload) =
        read_vector(&mut reader, inventory, MASTERY.vector, MASTERY.record_bytes)?;
    let mut by_key = BTreeMap::new();
    let mut pool = None;
    for item in decode_records(address, &payload)? {
        let key =
            items.resolve_item_path(&mut reader, ITEM_TYPES, item.item_type, strings, &mut pool)?;
        let progress = MasteryItemProgress {
            item_key: key.clone(),
            affinity: u64::from(item.affinity),
        };
        if by_key.insert(key, progress).is_some() {
            return Err(ReadError::invalid("duplicate mastery item key").into());
        }
    }
    if before != read_progression(&mut reader, profile, inventory)?
        || commit != read_commit_state(&mut reader, profile)?
    {
        return Err(ReadError::changed("mastery progression or inventory commit markers").into());
    }
    let total = before
        .non_item_xp
        .into_iter()
        .fold(u64::from(before.item_xp), |total, xp| total + u64::from(xp));
    Ok(MasterySnapshot::new(
        login.account_id.clone(),
        u32::from(before.rank),
        total,
        u64::from(before.item_xp),
        by_key.into_values().collect(),
    )?)
}

#[derive(..Eq)]
struct Progression {
    non_item_xp: [u32; 3],
    item_xp: u32,
    rank: u16,
}

fn read_progression(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
    profile: u64,
    inventory: u64,
) -> Result<Progression, ReadError> {
    require_calculated(reader, inventory)?;
    let mut non_item_xp = [0; 3];
    for (value, offset) in non_item_xp.iter_mut().zip(MASTERY.non_item_xp) {
        *value = read_pool(reader, profile, offset, MASTERY.non_item_xp_codec)?;
    }
    let item_xp = reader.read_object_u32(inventory, MASTERY.item_xp)?;
    let rank_address = reader.object_address(inventory, MASTERY.rank, 4)?;
    let bytes = reader.read_array::<4>(rank_address)?;
    let record = RecordView::new(&bytes, "mastery rank");
    let codec = MASTERY.rank_codec;
    let stored_address = reader.object_address(rank_address, codec.stored, 2)?;
    let rank = codec
        .decode(
            record.u16(codec.check)?,
            record.u16(codec.stored)?,
            stored_address,
        )
        .ok_or_else(|| ReadError::invalid("mastery rank integrity"))?;
    require_calculated(reader, inventory)?;
    Ok(Progression {
        non_item_xp,
        item_xp,
        rank,
    })
}

fn require_calculated(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
    inventory: u64,
) -> Result<(), ReadError> {
    if reader.read_object_u8(inventory, MASTERY.item_xp_dirty)? != 0 {
        return Err(ReadError::not_ready("mastery calculation"));
    }
    Ok(())
}

fn read_pool(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
    profile: u64,
    offset: ObjectOffset,
    codec: AddressEncodedScalarFacts,
) -> Result<u32, ReadError> {
    let address = reader.object_address(profile, offset, 8)?;
    let bytes = reader.read_array::<8>(address)?;
    let record = RecordView::new(&bytes, "mastery non-item pool");
    codec
        .decode(
            record.u32(codec.check)?,
            record.u32(codec.stored)?,
            reader.object_address(address, codec.stored, 4)?,
        )
        .ok_or_else(|| ReadError::invalid("mastery non-item pool integrity"))
}
