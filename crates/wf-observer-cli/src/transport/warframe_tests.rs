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
    Client, Topic,
    warframe::{
        AccountId, ChatChannel, ChatEvent, ChatMessage, ChatTime, ChatTopic, ChatUpdate,
        CurrencyBalances, CurrencySnapshot, InventoryFamily, InventoryFamilySnapshot,
        InventoryItemCount, InventorySnapshot, ItemKey, PlayerSnapshot, decode_chat,
        decode_currencies, decode_inventory, decode_player,
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

    fn publish_currencies(&self, account_id: &str, reset: bool) -> anyhow::Result<()> {
        self.publish_payload(
            "warframe.currencies",
            Some(serde_json::to_value(currencies(account_id)?)?),
            reset,
        )
    }

    fn publish_player(&self, account_id: &str, reset: bool) -> anyhow::Result<()> {
        self.publish_payload(
            "warframe.player",
            Some(serde_json::to_value(PlayerSnapshot {
                account_id: AccountId::new(account_id)?,
                username: "ExamplePlayer".parse()?,
            })?),
            reset,
        )
    }

    fn publish_chat(
        &self,
        account_id: &str,
        update: Option<ChatUpdate>,
        reset: bool,
    ) -> anyhow::Result<()> {
        let manifest = WarframeProvider.manifest();
        let cap = manifest
            .capabilities
            .iter()
            .find(|cap| cap.topic == ChatTopic::NAME)
            .context("missing chat capability")?;
        let ticket = self.state.ticket("inventory").context("missing session")?;
        let batch = PollBatch::new(manifest);
        if reset {
            batch.events().reset(cap)?;
        }
        batch
            .health()
            .update(cap, provider_sdk::CapabilityHealth::Available)?;
        if let Some(update) = update {
            batch.events().event(
                cap,
                &serde_json::to_value(ChatEvent {
                    account_id: AccountId::new(account_id)?,
                    update,
                })?,
            )?;
        }
        assert!(self.state.commit_poll(
            &ticket,
            batch,
            Instant::now() + Duration::from_secs(30)
        )?);
        Ok(())
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

fn chat_message() -> ChatUpdate {
    ChatUpdate::Message {
        value: ChatMessage {
            channel: ChatChannel::Squad,
            sender: Some("ExampleSender".into()),
            text: "Hello <Tenno>".into(),
            game_time: ChatTime::new(23, 59),
        },
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

async fn next_chat_event(
    sub: &wf_observer_sdk::raw::Subscription,
) -> anyhow::Result<wire::EventEnvelope> {
    timeout(Duration::from_secs(5), async {
        loop {
            if let wf_observer_sdk::raw::SubscriptionItem::Event(event) =
                sub.next().await?.context("chat stream ended")?
            {
                break anyhow::Ok(event.as_ref().clone());
            }
        }
    })
    .await?
}

#[tokio::test]
async fn player_and_chat_reach_typed_clients_with_independent_health() -> anyhow::Result<()> {
    let test = TestWarframe::start().await?;
    let client = Client::connect(test.server.endpoint().addr()).await?;
    let player_sub = client.subscribe_player(wire::SessionSelector::All).await?;
    let chat_sub = client.subscribe_chat(wire::SessionSelector::All).await?;
    test.publish_player(ACCOUNT_ID, false)?;
    test.publish_chat(ACCOUNT_ID, None, false)?;
    let player = decode_player(next_snapshot(&player_sub).await?)?;
    assert_eq!(player.data.username.as_str(), "ExamplePlayer");
    assert_eq!(
        client
            .player_snapshot(&player.metadata.source.session)
            .await?
            .data,
        player.data
    );
    let decoded = sdk::WarframePlayer::from_envelope(
        wire::DataEnvelope {
            metadata: player.metadata.clone(),
            payload: wire::JsonPayload::from_value(&serde_json::to_value(&player.data)?)?,
        }
        .into(),
    )?;
    assert_eq!(decoded.username, player.data.username.as_str());
    assert_eq!(decoded.account_id, ACCOUNT_ID);

    timeout(Duration::from_secs(5), async {
        loop {
            if let wf_observer_sdk::raw::SubscriptionItem::State(state) =
                chat_sub.next().await?.context("chat listener ended")?
                && let Some(topic) = state
                    .topics
                    .iter()
                    .find(|topic| topic.health == wire::CapabilityHealth::Available)
            {
                assert!(topic.snapshot.is_none());
                break anyhow::Ok(());
            }
        }
    })
    .await??;
    assert!(matches!(
        client
            .snapshot(&player.metadata.source.session, &ChatTopic::topic())
            .await,
        Err(wf_observer_sdk::raw::ClientError::Request(
            wire::RequestError::SnapshotsUnsupported { .. }
        ))
    ));
    for update in [
        chat_message(),
        ChatUpdate::Gap {
            channel: ChatChannel::Squad,
        },
    ] {
        test.publish_chat(ACCOUNT_ID, Some(update.clone()), false)?;
        let event = next_chat_event(&chat_sub).await?;
        assert_eq!(decode_chat(event.clone())?.data.update, update);
        let concrete = sdk::WarframeChatEvent::from_envelope(event.clone().into())?;
        assert_eq!(concrete.update, update);
        assert_eq!(concrete.account_id, ACCOUNT_ID);
        if matches!(update, ChatUpdate::Message { .. }) {
            let mut bad: sdk::EventEnvelope = event.into();
            bad.payload_json = bad.payload_json.replace("\"hour\":23", "\"hour\":24");
            assert!(sdk::WarframeChatEvent::from_envelope(bad).is_err());
        }
    }
    test.publish_payload(ChatTopic::NAME, None, false)?;
    assert_eq!(
        client
            .player_snapshot(&player.metadata.source.session)
            .await?
            .data,
        player.data
    );
    test.publish_player(OTHER_ACCOUNT_ID, true)?;
    test.publish_chat(OTHER_ACCOUNT_ID, None, true)?;
    let fresh = client
        .player_snapshot(&player.metadata.source.session)
        .await?;
    assert_eq!(fresh.data.account_id.as_str(), OTHER_ACCOUNT_ID);
    assert!(fresh.metadata.generation > player.metadata.generation);
    player_sub.close();
    chat_sub.close();
    client.close().await;
    test.close().await
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

fn currencies(account_id: &str) -> anyhow::Result<CurrencySnapshot> {
    Ok(CurrencySnapshot {
        account_id: AccountId::new(account_id)?,
        balances: CurrencyBalances {
            credits: i32::MAX,
            endo: 0,
            tradable_platinum: -7,
            non_tradable_platinum: 50,
        },
    })
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

#[tokio::test]
async fn currencies_stream_and_cache_are_independent_of_inventory_health() -> anyhow::Result<()> {
    let test = TestWarframe::start().await?;
    let client = Client::connect(test.server.endpoint().addr()).await?;
    let inventory_sub = client
        .subscribe_inventory(wire::SessionSelector::All)
        .await?;
    let sub = client
        .subscribe_currencies(wire::SessionSelector::All)
        .await?;
    test.publish_currencies(ACCOUNT_ID, false)?;
    test.publish(ACCOUNT_ID, 1, false, false)?;
    let data = decode_currencies(next_snapshot(&sub).await?)?;
    assert_eq!(data.data, currencies(ACCOUNT_ID)?);
    let cached = client
        .currencies_snapshot(&data.metadata.source.session)
        .await?;
    assert_eq!(cached.data, data.data);
    assert_eq!(cached.metadata, data.metadata);
    assert!(matches!(
        client
            .inventory_snapshot(&data.metadata.source.session)
            .await,
        Err(wf_observer_sdk::raw::ClientError::Request(
            wire::RequestError::Unavailable { .. }
        ))
    ));
    test.publish_currencies(OTHER_ACCOUNT_ID, true)?;
    let fresh = client
        .currencies_snapshot(&data.metadata.source.session)
        .await?;
    assert_eq!(fresh.data.account_id.as_str(), OTHER_ACCOUNT_ID);
    assert!(fresh.metadata.generation > data.metadata.generation);
    test.publish_payload("warframe.currencies", None, false)?;
    assert!(matches!(
        client
            .currencies_snapshot(&data.metadata.source.session)
            .await,
        Err(wf_observer_sdk::raw::ClientError::Request(
            wire::RequestError::Unavailable { .. }
        ))
    ));
    sub.close();
    inventory_sub.close();
    client.close().await;
    test.close().await
}

async fn demand(test: &TestWarframe, count: usize) -> anyhow::Result<()> {
    timeout(Duration::from_secs(5), async {
        while test.state.subscription_count() != count {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await?;
    Ok(())
}

#[tokio::test]
async fn typed_capabilities_share_demand_and_preserve_current_state() -> anyhow::Result<()> {
    use wf_observer_sdk::TryStreamExt as _;
    use wf_observer_sdk::raw::State;
    let test = TestWarframe::start().await?;
    let client = Client::connect_endpoint(&test.ticket()).await?;
    let session = session_ref(&client).await?;
    let currencies = wf_observer_sdk::raw::Capability::<
        wf_observer_sdk::raw::warframe::CurrenciesTopic,
    >::new(client.clone(), session);
    assert_eq!(test.state.subscription_count(), 0);
    assert!(currencies.cached().await?.is_none());
    let first = currencies.watch().await?;
    let second = currencies.watch().await?;
    demand(&test, 1).await?;
    assert!(matches!(first.next().await?, Some(State::Waiting)));
    test.publish_currencies(ACCOUNT_ID, false)?;
    let Some(State::Ready(value)) = timeout(Duration::from_secs(5), first.next()).await?? else {
        anyhow::bail!("expected typed currency data");
    };
    assert_eq!(value.data, super::warframe_tests::currencies(ACCOUNT_ID)?);
    let State::Ready(current) = first.current()? else {
        anyhow::bail!("current data missing");
    };
    assert!(std::sync::Arc::ptr_eq(&value, &current));
    assert_eq!(currencies.read().await?.data, value.data);
    demand(&test, 1).await?;
    first.close();
    assert!(first.current().is_err());
    assert!(first.next().await?.is_none());
    let mut stream = second.into_stream();
    assert!(matches!(stream.try_next().await?, Some(State::Ready(_))));
    drop(stream);
    demand(&test, 0).await?;
    drop(client);
    // Session/capability handles keep the client alive, without keeping demand alive.
    assert!(currencies.cached().await?.is_none());
    test.state.shutdown();
    test.server.shutdown().await
}

#[tokio::test]
async fn one_shot_reads_acquire_and_release_on_success_timeout_and_cancellation()
-> anyhow::Result<()> {
    use wf_observer_sdk::raw::ClientError;
    let test = TestWarframe::start().await?;
    let client = Client::connect(test.server.endpoint().addr()).await?;
    let currencies = wf_observer_sdk::raw::Capability::<
        wf_observer_sdk::raw::warframe::CurrenciesTopic,
    >::new(client.clone(), session_ref(&client).await?);
    let (value, published) = tokio::join!(currencies.read(), async {
        demand(&test, 1).await?;
        test.publish_currencies(ACCOUNT_ID, false)
    });
    published?;
    assert_eq!(value?.data.account_id.as_str(), ACCOUNT_ID);
    demand(&test, 0).await?;
    assert!(matches!(
        currencies
            .read_with_timeout(Duration::from_millis(40))
            .await,
        Err(ClientError::Timeout)
    ));
    demand(&test, 0).await?;
    let mut read = Box::pin(currencies.read());
    tokio::select! {
        result = &mut read => anyhow::bail!("read unexpectedly completed: {result:?}"),
        result = demand(&test, 1) => result?,
    }
    drop(read);
    demand(&test, 0).await?;
    client.close().await;
    test.state.shutdown();
    test.server.shutdown().await
}

#[tokio::test]
async fn typed_watch_invalidates_on_unavailability_and_reports_termination_once()
-> anyhow::Result<()> {
    use wf_observer_sdk::raw::{ClientError, State};
    let test = TestWarframe::start().await?;
    let client = Client::connect(test.server.endpoint().addr()).await?;
    let inventory = wf_observer_sdk::raw::Capability::<
        wf_observer_sdk::raw::warframe::InventoryTopic,
    >::new(client.clone(), session_ref(&client).await?);
    let watch = inventory.watch().await?;
    watch.next().await?;
    test.publish(ACCOUNT_ID, 7, false, true)?;
    assert!(matches!(watch.next().await?, Some(State::Ready(_))));
    test.publish(ACCOUNT_ID, 0, false, false)?;
    assert!(matches!(watch.next().await?, Some(State::Unavailable(_))));
    assert!(matches!(watch.current()?, State::Unavailable(_)));
    assert!(matches!(
        inventory.read().await,
        Err(ClientError::Request(wire::RequestError::Unavailable { .. }))
    ));
    test.state.shutdown();
    assert!(matches!(watch.next().await, Err(ClientError::Ended(_))));
    assert!(watch.next().await?.is_none());
    assert!(watch.current().is_err());
    client.close().await;
    test.server.shutdown().await
}

#[tokio::test]
async fn typed_reads_and_chat_cross_the_sdk_runtime_boundary() -> anyhow::Result<()> {
    let test = TestWarframe::start().await?;
    let client = sdk::connect(test.ticket()).await?;
    let game = client.warframe().single_session().await?;
    let inventory = game.inventory();
    assert!(inventory.cached().await?.is_none());
    let (value, published) = tokio::join!(inventory.read(), async {
        demand(&test, 1).await?;
        test.publish(ACCOUNT_ID, u64::MAX, false, true)
    });
    published?;
    let value = value?;
    assert_eq!(value.account_id, ACCOUNT_ID);
    assert!(
        value
            .families
            .iter()
            .flat_map(|family| &family.items)
            .any(|item| item.quantity == u64::MAX)
    );
    demand(&test, 0).await?;
    let chat = game.chat().watch().await?;
    assert!(matches!(
        chat.next().await?,
        Some(sdk::ChatObservation::State { .. })
    ));
    test.publish_chat(
        ACCOUNT_ID,
        Some(ChatUpdate::Gap {
            channel: ChatChannel::Trade,
        }),
        false,
    )?;
    loop {
        if let Some(sdk::ChatObservation::Gap {
            account_id,
            channel,
            ..
        }) = timeout(Duration::from_secs(5), chat.next()).await??
        {
            assert_eq!(account_id, ACCOUNT_ID);
            assert_eq!(channel, ChatChannel::Trade);
            break;
        }
    }
    chat.cancel();
    chat.shutdown().await?;
    assert!(chat.current().is_err());
    assert!(chat.next().await?.is_none());
    client.shutdown().await?;
    test.state.shutdown();
    test.server.shutdown().await
}

async fn session_ref(client: &Client) -> anyhow::Result<wire::SessionRef> {
    let state = client.status().await?;
    Ok(wire::SessionRef {
        run_id: state.cursor.run_id,
        session_id: "inventory".into(),
    })
}

#[tokio::test]
async fn shared_sdk_selects_sessions_explicitly_and_never_retargets_stale_handles()
-> anyhow::Result<()> {
    use wf_observer_sdk::{ObserverError, RequestError};
    let test = TestWarframe::start().await?;
    let client = wf_observer_sdk::connect(test.ticket()).await?;
    let scope = client.warframe();
    let game = scope.single_session().await?;
    let target = |id: u32, session_id: &str| runtime::TargetStatus {
        target: runtime::TargetInfo {
            process: runtime::RecordedProcess {
                pid: id,
                start_marker: 1,
            },
            executable: "Warframe.x64.exe".into(),
            provider_id: "opengameinterop.warframe".into(),
            game_id: "warframe".into(),
        },
        activity: runtime::Activity::Observing {
            session_id: session_id.into(),
        },
    };
    test.state.update_lifecycle(runtime::HostStatus {
        discovery_error: None,
        targets: vec![target(1, "inventory"), target(2, "second")],
    })?;
    assert!(matches!(
        scope.single_session().await,
        Err(ObserverError::AmbiguousSession)
    ));
    let sessions = scope.sessions().await?;
    assert_eq!(sessions.len(), 2);
    assert_eq!(scope.session(sessions[0].clone())?.info(), sessions[0]);
    assert_eq!(test.state.subscription_count(), 0);
    test.state.update_lifecycle(runtime::HostStatus {
        discovery_error: None,
        targets: vec![target(2, "second")],
    })?;
    assert!(matches!(
        game.currencies().read().await,
        Err(ObserverError::Request {
            error: RequestError::UnknownSession { .. }
        })
    ));
    test.state.update_lifecycle(runtime::HostStatus {
        discovery_error: None,
        targets: vec![],
    })?;
    assert!(matches!(
        scope.single_session().await,
        Err(ObserverError::NoSession)
    ));
    client.shutdown().await?;
    test.close().await
}

#[tokio::test]
async fn shared_sdk_streams_and_one_shot_cancellation_release_independent_demand()
-> anyhow::Result<()> {
    use wf_observer_sdk::{CurrenciesState, ObserverError, TryStreamExt as _};
    let test = TestWarframe::start().await?;
    let client = wf_observer_sdk::connect(test.ticket()).await?;
    let game = client.warframe().single_session().await?;
    let currencies = game.currencies();
    let watch = currencies.watch().await?;
    let mut stream = currencies.watch().await?.into_stream();
    demand(&test, 1).await?;
    assert!(matches!(
        stream.try_next().await?,
        Some(CurrenciesState::Waiting)
    ));
    test.publish_currencies(ACCOUNT_ID, false)?;
    assert!(matches!(
        stream.try_next().await?,
        Some(CurrenciesState::Ready { .. })
    ));
    let value = currencies.read().await?;
    assert_eq!(value.account_id, ACCOUNT_ID);
    assert_eq!(watch.current()?, CurrenciesState::Ready { value });
    drop(stream);
    demand(&test, 1).await?;
    watch.cancel();
    demand(&test, 0).await?;
    assert!(matches!(
        currencies.read_with_timeout(40).await,
        Err(ObserverError::Timeout)
    ));
    demand(&test, 0).await?;
    let mut read = Box::pin(currencies.read());
    tokio::select! {
        result = &mut read => anyhow::bail!("read unexpectedly completed: {result:?}"),
        result = demand(&test, 1) => result?,
    }
    drop(read);
    demand(&test, 0).await?;
    client.shutdown().await?;
    test.close().await
}
