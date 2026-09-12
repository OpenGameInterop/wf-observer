use super::{Server, start};
use crate::{
    runtime,
    service::{PollBatch, ServiceState},
};
use anyhow::Context as _;
use protocol::v1 as wire;
use provider_sdk::{EventSink as _, HealthSink as _, Provider};
use provider_warframe::WarframeProvider;
use std::time::Duration;
use tokio::time::{Instant, timeout};
use wf_observer_sdk as sdk;
use wf_observer_sdk::raw::{
    Client,
    warframe::{
        AccountId, InventoryFamily, InventoryFamilySnapshot, InventoryItemCount, InventorySnapshot,
        ItemKey, decode_inventory,
    },
};

const ACCOUNT_ID: &str = "0123456789abcdef01234567";
const OTHER_ACCOUNT_ID: &str = "abcdef0123456789abcdef01";

struct TestWarframe {
    state: ServiceState,
    server: Server,
}

impl TestWarframe {
    async fn start() -> anyhow::Result<Self> {
        let manifest = WarframeProvider.manifest();
        let state = ServiceState::new(&[manifest])?;
        state.update_lifecycle(runtime::HostStatus {
            discovery_error: None,
            targets: vec![runtime::TargetStatus {
                target: runtime::TargetInfo {
                    process: runtime::RecordedProcess {
                        pid: 1,
                        start_marker: 1,
                    },
                    executable: "Warframe.x64.exe".into(),
                    provider_id: manifest.id.into(),
                    game_id: manifest.game.id.into(),
                },
                activity: runtime::Activity::Observing {
                    session_id: "inventory".into(),
                },
            }],
        })?;
        let server = start(iroh::SecretKey::generate(), state.view()).await?;
        Ok(Self { state, server })
    }

    fn ticket(&self) -> String {
        iroh_tickets::endpoint::EndpointTicket::new(self.server.endpoint().addr()).to_string()
    }

    fn publish(
        &self,
        account_id: &str,
        quantity: u64,
        reset: bool,
        available: bool,
    ) -> anyhow::Result<()> {
        let payload = available
            .then(|| -> anyhow::Result<_> {
                Ok(serde_json::to_value(inventory(account_id, quantity)?)?)
            })
            .transpose()?;
        self.publish_payload("warframe.inventory", payload, reset)
    }

    fn publish_payload(
        &self,
        topic: &str,
        payload: Option<serde_json::Value>,
        reset: bool,
    ) -> anyhow::Result<()> {
        let manifest = WarframeProvider.manifest();
        let cap = manifest
            .capabilities
            .iter()
            .find(|cap| cap.topic == topic)
            .context("missing capability")?;
        let ticket = self.state.ticket("inventory").context("missing session")?;
        let batch = PollBatch::new(manifest);
        batch.game_build(Some("test-build"))?;
        if reset {
            batch.events().reset(cap)?;
        }
        if let Some(payload) = payload {
            batch.events().snapshot(cap, &payload)?;
        } else {
            batch.health().update(
                cap,
                provider_sdk::CapabilityHealth::Unavailable(
                    provider_sdk::UnavailableReason::TargetNotReady,
                ),
            )?;
        }
        assert!(self.state.commit_poll(
            &ticket,
            batch,
            Instant::now() + Duration::from_secs(30)
        )?);
        Ok(())
    }

    async fn close(self) -> anyhow::Result<()> {
        self.state.shutdown();
        self.server.shutdown().await
    }
}

async fn next_snapshot(
    sub: &wf_observer_sdk::raw::Subscription,
) -> anyhow::Result<wire::DataEnvelope> {
    timeout(Duration::from_secs(5), async {
        loop {
            if let wf_observer_sdk::raw::SubscriptionItem::State(state) =
                sub.next().await?.context("snapshot listener ended")?
                && let Some(snapshot) = state.topics.iter().find_map(|topic| topic.snapshot.clone())
            {
                break anyhow::Ok(snapshot);
            }
        }
    })
    .await?
}

fn inventory(account_id: &str, quantity: u64) -> anyhow::Result<InventorySnapshot> {
    let mut families: Vec<_> = InventoryFamily::ALL
        .iter()
        .map(|&family| InventoryFamilySnapshot {
            family,
            items: vec![],
        })
        .collect();
    families
        .iter_mut()
        .find(|f| f.family == InventoryFamily::MiscItems)
        .context("missing resources")?
        .items = vec![
        InventoryItemCount {
            item_key: ItemKey::new("/Lotus/Types/Items/MiscItems/AlloyPlate")?,
            quantity,
        },
        InventoryItemCount {
            item_key: ItemKey::new("/Lotus/Types/Items/MiscItems/OrokinCell")?,
            quantity: 5,
        },
    ];
    Ok(InventorySnapshot::new(
        AccountId::new(account_id)?,
        families,
    )?)
}

#[tokio::test]
async fn inventory_payload_reaches_raw_and_concrete_sdk_clients() -> anyhow::Result<()> {
    let test = TestWarframe::start().await?;
    let client = Client::connect(test.server.endpoint().addr()).await?;
    let sub = client
        .subscribe_inventory(wire::SessionSelector::All)
        .await?;
    test.publish(ACCOUNT_ID, u64::MAX, false, true)?;
    let data = decode_inventory(next_snapshot(&sub).await?)?;
    assert_eq!(data.data, inventory(ACCOUNT_ID, u64::MAX)?);
    let cached = client
        .inventory_snapshot(&data.metadata.source.session)
        .await?;
    assert_eq!(cached.metadata, data.metadata);
    assert_eq!(cached.data, data.data);

    let concrete = sdk::connect(test.ticket())
        .await
        .map_err(anyhow::Error::msg)?;
    let concrete_inventory = concrete.warframe().single_session().await?.inventory();
    let concrete_sub = concrete_inventory.watch().await?;
    assert_eq!(
        concrete_inventory
            .cached()
            .await?
            .context("missing inventory cache")?,
        sdk::WarframeInventory::from(data.clone())
    );
    let decoded = timeout(Duration::from_secs(5), async {
        loop {
            if let sdk::InventoryState::Ready { value } = concrete_sub
                .next()
                .await?
                .context("concrete inventory listener ended")?
            {
                break anyhow::Ok(value);
            }
        }
    })
    .await??;
    assert_eq!(decoded.account_id, ACCOUNT_ID);
    assert_eq!(decoded, sdk::WarframeInventory::from(data.clone()));
    // Both API layers reconstruct changed inventory before exposing typed values.
    for quantity in [7, u64::MAX - 1] {
        test.publish(ACCOUNT_ID, quantity, false, true)?;
        assert_eq!(
            decode_inventory(next_snapshot(&sub).await?)?.data,
            inventory(ACCOUNT_ID, quantity)?
        );
        timeout(Duration::from_secs(5), async {
            loop {
                if let Some(sdk::InventoryState::Ready { value: actual }) =
                    concrete_sub.next().await?
                {
                    assert_eq!(
                        actual.families,
                        sdk::WarframeInventory::from(wf_observer_sdk::raw::TypedData {
                            metadata: data.metadata.clone(),
                            data: inventory(ACCOUNT_ID, quantity)?,
                        })
                        .families
                    );
                    break anyhow::Ok(());
                }
            }
        })
        .await??;
    }
    test.publish(OTHER_ACCOUNT_ID, u64::MAX, true, true)?;
    let replacement = client
        .inventory_snapshot(&data.metadata.source.session)
        .await?;
    assert_eq!(replacement.data.account_id().as_str(), OTHER_ACCOUNT_ID);
    assert!(replacement.metadata.generation > data.metadata.generation);
    assert_eq!(
        concrete_inventory
            .cached()
            .await?
            .context("missing inventory cache")?,
        sdk::WarframeInventory::from(replacement)
    );
    concrete_sub.shutdown().await?;
    concrete.shutdown().await.map_err(anyhow::Error::msg)?;
    sub.close();
    client.close().await;
    test.close().await
}
