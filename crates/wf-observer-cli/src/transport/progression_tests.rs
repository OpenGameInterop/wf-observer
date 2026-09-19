use super::*;
use sdk::warframe::{
    DrifterIntrinsics, IntrinsicsSnapshot, RailjackIntrinsics, StarChartNodeProgress,
    StarChartSnapshot, decode_intrinsics, decode_star_chart,
};

fn ranks(account: &str, rank: u32) -> anyhow::Result<serde_json::Value> {
    Ok(serde_json::to_value(IntrinsicsSnapshot::new(
        AccountId::new(account)?,
        RailjackIntrinsics {
            unspent_points: 123,
            command: rank,
            ..Default::default()
        },
        DrifterIntrinsics {
            unspent_points: 4,
            combat: 10,
            ..Default::default()
        },
    )?)?)
}
fn nodes(account: &str, steel_path: bool) -> anyhow::Result<serde_json::Value> {
    Ok(serde_json::to_value(StarChartSnapshot::new(
        AccountId::new(account)?,
        (0..64)
            .map(|index| StarChartNodeProgress {
                node_key: format!("SolNode{index:04}"),
                completions: 1,
                steel_path_completed: index == 0 && steel_path,
            })
            .collect(),
    )?)?)
}

#[tokio::test]
async fn progression_topics_cross_raw_and_concrete_apis_independently() -> anyhow::Result<()> {
    let test = TestWarframe::start().await?;
    let raw = Client::connect(test.server.endpoint().addr()).await?;
    let raw_ranks = raw.subscribe_intrinsics(wire::SessionSelector::All).await?;
    let raw_nodes = raw.subscribe_star_chart(wire::SessionSelector::All).await?;
    test.publish_payload("warframe.intrinsics", Some(ranks(ACCOUNT_ID, 5)?), false)?;
    test.publish_payload(
        "warframe.star_chart",
        Some(nodes(ACCOUNT_ID, false)?),
        false,
    )?;
    let first_ranks = decode_intrinsics(next_snapshot(&raw_ranks).await?)?;
    let first_nodes = decode_star_chart(next_snapshot(&raw_nodes).await?)?;
    assert_eq!(first_ranks.data.railjack().command, 5);
    assert_eq!(
        raw.intrinsics_snapshot(&first_ranks.metadata.source.session)
            .await?
            .data,
        first_ranks.data
    );
    assert_eq!(
        raw.star_chart_snapshot(&first_nodes.metadata.source.session)
            .await?
            .data,
        first_nodes.data
    );
    let client = sdk::connect(test.ticket()).await?;
    let game = client.warframe().single_session().await?;
    let rank_watch = game.intrinsics().watch().await?;
    let node_watch = game.star_chart().watch().await?;
    assert_eq!(
        next_ranks(&rank_watch).await?,
        sdk::WarframeIntrinsics::from(first_ranks)
    );
    let old_nodes = next_nodes(&node_watch).await?;
    assert_eq!(old_nodes, sdk::WarframeStarChart::from(first_nodes));
    assert!(old_nodes.normal_completed("SolNode0000"));
    assert!(!old_nodes.steel_path_completed("SolNode0000"));
    assert_eq!(game.intrinsics().read().await?.railjack.command, 5);
    assert_eq!(game.star_chart().read().await?, old_nodes);
    test.publish_payload("warframe.star_chart", Some(nodes(ACCOUNT_ID, true)?), false)?;
    let update = next_nodes(&node_watch).await?;
    assert!(update.steel_path_completed("SolNode0000"));
    assert_eq!(
        sdk::WarframeStarChart::from(decode_star_chart(next_snapshot(&raw_nodes).await?)?),
        update
    );
    assert_eq!(
        game.intrinsics()
            .cached()
            .await?
            .context("missing ranks")?
            .railjack
            .command,
        5
    );
    test.publish_payload(
        "warframe.intrinsics",
        Some(ranks(OTHER_ACCOUNT_ID, 0)?),
        true,
    )?;
    let replacement = next_ranks(&rank_watch).await?;
    assert_eq!(replacement.account_id, OTHER_ACCOUNT_ID);
    assert_eq!(replacement.railjack.command, 0);
    assert_eq!(game.star_chart().cached().await?, Some(update));
    test.publish_payload("warframe.intrinsics", None, false)?;
    assert!(matches!(
        timeout(Duration::from_secs(5), rank_watch.next()).await??,
        Some(sdk::IntrinsicsState::Unavailable { .. })
    ));
    assert!(matches!(
        node_watch.current()?,
        sdk::StarChartState::Ready { .. }
    ));
    rank_watch.shutdown().await?;
    node_watch.shutdown().await?;
    raw_ranks.close();
    raw_nodes.close();
    demand(&test, 0).await?;
    client.shutdown().await?;
    raw.close().await;
    test.close().await
}

async fn next_ranks(watch: &sdk::IntrinsicsWatch) -> anyhow::Result<sdk::WarframeIntrinsics> {
    timeout(Duration::from_secs(5), async {
        loop {
            if let sdk::IntrinsicsState::Ready { value } =
                watch.next().await?.context("ranks ended")?
            {
                return Ok(value);
            }
        }
    })
    .await?
}
async fn next_nodes(watch: &sdk::StarChartWatch) -> anyhow::Result<sdk::WarframeStarChart> {
    timeout(Duration::from_secs(5), async {
        loop {
            if let sdk::StarChartState::Ready { value } =
                watch.next().await?.context("nodes ended")?
            {
                return Ok(value);
            }
        }
    })
    .await?
}
