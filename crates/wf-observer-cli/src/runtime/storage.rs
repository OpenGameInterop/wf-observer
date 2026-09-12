//! Runtime record persistence and replacement-safe registration ownership.

use std::{
    fs,
    io::{self, Write as _},
    path::{Path, PathBuf},
};

use anyhow::Context as _;
use memory_reader::ProcessInstance;

use crate::{paths, prelude::*, singleton::AgentLock};

use super::record::{HostStatus, RecordedProcess, ServiceInfo};

/// Removes a published record when its owning agent shuts down.
pub(crate) struct Registration {
    pub(super) path: PathBuf,
    pub(super) agent: ProcessInstance,
    pub(super) record: ServiceInfo,
    pub(super) registered: bool,
}

impl Registration {
    /// Atomically publishes a ready service before discovery begins.
    pub(crate) fn publish(endpoint_id: String) -> anyhow::Result<Self> {
        let agent = ProcessInstance::for_pid(std::process::id())
            .context("failed to identify the background agent process")?;
        let record = ServiceInfo::new(agent, endpoint_id);
        let path = paths::runtime_record_path()?;
        remove_optional(&paths::shutdown_request_path()?)
            .context("failed to clear the stale shutdown request")?;
        write_atomic(&path, &record)?;

        Ok(Self {
            path,
            agent,
            record,
            registered: true,
        })
    }

    /// Publishes a host transition without rewriting unchanged runtime metadata.
    pub(crate) fn update(&mut self, host: HostStatus) -> anyhow::Result<()> {
        if self.record.host != host {
            self.record.host = host;
            write_atomic(&self.path, &self.record)?;
        }
        Ok(())
    }

    /// Removes this agent's record without removing a replacement record.
    pub(crate) fn unregister(mut self) -> anyhow::Result<()> {
        let result = remove_if_owned_by(&self.path, self.agent);
        if result.is_ok() {
            self.registered = false;
        }
        result
    }

    /// Returns the exact process instance which owns this registration.
    pub(crate) const fn agent(&self) -> ProcessInstance {
        self.agent
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        if self.registered
            && let Err(error) = remove_if_owned_by(&self.path, self.agent)
        {
            warn!(%error, "failed to remove the agent runtime record");
        }
    }
}

/// Returns the live service even when no targets are alive.
pub(crate) fn current_agent() -> anyhow::Result<Option<ServiceInfo>> {
    let path = paths::runtime_record_path()?;
    let lock_path = paths::agent_lock_path()?;
    current_agent_at(&path, &lock_path)
}

pub(super) fn current_agent_at(
    path: &Path,
    lock_path: &Path,
) -> anyhow::Result<Option<ServiceInfo>> {
    let Some(bytes) = read_optional(path)? else {
        return Ok(None);
    };

    // Unknown/corrupt formats are not evidence that a service is dead. Leave
    // them intact and report the problem instead of hiding a potentially live owner.
    let record = ServiceInfo::decode(&bytes)
        .with_context(|| format!("failed to read runtime record {}", path.display()))?;
    if record.service.process.is_current() {
        return Ok(Some(record));
    }

    remove_stale_record(path, lock_path, &bytes)?;
    Ok(None)
}

pub(super) fn remove_stale_record(
    path: &Path,
    lock_path: &Path,
    expected: &[u8],
) -> anyhow::Result<()> {
    let Some(lock) = AgentLock::try_acquire(lock_path)? else {
        return Ok(());
    };

    if read_optional(path)?.as_deref() == Some(expected) {
        remove_optional(path)?;
    }
    drop(lock);
    Ok(())
}

pub(super) fn remove_if_owned_by(
    path: &Path,
    agent: impl Into<RecordedProcess>,
) -> anyhow::Result<()> {
    let agent = agent.into();
    let Some(bytes) = read_optional(path)? else {
        return Ok(());
    };
    let Ok(record) = ServiceInfo::decode(&bytes) else {
        return Ok(());
    };
    if record.service.process == agent {
        remove_optional(path)?;
    }
    Ok(())
}

pub(super) fn write_atomic(path: &Path, value: &impl serde::Serialize) -> anyhow::Result<()> {
    let parent = path
        .parent()
        .context("the runtime state path does not have a parent directory")?;
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;

    let mut temporary = tempfile::Builder::new()
        .prefix(".state-")
        .tempfile_in(parent)
        .with_context(|| format!("failed to create runtime state in {}", parent.display()))?;
    serde_json::to_writer(&mut temporary, value).context("failed to encode the runtime state")?;
    temporary
        .write_all(b"\n")
        .context("failed to finish the runtime state")?;
    temporary
        .flush()
        .context("failed to flush the runtime state")?;
    temporary
        .persist(path)
        .map_err(|error| error.error)
        .with_context(|| format!("failed to persist {}", path.display()))?;
    Ok(())
}

pub(super) fn read_optional(path: &Path) -> anyhow::Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("failed to read {}", path.display())),
    }
}

pub(super) fn remove_optional(path: &Path) -> anyhow::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("failed to remove {}", path.display())),
    }
}
