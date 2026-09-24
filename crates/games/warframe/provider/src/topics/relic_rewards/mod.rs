//! Bounded decoding of the visible picker. The session owns screen/login/world checks.

mod layout;
pub(crate) use layout::validate;

use memory_reader::ProcessMemory;
use provider_sdk::memory::{ObjectOffset, ReadError, RecordView, TargetReader, read_stable};
use warframe_model::{AccountId, RelicRewardChoice};

use crate::{
    item_type::{ItemTypeCache, ItemTypeError, facts::ITEM_TYPES},
    native_string,
    string_pool::StringTokenCache,
    world::{self, SharedObject},
};

const VECTOR: ObjectOffset = ObjectOffset::new(0x18b0);
const RECORD_BYTES: usize = 0x70;
const MAX_CHOICES: usize = 4;

pub(crate) fn read(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
    rules: SharedObject,
    account: &AccountId,
    items: &mut ItemTypeCache,
    strings: &mut StringTokenCache,
) -> Result<Vec<RelicRewardChoice>, ReadError> {
    world::require_vtable(reader, rules, world::MISSION_VTABLE)?;
    let ordered = read_stable(
        || read_once(reader, rules.object.get(), account, items, strings),
        || ReadError::changed("ordered relic rewards"),
    )?;
    Ok(ordered.into_iter().map(|(_, choice)| choice).collect())
}

fn read_once(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
    rules: u64,
    account: &AccountId,
    items: &mut ItemTypeCache,
    strings: &mut StringTokenCache,
) -> Result<Vec<(AccountId, RelicRewardChoice)>, ReadError> {
    let header = reader.read_object_array::<16>(rules, VECTOR)?;
    let (pointer, length) = vector_bounds(&header)?;
    let mut payload = vec![0; length];
    reader.read_at(pointer, &mut payload)?;
    let mut qualified = Vec::new();
    let mut choices = Vec::new();
    let mut pool = None;
    for (index, bytes) in payload.as_chunks::<RECORD_BYTES>().0.iter().enumerate() {
        if let Some(item_type) = reward_type(bytes)? {
            qualified.push(index);
            let item_key = items
                .resolve_item_path(reader, ITEM_TYPES, item_type, strings, &mut pool)
                .map_err(|error| match error {
                    ItemTypeError::Read(error) => error,
                    ItemTypeError::Invalid(at) => ReadError::invalid(at),
                })?;
            choices.push(RelicRewardChoice { item_key });
        }
    }
    let accounts = account_ids(reader, pointer, &payload, &qualified)?;
    let ordered = display_order(accounts, choices, account)?;
    let mut after = vec![0; length];
    reader.read_at(pointer, &mut after)?;
    if after != payload || reader.read_object_array::<16>(rules, VECTOR)? != header {
        return Err(ReadError::changed("relic reward vector"));
    }
    Ok(ordered)
}

fn vector_bounds(header: &[u8; 16]) -> Result<(u64, usize), ReadError> {
    let pointer = u64::from_le_bytes(header.as_chunks::<8>().0[0]);
    let length = u32::from_le_bytes(header.as_chunks::<4>().0[2]) as usize;
    let capacity = u32::from_le_bytes(header.as_chunks::<4>().0[3]) as usize;
    if length > MAX_CHOICES * RECORD_BYTES
        || length > capacity
        || capacity > 64 * RECORD_BYTES
        || !length.is_multiple_of(RECORD_BYTES)
        || !capacity.is_multiple_of(RECORD_BYTES)
        || (capacity != 0 && pointer == 0)
    {
        return Err(ReadError::invalid("relic reward vector bounds"));
    }
    if pointer != 0 {
        world::identity(pointer)?;
    }
    Ok((pointer, length))
}

fn reward_type(bytes: &[u8]) -> Result<Option<u64>, ReadError> {
    let record = RecordView::new(bytes, "relic reward record");
    match record.field(ObjectOffset::new(0x50), 1)?[0] {
        0 => Ok(None),
        1 => {
            let pointer = record.u64(ObjectOffset::new(0x48))?;
            if pointer == 0 {
                return Err(ReadError::not_ready("relic reward item"));
            }
            world::identity(pointer).map_err(|_| ReadError::invalid("reward item alignment"))?;
            Ok(Some(pointer))
        }
        _ => Err(ReadError::invalid("relic reward qualification")),
    }
}

/// Identify the constrained account-id field instead of assuming an unproved
/// string field name. Ambiguous matches and duplicate account IDs are rejected.
fn account_ids(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
    pointer: u64,
    payload: &[u8],
    qualified: &[usize],
) -> Result<Vec<AccountId>, ReadError> {
    if qualified.is_empty() {
        return Ok(Vec::new());
    }
    let mut found = None;
    for field in [0_u32, 0x10, 0x20, 0x30] {
        let mut accounts = Vec::new();
        for &index in qualified {
            let offset = index * RECORD_BYTES + field as usize;
            let header: &[u8; 16] = payload[offset..offset + 16]
                .try_into()
                .map_err(|_| ReadError::invalid("reward account string"))?;
            if native_string::encoded_length(header)? != 24 {
                break;
            }
            let object = reader.object_address(
                pointer,
                ObjectOffset::new(
                    u32::try_from(index * RECORD_BYTES)
                        .map_err(|_| ReadError::overflow("reward record"))?,
                ),
                RECORD_BYTES,
            )?;
            let text = native_string::read_string(reader, object, ObjectOffset::new(field), 24)?;
            let Ok(account) = AccountId::new(text) else {
                break;
            };
            if accounts.contains(&account) {
                break;
            }
            accounts.push(account);
        }
        if accounts.len() == qualified.len() && found.replace(accounts).is_some() {
            return Err(ReadError::invalid("ambiguous reward account field"));
        }
    }
    found.ok_or(ReadError::invalid("reward account field"))
}

fn display_order(
    accounts: Vec<AccountId>,
    choices: Vec<RelicRewardChoice>,
    local: &AccountId,
) -> Result<Vec<(AccountId, RelicRewardChoice)>, ReadError> {
    if accounts.len() != choices.len()
        || (!choices.is_empty() && accounts.iter().filter(|id| *id == local).count() != 1)
        || accounts
            .iter()
            .enumerate()
            .any(|(i, id)| accounts[..i].contains(id))
    {
        return Err(ReadError::invalid("reward display order"));
    }
    let mut ordered: Vec<_> = accounts.into_iter().zip(choices).collect();
    ordered
        .sort_unstable_by(|(a, _), (b, _)| (a != local).cmp(&(b != local)).then_with(|| a.cmp(b)));
    Ok(ordered)
}
