//! What the tests of format version 2 share: the network of the specification's example.

use super::network::Network;
use sliplib_yaml::Value;

/// The network of the specification's example, its `slipmesh:` block taken off.
pub(super) fn example() -> Value {
    let mut value: Value =
        sliplib_yaml::from_str(include_str!("../../../docs/example/mesh.yaml")).unwrap();
    value.as_mapping_mut().unwrap().shift_remove("slipmesh");
    value
}

pub(super) fn network(value: Value) -> Result<Network, String> {
    sliplib_yaml::from_value(value).map_err(|error| error.to_string())
}

/// The example with `overlay` laid over it.
pub(super) fn with(overlay: &str) -> Result<Network, String> {
    let overlay: Value = sliplib_yaml::from_str(overlay).unwrap();
    network(sliplib_yaml::merge(example(), &overlay))
}
