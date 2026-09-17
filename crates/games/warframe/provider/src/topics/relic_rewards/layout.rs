//! Instruction witnesses for the current build; failures are cached per topic.

use memory_reader::ProcessMemory;
use provider_sdk::memory::{ReadError, Rva, TargetReader};

const OPEN: Rva = Rva::new(0x014e_78b0);
const COPY: Rva = Rva::new(0x0061_afe0);
const STRING_COPY: Rva = Rva::new(0x004b_9060);
const SERIALIZER: Rva = Rva::new(0x00f8_2860);
const STORE_ITEM_TYPE: Rva = Rva::new(0x029f_1450);

pub(crate) fn validate(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
) -> Result<(), ReadError> {
    crate::world::validate_rules(reader)?;
    if read::<15>(reader, OPEN, 0x57)?
        != [
            0x40, 0x38, 0x68, 0x50, 0x75, 0x0e, 0x48, 0x83, 0xc0, 0x70, 0x48, 0x3b, 0xc2, 0x75,
            0xf1,
        ]
        || read::<7>(reader, OPEN, 0xac)? != [0x48, 0x8d, 0x83, 0x70, 0x18, 0, 0]
    {
        return Err(ReadError::layout("reward entry loop and vector"));
    }
    let call = read::<5>(reader, COPY, 0x10)?;
    if call[0] != 0xe8 || COPY.rip_target(0x15, &call[1..]) != Some(STRING_COPY) {
        return Err(ReadError::layout("reward first string copy"));
    }
    for (offset, field) in [(0x15, 0x10), (0x22, 0x20), (0x2f, 0x30)] {
        let code = read::<13>(reader, COPY, offset)?;
        if code[..9] != [0x48, 0x8d, 0x53, field, 0x48, 0x8d, 0x4f, field, 0xe8]
            || COPY.rip_target(offset + 13, &code[9..]) != Some(STRING_COPY)
        {
            return Err(ReadError::layout("reward string fields"));
        }
    }
    if read::<19>(reader, COPY, 0x4a)?
        != [
            0x48, 0x89, 0x4f, 0x48, 0x48, 0x8b, 0x43, 0x48, 0x48, 0x89, 0x47, 0x48, 0x0f, 0xb6,
            0x43, 0x50, 0x88, 0x47, 0x50,
        ]
    {
        return Err(ReadError::layout("reward item and qualification fields"));
    }
    if reader.read_module_array::<29>(STRING_COPY)?
        != [
            0x48, 0x89, 0x5c, 0x24, 0x10, 0x48, 0x89, 0x74, 0x24, 0x18, 0x57, 0x48, 0x83, 0xec,
            0x20, 0x48, 0x8b, 0xf2, 0x48, 0x8b, 0xf9, 0x48, 0x0f, 0xbe, 0x52, 0x0f, 0x80, 0xfa,
            0xff,
        ]
    {
        return Err(ReadError::layout("reward native string representation"));
    }
    let code = read::<14>(reader, SERIALIZER, 0x110)?;
    if code[..10] != [0x48, 0x8d, 0x4f, 0x48, 0x48, 0x8b, 0xd3, 0x4c, 0x8d, 5]
        || SERIALIZER.rip_target(0x11e, &code[10..]) != Some(STORE_ITEM_TYPE)
    {
        return Err(ReadError::layout("reward StoreItem serializer"));
    }
    Ok(())
}

fn read<const N: usize>(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
    function: Rva,
    offset: u32,
) -> Result<[u8; N], ReadError> {
    reader.read_module_array(
        function
            .checked_add(offset)
            .ok_or(ReadError::overflow("reward instruction"))?,
    )
}
