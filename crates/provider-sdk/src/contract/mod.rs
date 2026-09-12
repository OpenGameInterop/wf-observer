//! Compiled provider contracts. Native memory belongs to the host, never transport.
//!
//! These are host-side types, not RPC DTOs. Providers publish validated JSON;
//! the host supplies session identity, sequencing, and transport adaptation.

mod error;
mod manifest;
mod provider;
mod publication;
mod session;

pub use error::ProviderError;
pub use manifest::{CapabilityDescriptor, GameDescriptor, ProviderManifest};
pub use provider::Provider;
pub use publication::{CapabilityHealth, EventSink, HealthSink, UnavailableReason};
pub use session::{PollContext, PollResult, ProviderSession};
