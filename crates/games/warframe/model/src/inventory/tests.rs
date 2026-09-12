use super::{InventoryFamily, InventoryFamilySnapshot, InventoryItemCount, InventorySnapshot};
use crate::{AccountId, ItemKey};

fn inventory(quantity: u64) -> Result<InventorySnapshot, Box<dyn std::error::Error>> {
    let mut families: Vec<_> = InventoryFamily::ALL
        .iter()
        .map(|&family| InventoryFamilySnapshot {
            family,
            items: Vec::new(),
        })
        .collect();
    let resources = families
        .iter_mut()
        .find(|f| f.family == InventoryFamily::MiscItems)
        .ok_or("missing resource family")?;
    resources.items.push(InventoryItemCount {
        item_key: ItemKey::new("/Lotus/Types/Items/Example")?,
        quantity,
    });
    Ok(InventorySnapshot::new(
        AccountId::new("0123456789abcdef01234567")?,
        families,
    )?)
}

#[test]
fn quantities_round_trip_losslessly_and_keys_remain_validated()
-> Result<(), Box<dyn std::error::Error>> {
    let original = inventory(u64::MAX)?;
    let value = serde_json::to_value(&original)?;
    assert_eq!(value["account_id"], original.account_id().as_str());
    let resources = value["families"]
        .as_array()
        .ok_or("missing families")?
        .iter()
        .find(|f| f["family"] == "MiscItems")
        .ok_or("missing resources")?;
    assert_eq!(resources["items"][0]["quantity"], u64::MAX.to_string());
    assert_eq!(
        serde_json::from_value::<InventorySnapshot>(value)?,
        original
    );
    assert_eq!(
        original.quantity(
            InventoryFamily::MiscItems,
            &ItemKey::new("/Lotus/Types/Items/Example")?
        ),
        u64::MAX
    );
    for invalid in [
        "/Lotus/",
        "/Lotus//Item",
        "/Lotus/Item/",
        "/Lotus/Item\0",
        "/Other/Item",
    ] {
        assert!(serde_json::from_value::<ItemKey>(serde_json::json!(invalid)).is_err());
    }
    Ok(())
}

#[test]
fn incomplete_or_noncanonical_payloads_are_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let sample = inventory(7)?;
    let original = serde_json::to_value(&sample)?;
    let (_, families) = sample.clone().into_parts();
    assert_ne!(
        sample,
        InventorySnapshot::new(AccountId::new("abcdef0123456789abcdef01")?, families)?
    );
    let mut invalid_owner = original.clone();
    invalid_owner["account_id"] = serde_json::json!("invalid");
    assert!(serde_json::from_value::<InventorySnapshot>(invalid_owner).is_err());
    let mut anonymous = original.clone();
    anonymous
        .as_object_mut()
        .ok_or("missing snapshot")?
        .remove("account_id");
    assert!(serde_json::from_value::<InventorySnapshot>(anonymous).is_err());
    for count in [
        serde_json::json!(7),
        serde_json::json!("0"),
        serde_json::json!("07"),
        serde_json::json!("+7"),
        serde_json::json!("18446744073709551616"),
    ] {
        let mut value = original.clone();
        let families = value["families"].as_array_mut().ok_or("missing families")?;
        let resources = families
            .iter_mut()
            .find(|f| f["family"] == "MiscItems")
            .ok_or("missing resources")?;
        resources["items"][0]["quantity"] = count;
        assert!(serde_json::from_value::<InventorySnapshot>(value).is_err());
    }
    let mut missing = original.clone();
    missing["families"]
        .as_array_mut()
        .ok_or("missing families")?
        .pop();
    assert!(serde_json::from_value::<InventorySnapshot>(missing).is_err());
    let mut duplicate = original;
    let families = duplicate["families"]
        .as_array_mut()
        .ok_or("missing families")?;
    let resources = families
        .iter_mut()
        .find(|f| f["family"] == "MiscItems")
        .ok_or("missing resources")?;
    let items = resources["items"].as_array_mut().ok_or("missing items")?;
    items.push(items[0].clone());
    assert!(serde_json::from_value::<InventorySnapshot>(duplicate).is_err());
    Ok(())
}
