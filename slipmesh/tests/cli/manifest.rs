use crate::common::{Config, MESH};

#[test]
fn the_mesh_object_is_printed_and_the_file_is_left_as_it_was() {
    let config = Config::holding(MESH);
    let ran = config.run(&["manifest"]);
    assert_eq!(ran.code, Some(0), "{}", ran.stderr);
    assert_eq!(config.text(), MESH);
    let printed: sliplib_yaml::Value = sliplib_yaml::from_str(&ran.stdout).unwrap();
    assert_eq!(printed["apiVersion"], "slipmesh.net/v1alpha1");
    assert_eq!(printed["kind"], "Mesh");
    assert_eq!(printed["metadata"]["name"], "mesh");
    assert!(printed["spec"]["nodes"].is_mapping(), "{}", ran.stdout);
}
