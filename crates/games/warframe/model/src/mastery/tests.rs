use super::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn sample() -> Result<MasterySnapshot, Box<dyn std::error::Error>> {
    Ok(MasterySnapshot::new(
        AccountId::new("0123456789abcdef01234567")?,
        34,
        u64::MAX,
        0,
        vec![
            MasteryItemProgress {
                item_key: ItemKey::new("/Lotus/A")?,
                affinity: 0,
            },
            MasteryItemProgress {
                item_key: ItemKey::new("/Lotus/B")?,
                affinity: u64::MAX,
            },
        ],
    )?)
}

#[test]
fn progression_round_trips_exactly_and_preserves_zero_and_absence() -> TestResult {
    let snapshot = sample()?;
    let json = serde_json::to_value(&snapshot)?;
    assert_eq!(json["total_points"], u64::MAX.to_string());
    assert_eq!(json["item_points"], "0");
    assert_eq!(json["items"][1]["affinity"], u64::MAX.to_string());
    assert_eq!(serde_json::from_value::<MasterySnapshot>(json)?, snapshot);
    assert_eq!(snapshot.tracked_items(), 2);
    assert_eq!(
        snapshot
            .get(&ItemKey::new("/Lotus/A")?)
            .map(|item| item.affinity),
        Some(0)
    );
    assert!(snapshot.get(&ItemKey::new("/Lotus/C")?).is_none());
    let empty = MasterySnapshot::new(snapshot.account_id().clone(), 0, 0, 0, vec![])?;
    assert_eq!(empty.tracked_items(), 0);
    assert_eq!(
        serde_json::from_str::<MasterySnapshot>(&serde_json::to_string(&empty)?)?,
        empty
    );
    Ok(())
}

#[test]
fn invalid_owner_order_points_and_decimal_encodings_are_rejected() -> TestResult {
    let original = serde_json::to_value(sample()?)?;
    for field in ["account_id", "rank", "total_points", "item_points", "items"] {
        let mut json = original.clone();
        json.as_object_mut().ok_or("missing object")?.remove(field);
        assert!(serde_json::from_value::<MasterySnapshot>(json).is_err());
    }
    for field in ["total_points", "item_points", "affinity"] {
        for value in [
            serde_json::json!(0),
            serde_json::json!("00"),
            serde_json::json!("+1"),
            serde_json::json!("-1"),
            serde_json::json!("18446744073709551616"),
            serde_json::json!(" 1"),
            serde_json::json!("1.0"),
        ] {
            let mut json = original.clone();
            if field == "affinity" {
                json["items"][0][field] = value;
            } else {
                json[field] = value;
            }
            assert!(serde_json::from_value::<MasterySnapshot>(json).is_err());
        }
    }
    for case in 0..5 {
        let mut json = original.clone();
        match case {
            0 => json["account_id"] = serde_json::json!("invalid"),
            1 => json["items"][1]["item_key"] = json["items"][0]["item_key"].clone(),
            2 => json["items"]
                .as_array_mut()
                .ok_or("missing items")?
                .reverse(),
            3 => json["items"][0]["item_key"] = serde_json::json!("/Invalid/Item"),
            _ => {
                json["total_points"] = serde_json::json!("0");
                json["item_points"] = serde_json::json!("1");
            }
        }
        assert!(serde_json::from_value::<MasterySnapshot>(json).is_err());
    }
    Ok(())
}
