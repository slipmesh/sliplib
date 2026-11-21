//! What the tests share: an obfuscation with every minted field, and the example of the
//! specification.

use sliplib_config::Obfuscation;

/// Every field a minted obfuscation carries, at values the kernel module accepts.
pub fn minted_obfuscation() -> Obfuscation {
    Obfuscation {
        jc: Some(4),
        jmin: Some(10),
        jmax: Some(50),
        s1: Some(15),
        s2: Some(16),
        h1: Some(5.into()),
        h2: Some(6.into()),
        h3: Some(7.into()),
        h4: Some(8.into()),
        ..Obfuscation::default()
    }
}

/// The network of the specification's example.
pub const EXAMPLE_MESH: &str = include_str!("../../docs/example/mesh.yaml");
/// The secrets of the specification's example, laid over its network.
pub const EXAMPLE_SECRETS: &str = include_str!("../../docs/example/secrets.yaml");

/// `layers` laid over each other in order, as the files of version 2 are, read as one file.
pub fn laid(layers: &[&str]) -> sliplib_config::v2::File {
    let tree = layers
        .iter()
        .fold(sliplib_yaml::Value::Null, |tree, layer| {
            sliplib_yaml::merge(tree, &sliplib_yaml::from_str(layer).unwrap())
        });
    sliplib_config::v2::File::parse(&sliplib_yaml::to_string(&tree).unwrap()).unwrap()
}
