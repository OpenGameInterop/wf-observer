//! Runtime metadata and exact process identities.

use anyhow::{Context as _, ensure};
use derive_more::Display;
use memory_reader::ProcessInstance;

pub(super) const RECORD_VERSION: u32 = 1;

/// The current runtime record for a service and its observed processes.
#[derive(Debug, ..Eq, ..Serde)]
pub(crate) struct ServiceInfo {
    pub(super) schema_version: u32,
    pub(super) service: ServiceIdentity,
    #[serde(flatten)]
    pub(super) host: HostStatus,
}

/// Discovery health and the independent state of each supported process.
#[derive(Debug, Clone, Default, ..Eq, ..Serde)]
pub(crate) struct HostStatus {
    pub(crate) discovery_error: Option<String>,
    pub(crate) targets: Vec<TargetStatus>,
}

/// Metadata and provider ownership for one process, without a native attachment.
#[derive(Debug, Clone, ..Eq, ..Serde)]
pub(crate) struct TargetInfo {
    pub(crate) process: RecordedProcess,
    pub(crate) executable: String,
    pub(crate) provider_id: String,
    pub(crate) game_id: String,
}

/// The state of one supported process; failed attempts do not create sessions.
#[derive(Debug, Clone, ..Eq, ..Serde)]
pub(crate) struct TargetStatus {
    #[serde(flatten)]
    pub(crate) target: TargetInfo,
    #[serde(flatten)]
    pub(crate) activity: Activity,
}

#[derive(Debug, Clone, Display, ..Eq, ..Serde)]
#[serde(tag = "state", rename_all = "snake_case")]
pub(crate) enum Activity {
    #[display("attaching")]
    Attaching,
    #[display("observing")]
    Observing { session_id: String },
    #[display("retrying")]
    Retrying { error: String },
}

#[derive(Debug, ..Eq, ..Serde)]
pub(super) struct ServiceIdentity {
    pub(super) application_version: String,
    pub(super) process: RecordedProcess,
    pub(super) endpoint_id: String,
}

impl Activity {
    pub(crate) fn observing() -> anyhow::Result<Self> {
        let mut bytes = [0_u8; 16];
        getrandom::fill(&mut bytes).context("failed to generate a session ID")?;
        Ok(Self::Observing {
            session_id: format!("{:032x}", u128::from_le_bytes(bytes)),
        })
    }
}

impl ServiceInfo {
    pub(super) fn new(service: ProcessInstance, endpoint_id: String) -> Self {
        Self {
            schema_version: RECORD_VERSION,
            service: ServiceIdentity {
                application_version: env!("CARGO_PKG_VERSION").to_owned(),
                process: service.into(),
                endpoint_id,
            },
            host: HostStatus::default(),
        }
    }

    pub(super) fn decode(bytes: &[u8]) -> anyhow::Result<Self> {
        let record: Self = serde_json::from_slice(bytes)?;
        ensure!(
            record.schema_version == RECORD_VERSION,
            "unsupported runtime record schema version: {}",
            record.schema_version
        );
        Ok(record)
    }

    /// Returns the version of the executable which launched this agent.
    pub(crate) fn version(&self) -> &str {
        &self.service.application_version
    }

    /// Returns the agent's operating-system process identifier.
    pub(crate) fn pid(&self) -> u32 {
        self.service.process.pid
    }

    /// Returns whether the running service can be reused, regardless of its sessions.
    pub(crate) fn is_compatible_with(&self, version: &str) -> bool {
        self.version() == version
    }
}

#[derive(Debug, ..Copy, ..Ord, ..Serde)]
pub(crate) struct RecordedProcess {
    pub(crate) pid: u32,
    pub(crate) start_marker: u64,
}

impl RecordedProcess {
    fn current_instance(self) -> Option<ProcessInstance> {
        ProcessInstance::for_pid(self.pid)
            .filter(|instance| instance.start_marker() == self.start_marker)
    }

    pub(super) fn is_current(self) -> bool {
        self.current_instance().is_some()
    }
}

impl From<ProcessInstance> for RecordedProcess {
    fn from(instance: ProcessInstance) -> Self {
        Self {
            pid: instance.pid(),
            start_marker: instance.start_marker(),
        }
    }
}
