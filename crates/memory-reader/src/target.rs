//! Stable identities for target processes.

/// Borrowed process metadata supplied to a discovery matcher.
///
/// Values come from the native process view and may be empty, truncated, or
/// changed by the process. Matching and executable-name normalization belong to
/// the caller; this metadata is not a stable process identity.
#[derive(Debug, ..Copy, ..Eq)]
pub struct ProcessMetadata<'a> {
    /// PID for correlating a matcher result within this enumeration, not a stable identity.
    pub pid: u32,
    /// Process name reported by the native backend.
    pub name: &'a str,
    /// Executable path reported by the native backend.
    pub path: &'a str,
    /// Command line reported by the native backend.
    pub command_line: &'a str,
}

/// Opaque identity of one operating-system process instance.
#[derive(Debug, Hash, ..Copy, ..Eq)]
pub struct ProcessInstance {
    pid: u32,
    start_marker: u64,
}

impl ProcessInstance {
    pub(crate) const fn new(pid: u32, start_marker: u64) -> Self {
        Self { pid, start_marker }
    }

    /// Resolves the current operating-system identity for a process identifier.
    ///
    /// Returns `None` when the process does not exist or its creation marker is unavailable.
    #[must_use]
    pub fn for_pid(pid: u32) -> Option<Self> {
        process_start_marker(pid).map(|start_marker| Self::new(pid, start_marker))
    }

    /// Returns the operating-system process identifier.
    #[must_use]
    pub const fn pid(self) -> u32 {
        self.pid
    }

    /// Returns the platform-specific process creation marker.
    #[must_use]
    pub const fn start_marker(self) -> u64 {
        self.start_marker
    }

    /// Returns whether this identity still describes a live process instance.
    #[must_use]
    pub fn is_current(self) -> bool {
        process_start_marker(self.pid) == Some(self.start_marker)
    }
}

/// One process selected by a caller's discovery matcher.
#[derive(Clone, Debug, ..Eq)]
pub struct Target {
    instance: ProcessInstance,
    executable: String,
}

impl Target {
    /// Constructs target metadata, without opening a process or proving read access.
    ///
    /// The executable names the mapping to probe. [`crate::attach`] still validates
    /// the process instance and mapping before retaining an attachment.
    #[must_use]
    pub fn new(instance: ProcessInstance, executable: String) -> Self {
        Self {
            instance,
            executable,
        }
    }

    /// Returns the exact operating-system process instance.
    #[must_use]
    pub const fn instance(&self) -> ProcessInstance {
        self.instance
    }

    /// Returns the canonical executable/module name supplied by the discovery matcher.
    #[must_use]
    pub fn executable(&self) -> &str {
        &self.executable
    }
}

#[cfg(target_os = "linux")]
fn process_start_marker(pid: u32) -> Option<u64> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    start_marker_from_stat(&stat)
}

#[cfg(target_os = "linux")]
fn start_marker_from_stat(stat: &str) -> Option<u64> {
    let (_, fields) = stat.rsplit_once(") ")?;
    let mut fields = fields.split_whitespace();
    // Exited processes retain their starttime until reaped.
    if matches!(fields.next()?, "Z" | "X" | "x") {
        return None;
    }
    fields.nth(18)?.parse().ok()
}

#[cfg(not(target_os = "linux"))]
fn process_start_marker(pid: u32) -> Option<u64> {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

    let pid = Pid::from_u32(pid);
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing().without_tasks(),
    );
    system.process(pid).map(sysinfo::Process::start_time)
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::start_marker_from_stat;

    #[test]
    fn stat_markers_preserve_parenthesized_names_and_reject_exited_processes() {
        for (state, expected) in [
            ("R", Some(987_654)),
            ("S", Some(987_654)),
            ("D", Some(987_654)),
            ("T", Some(987_654)),
            ("t", Some(987_654)),
            ("I", Some(987_654)),
            ("Z", None),
            ("X", None),
            ("x", None),
        ] {
            // Fields 4–21 precede starttime. The name may itself contain ") ".
            let stat = format!(
                "123 (odd ) process (name)) {state} 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 987654 0"
            );
            assert_eq!(start_marker_from_stat(&stat), expected, "state {state}");
        }
        assert_eq!(start_marker_from_stat("123 (truncated) S 1 2"), None);
    }
}
