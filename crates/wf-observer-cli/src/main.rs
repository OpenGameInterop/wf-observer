#[macro_use(derive)]
extern crate derive_aliases;

mod agent;
mod application;
mod cli;
mod derive_alias;
mod identity;
mod launch;
mod lifecycle;
mod paths;
mod prelude;
mod provider_host;
mod providers;
mod runtime;
mod service;
mod singleton;
mod startup;
mod transport;

#[cfg(test)]
mod test_support;

use std::process::ExitCode;

use clap::Parser;

use crate::prelude::*;

#[tokio::main]
#[hotpath::main]
async fn main() -> ExitCode {
    hotpath::tokio_runtime!();

    let args = cli::Cli::parse();

    tracing_subscriber::fmt()
        .with_max_level(args.verbosity)
        .with_writer(std::io::stderr)
        .init();

    let result = match args.command() {
        cli::Command::Start => launch::start().await,
        cli::Command::Status => runtime::print_status(),
        cli::Command::Stop => runtime::stop().await,
        cli::Command::Agent => agent::run().await,
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            error!("{error:#}");
            ExitCode::FAILURE
        }
    }
}
