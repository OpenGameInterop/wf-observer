use memory_reader::ProcessMemory;
use provider_sdk::memory::{ReadError, Rva, TargetReader};
use warframe_model::InventoryFamily;

use crate::{roots::LOGIN, target::READ_LIMITS};

use super::{
    InventoryError,
    facts::{
        INVENTORY, InventoryFamilyFacts, InventoryRecordCountFacts, InventoryRecordFacts,
        InventoryRegistrationFacts,
    },
};

const MAX_FIELD_NAME_BYTES: usize = 32;

/// Checks the inventory getter, coherence fields, family registrations and record layouts.
/// Shared login, item-type and string-pool layouts are checked by the session.
pub(crate) fn validate_inventory_layout(
    memory: &mut (impl ProcessMemory + ?Sized),
    module_base: u64,
    image_size: u32,
) -> Result<(), InventoryError> {
    let mut reader = TargetReader::new(memory, module_base, image_size, READ_LIMITS)?;
    let getter = reader.read_module_array::<8>(INVENTORY.getter)?;
    if getter[..3] != [0x48, 0x8d, 0x81]
        || getter[3..7] != INVENTORY.offset.get().to_le_bytes()
        || getter[7] != 0xc3
    {
        return Err(ReadError::layout("inventory getter").into());
    }
    validate_commit_fields(&mut reader)?;
    for family in INVENTORY.families {
        validate_vector_reference(&mut reader, family)?;
        validate_registration(&mut reader, family)?;
        validate_record(&mut reader, family.family, family.record)?;
    }
    Ok(())
}

fn validate_commit_fields(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
) -> Result<(), InventoryError> {
    let facts = INVENTORY.commit;
    let constructor = reader.read_module_array::<14>(facts.constructor_vtable)?;
    // lea rax, [profile-data vtable]; lea rcx, [rdi+28h]; mov [rdi], rax.
    if constructor[..3] != [0x48, 0x8d, 0x05]
        || facts.constructor_vtable.rip_target(7, &constructor[3..7])
            != Some(LOGIN.profile_data.vtable)
        || constructor[7..] != [0x48, 0x8d, 0x4f, 0x28, 0x48, 0x89, 0x07]
    {
        return Err(ReadError::layout("inventory coherence owner").into());
    }
    let init = reader.read_module_array::<7>(facts.force_update_init)?;
    let read = reader.read_module_array::<7>(facts.force_update_read)?;
    // The same profile-data byte is initialized and read as an unsigned byte.
    if init[..3] != [0x44, 0x88, 0xa7]
        || init[3..] != INVENTORY.force_update.get().to_le_bytes()
        || read[..3] != [0x0f, 0xb6, 0x90]
        || read[3..] != INVENTORY.force_update.get().to_le_bytes()
    {
        return Err(ReadError::layout("inventory rebuild flag").into());
    }
    let sync = INVENTORY.sync_tokens.get();
    let init = reader.read_module_array::<26>(facts.sync_tokens_init)?;
    // Two qword+dword stores initialize the adjacent 12-byte fields in that constructor.
    if init[..3] != [0x48, 0x89, 0x87]
        || init[3..7] != sync.to_le_bytes()
        || init[7..9] != [0x89, 0x87]
        || init[9..13] != (sync + 8).to_le_bytes()
        || init[13..16] != [0x48, 0x89, 0x87]
        || init[16..20] != (sync + 12).to_le_bytes()
        || init[20..22] != [0x89, 0x87]
        || init[22..] != (sync + 20).to_le_bytes()
    {
        return Err(ReadError::layout("inventory synchronization fields").into());
    }
    let compare = reader.read_module_array::<34>(facts.sync_tokens_compare)?;
    let call = facts
        .sync_tokens_compare
        .checked_add(29)
        .ok_or_else(|| ReadError::layout("inventory synchronization comparison"))?;
    // lea rdi, [rcx+second]; movzx esi, dl; lea rbx, [rcx+first];
    // compare(first, second, 12). Both addresses use the same profile-data owner.
    if compare[..3] != [0x48, 0x8d, 0xb9]
        || compare[3..7] != (sync + 12).to_le_bytes()
        || compare[7..13] != [0x0f, 0xb6, 0xf2, 0x48, 0x8d, 0x99]
        || compare[13..17] != sync.to_le_bytes()
        || compare[17..30]
            != [
                0x48, 0x8b, 0xd7, 0x48, 0x8b, 0xcb, 0x41, 0xb8, 0x0c, 0, 0, 0, 0xe8,
            ]
        || call.rip_target(5, &compare[30..]) != Some(facts.compare_bytes)
    {
        return Err(ReadError::layout("inventory synchronization comparison").into());
    }
    Ok(())
}

fn validate_vector_reference(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
    facts: &InventoryFamilyFacts,
) -> Result<(), InventoryError> {
    let mut code = [0_u8; 7];
    reader.read_module(facts.vector_reference, &mut code)?;
    let offset = facts.vector.get();
    let valid = if offset == 0 {
        code[..3] == [0x48, 0x8b, 0xd1] || code[..3] == [0x48, 0x8b, 0xd6]
    } else if let Ok(offset) = i8::try_from(offset) {
        code[..4] == [0x48, 0x8d, 0x56, offset.cast_unsigned()]
    } else {
        code[..3] == [0x48, 0x8d, 0x96] && code[3..7] == offset.to_le_bytes()
    };
    if valid {
        Ok(())
    } else {
        Err(InventoryError::UnsupportedLayout(
            facts.family,
            "inventory family vector",
        ))
    }
}

fn validate_registration(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
    facts: &InventoryFamilyFacts,
) -> Result<(), InventoryError> {
    match facts.registration {
        InventoryRegistrationFacts::Named {
            field,
            name_reference,
            name,
        } => validate_named_registration(reader, facts, field, name_reference, name),
        InventoryRegistrationFacts::Direct { serializer_call } => {
            let mut code = [0_u8; 5];
            reader.read_module(serializer_call, &mut code)?;
            let follows_vector = serializer_call
                .get()
                .checked_sub(facts.vector_reference.get())
                .is_some_and(|distance| (7..=32).contains(&distance));
            if follows_vector
                && code[0] == 0xe8
                && serializer_call.rip_target(5, &code[1..]) == Some(facts.record.serializer)
            {
                Ok(())
            } else {
                Err(InventoryError::UnsupportedLayout(
                    facts.family,
                    "inventory family registration",
                ))
            }
        }
    }
}

fn validate_named_registration(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
    facts: &InventoryFamilyFacts,
    expected_name: &str,
    name_reference: Rva,
    name_rva: Rva,
) -> Result<(), InventoryError> {
    let mut name = [0_u8; MAX_FIELD_NAME_BYTES];
    reader.read_module(name_rva, &mut name)?;
    if name.get(..expected_name.len()) != Some(expected_name.as_bytes())
        || name.get(expected_name.len()) != Some(&0)
    {
        return Err(InventoryError::UnsupportedLayout(
            facts.family,
            "inventory family name",
        ));
    }

    let error = || InventoryError::UnsupportedLayout(facts.family, "inventory family registration");
    let mut code = reader.read_module_array::<15>(name_reference)?;
    if name_reference.rip_target(7, &code[3..7]) != Some(name_rva) {
        return Err(error());
    }
    let call_base = match code[..3] {
        [0x4c, 0x8d, 0x05] => name_reference,
        [0x48, 0x8d, 0x05] if code[7..] == [0x4c, 0x89, 0x6d, 0xd0, 0x48, 0x89, 0x45, 0xd8] => {
            code = reader.read_module_array(facts.vector_reference)?;
            facts.vector_reference
        }
        _ => return Err(error()),
    };
    let call = call_base
        .checked_add(10)
        .ok_or(InventoryError::UnsupportedLayout(
            facts.family,
            "inventory serializer call",
        ))?;
    if code[7..11] != [0x48, 0x8b, 0xcb, 0xe8]
        || call.rip_target(5, &code[11..]) != Some(facts.record.serializer)
    {
        return Err(error());
    }
    Ok(())
}

fn validate_record(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
    family: InventoryFamily,
    facts: InventoryRecordFacts,
) -> Result<(), InventoryError> {
    match facts.count {
        InventoryRecordCountFacts::Single {
            stride_reader,
            item_type_reader,
        } => {
            let error = || InventoryError::UnsupportedLayout(family, "single-item record");
            let item_type = u8::try_from(facts.item_type.get()).map_err(|_| error())?;
            let mut stride = [0_u8; 7];
            reader.read_module(stride_reader, &mut stride)?;
            let mut item = [0_u8; 7];
            reader.read_module(item_type_reader, &mut item)?;
            let stride_prefix = &stride[..3];
            let proven_stride = stride_prefix == [0x49, 0x8d, 0xae]
                || stride_prefix == [0x49, 0x69, 0xde]
                || stride_prefix == [0x4c, 0x8d, 0xb5];
            if !proven_stride
                || stride[3..] != facts.bytes.to_le_bytes()
                // The equipment serializer passes its ItemType field (rcx = rsi + offset)
                // and the archive (rdx = rdi) to the shared type serializer.
                || item != [0x48, 0x8d, 0x4e, item_type, 0x48, 0x8b, 0xd7]
            {
                return Err(error());
            }
        }
        InventoryRecordCountFacts::AddressEncoded {
            codec,
            stride_reader,
            codec_reader,
        } => {
            let error = || InventoryError::UnsupportedLayout(family, "counted record");
            if facts.item_type.get() != 0
                || codec.check.get().checked_add(4) != Some(codec.stored.get())
                || codec.rotate_left >= u32::BITS
                || codec.address_shift >= u64::BITS
                || !facts.bytes.is_power_of_two()
            {
                return Err(error());
            }
            let shift = u8::try_from(facts.bytes.trailing_zeros()).map_err(|_| error())?;
            let stored = u8::try_from(codec.stored.get()).map_err(|_| error())?;
            let rotate = u8::try_from(codec.rotate_left).map_err(|_| error())?;
            let address_shift = u8::try_from(codec.address_shift).map_err(|_| error())?;

            let mut stride_code = [0_u8; 8];
            reader.read_module(stride_reader, &mut stride_code)?;
            let mut codec_code = [0_u8; 130];
            reader.read_module(codec_reader, &mut codec_code)?;
            if stride_code != [0x8b, 0xcf, 0x33, 0xdb, 0x48, 0xc1, 0xe9, shift]
                || codec_code[..4] != [0x4c, 0x8d, 0x53, stored]
                || codec_code[15..24] != [0x41, 0x8b, 0x0a, 0x49, 0x8b, 0xd2, 0x48, 0xc1, 0xfa]
                || codec_code[24] != address_shift
                || codec_code[25..29] != [0x8b, 0xc2, 0xc1, 0xc1]
                || codec_code[29..31] != [rotate, 0x35]
                || codec_code[31..35] != codec.value_xor.to_le_bytes()
                || codec_code[124..126] != [0x81, 0xf2]
                || codec_code[126..130] != codec.check_xor.to_le_bytes()
            {
                return Err(error());
            }
        }
        InventoryRecordCountFacts::Plain {
            count,
            stride_reader,
            item_type_reader,
            count_reader,
        } => {
            let error = || InventoryError::UnsupportedLayout(family, "plain-count record");
            let stride = u8::try_from(facts.bytes).map_err(|_| error())?;
            let item_type = u8::try_from(facts.item_type.get()).map_err(|_| error())?;
            let count = u8::try_from(count.get()).map_err(|_| error())?;
            let mut stride_code = [0_u8; 4];
            reader.read_module(stride_reader, &mut stride_code)?;
            let mut item_type_code = [0_u8; 3];
            reader.read_module(item_type_reader, &mut item_type_code)?;
            let mut count_code = [0_u8; 4];
            reader.read_module(count_reader, &mut count_code)?;
            let valid_stride = stride_code == [0x48, 0x83, 0xc3, stride]
                || stride_code == [0x4c, 0x8d, 0x56, stride];
            let valid_item_type = item_type == 0
                && (item_type_code == [0x48, 0x8b, 0xce] || item_type_code == [0x48, 0x8b, 0xcb]);
            let valid_count =
                count_code == [0x48, 0x8d, 0x56, count] || count_code == [0x48, 0x8d, 0x53, count];
            if !valid_stride || !valid_item_type || !valid_count {
                return Err(error());
            }
        }
    }
    Ok(())
}
