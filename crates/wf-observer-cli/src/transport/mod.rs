//! Bounded Iroh RPC transport. Only the service read/subscription view crosses here.

mod connection;
mod requests;
mod server;

pub(crate) use server::{Server, start};

#[cfg(test)]
mod tests;
