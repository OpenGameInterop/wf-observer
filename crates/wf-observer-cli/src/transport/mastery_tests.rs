use super::*;
use sdk::warframe::{MasteryItemProgress, MasterySnapshot, decode_mastery};

fn mastery(account: &str, affinity: u64) -> anyhow::Result<MasterySnapshot> {
    // Enough unchanged rows for the service to choose delta delivery on updates.
    let items = (0..64)
        .map(|index| {
            Ok(MasteryItemProgress {
                item_key: ItemKey::new(format!("/Lotus/Weapons/Test{index:04}"))?,
                affinity: if index == 0 { affinity } else { 1000 },
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(MasterySnapshot::new(
        AccountId::new(account)?,
        34,
        u64::MAX,
        6000,
        items,
    )?)
}

fn publish(test: &TestWarframe, account: &str, affinity: u64, reset: bool) -> anyhow::Result<()> {
    test.publish_payload(
        "warframe.mastery",
        Some(serde_json::to_value(mastery(account, affinity)?)?),
        reset,
    )
}

async fn next_ready(watch: &sdk::MasteryWatch) -> anyhow::Result<sdk::WarframeMastery> {
    timeout(Duration::from_secs(5), async {
        loop {
            if let sdk::MasteryState::Ready { value } =
                watch.next().await?.context("mastery watch ended")?
            {
                return Ok(value);
            }
        }
    })
    .await?
}

#[tokio::test]
async fn mastery_crosses_raw_and_concrete_apis_with_updates_resets_and_health() -> anyhow::Result<()>
{
    let test = TestWarframe::start().await?;
    let raw = Client::connect(test.server.endpoint().addr()).await?;
    let sub = raw.subscribe_mastery(wire::SessionSelector::All).await?;
    publish(&test, ACCOUNT_ID, u64::MAX, false)?;
    let first = decode_mastery(next_snapshot(&sub).await?)?;
    assert_eq!(first.data, mastery(ACCOUNT_ID, u64::MAX)?);
    assert_eq!(
        raw.mastery_snapshot(&first.metadata.source.session)
            .await?
            .data,
        first.data
    );

    let client = sdk::connect(test.ticket()).await?;
    let capability = client.warframe().single_session().await?.mastery();
    let watch = capability.watch().await?;
    let second = capability.watch().await?;
    demand(&test, 2).await?;
    assert_eq!(
        next_ready(&watch).await?,
        sdk::WarframeMastery::from(first.clone())
    );
    assert_eq!(
        capability.cached().await?,
        Some(sdk::WarframeMastery::from(first.clone()))
    );
    assert_eq!(capability.read().await?.items[0].affinity, u64::MAX);
    demand(&test, 2).await?;

    for affinity in [0, u64::MAX - 1] {
        publish(&test, ACCOUNT_ID, affinity, false)?;
        let value = decode_mastery(next_snapshot(&sub).await?)?;
        assert_eq!(value.data, mastery(ACCOUNT_ID, affinity)?);
        assert_eq!(next_ready(&watch).await?, sdk::WarframeMastery::from(value));
    }
    let inventory_sub = raw.subscribe_inventory(wire::SessionSelector::All).await?;
    test.publish(ACCOUNT_ID, 1, false, false)?;
    assert!(capability.cached().await?.is_some());
    inventory_sub.close();
    demand(&test, 2).await?;
    publish(&test, OTHER_ACCOUNT_ID, 7, true)?;
    let replacement = next_ready(&watch).await?;
    assert_eq!(replacement.account_id, OTHER_ACCOUNT_ID);
    assert_ne!(
        replacement.metadata.generation,
        first.metadata.generation.to_string()
    );
    assert_eq!(capability.cached().await?, Some(replacement));
    test.publish_payload("warframe.mastery", None, false)?;
    let state = timeout(Duration::from_secs(5), watch.next()).await??;
    assert!(matches!(state, Some(sdk::MasteryState::Unavailable { .. })));
    assert!(matches!(
        watch.current()?,
        sdk::MasteryState::Unavailable { .. }
    ));
    assert!(capability.cached().await.is_err());
    watch.cancel();
    watch.shutdown().await?;
    assert!(watch.next().await?.is_none());
    // Dropping the Rust stream releases the final listener in this client.
    drop(second.into_stream());
    demand(&test, 1).await?;
    sub.close();
    demand(&test, 0).await?;
    client.shutdown().await?;
    raw.close().await;
    test.close().await
}

#[tokio::test]
async fn mastery_one_shot_reads_release_demand_on_success_timeout_and_cancellation()
-> anyhow::Result<()> {
    let test = TestWarframe::start().await?;
    let client = sdk::connect(test.ticket()).await?;
    let mastery = client.warframe().single_session().await?.mastery();
    assert!(mastery.cached().await?.is_none());
    demand(&test, 0).await?;
    let (value, published) = tokio::join!(mastery.read(), async {
        demand(&test, 1).await?;
        publish(&test, ACCOUNT_ID, 0, false)
    });
    published?;
    assert_eq!(value?.items[0].affinity, 0);
    demand(&test, 0).await?;
    assert!(matches!(
        mastery.read_with_timeout(40).await,
        Err(sdk::ObserverError::Timeout)
    ));
    demand(&test, 0).await?;
    let mut read = Box::pin(mastery.read());
    tokio::select! {
        result = &mut read => anyhow::bail!("read unexpectedly completed: {result:?}"),
        result = demand(&test, 1) => result?,
    }
    drop(read);
    demand(&test, 0).await?;
    client.shutdown().await?;
    test.close().await
}
