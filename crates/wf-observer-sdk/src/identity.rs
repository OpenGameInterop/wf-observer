//! Application-owned reader identities, independent of any connection.

use iroh::{EndpointId, SecretKey};

use crate::raw::ClientError;

/// A reusable reader keypair. Persist it per application installation or profile.
/// Share only [`Self::endpoint_id`] with the service owner for approval.
#[derive(Clone, derive_more::Debug)]
#[debug("ClientIdentity({})", self.endpoint_id())]
pub struct ClientIdentity {
    secret: SecretKey,
}

impl ClientIdentity {
    /// Generates a new identity. It needs its own approval on remote services.
    #[must_use]
    pub fn generate() -> Self {
        Self {
            secret: SecretKey::generate(),
        }
    }

    /// Restores an identity from its private 32-byte secret.
    ///
    /// # Errors
    /// Returns an identity error if the secret has the wrong length.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ClientError> {
        let bytes = bytes
            .try_into()
            .map_err(|_| ClientError::Identity("expected a 32-byte reader identity".into()))?;
        Ok(Self {
            secret: SecretKey::from_bytes(bytes),
        })
    }

    /// Exports the private secret for storage by the application. Never share it.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; 32] {
        self.secret.to_bytes()
    }

    /// The public endpoint ID to approve before attempting a connection.
    #[must_use]
    pub fn endpoint_id(&self) -> EndpointId {
        self.secret.public()
    }

    pub(crate) fn secret_key(&self) -> SecretKey {
        self.secret.clone()
    }

    /// Loads an application-owned file, or atomically creates it on first use.
    /// Existing unreadable or corrupt identities are never replaced.
    ///
    /// # Errors
    /// Returns an identity error if reading, creating, or decoding the file fails.
    #[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
    pub fn load_or_create(path: &std::path::Path) -> Result<Self, ClientError> {
        load_or_create(path)
            .map_err(|error| ClientError::Identity(format!("{}: {error:#}", path.display())))
    }
}

#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
fn load_or_create(path: &std::path::Path) -> anyhow::Result<ClientIdentity> {
    use std::{
        fs,
        io::{self, Write as _},
        path::Path,
    };
    match fs::read(path) {
        Ok(bytes) => return Ok(ClientIdentity::from_bytes(&bytes)?),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let identity = ClientIdentity::generate();
    // tempfile creates private files (0600 on Unix). Concurrent first launches
    // agree on the winner's key rather than overwriting an approved identity.
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(&identity.to_bytes())?;
    temporary.as_file().sync_all()?;
    match temporary.persist_noclobber(path) {
        Ok(_) => Ok(identity),
        Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => {
            Ok(ClientIdentity::from_bytes(&fs::read(path)?)?)
        }
        Err(error) => Err(error.error.into()),
    }
}

#[cfg(all(test, not(all(target_family = "wasm", target_os = "unknown"))))]
mod tests {
    use super::*;

    #[test]
    fn concurrent_creation_reuses_one_private_identity() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("client/identity.key");
        let barrier = std::sync::Barrier::new(8);
        let identities = std::thread::scope(|scope| {
            let threads: Vec<_> = (0..8)
                .map(|_| {
                    scope.spawn(|| {
                        barrier.wait();
                        ClientIdentity::load_or_create(&path)
                    })
                })
                .collect();
            threads
                .into_iter()
                .map(|thread| {
                    thread
                        .join()
                        .map_err(|_| ClientError::Identity("identity creator panicked".into()))?
                })
                .collect::<Result<Vec<_>, _>>()
        })?;
        let restored = ClientIdentity::from_bytes(&std::fs::read(&path)?)?;
        assert!(
            identities
                .iter()
                .all(|identity| identity.endpoint_id() == restored.endpoint_id())
        );
        assert_eq!(
            ClientIdentity::load_or_create(&path)?.endpoint_id(),
            restored.endpoint_id()
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(
                std::fs::metadata(&path)?.permissions().mode() & 0o777,
                0o600
            );
        }
        std::fs::write(&path, b"broken")?;
        assert!(matches!(
            ClientIdentity::load_or_create(&path),
            Err(ClientError::Identity(_))
        ));
        assert_eq!(std::fs::read(&path)?, b"broken");
        Ok(())
    }
}
