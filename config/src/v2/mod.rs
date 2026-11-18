//! `slipmesh.yaml` in format version 2, as `docs/slipmesh-yaml.md` describes it.

mod block;
mod check;
pub mod groups;
mod name;
pub mod network;
pub mod platform;
#[cfg(test)]
mod testing;

pub use block::{ALL, Block, Exclude, Include, NONE, Order, Platform, Rank};
pub use check::Invalid;
pub use name::{ClassName, Name, NameError};

/// What is named, by its name, in the order written: the order reaches the nodes' configs.
pub type Map<K, V> = indexmap::IndexMap<K, V>;
