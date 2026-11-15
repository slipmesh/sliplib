//! A node's `awg` and `router` configs, computed from the topology of `slipmesh.yaml` and the
//! secrets set in it. Nothing here creates a secret: what the file lacks is refused.

pub mod secrets;

#[cfg(test)]
mod test_support;
