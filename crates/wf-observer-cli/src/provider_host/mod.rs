//! Provider sessions borrow host-owned memory and publish through staged sinks.

mod session;

pub(crate) use session::HostedSession;

#[cfg(test)]
mod tests;
