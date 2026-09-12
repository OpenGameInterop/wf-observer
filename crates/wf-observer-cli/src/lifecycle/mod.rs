//! Synchronous discovery and attachment, owned by the background host worker.

mod backend;
mod worker;

pub(crate) use worker::run;

#[cfg(test)]
mod tests;
