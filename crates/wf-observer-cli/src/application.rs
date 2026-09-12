//! Foreground application lifecycle.

use std::io::{self, Write as _};

use anyhow::Context as _;
use iroh_tickets::endpoint::EndpointTicket;

use crate::{identity, paths, prelude::*, singleton::AgentLock, transport};

/// A running local application and its exclusive instance ownership.
pub(crate) struct RunningApplication {
    lock: AgentLock,
    server: transport::Server,
}

impl RunningApplication {
    /// Acquires instance ownership and starts the local transport.
    pub(crate) async fn start() -> anyhow::Result<Self> {
        let lock = AgentLock::acquire(&paths::agent_lock_path()?)?;
        Self::start_with_lock(lock).await
    }

    /// Starts the local transport with ownership acquired by its caller.
    pub(crate) async fn start_with_lock(lock: AgentLock) -> anyhow::Result<Self> {
        let secret_key = identity::load_or_create()?;
        let server = transport::start(secret_key).await?;

        Ok(Self { lock, server })
    }

    /// Returns the running transport endpoint.
    pub(crate) fn endpoint(&self) -> &iroh::Endpoint {
        self.server.endpoint()
    }

    /// Prints a connection ticket once the endpoint has had a chance to register.
    pub(crate) async fn print_ticket(&self) -> anyhow::Result<()> {
        if tokio::time::timeout(
            std::time::Duration::from_secs(iroh::NET_REPORT_TIMEOUT),
            self.endpoint().online(),
        )
        .await
        .is_err()
        {
            warn!("relay registration timed out; printing the available endpoint addresses");
        }

        let ticket = EndpointTicket::new(self.endpoint().addr());
        let mut stdout = io::stdout().lock();

        writeln!(stdout, "WF_OBSERVER_ENDPOINT_TICKET={ticket}")
            .context("failed to print the endpoint ticket")?;
        stdout
            .flush()
            .context("failed to flush the endpoint ticket")
    }

    /// Shuts down the transport before releasing instance ownership.
    pub(crate) async fn shutdown(self) -> anyhow::Result<()> {
        let Self { lock, server } = self;
        let result = server
            .shutdown()
            .await
            .context("failed to shut down the local transport");
        drop(lock);
        result
    }
}

/// Runs until an operating-system or supervising-process shutdown arrives.
pub(crate) async fn run(print_ticket: bool, shutdown_on_stdin_close: bool) -> anyhow::Result<()> {
    let application = RunningApplication::start().await?;

    if print_ticket {
        application.print_ticket().await?;
    }

    info!(endpoint_id = %application.endpoint().id(), "local application started");

    let shutdown_result = wait_for_shutdown(shutdown_on_stdin_close).await;
    if shutdown_result.is_ok() {
        info!("shutdown requested");
    }

    let server_result = application.shutdown().await;
    info!("local application stopped");

    shutdown_result?;
    server_result
}

async fn wait_for_shutdown(shutdown_on_stdin_close: bool) -> anyhow::Result<()> {
    if !shutdown_on_stdin_close {
        return wait_for_operating_system_shutdown().await;
    }

    tokio::select! {
        result = wait_for_operating_system_shutdown() => result,
        result = wait_for_stdin_close() => result,
    }
}

async fn wait_for_stdin_close() -> anyhow::Result<()> {
    use tokio::io::AsyncReadExt as _;

    let mut stdin = tokio::io::stdin();
    let mut buffer = [0_u8; 1024];

    loop {
        if stdin
            .read(&mut buffer)
            .await
            .context("failed to listen for the supervising process")?
            == 0
        {
            return Ok(());
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn ping_round_trip() -> anyhow::Result<()> {
        let server = transport::start(iroh::SecretKey::generate()).await?;
        let address = server.endpoint().addr();

        let exchange_result = async {
            let client = wf_observer_sdk::Client::connect(address).await?;
            let ping_result = client.ping().await;
            client.close().await;
            ping_result
        }
        .await;

        let shutdown_result = server
            .shutdown()
            .await
            .context("failed to shut down the test Iroh transport");

        exchange_result?;
        shutdown_result
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn ffi_ping_round_trip() -> anyhow::Result<()> {
        let server = transport::start(iroh::SecretKey::generate()).await?;
        let ticket = iroh_tickets::endpoint::EndpointTicket::new(server.endpoint().addr());

        let exchange_result = async {
            let client = wf_observer_ffi::connect(ticket.to_string())
                .await
                .map_err(anyhow::Error::msg)?;
            client.ping().await.map_err(anyhow::Error::msg)?;
            client.shutdown().await.map_err(anyhow::Error::msg)
        }
        .await;

        let shutdown_result = server
            .shutdown()
            .await
            .context("failed to shut down the FFI test Iroh transport");

        exchange_result?;
        shutdown_result
    }
}
