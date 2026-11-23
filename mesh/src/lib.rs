//! A node's `awg` and `router` configs from `slipmesh.yaml`. A secret the files lack is refused;
//! `mint`, behind its feature, creates them.

pub mod addressing;
#[cfg(feature = "mint")]
pub mod mint;
pub mod mtu;
pub mod network;
pub mod render;
pub mod secrets;
pub mod warnings;

pub use network::Mesh;

#[cfg(test)]
mod test_support;
