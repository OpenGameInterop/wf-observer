//! The same blocker must reach status, watches and reads without disabling peers.
use super::*;

#[tokio::test]
async fn dependency_failures_cross_the_transport_and_invalidate_only_their_topic()
-> anyhow::Result<()> {
    let test = TestWarframe::start().await?;
    let client = sdk::connect(test.ticket()).await?;
    let game = client.warframe().single_session().await?;
    let inventory = game.inventory();
    let watch = inventory.watch().await?;
    let player = game.player().watch().await?;
    watch.next().await?;
    player.next().await?;
    test.publish(ACCOUNT_ID, 7, false, true)?;
    test.publish_player(ACCOUNT_ID, false)?;
    assert!(matches!(
        watch.next().await?,
        Some(sdk::InventoryState::Ready { .. })
    ));
    assert!(matches!(
        player.next().await?,
        Some(sdk::PlayerState::Ready { .. })
    ));
    assert!(inventory.cached().await?.is_some());

    for (provider_failure, failure) in [
        (
            provider_sdk::DependencyFailure::TargetNotReady,
            sdk::DependencyFailure::TargetNotReady,
        ),
        (
            provider_sdk::DependencyFailure::UnsupportedBuild,
            sdk::DependencyFailure::UnsupportedBuild,
        ),
        (
            provider_sdk::DependencyFailure::ReadFailed,
            sdk::DependencyFailure::ReadFailed,
        ),
        (
            provider_sdk::DependencyFailure::ValidationFailed,
            sdk::DependencyFailure::ValidationFailed,
        ),
        (
            provider_sdk::DependencyFailure::ProviderFailed,
            sdk::DependencyFailure::ProviderFailed,
        ),
    ] {
        publish_failure(&test, provider_failure)?;
        let reason = sdk::UnavailableReason::DependencyUnavailable {
            dependency: "profile data".into(),
            failure,
        };
        assert_eq!(
            timeout(Duration::from_secs(5), watch.next()).await??,
            Some(sdk::InventoryState::Unavailable {
                reason: reason.clone()
            })
        );
        assert_eq!(
            watch.current()?,
            sdk::InventoryState::Unavailable {
                reason: reason.clone()
            }
        );
        assert_eq!(
            inventory.read().await,
            Err(sdk::ObserverError::Request {
                error: sdk::RequestError::Unavailable {
                    reason: reason.clone()
                },
            })
        );
        assert_eq!(
            inventory.cached().await,
            Err(sdk::ObserverError::Request {
                error: sdk::RequestError::Unavailable {
                    reason: reason.clone()
                },
            })
        );
        let status = client.status().await?;
        let sdk::TargetActivity::Observing { topics, .. } = &status.targets[0].activity else {
            anyhow::bail!("session stopped observing");
        };
        let health = &topics
            .iter()
            .find(|topic| topic.topic.topic == "warframe.inventory")
            .context("missing inventory status")?
            .health;
        assert_eq!(*health, sdk::CapabilityHealth::Unavailable { reason });
        assert_eq!(game.player().read().await?.username, "ExamplePlayer");
        assert!(game.player().cached().await?.is_some());
        assert!(matches!(player.current()?, sdk::PlayerState::Ready { .. }));
    }

    test.publish(ACCOUNT_ID, 8, false, true)?;
    assert!(matches!(
        timeout(Duration::from_secs(5), watch.next()).await??,
        Some(sdk::InventoryState::Ready { .. })
    ));
    assert!(inventory.cached().await?.is_some());
    watch.shutdown().await?;
    player.shutdown().await?;
    client.shutdown().await?;
    test.close().await
}

fn publish_failure(
    test: &TestWarframe,
    failure: provider_sdk::DependencyFailure,
) -> anyhow::Result<()> {
    let manifest = WarframeProvider.manifest();
    let cap = manifest
        .capabilities
        .iter()
        .find(|cap| cap.topic == "warframe.inventory")
        .context("missing inventory capability")?;
    let ticket = test.state.ticket("inventory").context("missing session")?;
    let batch = PollBatch::new(manifest);
    batch.health().update(
        cap,
        provider_sdk::CapabilityHealth::Unavailable(
            provider_sdk::UnavailableReason::DependencyUnavailable {
                dependency: "profile data".into(),
                failure,
            },
        ),
    )?;
    assert!(
        test.state
            .commit_poll(&ticket, batch, Instant::now() + Duration::from_secs(30))?
    );
    Ok(())
}
