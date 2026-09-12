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
    /// Whether the provider publishes full replacement snapshots.
    pub snapshots: bool,
    /// Whether the provider publishes transient events.
    pub events: bool,
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
