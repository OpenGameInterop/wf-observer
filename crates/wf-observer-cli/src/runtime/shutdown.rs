//! Cooperative shutdown requests and exact-owner exit waiting.

use std::{path::Path, time::Duration};

use anyhow::bail;
use memory_reader::ProcessInstance;

use crate::paths;

use super::{
    record::{RecordedProcess, ServiceInfo},
    storage::{current_agent, read_optional, remove_optional, write_atomic},
};

const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);
const SHUTDOWN_POLL_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug, ..Eq, ..Serde)]
pub(super) struct ShutdownRequest {
    pub(super) agent: RecordedProcess,
}

/// Requests cooperative shutdown and waits for the current agent to exit.
pub(crate) async fn stop() -> anyhow::Result<()> {
    let Some(record) = current_agent()? else {
        println!("Service is not running");
        return Ok(());
    };

    println!("Stopping service (PID {})...", record.pid());
    stop_agent(&record).await?;
    println!("Service stopped");
    Ok(())
}

/// Requests cooperative shutdown of `agent` and waits for that process instance to exit.
pub(crate) async fn stop_agent(agent: &ServiceInfo) -> anyhow::Result<()> {
    let instance = agent.service.process;
    let path = paths::shutdown_request_path()?;
    write_atomic(&path, &ShutdownRequest { agent: instance })?;

    wait_for_exit(instance).await?;
    remove_shutdown_request_at(&path, instance)?;
    Ok(())
}

/// Returns whether the current shutdown request names `agent`.
pub(crate) fn shutdown_requested(agent: ProcessInstance) -> anyhow::Result<bool> {
    shutdown_requested_at(&paths::shutdown_request_path()?, agent)
}

/// Removes a shutdown request only when it names `agent`.
pub(crate) fn clear_shutdown_request(agent: ProcessInstance) -> anyhow::Result<()> {
    remove_shutdown_request_at(&paths::shutdown_request_path()?, agent)
}

async fn wait_for_exit(agent: RecordedProcess) -> anyhow::Result<()> {
    let result = tokio::time::timeout(SHUTDOWN_TIMEOUT, async {
        while agent.is_current() {
            tokio::time::sleep(SHUTDOWN_POLL_INTERVAL).await;
        }
    })
    .await;

    if result.is_err() {
        bail!(
            "agent process {} did not stop within {SHUTDOWN_TIMEOUT:?}",
            agent.pid
        );
    }
    Ok(())
}

pub(super) fn shutdown_requested_at(
    path: &Path,
    agent: impl Into<RecordedProcess>,
) -> anyhow::Result<bool> {
    let agent = agent.into();
    let Some(bytes) = read_optional(path)? else {
        return Ok(false);
    };
    let Ok(request) = serde_json::from_slice::<ShutdownRequest>(&bytes) else {
        return Ok(false);
    };
    Ok(request.agent == agent)
}

pub(super) fn remove_shutdown_request_at(
    path: &Path,
    agent: impl Into<RecordedProcess>,
) -> anyhow::Result<()> {
    let agent = agent.into();
    let Some(bytes) = read_optional(path)? else {
        return Ok(());
    };
    let Ok(request) = serde_json::from_slice::<ShutdownRequest>(&bytes) else {
        return Ok(());
    };
    if request.agent == agent {
        remove_optional(path)?;
    }
    Ok(())
}
