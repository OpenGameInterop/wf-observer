//! Opt-in compatibility smoke test against a running, logged-in game.
use fastant::Instant;
use memory_reader::{AccessError, AttachedTarget, MemoryModule, ProcessMemory, Target};
use provider_sdk::{
    CapabilityDescriptor, CapabilityHealth, EventSink, HealthSink, PollContext, Provider,
    ProviderError,
};
use provider_warframe::WarframeProvider;
use std::time::Duration;

struct Counted {
    memory: AttachedTarget,
    reads: usize,
    bytes: usize,
}
impl ProcessMemory for Counted {
    fn target(&self) -> &Target {
        self.memory.target()
    }
    fn verify(&mut self) -> Result<(), AccessError> {
        self.memory.verify()
    }
    fn modules(&mut self) -> Result<Vec<MemoryModule>, AccessError> {
        self.memory.modules()
    }
    fn read_into(&mut self, at: u64, output: &mut [u8]) -> Result<(), AccessError> {
        self.reads += 1;
        self.bytes += output.len();
        self.memory.read_into(at, output)
    }
}

#[derive(Default)]
struct Output {
    snapshots: Vec<(&'static str, serde_json::Value)>,
    health: Vec<(&'static str, CapabilityHealth)>,
}
impl EventSink for Output {
    fn reset(&mut self, _: &CapabilityDescriptor) -> Result<(), ProviderError> {
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
        unreachable!("snapshot-only test")
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

#[test]
#[ignore = "requires one running, logged-in Warframe instance and read access"]
fn current_game_mastery() -> Result<(), Box<dyn std::error::Error>> {
    let provider = WarframeProvider;
    let targets = memory_reader::discover_targets_by(|metadata| {
        provider.identify_process(metadata).map(str::to_owned)
    })?;
    let [target] = targets.as_slice() else {
        return Err("expected exactly one Warframe process".into());
    };
    let mut memory = Counted {
        memory: memory_reader::attach(target)?,
        reads: 0,
        bytes: 0,
    };
    let mut session = provider.start(target, &mut memory)?;
    let demand: Vec<_> = provider
        .manifest()
        .capabilities
        .iter()
        .filter(|cap| cap.topic == "warframe.mastery")
        .collect();
    assert_eq!(demand.len(), 1);
    for second in 0..2 {
        memory.reads = 0;
        memory.bytes = 0;
        let mut events = Output::default();
        let mut health = Output::default();
        memory.verify()?;
        let started = Instant::now();
        let next = session.poll(&mut PollContext {
            demand: &demand,
            now: Duration::from_secs(second),
            memory: &mut memory,
            events: &mut events,
            health: &mut health,
        })?;
        let elapsed = started.elapsed();
        memory.verify()?;
        session.poll_completed(true);
        assert_eq!(events.snapshots.len(), 1, "health: {:?}", health.health);
        for (_, value) in events.snapshots {
            let value: warframe_model::MasterySnapshot = serde_json::from_value(value)?;
            println!(
                "rank {}, {} total points, {} item points, {} retained items",
                value.rank(),
                value.total_points(),
                value.item_points(),
                value.tracked_items()
            );
        }
        println!(
            "sample {second}: {} reads, {} bytes, {elapsed:?}; next {next:?}",
            memory.reads, memory.bytes
        );
    }
    Ok(())
}
