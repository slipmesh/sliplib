//! Secrets in types that keep them out of logs and diffs.

mod secret;
pub mod wireguard;

pub use secret::Secret;
