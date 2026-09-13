//! Persistent user settings, separate from transient service metadata.

use std::{
    fs,
    io::{self, Write as _},
    path::Path,
};

use anyhow::Context as _;

/// Controls the service's network reachability, independently of reader authorization.
#[derive(Debug, Default, clap::ValueEnum, derive_more::Display, ..Copy, ..Eq, ..Serde)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AccessMode {
    /// Accepts connections from this machine only.
    #[default]
    #[display("local")]
    Local,
    /// Accepts direct and relayed connections from other devices.
    #[display("remote")]
    Remote,
}

#[derive(Debug, Default, ..Eq, ..Serde)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Settings {
    pub(crate) access: AccessMode,
}

pub(crate) fn load() -> anyhow::Result<Settings> {
    load_at(&crate::paths::settings_path()?)
}

/// The caller holds the command lock while saving and applying these settings.
pub(crate) fn save(settings: &Settings) -> anyhow::Result<()> {
    save_at(&crate::paths::settings_path()?, settings)
}

fn load_at(path: &Path) -> anyhow::Result<Settings> {
    match fs::read_to_string(path) {
        Ok(text) => {
            toml::from_str(&text).with_context(|| format!("invalid settings in {}", path.display()))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Settings::default()),
        Err(error) => Err(error).with_context(|| format!("failed to read {}", path.display())),
    }
}

fn save_at(path: &Path, settings: &Settings) -> anyhow::Result<()> {
    let parent = path.parent().context("the settings path has no parent")?;
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;
    let text = toml::to_string_pretty(settings).context("failed to encode settings")?;
    let mut temporary = tempfile::Builder::new()
        .prefix(".settings-")
        .tempfile_in(parent)
        .with_context(|| {
            format!(
                "failed to create temporary settings in {}",
                parent.display()
            )
        })?;
    temporary
        .write_all(text.as_bytes())
        .and_then(|()| temporary.as_file().sync_all())
        .with_context(|| format!("failed to write {}", path.display()))?;
    temporary
        .persist(path)
        .map_err(|error| error.error)
        .with_context(|| format!("failed to persist {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_settings_do_not_fall_back_to_a_different_mode() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("settings.toml");
        for text in [
            "access =",
            "access = \"remtoe\"",
            "acess = \"remote\"",
            "access = true",
        ] {
            fs::write(&path, text)?;
            assert!(load_at(&path).is_err(), "accepted {text}");
            assert_eq!(fs::read_to_string(&path)?, text);
        }
        Ok(())
    }

    #[test]
    fn defaults_to_local_and_persists_explicit_modes() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("settings.toml");
        assert_eq!(load_at(&path)?.access, AccessMode::Local);
        for access in [AccessMode::Remote, AccessMode::Local] {
            save_at(&path, &Settings { access })?;
            assert_eq!(load_at(&path)?.access, access);
            assert!(fs::read_to_string(&path)?.contains(&format!("access = \"{access}\"")));
        }
        Ok(())
    }
}
