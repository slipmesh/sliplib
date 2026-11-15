use crate::common::{Config, SLIPMESH, mint, topology};
use sliplib_yaml::MultiDoc;

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

#[test]
fn mint_names_what_it_wrote_and_prints_no_key() {
    let config = Config::holding(SLIPMESH);
    let ran = config.run(&["mint"]);
    assert_eq!(ran.code, Some(0), "{}", ran.stderr);
    for written in [
        "nodes[a].mesh_private_key",
        "nodes[b].mesh_private_key",
        "nodes[c].mesh_private_key",
        "nodes[d].mesh_private_key",
        "mesh.links[a|b].obfuscation",
        "cluster.interface_prefix.mesh",
        "cluster.route_proto_id.egress",
    ] {
        assert!(ran.stdout.contains(written), "{written}: {}", ran.stdout);
    }
    for node in topology(&config).nodes {
        let key = node.mesh_private_key.unwrap();
        assert!(!ran.stdout.contains(key.expose()), "{}", node.name);
    }
}

#[test]
fn a_run_with_nothing_to_write_leaves_the_file_and_its_comments_as_they_are() {
    let config = Config::holding(SLIPMESH);
    mint(&config);
    let commented = format!("# kept by hand\n{}", config.text());
    std::fs::write(&config.path, &commented).unwrap();
    let ran = config.run(&["mint"]);
    assert_eq!(ran.code, Some(0), "{}", ran.stderr);
    assert_eq!(ran.stdout, "");
    assert_eq!(config.text(), commented);
}
