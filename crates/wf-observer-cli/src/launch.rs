//! Public service startup and background-agent launch.

use std::{path::PathBuf, process::Stdio, time::Duration};

use anyhow::{Context as _, bail};
use tokio::process::{Child, Command};

use crate::{
    paths,
    runtime::{self, ServiceInfo},
    singleton::AgentLock,
    startup::{self, Status},
};

const STARTUP_TIMEOUT: Duration = Duration::from_secs(15);
const FAILED_CHILD_EXIT_TIMEOUT: Duration = Duration::from_secs(2);
const RECONCILIATION_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Starts or reuses the background service without requiring a running game.
pub(crate) async fn start() -> anyhow::Result<()> {
    if reconcile_existing_agent().await? {
        return Ok(());
    }

    launch_agent().await
}

async fn launch_agent() -> anyhow::Result<()> {
    let mut retries_remaining = 1;

    loop {
        match launch_agent_once().await {
            Ok(agent_pid) => {
                println!("Service started (PID {agent_pid})");
                return Ok(());
            }
            Err(error) => match competing_agent().await {
                Ok(CompetingAgent::Compatible(pid)) => {
                    report_already_running(pid);
                    return Ok(());
                }
                Ok(CompetingAgent::Vacant) if retries_remaining > 0 => {
                    retries_remaining -= 1;
                }
                Ok(CompetingAgent::Incompatible | CompetingAgent::Vacant) => return Err(error),
                Err(reconciliation_error) => {
                    return Err(error.context(format!(
                        "failed to inspect the competing background agent: \
                         {reconciliation_error}"
                    )));
                }
            },
        }
    }
}

async fn launch_agent_once() -> anyhow::Result<u32> {
    let mut child = spawn_agent()?;
    let agent_pid = child
        .id()
        .context("the operating system did not report the background agent PID")?;
    let stdout = child
        .stdout
        .take()
        .context("the background agent startup channel was not captured")?;

    let status = match tokio::time::timeout(STARTUP_TIMEOUT, startup::read(stdout)).await {
        Ok(Ok(status)) => status,
        Ok(Err(error)) => {
            reap_failed_child(&mut child).await?;
            return Err(anyhow::Error::new(error)
                .context("failed to read the background agent startup status"));
        }
        Err(_) => {
            terminate_child(&mut child).await?;
            bail!("background agent did not start within {STARTUP_TIMEOUT:?}");
        }
    };

    match status {
        Status::Ready => {
            if let Some(exit_status) = child
                .try_wait()
                .context("failed to inspect the background agent")?
            {
                bail!("background agent exited during startup: {exit_status}");
            }

            Ok(agent_pid)
        }
        Status::Failed(message) => {
            reap_failed_child(&mut child).await?;
            Err(anyhow::Error::msg(message))
        }
    }
}

async fn reconcile_existing_agent() -> anyhow::Result<bool> {
    let Some(agent) = runtime::current_agent()? else {
        return Ok(false);
    };

    if is_compatible(&agent) {
        report_already_running(agent.pid());
        return Ok(true);
    }

    if agent.version() != env!("CARGO_PKG_VERSION") {
        println!("Existing service version: {}", agent.version());
        println!("Installed version:      {}", env!("CARGO_PKG_VERSION"));
    }
    println!("Replacing existing service...");
    runtime::stop_agent(&agent)
        .await
        .context("failed to stop the existing background agent")?;
    Ok(false)
}

fn is_compatible(agent: &ServiceInfo) -> bool {
    agent.is_compatible_with(env!("CARGO_PKG_VERSION"))
}

fn report_already_running(pid: u32) {
    println!("Service already running (PID {pid})");
}

#[derive(Debug, ..Copy, ..Eq)]
enum CompetingAgent {
    Compatible(u32),
    Incompatible,
    Vacant,
}

async fn competing_agent() -> anyhow::Result<CompetingAgent> {
    let lock_path = paths::agent_lock_path()?;
    let deadline = tokio::time::Instant::now() + STARTUP_TIMEOUT;

    loop {
        if let Some(agent) = runtime::current_agent()? {
            return Ok(if is_compatible(&agent) {
                CompetingAgent::Compatible(agent.pid())
            } else {
                CompetingAgent::Incompatible
            });
        }

        if let Some(lock) = AgentLock::try_acquire(&lock_path)? {
            drop(lock);
            return Ok(CompetingAgent::Vacant);
        }
        if tokio::time::Instant::now() >= deadline {
            bail!("competing background agent did not become ready within {STARTUP_TIMEOUT:?}");
        }

        tokio::time::sleep(RECONCILIATION_POLL_INTERVAL).await;
    }
}

fn spawn_agent() -> anyhow::Result<Child> {
    let executable = current_executable()?;
    let mut command = Command::new(executable);
    command
        .arg("_agent")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    configure_detached(&mut command);

    command
        .spawn()
        .context("failed to start the background agent")
}

fn current_executable() -> anyhow::Result<PathBuf> {
    std::env::current_exe().context("failed to locate the current wf-observer executable")
}

async fn reap_failed_child(child: &mut Child) -> anyhow::Result<()> {
    match tokio::time::timeout(FAILED_CHILD_EXIT_TIMEOUT, child.wait()).await {
        Ok(result) => {
            result.context("failed to wait for the failed background agent")?;
            Ok(())
        }
        Err(_) => terminate_child(child).await,
    }
}

async fn terminate_child(child: &mut Child) -> anyhow::Result<()> {
    if child
        .try_wait()
        .context("failed to inspect the background agent before termination")?
        .is_some()
    {
        return Ok(());
    }

    child
        .start_kill()
        .context("failed to terminate the background agent")?;
    child
        .wait()
        .await
        .context("failed to reap the background agent")?;
    Ok(())
}

#[cfg(windows)]
fn configure_detached(command: &mut Command) {
    use windows_sys::Win32::System::Threading::{CREATE_NEW_PROCESS_GROUP, DETACHED_PROCESS};

    command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
}

#[cfg(not(windows))]
fn configure_detached(_command: &mut Command) {}
