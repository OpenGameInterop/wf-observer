//! Acquisition fixtures bypass code validation; observed code is tested separately.
use super::*;
use crate::session::fixture::{ACCOUNT, Memory, OTHER_ACCOUNT};
use provider_sdk::{EventSink, HealthSink};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

pub(super) fn session(base: u64) -> WarframeSession {
    let mut session = WarframeSession {
        executable: CachedCheck::Passed(Executable {
            base,
            actual: crate::target::BUILD,
        }),
        layouts: SharedLayouts::validated(),
        ..WarframeSession::default()
    };
    session.mastery.layout = CachedCheck::Passed(());
    session.inventory.layout = CachedCheck::Passed(());
    session
}

// Fixed offsets and codecs describe independent synthetic game objects.
fn protected(memory: &mut Memory, address: u64, value: u32) {
    let [a, b, c, d, ..] = ((address + 4) >> 3).to_le_bytes();
    let stored = (value ^ u32::from_le_bytes([a, b, c, d]) ^ 0x635b_f253).rotate_right(30);
    memory.put(address, &(stored ^ 0xe19c_9bbd).to_le_bytes());
    memory.put(address + 4, &stored.to_le_bytes());
}

fn memory(base: u64) -> Memory {
    let mut memory = Memory::new(base);
    let inventory = memory.data() + 0xd5d0;
    let payload = memory.heap() + 0x40_0000;
    memory.put(inventory + 0xf0, &payload.to_le_bytes());
    memory.put(inventory + 0xf8, &48_u32.to_le_bytes());
    memory.put(inventory + 0xfc, &48_u32.to_le_bytes());
    // Reverse key order, a null slot, and a real zero-affinity record.
    for (index, (item, xp)) in [
        (memory.heap() + 0x31_0000, 219_081_020),
        (0, u32::MAX),
        (memory.heap() + 0x30_0000, 0),
    ]
    .into_iter()
    .enumerate()
    {
        let record = payload + index as u64 * 16;
        memory.put(record, &item.to_le_bytes());
        protected(&mut memory, record + 8, xp);
    }
    for (offset, value) in [(0xe300, 1000), (0xe308, 2000), (0xe310, 0)] {
        let address = memory.data() + offset;
        protected(&mut memory, address, value);
    }
    memory.put(inventory + 0xd20, &[0]);
    memory.put(inventory + 0xd24, &6000_u32.to_le_bytes());
    let [a, b, ..] = ((inventory + 0x3fe) >> 3).to_le_bytes();
    let rank = (0x0022_u16 ^ u16::from_le_bytes([a, b]) ^ 0x1575).rotate_right(5);
    memory.put(inventory + 0x3fc, &(rank ^ 0xb80e).to_le_bytes());
    memory.put(inventory + 0x3fe, &rank.to_le_bytes());
    memory
}

#[derive(Default)]
pub(super) struct Output {
    pub(super) snapshots: Vec<(&'static str, serde_json::Value)>,
    pub(super) resets: Vec<&'static str>,
    pub(super) health: Vec<(&'static str, CapabilityHealth)>,
}
impl EventSink for Output {
    fn reset(&mut self, cap: &CapabilityDescriptor) -> Result<(), ProviderError> {
        self.resets.push(cap.topic);
        Ok(())
    }
    fn snapshot(
        &mut self,
        cap: &CapabilityDescriptor,
        value: &serde_json::Value,
    ) -> Result<(), ProviderError> {
        self.snapshots.push((cap.topic, value.clone()));
        Ok(())
    }
    fn event(
        &mut self,
        _: &CapabilityDescriptor,
        _: &serde_json::Value,
    ) -> Result<(), ProviderError> {
        unreachable!("snapshot tests")
    }
}
impl HealthSink for Output {
    fn update(
        &mut self,
        cap: &CapabilityDescriptor,
        health: CapabilityHealth,
    ) -> Result<(), ProviderError> {
        self.health.push((cap.topic, health));
        Ok(())
    }
}

pub(super) fn poll(
    session: &mut WarframeSession,
    memory: &mut Memory,
    second: u64,
    demand: &[&'static CapabilityDescriptor],
) -> TestResult<Output> {
    let mut events = Output::default();
    let mut health = Output::default();
    session.poll(&mut PollContext {
        demand,
        now: Duration::from_secs(second),
        memory,
        events: &mut events,
        health: &mut health,
    })?;
    session.poll_completed(true);
    events.health = health.health;
    Ok(events)
}

fn snapshot(output: &Output) -> TestResult<MasterySnapshot> {
    assert!(
        output
            .health
            .iter()
            .all(|(topic, _)| *topic != MASTERY.topic)
    );
    let value = &output
        .snapshots
        .iter()
        .find(|(topic, _)| *topic == MASTERY.topic)
        .ok_or("missing mastery")?
        .1;
    Ok(serde_json::from_value(value.clone())?)
}

#[test]
fn mastery_is_independent_sorted_and_uses_retained_raw_affinity() -> TestResult {
    for base in [0x1_4000_0000, 0x2_8000_0000] {
        let mut memory = memory(base);
        let mut session = session(base);
        session.inventory.layout = CachedCheck::Failed(Retry {
            reason: UnavailableReason::UnsupportedBuild,
            at: Duration::from_secs(5),
        });
        let value = snapshot(&poll(&mut session, &mut memory, 0, &[&MASTERY])?)?;
        assert_eq!(value.account_id().as_str().as_bytes(), ACCOUNT);
        assert_eq!(
            (value.rank(), value.item_points(), value.total_points()),
            (34, 6000, 9000)
        );
        assert_eq!(
            (
                value.mission_points(),
                value.railjack_intrinsic_points(),
                value.drifter_intrinsic_points()
            ),
            (1000, 2000, 0)
        );
        assert_eq!(value.tracked_items(), 2);
        assert!(value.items()[0].item_key.as_str().ends_with("AlloyPlate"));
        assert_eq!(value.items()[0].affinity, 0);
        assert_eq!(value.items()[1].affinity, 219_081_020);
        assert!(!memory.reads.contains(&(memory.data() + 0xd5d0 + 0xd0)));
        memory.reads.clear();
        poll(&mut session, &mut memory, 1, &[])?;
        assert!(memory.reads.is_empty());
        let resumed = snapshot(&poll(&mut session, &mut memory, 2, &[&MASTERY])?)?;
        assert_eq!(resumed, value);
    }
    Ok(())
}

#[test]
fn empty_progression_and_totals_larger_than_u32_are_valid() -> TestResult {
    let base = 0x1_4000_0000;
    let mut memory = memory(base);
    let inventory = memory.data() + 0xd5d0;
    memory.put(inventory + 0xf0, &[0; 16]);
    let mut session = session(base);
    assert!(
        snapshot(&poll(&mut session, &mut memory, 0, &[&MASTERY])?)?
            .items()
            .is_empty()
    );
    for offset in [0xe300, 0xe308, 0xe310] {
        let address = memory.data() + offset;
        protected(&mut memory, address, u32::MAX);
    }
    memory.put(inventory + 0xd24, &u32::MAX.to_le_bytes());
    assert_eq!(
        snapshot(&poll(&mut session, &mut memory, 1, &[&MASTERY])?)?.total_points(),
        u64::from(u32::MAX) * 4
    );
    Ok(())
}

#[test]
fn corruption_duplicates_and_partial_reads_do_not_become_empty_mastery() -> TestResult {
    let base = 0x1_4000_0000;
    for case in 0..9 {
        let mut memory = memory(base);
        let inventory = memory.data() + 0xd5d0;
        let payload = memory.heap() + 0x40_0000;
        match case {
            0 => memory.put(payload + 8, &[0; 4]),
            1 => memory.put(inventory + 0x3fc, &[0; 2]),
            2 => memory.put(memory.data() + 0xe308, &[0; 4]),
            3 => memory.omit(payload + 47),
            4 => memory.put(payload + 32, &(memory.heap() + 0x31_0000).to_le_bytes()),
            5 => memory.put(payload, &1_u64.to_le_bytes()),
            6 => memory.put(inventory + 0xf8, &47_u32.to_le_bytes()),
            7 => memory.put(inventory + 0xfc, &u32::MAX.to_le_bytes()),
            // Distinct native identities resolving to the same canonical key.
            _ => memory.put(memory.heap() + 0x31_0000 + 44, &2_u32.to_le_bytes()),
        }
        let mut session = session(base);
        let output = poll(&mut session, &mut memory, 0, &[&MASTERY, &INVENTORY])?;
        assert!(
            output
                .snapshots
                .iter()
                .all(|(topic, _)| *topic != MASTERY.topic)
        );
        assert!(
            output
                .snapshots
                .iter()
                .any(|(topic, _)| *topic == INVENTORY.topic)
        );
        assert!(matches!(
            output.health.as_slice(),
            [("warframe.mastery", CapabilityHealth::Unavailable(_))]
        ));
    }
    Ok(())
}

#[test]
fn rebuilding_recalculation_and_changed_samples_retry_then_recover() -> TestResult {
    let base = 0x1_4000_0000;
    for case in 0..6 {
        let mut memory = memory(base);
        let inventory = memory.data() + 0xd5d0;
        let payload = memory.heap() + 0x40_0000;
        match case {
            0 => memory.put(inventory + 0xd20, &[1]),
            1 => memory.put(memory.data() + 0x0001_1c60, &[1]),
            2 => memory.change_on_read(payload, memory.data() + 0xfdc0, &[1]),
            3 => memory.change_on_read(payload, inventory + 0xd24, &6001_u32.to_le_bytes()),
            4 => memory.change_on_read(payload, inventory + 0xf8, &32_u32.to_le_bytes()),
            _ => memory.change_on_read(payload, inventory + 0xd20, &[1]),
        }
        let mut session = session(base);
        let output = poll(&mut session, &mut memory, 0, &[&MASTERY])?;
        assert!(output.snapshots.is_empty());
        assert_eq!(
            output.health,
            [(
                MASTERY.topic,
                CapabilityHealth::Unavailable(
                    UnavailableReason::TargetNotReady.with_dependency(MASTERY.topic)
                )
            )]
        );
        memory.put(inventory + 0xd20, &[0]);
        memory.put(memory.data() + 0x0001_1c60, &[0]);
        snapshot(&poll(&mut session, &mut memory, 1, &[&MASTERY])?)?;
    }
    Ok(())
}

#[test]
fn account_change_and_logout_reset_mastery_before_publication() -> TestResult {
    let base = 0x1_4000_0000;
    let mut memory = memory(base);
    let mut session = session(base);
    snapshot(&poll(&mut session, &mut memory, 0, &[&MASTERY])?)?;
    memory.change_on_read(memory.heap() + 0x40_0000, memory.account(), OTHER_ACCOUNT);
    let changed = poll(&mut session, &mut memory, 1, &[&MASTERY])?;
    assert_eq!(changed.resets, [MASTERY.topic]);
    assert!(changed.snapshots.is_empty());
    let replacement = snapshot(&poll(&mut session, &mut memory, 2, &[&MASTERY])?)?;
    assert_eq!(replacement.account_id().as_str().as_bytes(), OTHER_ACCOUNT);
    memory.put(memory.heap() + 0x4_0000 + 0x1e1, &[0]);
    let logout = poll(&mut session, &mut memory, 3, &[&MASTERY])?;
    assert_eq!(logout.resets, [MASTERY.topic]);
    assert!(logout.snapshots.is_empty());
    Ok(())
}

#[test]
fn shared_and_mastery_layout_failures_keep_independent_retry_deadlines() -> TestResult {
    let base = 0x1_4000_0000;
    for failed in 0..7 {
        let mut memory = memory(base);
        let mut session = session(base);
        let check = match failed {
            0 => &mut session.layouts.account,
            1 => &mut session.layouts.strings,
            2 => &mut session.layouts.items,
            3 => &mut session.layouts.inventory_owner,
            5 => &mut session.layouts.profile_data,
            6 => &mut session.layouts.profile_commit,
            _ => &mut session.mastery.layout,
        };
        *check = CachedCheck::Failed(Retry {
            reason: UnavailableReason::UnsupportedBuild,
            at: Duration::from_secs(5),
        });
        for second in 0..4 {
            let demand: &[&'static CapabilityDescriptor] =
                if second == 1 { &[] } else { &[&MASTERY] };
            let output = poll(&mut session, &mut memory, second, demand)?;
            assert!(output.snapshots.is_empty());
            assert!(memory.reads.is_empty());
        }
    }
    // Mastery's layout failure does not prevent demanded inventory from publishing.
    let mut memory = memory(base);
    let mut session = session(base);
    session.mastery.layout = CachedCheck::Failed(Retry {
        reason: UnavailableReason::UnsupportedBuild,
        at: Duration::from_secs(5),
    });
    let output = poll(&mut session, &mut memory, 0, &[&MASTERY, &INVENTORY])?;
    assert_eq!(output.snapshots.len(), 1);
    assert_eq!(output.snapshots[0].0, INVENTORY.topic);
    Ok(())
}
