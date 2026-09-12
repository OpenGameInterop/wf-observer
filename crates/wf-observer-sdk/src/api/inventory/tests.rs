use super::WarframeInventory;
use crate::api::{DataEnvelope, EnvelopeMetadata, SessionRef, TopicRef, TopicSource};
use crate::warframe as model;

const ACCOUNT_ID: &str = "0123456789abcdef01234567";

fn envelope() -> anyhow::Result<DataEnvelope> {
    let item_key = model::ItemKey::new("/Lotus/Types/Items/Test")?;
    let families = model::InventoryFamily::ALL
        .iter()
        .map(|&family| model::InventoryFamilySnapshot {
            family,
            items: if family == model::InventoryFamily::MiscItems {
                vec![model::InventoryItemCount {
                    item_key: item_key.clone(),
                    quantity: u64::MAX,
                }]
            } else {
                vec![]
            },
        })
        .collect();
    Ok(DataEnvelope {
        metadata: EnvelopeMetadata {
            source: TopicSource {
                session: SessionRef {
                    run_id: "run".into(),
                    session_id: "session".into(),
                },
                game_id: "warframe".into(),
                topic: TopicRef {
                    provider_id: "opengameinterop.warframe".into(),
                    topic: "warframe.inventory".into(),
                    schema_version: 1,
                },
            },
            generation: u64::MAX.to_string(),
            sequence: u64::MAX.to_string(),
        },
        payload_json: serde_json::to_string(&model::InventorySnapshot::new(
            model::AccountId::new(ACCOUNT_ID)?,
            families,
        )?)?,
    })
}

#[test]
fn typed_decoding_preserves_exact_counts_and_rejects_wrong_identity_or_schema() -> anyhow::Result<()>
{
    let original = envelope()?;
    let inventory = WarframeInventory::try_from(original.clone())?;
    assert_eq!(inventory.metadata, original.metadata);
    assert_eq!(inventory.account_id, ACCOUNT_ID);
    let items: Vec<_> = inventory
        .families
        .iter()
        .flat_map(|family| &family.items)
        .collect();
    assert_eq!(items[0].quantity, u64::MAX);
    for case in 0..6 {
        let mut bad = original.clone();
        match case {
            0 => bad.metadata.source.topic.schema_version += 1,
            1 => bad.metadata.source.topic.provider_id = "unrelated".into(),
            2 => bad.metadata.source.game_id = "unrelated".into(),
            3 => {
                bad.payload_json =
                    serde_json::json!({"account_id": ACCOUNT_ID, "families": []}).to_string();
            }
            4 => bad.payload_json = bad.payload_json.replace(ACCOUNT_ID, "invalid"),
            _ => bad.metadata.sequence = "01".into(),
        }
        assert!(WarframeInventory::from_envelope(bad).is_err());
    }
    Ok(())
}
