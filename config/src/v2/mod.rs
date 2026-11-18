//! `slipmesh.yaml` in format version 2, as `docs/slipmesh-yaml.md` describes it.

mod block;
mod name;

pub use block::{ALL, Block, Exclude, Include, NONE, Order, Platform, Rank};
pub use name::{ClassName, Name, NameError};
