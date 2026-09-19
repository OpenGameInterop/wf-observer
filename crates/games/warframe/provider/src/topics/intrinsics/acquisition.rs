use super::facts::{BLOCK, BLOCK_BYTES, POINT_SCALE};
use crate::{profile_inventory::read_commit_state, roots::LoginIdentity, target::READ_LIMITS};
use memory_reader::ProcessMemory;
use provider_sdk::memory::{ObjectOffset, ReadError, RecordView, TargetReader};
use warframe_model::{DrifterIntrinsics, IntrinsicsSnapshot, RailjackIntrinsics};

pub(crate) fn read_intrinsics(
    memory: &mut (impl ProcessMemory + ?Sized),
    module_base: u64,
    image_size: u32,
    login: &LoginIdentity,
) -> Result<IntrinsicsSnapshot, ReadError> {
    let mut reader = TargetReader::new(memory, module_base, image_size, READ_LIMITS)?;
    let profile = login.profile_data.get();
    let commit = read_commit_state(&mut reader, profile)?;
    let before = reader.read_object_array::<BLOCK_BYTES>(profile, BLOCK)?;
    let value = decode(&before, login)?;
    if before != reader.read_object_array::<BLOCK_BYTES>(profile, BLOCK)?
        || commit != read_commit_state(&mut reader, profile)?
    {
        return Err(ReadError::changed("Intrinsic balances or ranks"));
    }
    Ok(value)
}

fn decode(
    bytes: &[u8; BLOCK_BYTES],
    login: &LoginIdentity,
) -> Result<IntrinsicsSnapshot, ReadError> {
    let record = RecordView::new(bytes, "Intrinsic progression");
    // Each slot is an unsigned 32-bit value, including the unspent point pools.
    let word = |index: u32| record.u32(ObjectOffset::new(index * 4));
    IntrinsicsSnapshot::new(
        login.account_id.clone(),
        RailjackIntrinsics {
            unspent_points: word(0)? / POINT_SCALE,
            piloting: word(3)?,
            gunnery: word(4)?,
            tactical: word(5)?,
            engineering: word(6)?,
            command: word(7)?,
        },
        DrifterIntrinsics {
            unspent_points: word(1)? / POINT_SCALE,
            combat: word(8)?,
            riding: word(9)?,
            opportunity: word(10)?,
            endurance: word(11)?,
        },
    )
    .map_err(|_| ReadError::invalid("Intrinsic rank"))
}
