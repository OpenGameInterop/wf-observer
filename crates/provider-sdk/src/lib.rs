//! Contracts and decoding utilities for compiled game providers.
//!
//! Contract types are re-exported here; optional-to-use helpers live in
//! [`memory`] and [`pe`]. The host owns attachments, scheduling, service state,
//! and transport. Providers own game-specific layouts and validation.
//!
//! This is a host-side SDK, not a client or wire-protocol dependency.

#[macro_use(derive)]
extern crate derive_aliases;

mod derive_alias;

mod contract;
pub mod memory;
pub mod pe;

pub use contract::{
    CapabilityDescriptor, CapabilityHealth, EventSink, GameDescriptor, HealthSink, PollContext,
    PollResult, Provider, ProviderError, ProviderManifest, ProviderSession, SnapshotDelivery,
    UnavailableReason,
};
