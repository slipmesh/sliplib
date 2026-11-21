//! A node's `awg` and `router` configs, computed from the network of `slipmesh.yaml` and the
//! secrets set in it. Rendering creates no secret: what the rendered node needs and the file lacks
//! is refused. Creating one is `mint`, behind the feature of that name, for the one tool that
//! writes the file.

pub mod addressing;
#[cfg(feature = "mint")]
pub mod mint;
pub mod model;
pub mod mtu;
pub mod render;
pub mod secrets;
pub mod v2;
pub mod warnings;

#[cfg(test)]
mod test_support;
