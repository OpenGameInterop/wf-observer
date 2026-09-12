//! Checks the descriptor constructor and path builder.

use memory_reader::ProcessMemory;
use provider_sdk::memory::TargetReader;

use super::{ItemTypeError, facts::ItemTypeFacts};

pub(crate) fn validate_item_types(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
    facts: ItemTypeFacts,
) -> Result<(), ItemTypeError> {
    validate_leaf_constructor(reader, facts)?;
    validate_type_pair_builder(reader, facts)
}

fn validate_leaf_constructor(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
    facts: ItemTypeFacts,
) -> Result<(), ItemTypeError> {
    let prefix = u8::try_from(facts.prefix_object.get())
        .map_err(|_| item_layout_error("item type constructor"))?;
    let parent =
        u8::try_from(facts.parent.get()).map_err(|_| item_layout_error("item type constructor"))?;
    let suffix = u8::try_from(facts.suffix_token.get())
        .map_err(|_| item_layout_error("item type constructor"))?;
    let mut code = [0_u8; 62];
    reader.read_module(facts.leaf_constructor, &mut code)?;
    let reference = facts
        .leaf_constructor
        .checked_add(14)
        .ok_or_else(|| item_layout_error("item type constructor"))?;
    if code[..17]
        != [
            0x45, 0x33, 0xc9, 0xc7, 0x41, 0x08, 0xff, 0xff, 0xff, 0xff, 0x44, 0x89, 0x49, 0x0c,
            0x48, 0x8d, 0x05,
        ]
        || reference.rip_target(7, &code[17..21]) != Some(facts.leaf_vtable)
        || code[21..]
            != [
                0x4c, 0x89, 0x49, prefix, 0x44, 0x89, 0x49, 0x24, 0x4c, 0x89, 0x41, parent, 0xc7,
                0x41, 0x20, 0xff, 0xff, 0xff, 0xff, 0x89, 0x51, suffix, 0xc7, 0x41, 0x28, 0x00,
                0x40, 0x00, 0x00, 0x4c, 0x89, 0x49, 0x30, 0x66, 0x83, 0x49, 0x28, 0x08, 0x48, 0x89,
                0x01,
            ]
    {
        return Err(item_layout_error("item type constructor"));
    }
    Ok(())
}

fn validate_type_pair_builder(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
    facts: ItemTypeFacts,
) -> Result<(), ItemTypeError> {
    let prefix = u8::try_from(facts.prefix_object.get())
        .map_err(|_| item_layout_error("item type path builder"))?;
    let suffix = u8::try_from(facts.suffix_token.get())
        .map_err(|_| item_layout_error("item type path builder"))?;
    let mut code = [0_u8; 35];
    reader.read_module(facts.type_pair_builder, &mut code)?;
    if code
        != [
            0x48, 0x8b, 0x41, prefix, 0x48, 0x85, 0xc0, 0x74, 0x0e, 0x8b, 0x00, 0x89, 0x02, 0x8b,
            0x41, suffix, 0x89, 0x42, 0x04, 0x48, 0x8b, 0xc2, 0xc3, 0x89, 0x02, 0x8b, 0x41, suffix,
            0x89, 0x42, 0x04, 0x48, 0x8b, 0xc2, 0xc3,
        ]
    {
        return Err(item_layout_error("item type path builder"));
    }
    Ok(())
}

const fn item_layout_error(location: &'static str) -> ItemTypeError {
    ItemTypeError::Invalid(location)
}
