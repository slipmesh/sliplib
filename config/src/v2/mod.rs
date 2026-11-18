//! `slipmesh.yaml` in format version 2, as `docs/slipmesh-yaml.md` describes it.

mod block;
pub mod groups;
mod name;
pub mod network;

pub use block::{ALL, Block, Exclude, Include, NONE, Order, Platform, Rank};
pub use name::{ClassName, Name, NameError};
