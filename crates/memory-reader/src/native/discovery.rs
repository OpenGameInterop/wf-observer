//! Caller-selected process enumeration through memflow-native.

use memflow::prelude::v1::{Os, ProcessInfo};

use crate::{DiscoveryError, ProcessInstance, ProcessMetadata, Target};

use super::access::native_os;

/// Discovers caller-selected processes using the same native view used for attachment.
///
/// The matcher returns the canonical executable/module name to use for the
/// attachment's read-access probe, or `None` to ignore a process. It is called
/// once for each enumerated process. Results are sorted by PID; matches whose
/// process creation marker is unavailable are omitted. Metadata can become stale
/// during enumeration, so [`crate::attach`] rechecks identity and the selected mapping.
///
/// # Errors
///
/// Returns an error if memflow-native cannot initialize or enumerate host processes.
pub fn discover_targets_by<F>(matcher: F) -> Result<Vec<Target>, DiscoveryError>
where
    F: FnMut(&ProcessMetadata<'_>) -> Option<String>,
{
    let mut os = native_os()?;
    let processes = os.process_info_list().map_err(DiscoveryError::Enumerate)?;
    Ok(targets_from_processes(processes, matcher))
}

pub(super) fn targets_from_processes(
    processes: Vec<ProcessInfo>,
    mut matcher: impl FnMut(&ProcessMetadata<'_>) -> Option<String>,
) -> Vec<Target> {
    let mut targets = processes
        .into_iter()
        .filter_map(|process| {
            let metadata = ProcessMetadata {
                pid: process.pid,
                name: process.name.as_ref(),
                path: process.path.as_ref(),
                command_line: process.command_line.as_ref(),
            };
            let executable = matcher(&metadata)?;
            let instance = ProcessInstance::for_pid(process.pid)?;
            Some(Target::new(instance, executable))
        })
        .collect::<Vec<_>>();
    targets.sort_by_key(|target| target.instance().pid());
    targets
}
