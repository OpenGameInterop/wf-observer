//! Conversion from compiled provider contracts; protocol never imports providers.

use protocol::v1 as wire;
use provider_sdk::ProviderManifest;

pub(super) fn catalog(manifests: &[&ProviderManifest]) -> anyhow::Result<wire::Catalog> {
    let mut providers = Vec::new();
    for manifest in manifests {
        anyhow::ensure!(
            identifier(manifest.id) && identifier(manifest.game.id),
            "invalid provider identity"
        );
        anyhow::ensure!(
            !providers
                .iter()
                .any(|p: &wire::ProviderDescriptor| p.id == manifest.id),
            "duplicate provider ID"
        );
        let mut capabilities = Vec::new();
        for cap in manifest.capabilities {
            anyhow::ensure!(
                identifier(cap.topic) && cap.schema_version > 0 && (cap.snapshots || cap.events),
                "invalid capability"
            );
            anyhow::ensure!(
                !capabilities
                    .iter()
                    .any(|c: &wire::CapabilityDescriptor| c.topic == cap.topic
                        && c.schema_version == cap.schema_version),
                "duplicate capability"
            );
            capabilities.push(wire::CapabilityDescriptor {
                topic: cap.topic.into(),
                schema_version: cap.schema_version,
                snapshots: cap.snapshots,
                events: cap.events,
            });
        }
        providers.push(wire::ProviderDescriptor {
            id: manifest.id.into(),
            name: manifest.name.into(),
            version: manifest.version.into(),
            game: wire::GameDescriptor {
                id: manifest.game.id.into(),
                name: manifest.game.name.into(),
            },
            capabilities,
        });
    }
    Ok(wire::Catalog { providers })
}

pub(super) fn identifier(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256
}

pub(super) fn limit(resource: wire::Resource) -> wire::RequestError {
    wire::RequestError::LimitExceeded { resource }
}

pub(super) fn invalid(message: &str) -> wire::RequestError {
    wire::RequestError::InvalidRequest {
        message: message.into(),
    }
}

pub(super) fn message_fits<T: serde::Serialize>(
    value: &T,
    message_bytes: u32,
) -> Result<(), wire::RequestError> {
    let size = postcard::experimental::serialized_size(value)
        .map_err(|_| invalid("response encoding failed"))?;
    if size > message_bytes as usize {
        Err(limit(wire::Resource::MessageBytes))
    } else {
        Ok(())
    }
}
