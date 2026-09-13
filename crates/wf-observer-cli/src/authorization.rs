//! Immutable reader policy installed when the service starts.

use std::{collections::BTreeSet, fs, io, path::Path};

use anyhow::{Context as _, ensure};
use iroh::EndpointId;

use crate::{
    paths,
    settings::{self, AccessMode},
};

#[derive(Debug, Default, ..Serde)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Allowlist {
    peers: Vec<Peer>,
}

#[derive(Debug, ..Serde)]
#[serde(deny_unknown_fields)]
struct Peer {
    endpoint_id: EndpointId,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

impl Allowlist {
    pub(crate) fn load() -> anyhow::Result<Self> {
        Self::load_at(&paths::allowlist_path()?)
    }

    fn load_at(path: &Path) -> anyhow::Result<Self> {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(error) => {
                return Err(error).with_context(|| format!("failed to read {}", path.display()));
            }
        };
        let list: Self = toml::from_str(&text)
            .with_context(|| format!("invalid allowlist in {}", path.display()))?;
        let mut ids = BTreeSet::new();
        for peer in &list.peers {
            ensure!(
                ids.insert(peer.endpoint_id),
                "duplicate endpoint ID in {}",
                path.display()
            );
            validate_name(peer.name.as_deref())?;
        }
        Ok(list)
    }

    pub(crate) fn save(&self) -> anyhow::Result<()> {
        settings::save_at(&paths::allowlist_path()?, self)
    }

    pub(crate) fn allow(
        &mut self,
        endpoint_id: EndpointId,
        name: Option<String>,
    ) -> anyhow::Result<()> {
        validate_name(name.as_deref())?;
        if let Some(peer) = self
            .peers
            .iter_mut()
            .find(|peer| peer.endpoint_id == endpoint_id)
        {
            if name.is_some() {
                peer.name = name;
            }
        } else {
            self.peers.push(Peer { endpoint_id, name });
            self.peers.sort_by_key(|peer| peer.endpoint_id);
        }
        Ok(())
    }

    pub(crate) fn revoke(&mut self, endpoint_id: EndpointId) {
        self.peers.retain(|peer| peer.endpoint_id != endpoint_id);
    }

    pub(crate) fn print(&self) {
        println!("Approved peers: {}", self.peers.len());
        for peer in &self.peers {
            println!(
                "{}{}",
                peer.endpoint_id,
                peer.name
                    .as_ref()
                    .map_or_else(String::new, |name| format!("  {name}"))
            );
        }
    }

    pub(crate) fn policy(&self, mode: AccessMode) -> Policy {
        Policy::new(mode, self.peers.iter().map(|peer| peer.endpoint_id))
    }
}

fn validate_name(name: Option<&str>) -> anyhow::Result<()> {
    ensure!(
        !name.is_some_and(|name| name.chars().any(char::is_control)),
        "peer names cannot contain control characters"
    );
    Ok(())
}

/// Effective policy published by the agent and compared directly on startup.
#[derive(Debug, Clone, Default, ..Eq, ..Serde)]
pub(crate) struct Policy {
    pub(crate) mode: AccessMode,
    pub(crate) approved_peers: BTreeSet<EndpointId>,
}

impl Policy {
    pub(crate) fn new(mode: AccessMode, allowed: impl IntoIterator<Item = EndpointId>) -> Self {
        Self {
            mode,
            approved_peers: if mode == AccessMode::Remote {
                allowed.into_iter().collect()
            } else {
                BTreeSet::new()
            },
        }
    }

    pub(crate) fn load(mode: AccessMode) -> anyhow::Result<Self> {
        // Local-only remains available even if a remote approval file needs repair.
        match mode {
            AccessMode::Local => Ok(Self::default()),
            AccessMode::Remote => Ok(Allowlist::load()?.policy(mode)),
        }
    }

    pub(crate) fn permits(&self, peer: EndpointId) -> bool {
        self.mode == AccessMode::Local || self.approved_peers.contains(&peer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_allowlist_denies_remote_peers_and_corrupt_files_are_rejected() -> anyhow::Result<()>
    {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("allowlist.toml");
        let id = iroh::SecretKey::generate().public();
        assert!(
            !Allowlist::load_at(&path)?
                .policy(AccessMode::Remote)
                .permits(id)
        );
        for text in ["peers =", "[[peers]]\nendpoint_id = 'invalid'", "peer = []"] {
            fs::write(&path, text)?;
            assert!(Allowlist::load_at(&path).is_err());
        }
        let peer = format!("[[peers]]\nendpoint_id = '{id}'\n");
        fs::write(&path, format!("{peer}{peer}"))?;
        assert!(Allowlist::load_at(&path).is_err());
        Ok(())
    }

    #[test]
    fn approvals_persist_and_policy_changes_only_when_permissions_change() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("allowlist.toml");
        let id = iroh::SecretKey::generate().public();
        let other = iroh::SecretKey::generate().public();
        let mut list = Allowlist::default();
        list.allow(id, Some("Phone".into()))?;
        let approved = list.policy(AccessMode::Remote);
        list.allow(id, Some("Renamed phone".into()))?;
        assert_eq!(approved, list.policy(AccessMode::Remote));
        settings::save_at(&path, &list)?;
        let mut restored = Allowlist::load_at(&path)?;
        assert!(restored.policy(AccessMode::Remote).permits(id));
        assert!(!restored.policy(AccessMode::Remote).permits(other));
        restored.revoke(id);
        assert_ne!(approved, restored.policy(AccessMode::Remote));
        assert!(!restored.policy(AccessMode::Remote).permits(id));
        assert!(restored.policy(AccessMode::Local).permits(id));
        Ok(())
    }
}
