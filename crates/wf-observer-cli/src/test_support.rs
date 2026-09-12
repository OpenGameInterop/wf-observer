//! Service test data and a four-byte counter provider.

use crate::{
    runtime,
    service::{PollBatch, ServiceState},
};
use anyhow::Context as _;
use memory_reader::{
    AccessError, MemoryModule, ProcessInstance, ProcessMemory, ProcessMetadata, Target,
};
use protocol::v1 as wire;
use provider_sdk::{
    CapabilityDescriptor, EventSink as _, GameDescriptor, PollContext, PollResult, Provider,
    ProviderError, ProviderManifest, ProviderSession,
};

pub(crate) static CAPS: &[CapabilityDescriptor] = &[
    CapabilityDescriptor {
        topic: "fixture.count",
        schema_version: 1,
        snapshots: Some(provider_sdk::SnapshotDelivery::Full),
        events: true,
    },
    CapabilityDescriptor {
        topic: "fixture.shared",
        schema_version: 1,
        snapshots: Some(provider_sdk::SnapshotDelivery::Full),
        events: false,
    },
];
pub(crate) static MANIFEST: ProviderManifest = ProviderManifest {
    id: "fixture.provider",
    name: "Fixture",
    version: "test",
    game: GameDescriptor {
        id: "fixture",
        name: "Fixture",
    },
    capabilities: CAPS,
};

pub(crate) struct FixtureProvider;
pub(crate) static PROVIDER: FixtureProvider = FixtureProvider;

impl Provider for FixtureProvider {
    fn manifest(&self) -> &'static ProviderManifest {
        &MANIFEST
    }
    fn identify_process(&self, process: &ProcessMetadata<'_>) -> Option<&'static str> {
        (process.name == "fixture.exe").then_some("fixture.exe")
    }
    fn start(
        &self,
        _: &Target,
        _: &mut dyn ProcessMemory,
    ) -> Result<Box<dyn ProviderSession>, ProviderError> {
        Ok(Box::new(FixtureSession))
    }
}

struct FixtureSession;
impl ProviderSession for FixtureSession {
    fn poll(&mut self, context: &mut PollContext<'_>) -> Result<PollResult, ProviderError> {
        if context.demand.is_empty() {
            return Ok(PollResult::Idle);
        }
        let mut bytes = [0; 4];
        context.memory.read_into(0x1000, &mut bytes)?;
        let value = serde_json::json!({ "count": u32::from_le_bytes(bytes) });
        for cap in context.demand {
            context.events.snapshot(cap, &value)?;
            if cap.events {
                context.events.event(cap, &value)?;
            }
        }
        Ok(PollResult::After(std::time::Duration::from_millis(50)))
    }
}

pub(crate) struct FixtureMemory {
    target: Target,
    pub(crate) value: u32,
    pub(crate) reads: usize,
    pub(crate) failed: bool,
}
impl FixtureMemory {
    pub(crate) fn new() -> anyhow::Result<Self> {
        let instance =
            ProcessInstance::for_pid(std::process::id()).context("test process missing")?;
        Ok(Self {
            target: Target::new(instance, "fixture.exe".into()),
            value: 7,
            reads: 0,
            failed: false,
        })
    }
}
impl ProcessMemory for FixtureMemory {
    fn target(&self) -> &Target {
        &self.target
    }
    fn verify(&mut self) -> Result<(), AccessError> {
        Ok(())
    }
    fn modules(&mut self) -> Result<Vec<MemoryModule>, AccessError> {
        Ok(vec![])
    }
    fn read_into(&mut self, address: u64, buffer: &mut [u8]) -> Result<(), AccessError> {
        self.reads += 1;
        if self.failed || address != 0x1000 || buffer.len() != 4 {
            return Err(AccessError::InvalidReadRange {
                address,
                length: buffer.len(),
            });
        }
        buffer.copy_from_slice(&self.value.to_le_bytes());
        Ok(())
    }
}

pub(crate) fn state() -> anyhow::Result<ServiceState> {
    ServiceState::new(&[&MANIFEST])
}

pub(crate) fn lag_batch(state: &ServiceState) -> anyhow::Result<PollBatch> {
    let batch = PollBatch::new(&MANIFEST);
    batch
        .events()
        .snapshot(&CAPS[0], &serde_json::json!({"count": 7}))?;
    let padding = "x".repeat(1024);
    let count = state.view().message_bytes() as usize / padding.len() + 1;
    let payload = serde_json::json!({"padding": padding});
    for _ in 0..count {
        batch.events().event(&CAPS[0], &payload)?;
    }
    Ok(batch)
}

pub(crate) fn observe(state: &ServiceState, sessions: &[&str]) -> anyhow::Result<()> {
    state.update_lifecycle(runtime::HostStatus {
        discovery_error: None,
        targets: sessions
            .iter()
            .zip(1_u32..)
            .map(|(id, pid)| runtime::TargetStatus {
                target: runtime::TargetInfo {
                    process: runtime::RecordedProcess {
                        pid,
                        start_marker: 1,
                    },
                    executable: "fixture.exe".into(),
                    provider_id: MANIFEST.id.into(),
                    game_id: MANIFEST.game.id.into(),
                },
                activity: runtime::Activity::Observing {
                    session_id: (*id).into(),
                },
            })
            .collect(),
    })?;
    Ok(())
}

pub(crate) fn topic(index: usize) -> wire::TopicRef {
    let cap = &CAPS[index];
    wire::TopicRef {
        provider_id: MANIFEST.id.into(),
        topic: cap.topic.into(),
        schema_version: cap.schema_version,
    }
}

pub(crate) fn request(state: &ServiceState, id: &str) -> anyhow::Result<wire::GetSnapshot> {
    Ok(wire::GetSnapshot {
        session: wire::SessionRef {
            run_id: state.view().status()?.cursor.run_id,
            session_id: id.into(),
        },
        topic: topic(0),
    })
}

pub(crate) fn selection() -> wire::Subscribe {
    wire::Subscribe {
        sessions: wire::SessionSelector::All,
        topics: vec![topic(0)],
    }
}
