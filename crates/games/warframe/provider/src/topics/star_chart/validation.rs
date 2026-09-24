use super::facts::{GETTER, MISSIONS_NAME, SERIALIZER_REFERENCE, VECTOR};
use crate::{
    code::{require_bytes, require_offset},
    target::READ_LIMITS,
};
use memory_reader::ProcessMemory;
use provider_sdk::memory::{ReadError, Rva, TargetReader};

pub(crate) fn validate_star_chart_layout(
    memory: &mut (impl ProcessMemory + ?Sized),
    base: u64,
    image_size: u32,
) -> Result<(), ReadError> {
    let mut reader = TargetReader::new(memory, base, image_size, READ_LIMITS)?;
    require_offset(
        &mut reader,
        GETTER,
        &[0x48, 0x8d, 0x81],
        VECTOR,
        "mission progress getter",
    )?;
    require_bytes(
        &mut reader,
        Rva::new(0x006f_ace7),
        &[0xc3],
        "embedded mission progress",
    )?;
    require_offset(
        &mut reader,
        SERIALIZER_REFERENCE,
        &[0x49, 0x8d, 0x8c, 0x24],
        VECTOR,
        "Missions serializer field",
    )?;
    let reference = Rva::new(0x003d_3782);
    let code = reader.read_module_array::<7>(reference)?;
    if code[..3] != [0x48, 0x8d, 0x15] || reference.rip_target(7, &code[3..]) != Some(MISSIONS_NAME)
    {
        return Err(ReadError::layout("Missions serializer name reference"));
    }
    require_bytes(
        &mut reader,
        MISSIONS_NAME,
        b"Missions\0",
        "Missions serializer name",
    )?;
    require_bytes(
        &mut reader,
        Rva::new(0x0050_b56b),
        &[
            0x8b, 0x0f, 0x0f, 0x1f, 0x00, 0x39, 0x08, 0x74, 0x28, 0x48, 0x83, 0xc0, 0x30,
        ],
        "mission tag lookup and stride",
    )?;
    require_bytes(
        &mut reader,
        Rva::new(0x0050_b5bb),
        &[
            0x8b, 0x07, 0x48, 0x8d, 0x57, 0x10, 0x89, 0x03, 0x48, 0x8d, 0x4b, 0x10, 0x8b, 0x47,
            0x04, 0x89, 0x43, 0x04, 0x0f, 0xb6, 0x47, 0x08, 0x88, 0x43, 0x08,
        ],
        "mission progress fields",
    )?;
    require_bytes(
        &mut reader,
        Rva::new(0x0050_b59c),
        &[
            0xff, 0x40, 0x04, 0x0f, 0xb6, 0x4f, 0x08, 0x3a, 0x48, 0x08, 0x76, 0x46, 0x88, 0x48,
            0x08,
        ],
        "completion count and highest tier update",
    )?;
    // The mastery calculator credits the Normal path for a retained record,
    // and additionally credits Steel Path when its tier is at least one.
    require_bytes(
        &mut reader,
        Rva::new(0x0089_55bf),
        &[
            0x8d, 0x45, 0x01, 0xa8, 0xfe, 0x75, 0x03, 0x41, 0x03, 0xf8, 0x80, 0x7b, 0x08, 0x01,
            0x72, 0x0a, 0x8d, 0x45, 0x01, 0xa8, 0xfd, 0x75, 0x03, 0x41, 0x03, 0xf0, 0x41, 0x8b,
            0x46, 0x08, 0x48, 0x83, 0xc3, 0x30,
        ],
        "Normal and Steel Path completion semantics",
    )
}
