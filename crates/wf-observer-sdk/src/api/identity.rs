use crate::{
    api::{ObserverClient, ObserverError, runtime},
    raw::ClientIdentity,
};

/// A reader identity shared by connections. Approve its public endpoint ID on
/// the service, and keep its private secret in application-owned storage.
#[derive(Clone)]
pub struct ObserverIdentity {
    inner: ClientIdentity,
}

/// Creates an identity without connecting. Persist `secret_bytes()` before use.
#[boltffi::export]
#[must_use]
pub fn create_identity() -> ObserverIdentity {
    ObserverIdentity {
        inner: ClientIdentity::generate(),
    }
}

/// Restores an application-owned private identity without connecting.
///
/// # Errors
/// Returns an identity error if the secret is not 32 bytes long.
#[boltffi::export]
#[allow(
    clippy::needless_pass_by_value,
    reason = "BoltFFI owns incoming byte buffers"
)]
pub fn restore_identity(secret: Vec<u8>) -> Result<ObserverIdentity, ObserverError> {
    Ok(ObserverIdentity {
        inner: ClientIdentity::from_bytes(&secret)?,
    })
}

/// Loads or creates an identity at an application-owned path. Existing corrupt
/// files produce an error; they are never silently replaced with a new key.
///
/// # Errors
/// Returns identity storage errors. Browsers must use create/restore instead.
#[boltffi::export]
pub fn load_identity(path: String) -> Result<ObserverIdentity, ObserverError> {
    #[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
    {
        let path = std::path::PathBuf::from(path);
        Ok(ObserverIdentity {
            inner: ClientIdentity::load_or_create(&path)?,
        })
    }
    #[cfg(all(target_family = "wasm", target_os = "unknown"))]
    {
        let _ = path;
        Err(ObserverError::Identity {
            message: "browser applications must persist secret_bytes() and use restore_identity()"
                .into(),
        })
    }
}

#[boltffi::export]
impl ObserverIdentity {
    /// Public endpoint ID to add with `wf-observer peers allow <endpoint-id>`.
    #[must_use]
    pub fn endpoint_id(&self) -> String {
        self.inner.endpoint_id().to_string()
    }

    /// Private key bytes for application-owned storage. Never share these bytes.
    #[must_use]
    pub fn secret_bytes(&self) -> Vec<u8> {
        self.inner.to_bytes().to_vec()
    }

    /// Connects using this identity and a service endpoint ID or ticket.
    ///
    /// # Errors
    /// Returns endpoint, runtime, transport, or `NotAuthorized` errors.
    pub async fn connect(&self, endpoint: String) -> Result<ObserverClient, ObserverError> {
        let identity = self.inner.clone();
        let inner = runtime::execute(Box::pin(async move {
            crate::client::Client::connect_endpoint_with_identity(&endpoint, &identity).await
        }))
        .await??;
        Ok(ObserverClient { inner })
    }

    /// Connects to the current user's running service using this identity.
    ///
    /// # Errors
    /// Returns discovery or connection errors. Unavailable in browsers.
    pub async fn connect_local(&self) -> Result<ObserverClient, ObserverError> {
        #[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
        {
            let identity = self.inner.clone();
            let inner = runtime::execute(Box::pin(async move {
                crate::client::Client::connect_local_with_identity(&identity).await
            }))
            .await??;
            Ok(ObserverClient { inner })
        }
        #[cfg(all(target_family = "wasm", target_os = "unknown"))]
        Err(ObserverError::LocalDiscovery {
            message: "browser clients require an explicit endpoint".into(),
        })
    }
}
