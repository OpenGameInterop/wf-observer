//! Built-in provider ownership and single-pass process discovery.

mod registry;

pub(crate) use registry::{Candidate, PROVIDERS, discover};
