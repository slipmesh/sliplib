//! `slipmesh.yaml`, which every tool reads, and the patch format: what the generator writes for a
//! node and what reads it - Talos, the daemons on the node, and the RouterOS side. Types and their
//! checks only; reading and writing files is `sliplib-yaml`'s.

pub mod awg;
mod metrics;
pub mod nftables;
mod obfuscation;
mod range;
pub mod router;
pub mod slipmesh_file;
pub mod talos;
pub mod v1;
pub mod v2;

pub use metrics::{MetricsConfig, MetricsError};
pub use obfuscation::{
    DEFAULT_MAGIC_HEADERS, HEADER_PROTECTION_NONCE_SIZE, Obfuscation, ObfuscationError, TrailerLoss,
};
pub use range::{End, Range, RangeError};
