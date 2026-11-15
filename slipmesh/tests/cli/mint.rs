use crate::common::Config;
use sliplib_config::mesh_config::MeshConfig;
use sliplib_config::slipmesh_file::SlipmeshFile;
use sliplib_yaml::MultiDoc;

const SLIPMESH: &str = r#"slipmesh:
  kind: network
cluster:
  bgp_as: 64512
  loopback_networks: {ipv4: "198.51.100.0/25", ipv6: "3fff::/32"}
nodes:
  - {name: a, node_id: "198.51.100.1", endpoint: "192.0.2.10"}
  - {name: b, node_id: "198.51.100.2", endpoint: "192.0.2.11"}
  - {name: c, node_id: "198.51.100.3"}
  - {name: d, node_id: "198.51.100.4"}
mesh:
  links:
    - {pair: [a, b], port: 51820}
---
slipmesh:
  kind: patch
  include: [a]
apiVersion: v1alpha1
kind: UnattendedInstallConfig
installer:
    disk: /dev/vda
"#;

fn mint(config: &Config) {
    let ran = config.run(&["mint"]);
    assert_eq!(ran.code, Some(0), "{}", ran.stderr);
}

fn topology(config: &Config) -> MeshConfig {
    SlipmeshFile::parse(&config.text()).unwrap().into_topology()
}

#[cfg(unix)]
#[test]
fn an_edit_through_a_symlink_edits_its_target() {
    let config = Config::holding(SLIPMESH);
    let target = config.path.with_file_name("target.yaml");
    std::fs::rename(&config.path, &target).unwrap();
    std::os::unix::fs::symlink(&target, &config.path).unwrap();
    mint(&config);
    assert!(
        std::fs::symlink_metadata(&config.path)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(
        std::fs::read_to_string(&target)
            .unwrap()
            .contains("mesh_private_key")
    );
}

#[test]
fn mint_writes_what_it_minted_into_slipmesh_yaml() {
    let config = Config::holding(SLIPMESH);
    mint(&config);
    let topology = topology(&config);
    for node in &topology.nodes {
        assert!(node.mesh_private_key.is_some(), "{}", node.name);
    }
    assert!(topology.mesh.links[0].obfuscation.h1.is_some());
}

#[test]
fn a_run_that_mints_nothing_still_writes_the_keys_it_lacks() {
    let config = Config::holding(SLIPMESH);
    mint(&config);
    let mut yaml = MultiDoc::parse(&config.text()).unwrap();
    let cluster = yaml.document_mut(0).unwrap()["cluster"]
        .as_mapping_mut()
        .unwrap();
    cluster.shift_remove("interface_prefix");
    cluster.shift_remove("route_proto_id");
    std::fs::write(&config.path, yaml.text().unwrap()).unwrap();

    mint(&config);
    let cluster = topology(&config).cluster;
    assert_eq!(cluster.interface_prefix.mesh.as_deref(), Some("mesh-"));
    assert_eq!(cluster.route_proto_id.egress, Some(152));
}

#[test]
fn a_second_run_changes_nothing() {
    let config = Config::holding(SLIPMESH);
    mint(&config);
    let first = config.text();
    mint(&config);
    assert_eq!(config.text(), first);
}
