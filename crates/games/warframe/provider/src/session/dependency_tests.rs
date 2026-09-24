//! Ownership failures must stop only the topics that consume that owner.
use super::*;
use crate::session::fixture::{Memory, OTHER_ACCOUNT};
use provider_sdk::{EventSink, HealthSink};

type TestResult = Result<(), Box<dyn std::error::Error>>;
const BASE: u64 = 0x1_4000_0000;

fn session() -> WarframeSession {
    let mut session = WarframeSession {
        executable: CachedCheck::Passed(Executable {
            base: BASE,
            actual: crate::target::BUILD,
        }),
        layouts: SharedLayouts::validated(),
        ..WarframeSession::default()
    };
    session.player.layout = CachedCheck::Passed(());
    session.inventory.layout = CachedCheck::Passed(());
    session
}

fn memory() -> Memory {
    let mut memory = Memory::new(BASE);
    let profile = memory.heap() + 0x4_0000;
    memory.put(BASE + 0x0215_1328 + 8, &(BASE + 0x008b_9520).to_le_bytes());
    let mut name = [0; 16];
    name[..5].copy_from_slice(b"Tenno");
    name[15] = 10;
    memory.put(profile + 0x50, &name);
    memory
}

#[derive(Default)]
struct Output {
    snapshots: Vec<&'static str>,
    resets: Vec<&'static str>,
    health: Vec<(&'static str, CapabilityHealth)>,
}
impl EventSink for Output {
    fn reset(&mut self, cap: &CapabilityDescriptor) -> Result<(), ProviderError> {
        self.resets.push(cap.topic);
        Ok(())
    }
    fn snapshot(
        &mut self,
        cap: &CapabilityDescriptor,
        _: &serde_json::Value,
    ) -> Result<(), ProviderError> {
        self.snapshots.push(cap.topic);
        Ok(())
    }
    fn event(
        &mut self,
        _: &CapabilityDescriptor,
        _: &serde_json::Value,
    ) -> Result<(), ProviderError> {
        Err(ProviderError::Failed("unexpected event".into()))
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
fn poll(
    session: &mut WarframeSession,
    memory: &mut Memory,
    demand: &[&'static CapabilityDescriptor],
    second: u64,
) -> Result<Output, ProviderError> {
    let mut output = Output::default();
    let mut health = Output::default();
    session.poll(&mut PollContext {
        memory,
        demand,
        now: Duration::from_secs(second),
        events: &mut output,
        health: &mut health,
    })?;
    session.poll_completed(true);
    output.health = health.health;
    Ok(output)
}

#[test]
fn player_validates_account_without_profile_data_code_or_objects() -> TestResult {
    let mut memory = memory();
    // Original account getter/manager instructions, independent of production facts.
    let mut getter = vec![0x48, 0x8b, 0x05];
    getter.extend((0x027a_53c0_i32 - 0x0029_cc30 - 7).to_le_bytes());
    getter.extend([0x48, 0x8b, 0, 0xc3]);
    memory.put(BASE + 0x0029_cc30, &getter);
    memory.put(BASE + 0x0024_4ee0, &[0x48, 0x8b, 0x81, 0xd8, 1, 0, 0, 0xc3]);
    memory.put(BASE + 0x0056_ee90, &[0x48, 0x8d, 0x81, 0x18, 1, 0, 0, 0xc3]);
    let mut lookup = [0; 31];
    lookup[15..22].copy_from_slice(&[0x48, 0x8b, 0x99, 0x30, 2, 0, 0]);
    lookup[25..31].copy_from_slice(&[0x8b, 0x81, 0x38, 2, 0, 0]);
    memory.put(BASE + 0x0020_9260, &lookup);
    memory.put(
        BASE + 0x00f7_a940,
        &[0x83, 0xb9, 0x68, 1, 0, 0, 4, 0x0f, 0x94, 0xc0, 0xc3],
    );
    memory.omit(memory.heap() + 0x4_0000 + 0x208);
    let mut session = session();
    session.layouts.account = CachedCheck::Unchecked;
    session.layouts.profile_data = CachedCheck::Unchecked;
    let output = poll(&mut session, &mut memory, &[&PLAYER], 0)?;
    assert_eq!(output.snapshots, [PLAYER.topic]);
    assert!(output.health.is_empty());
    assert!(matches!(
        session.layouts.profile_data,
        CachedCheck::Unchecked
    ));
    assert!(!memory.reads.contains(&(BASE + 0x008e_1ff0)));
    assert!(!memory.reads.contains(&(memory.heap() + 0x4_0000 + 0x208)));
    Ok(())
}

#[test]
fn profile_data_layout_failure_does_not_disable_player_or_accelerate_retries() -> TestResult {
    let mut memory = memory();
    let mut session = session();
    session.layouts.profile_data = CachedCheck::Failed(Retry {
        reason: UnavailableReason::UnsupportedBuild,
        at: Duration::from_secs(5),
    });
    for second in 0..5 {
        let output = poll(&mut session, &mut memory, &[&PLAYER, &INVENTORY], second)?;
        assert_eq!(output.snapshots, [PLAYER.topic]);
        assert!(
            output
                .health
                .iter()
                .all(|(topic, _)| *topic == INVENTORY.topic)
        );
        assert_eq!(session.inventory.next_poll, Duration::from_secs(5));
    }
    assert!(!memory.reads.contains(&(memory.heap() + 0x4_0000 + 0x208)));
    Ok(())
}

#[test]
fn missing_or_invalid_profile_data_keeps_player_available() -> TestResult {
    for invalid in [false, true] {
        let mut memory = memory();
        if invalid {
            memory.omit(memory.data());
        } else {
            memory.put(memory.heap() + 0x4_0000 + 0x208, &0_u64.to_le_bytes());
        }
        let output = poll(&mut session(), &mut memory, &[&PLAYER, &INVENTORY], 0)?;
        assert_eq!(output.snapshots, [PLAYER.topic]);
        assert_eq!(output.health.len(), 1);
        assert_eq!(output.health[0].0, INVENTORY.topic);
    }
    Ok(())
}

#[test]
fn profile_data_replacement_resets_only_its_consumers_and_rejects_torn_samples() -> TestResult {
    for mid_sample in [false, true] {
        let mut memory = memory();
        let mut session = session();
        poll(&mut session, &mut memory, &[&PLAYER, &INVENTORY], 0)?;
        let new_control = memory.heap() + 0x7_0000;
        memory.put(new_control, &memory.data().to_le_bytes());
        let field = memory.heap() + 0x4_0000 + 0x208;
        if mid_sample {
            memory.change_on_read(memory.payload(), field, &new_control.to_le_bytes());
        } else {
            memory.put(field, &new_control.to_le_bytes());
        }
        let output = poll(&mut session, &mut memory, &[&PLAYER, &INVENTORY], 1)?;
        assert_eq!(output.resets, [INVENTORY.topic]);
        assert!(output.snapshots.contains(&PLAYER.topic));
        assert_eq!(output.snapshots.contains(&INVENTORY.topic), !mid_sample);
        let recovered = poll(&mut session, &mut memory, &[&PLAYER, &INVENTORY], 2)?;
        assert!(recovered.snapshots.contains(&INVENTORY.topic));
        assert!(recovered.resets.is_empty());
    }
    Ok(())
}

#[test]
fn account_change_still_discards_both_player_and_profile_data() -> TestResult {
    for replace_data_first in [false, true] {
        let mut memory = memory();
        let mut session = session();
        poll(&mut session, &mut memory, &[&PLAYER, &INVENTORY], 0)?;
        if replace_data_first {
            let control = memory.heap() + 0x7_0000;
            memory.put(control, &memory.data().to_le_bytes());
            memory.put(memory.heap() + 0x4_0000 + 0x208, &control.to_le_bytes());
        }
        memory.change_on_read(memory.payload(), memory.account(), OTHER_ACCOUNT);
        let output = poll(&mut session, &mut memory, &[&PLAYER, &INVENTORY], 1)?;
        assert!(output.snapshots.is_empty());
        assert_eq!(output.resets, [INVENTORY.topic, PLAYER.topic]);
        assert_eq!(output.health.len(), 2);
    }
    Ok(())
}
