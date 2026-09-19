use super::{
    facts::{RECORD_BYTES, VECTOR},
    layout::decode_records,
};
use crate::{
    profile_inventory::{read_commit_state, read_vector},
    roots::LoginIdentity,
    string_pool::{StringTokenCache, facts::STRINGS},
    target::READ_LIMITS,
};
use memory_reader::ProcessMemory;
use provider_sdk::memory::{ReadError, TargetReader};
use std::collections::BTreeMap;
use warframe_model::{StarChartNodeProgress, StarChartSnapshot};

pub(crate) fn read_star_chart(
    memory: &mut (impl ProcessMemory + ?Sized),
    base: u64,
    image_size: u32,
    login: &LoginIdentity,
    strings: &mut StringTokenCache,
) -> Result<StarChartSnapshot, ReadError> {
    let mut reader = TargetReader::new(memory, base, image_size, READ_LIMITS)?;
    let profile = login.profile_data.get();
    let commit = read_commit_state(&mut reader, profile)?;
    let (address, bytes) = read_vector(&mut reader, profile, VECTOR, RECORD_BYTES)?;
    let before = decode_records(&bytes)?;
    let mut nodes = BTreeMap::new();
    if !before.is_empty() {
        let pool = StringTokenCache::pool(&mut reader, STRINGS)?;
        for record in &before {
            let key = strings.resolve(&mut reader, pool, record.token)?;
            let node = StarChartNodeProgress {
                node_key: key.clone(),
                completions: record.completions,
                steel_path_completed: record.tier >= 1,
            };
            if nodes.insert(key, node).is_some() {
                return Err(ReadError::invalid("duplicate mission node"));
            }
        }
    }
    // The vector may update a count/tier in place without changing its header.
    // Compare observed fields, excluding unused timestamp strings and padding.
    let (after_address, after) = read_vector(&mut reader, profile, VECTOR, RECORD_BYTES)?;
    if address != after_address
        || before != decode_records(&after)?
        || commit != read_commit_state(&mut reader, profile)?
    {
        return Err(ReadError::changed("mission progression"));
    }
    StarChartSnapshot::new(login.account_id.clone(), nodes.into_values().collect())
        .map_err(|_| ReadError::invalid("mission node records"))
}
