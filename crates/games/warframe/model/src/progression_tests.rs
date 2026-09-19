use crate::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;
fn account() -> Result<AccountId, InvalidAccountId> {
    AccountId::new("0123456789abcdef01234567")
}

#[test]
fn intrinsic_ranks_validate_through_json_including_every_branch() -> TestResult {
    let snapshot = IntrinsicsSnapshot::new(
        account()?,
        RailjackIntrinsics::default(),
        DrifterIntrinsics::default(),
    )?;
    let original = serde_json::to_value(&snapshot)?;
    assert_eq!(
        serde_json::from_value::<IntrinsicsSnapshot>(original.clone())?,
        snapshot
    );
    for (group, fields) in [
        (
            "railjack",
            &["piloting", "gunnery", "tactical", "engineering", "command"][..],
        ),
        (
            "drifter",
            &["combat", "riding", "opportunity", "endurance"][..],
        ),
    ] {
        for field in fields {
            let mut json = original.clone();
            json[group][field] = serde_json::json!(10);
            assert!(serde_json::from_value::<IntrinsicsSnapshot>(json.clone()).is_ok());
            for invalid in [
                serde_json::json!(11),
                serde_json::json!(-1),
                serde_json::Value::Null,
            ] {
                json[group][field] = invalid;
                assert!(serde_json::from_value::<IntrinsicsSnapshot>(json.clone()).is_err());
            }
            json[group]
                .as_object_mut()
                .ok_or("missing group")?
                .remove(*field);
            assert!(serde_json::from_value::<IntrinsicsSnapshot>(json).is_err());
        }
    }
    let mut json = original;
    json["railjack"]["unspent_points"] = serde_json::json!(u32::MAX);
    assert!(serde_json::from_value::<IntrinsicsSnapshot>(json).is_ok());
    Ok(())
}

#[test]
fn star_chart_preserves_credit_separate_from_counts_and_metadata() -> TestResult {
    let snapshot = StarChartSnapshot::new(
        account()?,
        vec![
            StarChartNodeProgress {
                node_key: "JunctionA".into(),
                completions: 0,
                steel_path_completed: false,
            },
            StarChartNodeProgress {
                node_key: "SolNode1".into(),
                completions: 3,
                steel_path_completed: true,
            },
        ],
    )?;
    assert!(snapshot.completed("JunctionA", StarChartDifficulty::Normal));
    assert!(!snapshot.completed("JunctionA", StarChartDifficulty::SteelPath));
    assert!(snapshot.completed("SolNode1", StarChartDifficulty::Normal));
    assert!(snapshot.completed("SolNode1", StarChartDifficulty::SteelPath));
    assert!(!snapshot.completed("SolNode999", StarChartDifficulty::Normal));
    let original = serde_json::to_value(&snapshot)?;
    assert_eq!(
        serde_json::from_value::<StarChartSnapshot>(original.clone())?,
        snapshot
    );
    for key in ["", "bad tag", "\n", "é"] {
        let mut json = original.clone();
        json["nodes"][0]["node_key"] = serde_json::json!(key);
        assert!(serde_json::from_value::<StarChartSnapshot>(json).is_err());
    }
    for case in 0..3 {
        let mut json = original.clone();
        match case {
            0 => json["nodes"][1]["node_key"] = json["nodes"][0]["node_key"].clone(),
            1 => json["nodes"]
                .as_array_mut()
                .ok_or("missing nodes")?
                .reverse(),
            _ => {
                json["nodes"][0]
                    .as_object_mut()
                    .ok_or("missing node")?
                    .remove("steel_path_completed");
            }
        }
        assert!(serde_json::from_value::<StarChartSnapshot>(json).is_err());
    }
    assert!(
        StarChartSnapshot::new(account()?, vec![])?
            .nodes()
            .is_empty()
    );
    Ok(())
}
