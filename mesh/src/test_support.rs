//! The tests' minted obfuscation, and the specification's example.

use sliplib_config::Obfuscation;

/// At values the module accepts.
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

pub const EXAMPLE_MESH: &str = include_str!("../../docs/example/mesh.yaml");
pub const EXAMPLE_SECRETS: &str = include_str!("../../docs/example/secrets.yaml");

/// In order, as one file.
pub fn laid(layers: &[&str]) -> sliplib_config::v2::File {
    let tree = layers
        .iter()
        .fold(sliplib_yaml::Value::Null, |tree, layer| {
            sliplib_yaml::merge(tree, &sliplib_yaml::from_str(layer).unwrap())
        });
    sliplib_config::v2::File::parse(&sliplib_yaml::to_string(&tree).unwrap()).unwrap()
}

/// The secrets over the mesh, `overlay` over both.
pub fn example_files(overlay: &str) -> sliplib_config::v2::File {
    laid(&[EXAMPLE_MESH, EXAMPLE_SECRETS, overlay])
}

/// Keyless, to be laid on no link.
pub const NODE_D: &str = "{platform: talos, node_id: 0.0.0.5, underlay: {wan: {}}}";

/// [`example_files`] as a mesh.
pub fn example(overlay: &str) -> crate::Mesh<'static> {
    mesh(example_files(overlay))
}

/// Leaked: a test holds the mesh alone.
pub fn mesh(files: sliplib_config::v2::File) -> crate::Mesh<'static> {
    crate::Mesh::of(Box::leak(Box::new(files))).unwrap()
}
