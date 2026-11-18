//! `slipmesh.yaml`, which every tool reads, and the daemons' configs: what a generator writes for a
//! node from it and what reads that - the daemons on the node, and the RouterOS side. Types and
//! their checks only; reading and writing files is `sliplib-yaml`'s.

pub mod awg;
pub mod mesh_config;
mod metrics;
pub mod nftables;
mod obfuscation;
mod range;
pub mod router;
pub mod slipmesh_file;

pub use metrics::{MetricsConfig, MetricsError};
pub use obfuscation::{
    DEFAULT_MAGIC_HEADERS, HEADER_PROTECTION_NONCE_SIZE, Obfuscation, ObfuscationError, TrailerLoss,
};
pub use range::{End, Range, RangeError};
