//! Discovery of the current user's already-running local service.

use crate::raw::{Client, ClientError};

#[derive(serde::Deserialize)]
struct RuntimeRecord {
    schema_version: u32,
    service: ServiceIdentity,
}

#[derive(serde::Deserialize)]
struct ServiceIdentity {
    endpoint_id: String,
}

impl Client {
    /// Connects to the current user's local service; does not launch it.
    ///
    /// # Errors
    /// Returns discovery errors if the runtime record is absent or invalid,
    /// or connection errors if the recorded service cannot be reached.
    pub async fn connect_local() -> Result<Self, ClientError> {
        let project = directories::ProjectDirs::from("", "", "wf-observer").ok_or_else(|| {
            ClientError::LocalDiscovery("application directories are unavailable".into())
        })?;
        let directory = project.runtime_dir().unwrap_or_else(|| project.cache_dir());
        let path = directory.join("runtime.json");
        let bytes = std::fs::read(&path).map_err(|error| {
            ClientError::LocalDiscovery(format!(
                "{}: {error}; start wf-observer first",
                path.display()
            ))
        })?;
        let record: RuntimeRecord = serde_json::from_slice(&bytes)
            .map_err(|error| ClientError::LocalDiscovery(error.to_string()))?;
        if record.schema_version != 1 {
            return Err(ClientError::LocalDiscovery(
                "unsupported runtime record version".into(),
            ));
        }
        Self::connect_endpoint(&record.service.endpoint_id).await
    }
}
