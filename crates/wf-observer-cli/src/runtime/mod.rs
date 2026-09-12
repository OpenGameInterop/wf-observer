//! Versioned per-user service metadata, independent of its game sessions.

mod record;
mod shutdown;
mod status;
mod storage;

pub(crate) use record::{
    Activity, HostStatus, RecordedProcess, ServiceInfo, TargetInfo, TargetStatus,
};
pub(crate) use shutdown::{clear_shutdown_request, shutdown_requested, stop, stop_agent};
pub(crate) use status::print_status;
pub(crate) use storage::{Registration, current_agent};

#[cfg(test)]
mod tests;
