//! Static descriptions of a compiled provider and its implemented capabilities.

/// One game's stable identity and human-readable name.
#[derive(Debug, ..Copy)]
pub struct GameDescriptor {
    /// Stable game identifier, independent of its build.
    pub id: &'static str,
    /// Human-readable game name.
    pub name: &'static str,
}

/// One implemented JSON topic and its supported publication modes.
#[derive(Debug, ..Copy)]
pub struct CapabilityDescriptor {
    /// Namespaced topic identifier, independent of the wire protocol.
    pub topic: &'static str,
    /// Version of the topic's JSON schema.
    pub schema_version: u32,
    /// Snapshot support and the host's preferred delivery representation.
    pub snapshots: Option<SnapshotDelivery>,
    /// Whether the provider publishes transient events.
    pub events: bool,
}

/// Providers always publish complete snapshots; the host may compress their delivery.
#[derive(Debug, ..Copy, ..Eq)]
pub enum SnapshotDelivery {
    Full,
    /// Send an acknowledged JSON patch when it is smaller than the replacement.
    Delta,
}

/// A provider's compiled identity and capabilities, available even without a game.
#[derive(Debug)]
pub struct ProviderManifest {
    /// Stable provider identifier.
    pub id: &'static str,
    /// Human-readable provider name.
    pub name: &'static str,
    /// Provider implementation version, independent of wire/topic versions and game builds.
    pub version: &'static str,
    /// The game identified by this provider.
    pub game: GameDescriptor,
    /// Implemented capabilities only. An empty list promises no telemetry.
    pub capabilities: &'static [CapabilityDescriptor],
}
