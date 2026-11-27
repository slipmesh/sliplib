//! `slipmesh.yaml`, and what every platform's daemons share of their configs.

pub mod awg;
mod mark;
pub mod mesh_object;
mod obfuscation;
mod prefix;
mod range;
pub mod router;
pub mod v2;

pub use mark::Mark;
pub use obfuscation::{
    DEFAULT_MAGIC_HEADERS, HEADER_PROTECTION_NONCE_SIZE, Obfuscation, ObfuscationError, TrailerLoss,
};
pub use prefix::{Prefix, Prefix4, Prefix6};
pub use range::{End, Range, RangeError};
