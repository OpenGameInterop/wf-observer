//! Owned identities. Process addresses and native creation markers never cross RPC.

/// A checkpoint in one service run, not a durable replay cursor.
#[derive(Debug, Clone, ..Eq, ..Serde)]
pub struct ServiceCursor {
    /// Opaque ID freshly generated whenever the service starts.
    pub run_id: String,
    /// Latest service-run update revision, shared by all topics and sessions.
    /// Advances for data, health and lifecycle changes; zero precedes the first change.
    pub sequence: u64,
}

/// One provider attachment; never implicitly redirected after restart or exit.
#[boltffi::data]
#[derive(Debug, Clone, Hash, ..Ord, ..Serde)]
pub struct SessionRef {
    pub run_id: String,
    pub session_id: String,
}

/// An exact compiled topic/schema, independent of wire and provider versions.
#[boltffi::data]
#[derive(Debug, Clone, Hash, ..Ord, ..Serde)]
pub struct TopicRef {
    pub provider_id: String,
    pub topic: String,
    pub schema_version: u32,
}

/// Consumer-safe origin of a capability publication.
#[boltffi::data]
#[derive(Debug, Clone, ..Eq, ..Serde)]
pub struct TopicSource {
    pub session: SessionRef,
    pub game_id: String,
    pub topic: TopicRef,
}
