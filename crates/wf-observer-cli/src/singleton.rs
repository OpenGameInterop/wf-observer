//! Single-process ownership for the local agent.

use std::{
    fs::{self, File, OpenOptions, TryLockError},
    path::Path,
};

use anyhow::Context as _;

/// Serializes start, stop, and access changes without blocking the agent's startup.
pub(crate) fn command_lock() -> anyhow::Result<AgentLock> {
    AgentLock::try_acquire(&crate::paths::command_lock_path()?)?
        .context("another service-management command is running; retry when it finishes")
}

/// An exclusive lock held for the lifetime of the local agent.
pub(crate) struct AgentLock {
    #[allow(
        dead_code,
        reason = "retaining the file handle keeps the singleton lock active"
    )]
    file: File,
}

impl AgentLock {
    /// Attempts to acquire exclusive ownership at `path`.
    pub(crate) fn acquire(path: &Path) -> anyhow::Result<Self> {
        Self::try_acquire(path)?.context("Warframe Observer is already running")
    }

    /// Attempts to acquire ownership without treating an existing owner as an error.
    pub(crate) fn try_acquire(path: &Path) -> anyhow::Result<Option<Self>> {
        let directory = path
            .parent()
            .context("the agent lock path does not have a parent directory")?;
        fs::create_dir_all(directory)
            .with_context(|| format!("failed to create {}", directory.display()))?;

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .with_context(|| format!("failed to open {}", path.display()))?;

        match file.try_lock() {
            Ok(()) => Ok(Some(Self { file })),
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Error(error)) => {
                Err(error).with_context(|| format!("failed to lock {}", path.display()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_a_second_owner_until_the_first_is_dropped() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("agent.lock");
        let first = AgentLock::acquire(&path)?;

        let Err(error) = AgentLock::acquire(&path) else {
            anyhow::bail!("a second owner acquired the singleton lock");
        };
        assert_eq!(error.to_string(), "Warframe Observer is already running");

        drop(first);
        let _replacement = AgentLock::acquire(&path)?;
        Ok(())
    }

    #[test]
    fn ignores_an_unlocked_file_left_by_an_old_process() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("agent.lock");
        fs::write(&path, "stale")?;

        let _lock = AgentLock::acquire(&path)?;
        Ok(())
    }
}
