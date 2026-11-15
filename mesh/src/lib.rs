//! A node's `awg` and `router` configs, computed from the topology of `slipmesh.yaml` and the
//! secrets set in it. Nothing here creates a secret: what the rendered node needs and the file
//! lacks is refused.

pub mod addressing;
pub mod mtu;
pub mod render;
pub mod secrets;

#[cfg(test)]
mod test_support;
