//! Layout regressions against captured executable bytes, without live account data.
//! Mastery, progression and profile commit markers have their own captured fixtures.
use crate::topics::progression_tests::Image;
use provider_sdk::memory::TargetReader;

#[path = "layout_windows.rs"]
mod observed;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn image(base: u64) -> Image {
    let mut image = Image {
        bytes: std::collections::BTreeMap::new(),
    };
    image.extend(base, observed::WINDOWS);
    // The captured movie vtable contains one absolute code pointer. Apply the
    // same base relocation as the PE loader when testing a different base.
    let method = (base + 0x0178_e190_u64).to_le_bytes();
    image.bytes.extend((base + 0x021e_d830..).zip(method));
    image
}

fn validate(image: &mut Image, base: u64) -> TestResult {
    let size = crate::target::BUILD.image_size;
    crate::roots::validate_account_layout(image, base, size)?;
    crate::roots::validate_profile_data_layout(image, base, size)?;
    crate::topics::validate_player_layout(image, base, size)?;
    crate::topics::validate_inventory_layout(image, base, size)?;
    crate::topics::validate_currencies_layout(image, base, size)?;
    crate::topics::validate_chat_layout(image, base, size)?;
    let mut reader = TargetReader::new(image, base, size, crate::target::READ_LIMITS)?;
    crate::world::validate_client(&mut reader)?;
    crate::world::validate_rules(&mut reader)?;
    crate::topics::screens::validate(&mut reader)?;
    crate::topics::relic_rewards::validate(&mut reader)?;
    crate::string_pool::validate_string_pool_layout(
        &mut reader,
        crate::string_pool::facts::STRINGS,
    )?;
    crate::item_type::validate_item_types(&mut reader, crate::item_type::facts::ITEM_TYPES)?;
    Ok(())
}

#[test]
fn observed_layouts_validate_at_unrelated_module_bases() -> TestResult {
    for base in [0x1_4000_0000, 0x2_8000_0000] {
        validate(&mut image(base), base)?;
    }
    Ok(())
}

#[test]
fn changed_roots_fields_codecs_and_record_witnesses_are_rejected() -> TestResult {
    let base = 0x1_4000_0000;
    validate(&mut image(base), base)?;
    for rva in [
        // Account, profile-data and player getters.
        0x0124_3dd0,
        0x0086_6c20,
        0x0146_0580,
        // Beast/wreckage strides and the counted-stack codec.
        0x0168_1fd4,
        0x013e_d0a4,
        0x01bf_13a0,
        // Named currency identity, Credits/Endo and both Platinum balances.
        0x0285_7180,
        0x0070_1a54,
        0x0135_4149,
        0x0070_1ad3,
        0x0070_1aff,
        0x007e_5224,
        // Channel storage, names, private participants, entries and retention.
        0x00ba_a4ee,
        0x00ba_a508,
        0x00ba_a98d,
        0x00ba_b41d,
        0x00ba_b443,
        0x00ba_b452,
        0x00ba_b45e,
        0x00ba_b46c,
        0x00ba_b47a,
        0x00ba_b4d1,
        // Client, mission owner, visibility, reward stride/vector/string/item.
        0x0001_f7af,
        0x0122_234f,
        0x0178_e190,
        0x0065_e3d0,
        0x0065_e41f,
        0x0133_0fd7,
        0x00e8_36aa,
        // String-pool and item-type decoders.
        0x0071_5f40,
        0x0142_3e40,
    ] {
        let mut image = image(base);
        *image
            .bytes
            .get_mut(&(base + rva))
            .ok_or("missing witness")? ^= 1;
        assert!(
            validate(&mut image, base).is_err(),
            "accepted changed witness {rva:x}"
        );
    }
    Ok(())
}
