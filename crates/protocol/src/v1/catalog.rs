//! Compiled capabilities, available without a game.

/// Compiled providers, including providers with no telemetry capabilities.
#[boltffi::data]
#[derive(Debug, Clone, ..Eq, ..Serde)]
pub struct Catalog {
    pub providers: Vec<ProviderDescriptor>,
}

/// Owned transport description; not the host-side provider manifest.
#[boltffi::data]
#[derive(Debug, Clone, ..Eq, ..Serde)]
pub struct ProviderDescriptor {
    pub id: String,
    pub name: String,
    pub version: String,
    pub game: GameDescriptor,
    pub capabilities: Vec<CapabilityDescriptor>,
}

#[boltffi::data]
#[derive(Debug, Clone, ..Eq, ..Serde)]
pub struct GameDescriptor {
    pub id: String,
    pub name: String,
}

/// Declared implementation support, not current build compatibility or readiness.
#[boltffi::data]
#[derive(Debug, Clone, ..Eq, ..Serde)]
pub struct CapabilityDescriptor {
    pub topic: String,
    pub schema_version: u32,
    pub snapshots: bool,
    pub events: bool,
}
