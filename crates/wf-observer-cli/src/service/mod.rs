//! Memory-free service state and the restricted RPC-facing read/subscription view.

mod batch;
mod catalog;
mod inner;
mod publication;
mod queue;
mod state;
mod subscriptions;

pub(crate) use batch::PollBatch;
pub(crate) use publication::PollTicket;
pub(crate) use state::{POLL_GRACE, ServiceState, ServiceView};

const RETAINED_SNAPSHOT_BYTES: usize = 32 * 1024 * 1024;

#[cfg(test)]
mod tests;
