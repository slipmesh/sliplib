//! `slipmesh.yaml`, and what every platform's daemons share of their configs.

pub mod awg;
mod obfuscation;
mod range;
pub mod router;
pub mod v2;

pub use obfuscation::{
    DEFAULT_MAGIC_HEADERS, HEADER_PROTECTION_NONCE_SIZE, Obfuscation, ObfuscationError, TrailerLoss,
};
pub use range::{End, Range, RangeError};
