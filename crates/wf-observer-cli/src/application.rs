//! Transport and shared service-state lifecycle.

use anyhow::Context as _;

use crate::{identity, providers, service::ServiceState, singleton::AgentLock, transport};
use tokio_util::task::AbortOnDropHandle;

/// A running local application and its exclusive instance ownership.
pub(crate) struct RunningApplication {
    lock: AgentLock,
    server: transport::Server,
    state: ServiceState,
    maintenance: AbortOnDropHandle<()>,
}

impl RunningApplication {
    /// Starts the local transport with ownership acquired by its caller.
    pub(crate) async fn start_with_lock(lock: AgentLock) -> anyhow::Result<Self> {
        let secret_key = identity::load_or_create()?;
        let manifests: Vec<_> = providers::PROVIDERS
            .iter()
            .map(|provider| provider.manifest())
            .collect();
        let state = ServiceState::new(&manifests)?;
        let server = transport::start(secret_key, state.view()).await?;
        let watched = state.clone();
        let maintenance = AbortOnDropHandle::new(tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                watched.expire(tokio::time::Instant::now());
            }
        }));

        Ok(Self {
            lock,
            server,
            state,
            maintenance,
        })
    }

    /// Returns the running transport endpoint.
    pub(crate) fn endpoint(&self) -> &iroh::Endpoint {
        self.server.endpoint()
    }

    pub(crate) fn state(&self) -> ServiceState {
        self.state.clone()
    }

    /// Shuts down the transport before releasing instance ownership.
    pub(crate) async fn shutdown(self) -> anyhow::Result<()> {
        let Self {
            lock,
            server,
            state,
            maintenance,
        } = self;
        state.shutdown();
        maintenance.abort();
        let _ = maintenance.await;
        let result = server
            .shutdown()
            .await
            .context("failed to shut down the local transport");
        drop(lock);
        result
    }
}

#[cfg(not(unix))]
pub(crate) async fn wait_for_operating_system_shutdown() -> anyhow::Result<()> {
    tokio::signal::ctrl_c()
        .await
        .context("failed to listen for Ctrl+C")
}

#[cfg(unix)]
pub(crate) async fn wait_for_operating_system_shutdown() -> anyhow::Result<()> {
    use tokio::signal::unix::{SignalKind, signal};

    let mut terminate = signal(SignalKind::terminate()).context("failed to listen for SIGTERM")?;

    tokio::select! {
        result = tokio::signal::ctrl_c() => {
            result.context("failed to listen for SIGINT")?;
        }
        signal = terminate.recv() => {
            signal.context("SIGTERM listener ended unexpectedly")?;
        }
    }

    Ok(())
}
