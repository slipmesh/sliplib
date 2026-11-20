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

/// The value at `path` of the example's tree, and the key it stands under there.
fn parent<'a>(value: &'a mut Value, path: &[&'a str]) -> (&'a mut sliplib_yaml::Mapping, &'a str) {
    let (key, above) = path.split_last().unwrap();
    let parent = above.iter().fold(value, |value, key| &mut value[*key]);
    (parent.as_mapping_mut().unwrap(), key)
}

/// The example with the key at `path` taken out.
pub(super) fn left_out(path: &[&str]) -> Result<Network, String> {
    let mut value = example();
    let (parent, key) = parent(&mut value, path);
    parent.shift_remove(key).unwrap();
    network(value)
}

/// The example with the key at `path` written `null`, read from the text: laid over a value,
/// `null` takes the key out, and a value read as a list or a mapping takes `null` as an empty one.
pub(super) fn written_null(path: &[&str]) -> Result<Network, String> {
    let mut value = example();
    let (parent, key) = parent(&mut value, path);
    *parent.get_mut(key).unwrap() = Value::Null;
    let text = sliplib_yaml::to_string(&value).unwrap();
    sliplib_yaml::from_str(&text).map_err(|error| error.to_string())
}
