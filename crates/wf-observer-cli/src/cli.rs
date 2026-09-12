use clap::{Parser, Subcommand};
use clap_verbosity_flag::{InfoLevel, Verbosity};

/// Runs the local Warframe Observer application.
#[derive(Debug, Parser)]
#[command(name = "wf-observer", version, arg_required_else_help = true)]
pub(crate) struct Cli {
    #[command(flatten)]
    pub verbosity: Verbosity<InfoLevel>,
    #[command(subcommand)]
    command: Command,
}

impl Cli {
    /// Returns the requested command.
    pub(crate) fn command(self) -> Command {
        self.command
    }
}

/// Top-level application commands.
#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Starts a background service that waits for supported games.
    Start,
    /// Reports discovery health and each process's session or attachment failure.
    Status,
    /// Stops the background service, even while no game is running.
    Stop,
    /// Runs the internal background service.
    #[command(name = "_agent", hide = true)]
    Agent,
}
