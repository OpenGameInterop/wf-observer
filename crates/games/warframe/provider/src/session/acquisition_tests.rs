use std::time::Duration;

use provider_sdk::{
    CapabilityDescriptor, CapabilityHealth, EventSink, HealthSink, PollContext, PollResult,
    ProviderError, ProviderSession, UnavailableReason,
};
use warframe_model::{InventoryFamily, InventorySnapshot, ItemKey};

use super::{CachedCheck, Executable, INVENTORY, WarframeSession};

#[path = "fixture.rs"]
mod fixture;
use fixture::{ACCOUNT, Memory, OTHER_ACCOUNT};

pub(super) type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

// These tests exercise acquisition after successful identification and layout
// validation. The fixture contains synthetic objects, not executable code.
fn session(base: u64) -> WarframeSession {
    let mut session = WarframeSession {
        executable: CachedCheck::Passed(Executable {
            base,
            actual: crate::target::BUILD,
        }),
        login_layout: CachedCheck::Passed(()),
        string_layout: CachedCheck::Passed(()),
        item_layout: CachedCheck::Passed(()),
        ..WarframeSession::default()
    };
    session.inventory.layout = CachedCheck::Passed(());
    session
}

#[derive(Default)]
struct Output {
    snapshots: Vec<InventorySnapshot>,
    resets: usize,
    health: Vec<CapabilityHealth>,
}

impl EventSink for Output {
    fn reset(&mut self, cap: &CapabilityDescriptor) -> Result<(), ProviderError> {
        assert_eq!(cap.topic, INVENTORY.topic);
        self.resets += 1;
        Ok(())
    }

    fn snapshot(
        &mut self,
        cap: &CapabilityDescriptor,
        value: &serde_json::Value,
    ) -> Result<(), ProviderError> {
        assert_eq!(cap.topic, INVENTORY.topic);
        self.snapshots.push(
            serde_json::from_value(value.clone())
                .map_err(|error| ProviderError::Failed(error.to_string()))?,
        );
        Ok(())
    }

    fn event(
        &mut self,
        _: &CapabilityDescriptor,
        _: &serde_json::Value,
    ) -> Result<(), ProviderError> {
        Err(ProviderError::Failed("unexpected inventory event".into()))
    }
}

impl HealthSink for Output {
    fn update(
        &mut self,
        cap: &CapabilityDescriptor,
        health: CapabilityHealth,
    ) -> Result<(), ProviderError> {
        assert_eq!(cap.topic, INVENTORY.topic);
        self.health.push(health);
        Ok(())
    }
}

fn poll(session: &mut WarframeSession, memory: &mut Memory, second: u64) -> TestResult<Output> {
    let mut events = Output::default();
    let mut health = Output::default();
    let result = session.poll(&mut PollContext {
        demand: &[&INVENTORY],
        now: Duration::from_secs(second),
        memory,
        events: &mut events,
        health: &mut health,
    })?;
    assert!(matches!(result, PollResult::After(_)));
    session.poll_completed(true);
    events.health = health.health;
    Ok(events)
}

fn only_snapshot(output: &Output) -> TestResult<&InventorySnapshot> {
    assert!(output.health.is_empty(), "{:?}", output.health);
    let [snapshot] = output.snapshots.as_slice() else {
        return Err("expected exactly one inventory snapshot".into());
    };
    Ok(snapshot)
}

#[test]
fn validated_session_reads_and_aggregates_inventory_at_unrelated_bases() -> TestResult {
    for base in [0x1_4000_0000, 0x2_8000_0000] {
        let mut memory = Memory::new(base);
        let mut session = session(base);
        let output = poll(&mut session, &mut memory, 0)?;
        let snapshot = only_snapshot(&output)?;
        assert_eq!(snapshot.account_id().as_str().as_bytes(), ACCOUNT);
        assert_eq!(snapshot.families().len(), InventoryFamily::ALL.len());
        for (path, expected) in [("AlloyPlate", 10), ("OrokinCell", 5)] {
            let key = ItemKey::new(format!("/Lotus/Types/Items/MiscItems/{path}"))?;
            assert_eq!(
                snapshot.quantity(InventoryFamily::MiscItems, &key),
                expected
            );
        }
        assert!(snapshot.families().iter().all(|family| {
            family.family == InventoryFamily::MiscItems || family.items.is_empty()
        }));
        assert_eq!(
            only_snapshot(&poll(&mut session, &mut memory, 1)?)?,
            snapshot
        );
        assert_eq!(output.resets, 0);
    }
    Ok(())
}

#[test]
fn readable_empty_vectors_publish_a_complete_empty_inventory() -> TestResult {
    let mut memory = Memory::new(0x1_4000_0000);
    memory.put(memory.data() + 0xd5d0 + 0xd0, &[0; 16]);
    let output = poll(&mut session(0x1_4000_0000), &mut memory, 0)?;
    let snapshot = only_snapshot(&output)?;
    assert_eq!(snapshot.families().len(), InventoryFamily::ALL.len());
    assert!(
        snapshot
            .families()
            .iter()
            .all(|family| family.items.is_empty())
    );
    Ok(())
}

#[test]
fn account_replacement_during_acquisition_discards_output_and_resets() -> TestResult {
    let mut memory = Memory::new(0x1_4000_0000);
    let mut session = session(0x1_4000_0000);
    only_snapshot(&poll(&mut session, &mut memory, 0)?)?;
    memory.change_on_read(memory.payload(), memory.account(), OTHER_ACCOUNT);
    let output = poll(&mut session, &mut memory, 1)?;
    assert!(output.snapshots.is_empty());
    assert_eq!(output.resets, 1);
    assert_eq!(
        output.health,
        [CapabilityHealth::Unavailable(
            UnavailableReason::TargetNotReady
        )]
    );
    let output = poll(&mut session, &mut memory, 2)?;
    assert_eq!(
        only_snapshot(&output)?.account_id().as_str().as_bytes(),
        OTHER_ACCOUNT
    );
    Ok(())
}

#[test]
fn rebuild_and_sync_changes_reject_samples_then_recover() -> TestResult {
    for (offset, during_read) in [(0x11c60, false), (0x11c60, true), (0xfdc0, true)] {
        let mut memory = Memory::new(0x1_4000_0000);
        let mut session = session(0x1_4000_0000);
        only_snapshot(&poll(&mut session, &mut memory, 0)?)?;
        let marker = memory.data() + offset;
        if during_read {
            memory.change_on_read(memory.payload(), marker, &[1]);
        } else {
            memory.put(marker, &[1]);
        }
        let output = poll(&mut session, &mut memory, 1)?;
        assert!(output.snapshots.is_empty());
        assert_eq!(
            output.health,
            [CapabilityHealth::Unavailable(
                UnavailableReason::TargetNotReady
            )]
        );
        memory.put(marker, &[0]);
        only_snapshot(&poll(&mut session, &mut memory, 2)?)?;
    }
    Ok(())
}

#[test]
fn corrupt_counts_and_missing_record_bytes_are_not_empty_inventory() -> TestResult {
    for unreadable in [false, true] {
        let mut memory = Memory::new(0x1_4000_0000);
        if unreadable {
            memory.omit(memory.payload() + 47);
        } else {
            memory.put(memory.payload() + 8, &[0; 4]);
        }
        let output = poll(&mut session(0x1_4000_0000), &mut memory, 0)?;
        assert!(output.snapshots.is_empty());
        assert!(
            matches!(output.health.as_slice(),
                [CapabilityHealth::Unavailable(UnavailableReason::ReadFailed { .. })] if unreadable
            ) || matches!(output.health.as_slice(),
                [CapabilityHealth::Unavailable(UnavailableReason::ValidationFailed { .. })] if !unreadable
            )
        );
    }
    Ok(())
}
