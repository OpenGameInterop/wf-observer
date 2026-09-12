//! Shared types and wire formats for communication between the memory reader,
//! service and client libraries.
//!
//! # FFI boundary
//!
//! Consumer-facing data types should also be exposed through
//! `BoltFFI` when it can represent them without changing their natural Rust
//! design. If satisfying `BoltFFI` would dictate a type's design, the binding
//! crate should instead own an FFI-specific adapter and convert at its boundary.

#[macro_use(derive)]
extern crate derive_aliases;

use irpc::{channel::oneshot, rpc_requests};

mod derive_alias;

pub mod v1;

/// Iroh ALPN for wire-protocol version 0.
pub const ALPN_V0: &[u8] = b"wf-observer/0";

/// Verifies that the application is reachable and speaking protocol v0.
#[derive(Debug, ..Copy, ..Eq, ..Serde)]
pub struct Ping;

/// Successful response to [`Ping`].
#[derive(Debug, ..Copy, ..Eq, ..Serde)]
pub struct Pong;

/// RPC operations supported by wire-protocol version 0.
///
/// Postcard encodes enum variants by declaration order.
/// Adding, removing, or reordering a variant requires a new wire-protocol version.
#[rpc_requests(message = ObserverMessage)]
#[derive(Debug, ..Serde)]
pub enum ObserverProtocol {
    /// Verifies that the application is reachable.
    #[rpc(tx = oneshot::Sender<Pong>)]
    Ping(Ping),
}
