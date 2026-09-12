//! Transport-independent publication and health updates.

use serde_json::Value;

use crate::{CapabilityDescriptor, ProviderError};

/// Whether a capability can currently provide validated data.
#[derive(Debug, Clone, ..Eq)]
pub enum CapabilityHealth {
    /// A successful acquisition confirms this capability's data is current.
    Available,
    /// Data must not be presented as current, for the supplied reason.
    Unavailable(UnavailableReason),
}

/// Capability failures without process-private diagnostic details.
#[derive(Debug, Clone, ..Eq)]
pub enum UnavailableReason {
    TargetNotReady,
    UnsupportedBuild,
    ReadFailed { message: String },
    ValidationFailed { message: String },
    ProviderFailed { message: String },
}

/// Accepts data only for the provider's declared capabilities and bound session.
///
/// Implementations must bound payloads and buffering, validate the descriptor's
/// topic/schema and publication mode, and return promptly without waiting for
/// remote readers. JSON is domain data, never raw process memory or addresses.
pub trait EventSink {
    /// Invalidates this topic's previous source lifetime before new publication.
    ///
    /// Once accepted, the invalidation survives a later poll or publication error,
    /// provided final process verification succeeds and the source generation is
    /// still current. Data from that failed poll is discarded. Request at most one
    /// reset per capability per poll; resets do not consume the data budget.
    ///
    /// # Errors
    ///
    /// Returns an error for an undeclared capability, duplicate, or rejected reset.
    fn reset(&mut self, capability: &CapabilityDescriptor) -> Result<(), ProviderError>;

    /// Replaces this capability's current snapshot with validated data.
    ///
    /// When committed, confirms successful acquisition and renews only this
    /// capability's freshness deadline, even if the payload is unchanged.
    ///
    /// # Errors
    ///
    /// Returns an error if publication is rejected or exceeds host limits.
    fn snapshot(
        &mut self,
        capability: &CapabilityDescriptor,
        payload: &Value,
    ) -> Result<(), ProviderError>;

    /// Publishes a transient event; it does not replace the current snapshot.
    ///
    /// Does not change health or renew freshness. Confirm successful acquisition
    /// separately through a snapshot or an Available health update.
    ///
    /// # Errors
    ///
    /// Returns an error if publication is rejected or exceeds host limits.
    fn event(
        &mut self,
        capability: &CapabilityDescriptor,
        payload: &Value,
    ) -> Result<(), ProviderError>;
}

/// Accepts health for declared capabilities of the bound provider session.
pub trait HealthSink {
    /// Updates capability health. Unavailability invalidates current data.
    ///
    /// Report Available only after successful acquisition, not as a keepalive.
    /// It renews only this capability's freshness deadline. Snapshot-capable
    /// topics must retain a valid snapshot or publish one in the same poll.
    /// Event-only topics report Available after a successful scan even if no
    /// event occurred.
    ///
    /// # Errors
    ///
    /// Returns an error if the capability is undeclared or the update is rejected.
    fn update(
        &mut self,
        capability: &CapabilityDescriptor,
        health: CapabilityHealth,
    ) -> Result<(), ProviderError>;
}
