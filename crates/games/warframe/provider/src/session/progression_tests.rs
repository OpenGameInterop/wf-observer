use super::mastery_tests::{poll, session as base_session};
use super::*;
use crate::session::fixture::{Memory, OTHER_ACCOUNT};
use warframe_model::{IntrinsicsSnapshot, StarChartDifficulty, StarChartSnapshot};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn session(base: u64) -> WarframeSession {
    let mut session = base_session(base);
    session.intrinsics.layout = CachedCheck::Passed(());
    session.star_chart.layout = CachedCheck::Passed(());
    session
}

fn memory(base: u64) -> Memory {
    let mut memory = Memory::new(base);
    let block = memory.data() + 0x0001_707c;
    for (i, value) in [123_999_u32, 4500, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
        .into_iter()
        .enumerate()
    {
        memory.put(block + i as u64 * 4, &value.to_le_bytes());
    }
    let payload = memory.heap() + 0x50_0000;
    memory.put(memory.data() + 0xff48, &payload.to_le_bytes());
    memory.put(memory.data() + 0xff50, &96_u32.to_le_bytes());
    memory.put(memory.data() + 0xff54, &96_u32.to_le_bytes());
    for (i, (key, count, tier)) in [("SolNode1", 8_u32, 1_u8), ("JunctionA", 0, 0)]
        .into_iter()
        .enumerate()
    {
        let token = if i == 0 { 10_u32 } else { 11 };
        let record = payload + i as u64 * 48;
        memory.put(record, &[0; 48]);
        memory.put(record, &token.to_le_bytes());
        memory.put(record + 4, &count.to_le_bytes());
        memory.put(record + 8, &[tier]);
        let text = memory.heap() + 0x60_0000 + i as u64 * 0x1000;
        memory.put(
            memory.heap() + 0x32_0000 + u64::from(token) * 16,
            &text.to_le_bytes(),
        );
        memory.put(text, &[0; 64]);
        memory.put(text, key.as_bytes());
    }
    memory
}

#[test]
fn progression_is_sorted_independent_and_inactive_without_demand() -> TestResult {
    for base in [0x1_4000_0000, 0x2_8000_0000] {
        let mut memory = memory(base);
        let mut session = session(base);
        // Neither topic needs item types, mastery, or the embedded inventory getter.
        session.layouts.items = CachedCheck::Failed(Retry {
            reason: UnavailableReason::UnsupportedBuild,
            at: Duration::from_secs(5),
        });
        session.layouts.inventory_owner = CachedCheck::Failed(Retry {
            reason: UnavailableReason::UnsupportedBuild,
            at: Duration::from_secs(5),
        });
        let output = poll(&mut session, &mut memory, 0, &[&INTRINSICS, &STAR_CHART])?;
        assert!(output.health.is_empty());
        assert!(!memory.reads.contains(&(base + 0x0148_fde0)));
        let intrinsics: IntrinsicsSnapshot = serde_json::from_value(output.snapshots[0].1.clone())?;
        assert_eq!(
            (
                intrinsics.railjack().unspent_points,
                intrinsics.drifter().unspent_points
            ),
            (123, 4)
        );
        assert_eq!(
            (
                intrinsics.railjack().piloting,
                intrinsics.railjack().command
            ),
            (1, 5)
        );
        assert_eq!(
            (intrinsics.drifter().combat, intrinsics.drifter().endurance),
            (6, 9)
        );
        let chart: StarChartSnapshot = serde_json::from_value(output.snapshots[1].1.clone())?;
        assert_eq!(chart.nodes()[0].node_key, "JunctionA");
        assert!(chart.completed("JunctionA", StarChartDifficulty::Normal));
        assert!(!chart.completed("JunctionA", StarChartDifficulty::SteelPath));
        assert!(chart.completed("SolNode1", StarChartDifficulty::SteelPath));
        assert!(!memory.reads.contains(&(memory.data() + 0xd608 + 0xf0)));
        memory.reads.clear();
        poll(&mut session, &mut memory, 1, &[])?;
        assert!(memory.reads.is_empty());
        memory.put(memory.data() + 0x0001_707c, &[0; 48]);
        memory.put(memory.data() + 0xff48, &[0; 16]);
        let empty = poll(&mut session, &mut memory, 2, &[&INTRINSICS, &STAR_CHART])?;
        assert!(empty.health.is_empty());
        let ranks: IntrinsicsSnapshot = serde_json::from_value(empty.snapshots[0].1.clone())?;
        let nodes: StarChartSnapshot = serde_json::from_value(empty.snapshots[1].1.clone())?;
        assert_eq!(
            ranks.railjack(),
            warframe_model::RailjackIntrinsics::default()
        );
        assert!(nodes.nodes().is_empty());
    }
    Ok(())
}

#[test]
fn invalid_records_and_unstable_reads_fail_only_the_affected_topic() -> TestResult {
    let base = 0x1_4000_0000;
    for case in 0..10 {
        let mut memory = memory(base);
        let payload = memory.heap() + 0x50_0000;
        let block = memory.data() + 0x0001_707c;
        match case {
            0 => memory.put(block + 12, &11_u32.to_le_bytes()),
            1 => memory.omit(block + 47),
            2 => memory.change_on_nth_read(block, 2, block, &124_000_u32.to_le_bytes()),
            3 => memory.put(payload + 48, &10_u32.to_le_bytes()),
            4 => memory.put(payload, &0_u32.to_le_bytes()),
            5 => memory.put(memory.data() + 0xff50, &95_u32.to_le_bytes()),
            6 => memory.omit(payload + 95),
            7 => memory.change_on_nth_read(payload, 2, payload + 4, &9_u32.to_le_bytes()),
            8 => memory.change_on_nth_read(payload, 2, payload + 56, &[1]),
            _ => memory.put(memory.data() + 0xff54, &u32::MAX.to_le_bytes()),
        }
        let mut session = session(base);
        let output = poll(&mut session, &mut memory, 0, &[&INTRINSICS, &STAR_CHART])?;
        let failed = if case < 3 {
            INTRINSICS.topic
        } else {
            STAR_CHART.topic
        };
        assert_eq!(output.snapshots.len(), 1, "case {case}");
        assert_ne!(output.snapshots[0].0, failed);
        assert_eq!(output.health.len(), 1);
        assert_eq!(output.health[0].0, failed);
    }
    Ok(())
}

#[test]
fn ownership_changes_reset_both_topics_before_any_publication() -> TestResult {
    let base = 0x1_4000_0000;
    let mut memory = memory(base);
    let mut session = session(base);
    poll(&mut session, &mut memory, 0, &[&INTRINSICS, &STAR_CHART])?;
    memory.change_on_read(memory.heap() + 0x50_0000, memory.account(), OTHER_ACCOUNT);
    let changed = poll(&mut session, &mut memory, 1, &[&INTRINSICS, &STAR_CHART])?;
    assert!(changed.snapshots.is_empty());
    assert_eq!(changed.resets, [INTRINSICS.topic, STAR_CHART.topic]);
    let replacement = poll(&mut session, &mut memory, 2, &[&INTRINSICS, &STAR_CHART])?;
    assert_eq!(replacement.snapshots.len(), 2);
    for (_, snapshot) in replacement.snapshots {
        assert_eq!(
            snapshot["account_id"]
                .as_str()
                .ok_or("missing owner")?
                .as_bytes(),
            OTHER_ACCOUNT
        );
    }
    memory.put(memory.heap() + 0x4_0000 + 0x1e1, &[0]);
    let logout = poll(&mut session, &mut memory, 3, &[&INTRINSICS, &STAR_CHART])?;
    assert!(logout.snapshots.is_empty());
    assert_eq!(logout.resets, [INTRINSICS.topic, STAR_CHART.topic]);
    Ok(())
}
