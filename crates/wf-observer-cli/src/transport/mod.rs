//! Bounded Iroh RPC transport. Only the service read/subscription view crosses here.

mod connection;
mod requests;
mod server;

pub(crate) use server::{Server, start};

#[cfg(test)]
mod tests;

#[cfg(test)]
mod client_tests;

#[cfg(test)]
mod sdk_api_tests;
