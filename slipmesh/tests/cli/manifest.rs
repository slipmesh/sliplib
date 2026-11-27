use crate::common::{Config, MESH};

#[test]
fn the_mesh_object_lists_every_node_and_the_file_is_left_as_it_was() {
    let config = Config::holding(MESH);
    let ran = config.run(&["manifest"]);
    assert_eq!(ran.code, Some(0), "{}", ran.stderr);
    assert_eq!(config.text(), MESH);
    assert_eq!(
        sliplib_yaml::from_str::<sliplib_yaml::Value>(&ran.stdout).unwrap(),
        sliplib_yaml::from_str::<sliplib_yaml::Value>(
            "apiVersion: slipmesh.net/v1alpha1
kind: Mesh
metadata: {name: mesh}
spec:
  nodes:
    node-a: {endpoint: {ipv4: 192.0.2.1}}
    node-b: {endpoint: {ipv4: node-b.example.com}}
    node-c: {}
    router1: {endpoint: {ipv4: 192.0.2.4}}"
        )
        .unwrap()
    );
}
