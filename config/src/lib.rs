//! The patch format: what the generator writes for a node and what reads it - Talos, the daemons
//! on the node, and the RouterOS side. Types and their checks only; reading and writing files is
//! `sliplib-yaml`'s.

pub mod awg;
mod metrics;
mod obfuscation;
pub mod router;
pub mod talos;

pub use metrics::MetricsConfig;
pub use obfuscation::{DEFAULT_MAGIC_HEADERS, HEADER_PROTECTION_NONCE_SIZE, Obfuscation};
