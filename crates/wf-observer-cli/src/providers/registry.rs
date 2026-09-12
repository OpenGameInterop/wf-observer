//! A fixed registry: registration order defines priority for overlapping matches.

use std::collections::BTreeMap;

use anyhow::Context as _;
use memory_reader::Target;
use provider_sdk::Provider;
use provider_warframe::WarframeProvider;

// A process has one owner. More-specific providers must precede broader ones;
// adding a provider here is an explicit priority decision, not plugin discovery.
pub(crate) static PROVIDERS: &[&dyn Provider] = &[&WarframeProvider];

pub(crate) struct Candidate {
    pub(crate) target: Target,
    pub(crate) provider: &'static dyn Provider,
}

pub(crate) fn discover() -> anyhow::Result<Vec<Candidate>> {
    let mut owners = BTreeMap::new();
    let targets = memory_reader::discover_targets_by(|process| {
        let (provider, executable) = PROVIDERS.iter().find_map(|&provider| {
            provider
                .identify_process(process)
                .map(|executable| (provider, executable))
        })?;
        owners.insert(process.pid, provider);
        Some(executable.to_owned())
    })
    .context("failed to discover supported processes")?;

    // Carry the provider selected from the original enumeration. Never rematch
    // a PID after discovery; Target supplies the process creation marker used
    // for attachment validation and long-lived session identity.
    targets
        .into_iter()
        .map(|target| {
            let provider = owners
                .get(&target.instance().pid())
                .copied()
                .context("discovered target has no provider owner")?;
            Ok(Candidate { target, provider })
        })
        .collect()
}
