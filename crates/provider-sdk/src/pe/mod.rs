//! Minimal mapped amd64 PE header inspection, independent of game compatibility policy.

mod headers;

pub use headers::{HeaderError, ImageHeaders, read_amd64_headers};
