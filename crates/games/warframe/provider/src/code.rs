//! Small bounded instruction checks shared by progression layouts.
use memory_reader::ProcessMemory;
use provider_sdk::memory::{ObjectOffset, ReadError, Rva, TargetReader};

pub(crate) fn require_bytes(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
    at: Rva,
    expected: &[u8],
    location: &'static str,
) -> Result<(), ReadError> {
    let mut actual = vec![0; expected.len()];
    reader.read_module(at, &mut actual)?;
    if actual != expected {
        return Err(ReadError::layout(location));
    }
    Ok(())
}

pub(crate) fn require_offset(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
    at: Rva,
    instruction: &[u8],
    offset: ObjectOffset,
    location: &'static str,
) -> Result<(), ReadError> {
    let mut expected = instruction.to_vec();
    expected.extend(offset.get().to_le_bytes());
    require_bytes(reader, at, &expected, location)
}
