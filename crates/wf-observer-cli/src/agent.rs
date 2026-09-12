//! Persistent background service lifecycle.

use std::{sync::mpsc, time::Duration};

use anyhow::Context as _;
use memory_reader::ProcessInstance;

use crate::{
    application::{self, RunningApplication},
    lifecycle, paths,
    prelude::*,
    runtime::{self, Registration},
    singleton::AgentLock,
    startup,
};

const SHUTDOWN_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Runs the hidden background-service entrypoint without requiring a target.
pub(crate) async fn run() -> anyhow::Result<()> {
    let agent = match RunningAgent::start().await {
        Ok(agent) => agent,
        Err(error) => return Err(report_startup_failure(error)),
    };

    if let Err(error) = startup::report_ready().context("failed to report service readiness") {
        return agent.shutdown(Err(error)).await;
    }

    agent.run().await
}

struct RunningAgent {
    // Keep this before `application` so fallback cleanup runs while its lock is held.
    registration: Registration,
    application: RunningApplication,
}

impl RunningAgent {
    async fn start() -> anyhow::Result<Self> {
        detach_current_process()?;

        let lock = AgentLock::acquire(&paths::agent_lock_path()?)?;
        let application = RunningApplication::start_with_lock(lock).await?;
        let registration = match Registration::publish(application.endpoint().id().to_string()) {
            Ok(registration) => registration,
            Err(error) => {
                return match application.shutdown().await {
                    Ok(()) => Err(error),
                    Err(shutdown_error) => Err(error.context(format!(
                        "failed to shut down after the runtime record could not be published: \
                         {shutdown_error}"
                    ))),
                };
            }
        };

        info!(endpoint_id = %application.endpoint().id(), "background service started");
        Ok(Self {
            registration,
            application,
        })
    }

    async fn run(self) -> anyhow::Result<()> {
        let Self {
            registration,
            application,
        } = self;
        let instance = registration.agent();
        let (stop_tx, stop_rx) = mpsc::channel();
        let state = application.state();
        let worker_state = state.clone();
        let mut worker =
            tokio::task::spawn_blocking(move || run_worker(registration, &stop_rx, &worker_state));

        let result = tokio::select! {
            result = &mut worker => result.context("discovery worker failed").and_then(|result| result),
            shutdown = wait_for_shutdown(instance) => {
                // Fence in-flight native reads before waiting for the worker.
                state.shutdown();
                let _ = stop_tx.send(());
                let joined = worker.await.context("discovery worker failed").and_then(|result| result);
                shutdown.and(joined)
            }
        };

        // The worker has dropped all attachments and its registration before transport
        // shutdown releases the singleton lock.
        let shutdown = application.shutdown().await;
        info!("background service stopped");
        result.and(shutdown)
    }

    async fn shutdown(self, reason: anyhow::Result<()>) -> anyhow::Result<()> {
        let Self {
            application,
            registration,
        } = self;
        let clear_request = runtime::clear_shutdown_request(registration.agent());
        let unregister = registration.unregister();
        let shutdown = application.shutdown().await;
        reason.and(clear_request).and(unregister).and(shutdown)
    }
}

fn run_worker(
    mut registration: Registration,
    stop: &mpsc::Receiver<()>,
    state: &crate::service::ServiceState,
) -> anyhow::Result<()> {
    let result = lifecycle::run(&mut registration, stop, state);
    state.shutdown();
    let clear_request = runtime::clear_shutdown_request(registration.agent());
    let unregister = registration.unregister();
    result.and(clear_request).and(unregister)
}

async fn wait_for_shutdown(agent: ProcessInstance) -> anyhow::Result<()> {
    tokio::select! {
        result = application::wait_for_operating_system_shutdown() => result,
        result = wait_for_shutdown_request(agent) => result,
    }
}

async fn wait_for_shutdown_request(agent: ProcessInstance) -> anyhow::Result<()> {
    loop {
        if runtime::shutdown_requested(agent)? {
            return Ok(());
        }
        tokio::time::sleep(SHUTDOWN_POLL_INTERVAL).await;
    }
}

fn report_startup_failure(error: anyhow::Error) -> anyhow::Error {
    match startup::send_failure(&error) {
        Ok(()) => error,
        Err(report_error) => error.context(format!(
            "failed to report the service startup failure: {report_error}"
        )),
    }
}

#[cfg(unix)]
fn detach_current_process() -> anyhow::Result<()> {
    rustix::process::setsid()
        .map(|_| ())
        .context("failed to detach the background service from the invoking terminal")
}

#[cfg(not(unix))]
fn detach_current_process() -> anyhow::Result<()> {
    Ok(())
}
