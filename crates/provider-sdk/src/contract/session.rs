//! Synchronous, bounded provider polling with host-owned scheduling.

use std::time::Duration;

use memory_reader::ProcessMemory;

use crate::{CapabilityDescriptor, EventSink, HealthSink, ProviderError};

/// Temporary access to one session's memory and publication destinations.
///
/// The host binds both sinks to this session. Providers do not choose session
/// identifiers or call transport. These borrows cannot be retained between polls.
/// The host verifies the process before and after polling, and stages sink output
/// until the final verification succeeds. On verification failure it discards
/// the output, session caches, and attachment. Providers need not repeat process
/// identity checks, but must still validate game-level data consistency.
/// Accepted source resets survive data-publication failure after verification;
/// the host invalidates the affected topics without publishing rejected data.
pub struct PollContext<'a> {
    /// Complete current topic demand, already combined across subscribers.
    pub demand: &'a [&'static CapabilityDescriptor],
    /// Host-supplied monotonic time since this session started.
    pub now: Duration,
    /// Read-only memory of this session's target.
    pub memory: &'a mut dyn ProcessMemory,
    /// Validated snapshots and events for this session.
    pub events: &'a mut dyn EventSink,
    /// Per-capability health for this session.
    pub health: &'a mut dyn HealthSink,
}

/// Scheduling requested by a successful poll.
#[derive(Debug, ..Copy, ..Eq)]
pub enum PollResult {
    /// Poll again no sooner than this delay. The host must impose a positive
    /// minimum delay even if the provider requests zero, and may poll later.
    After(Duration),
    /// Suspend polling until demand changes. The host still verifies liveness.
    Idle,
}

/// Parsing/resolution state for one attachment, without ownership of its memory.
pub trait ProviderSession: Send {
    /// Starts an initial or host-reset topic lifetime before its next poll.
    /// Event readers must discard their old baselines here, including when demand
    /// stopped and resumed between polls. A provider's own accepted source reset
    /// does not call this hook: the provider already knows that source changed.
    fn begin_generation(&mut self, _capability: &CapabilityDescriptor) {}

    /// Acknowledges the preceding poll's staged output. Called before another poll,
    /// including on publication/provider failure. `true` means the host accepted
    /// the batch, not that every remote subscriber received it. Commit tentative
    /// event cursors only on `true`; otherwise retain the prior accepted position.
    fn poll_completed(&mut self, _committed: bool) {}

    /// Last identified game build, independent of provider and topic versions.
    /// This is cached metadata: the getter must not perform memory access.
    /// The host bounds and publishes it alongside accepted poll output.
    fn game_build(&self) -> Option<&str> {
        None
    }

    /// Performs a bounded unit of work and returns scheduling to the host.
    ///
    /// Do not sleep, spin waiting for data, or read unbounded game-controlled
    /// lengths. Split larger work across polls. The host checks cancellation
    /// between calls; synchronous native reads cannot be interrupted mid-call.
    ///
    /// Confirm each completed acquisition with a snapshot or Available health,
    /// including unchanged data and quiet event scans. Acquiring one topic does
    /// not renew another topic's freshness deadline.
    ///
    /// # Errors
    ///
    /// Returns an error when memory access, validation, or publication fails.
    fn poll(&mut self, context: &mut PollContext<'_>) -> Result<PollResult, ProviderError>;
}
