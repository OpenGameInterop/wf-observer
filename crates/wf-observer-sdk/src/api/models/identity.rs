//! Owned identities. Process addresses and native creation markers never cross RPC.

/// A checkpoint in one service run, not a durable replay cursor.
#[boltffi::data]
#[derive(Debug, Clone, ..Eq)]
pub struct ServiceCursor {
    /// Opaque ID freshly generated whenever the service starts.
    pub run_id: String,
    /// Latest service-run update revision, encoded as unsigned decimal text.
    /// All topics and sessions share it, including health and lifecycle changes.
    /// Zero precedes the first change.
    pub sequence: String,
}
