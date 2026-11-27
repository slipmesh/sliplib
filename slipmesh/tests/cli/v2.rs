//! The commands on a file of format version 2: the example of its specification.

use crate::common::{Config, MESH, TALOS, edited, mint, minted};
use sliplib_config::v2::File;
use sliplib_config::v2::network::LinkKind;
use sliplib_yaml::Value;

#[test]
fn mint_writes_a_key_for_every_node_on_a_link_and_the_obfuscation_of_every_amneziawg_link() {
    let config = Config::holding(MESH);
    let ran = config.run(&["mint"]);
    assert_eq!(ran.code, Some(0), "{}", ran.stderr);

    let file = File::parse(&config.text()).unwrap();
    let network = file.network();
    for (name, node) in &network.nodes {
        let key = node.wireguard.as_ref().and_then(|w| w.private_key.as_ref());
        assert!(key.is_some(), "{name}");
    }
    for (name, link) in &network.mesh.links {
        if let LinkKind::Amneziawg(block) = &link.kind {
            let obfuscation = &block.obfuscation.as_ref().unwrap().0;
            assert!(
                obfuscation.jc.is_some() && obfuscation.h1.is_some(),
                "{name}"
            );
        }
    }
    // A link of type `wireguard` takes no obfuscation, and its block gains no key.
    let written = config.documents();
    assert_eq!(
        written[0]["mesh"]["links"]["a-r1"]["wireguard"],
        sliplib_yaml::from_str::<Value>("{port: 51832}").unwrap()
    );
    // Written under the link, not copied down from the network's defaults.
    assert!(written[0]["mesh"]["links"]["a-b"]["amneziawg"]["obfuscation"]["s4"].is_null());

    for route in [
        "nodes.node-a.wireguard.private_key",
        "nodes.router1.wireguard.private_key",
        "mesh.links.a-b.amneziawg.obfuscation",
        "mesh.links.a-c.amneziawg.obfuscation",
    ] {
        assert!(ran.stdout.contains(route), "{}", ran.stdout);
    }
    let key = written[0]["nodes"]["node-a"]["wireguard"]["private_key"]
        .as_str()
        .unwrap();
    assert!(!ran.stdout.contains(key) && !ran.stderr.contains(key));
}

#[test]
fn mint_writes_into_the_network_document_wherever_it_stands_in_the_file() {
    let config = Config::holding(&format!("{TALOS}---\n{MESH}"));
    mint(&config);
    let written = config.documents();
    let network = written.last().unwrap();
    assert!(network["nodes"]["node-a"]["wireguard"]["private_key"].is_string());
    assert!(network["mesh"]["links"]["a-b"]["amneziawg"]["obfuscation"]["h1"].is_number());
    assert!(written[0].get("nodes").is_none());
}

#[test]
fn check_refuses_a_file_without_the_interface_prefix_once_naming_mint() {
    let config = minted();
    edited(&config, |network| {
        network["mesh"]
            .as_mapping_mut()
            .unwrap()
            .shift_remove("interface_prefix");
    });
    let checked = config.run(&["check"]);
    assert_eq!(checked.code, Some(1));
    assert_eq!(
        checked.stderr,
        format!("{}\n", sliplib_mesh::network::Error::NoInterfacePrefix)
    );
}

#[test]
fn rekey_gives_the_node_a_new_key_and_changes_no_other_value() {
    let config = minted();
    let before = config.documents();

    let ran = config.run(&["rekey", "--node", "node-b"]);
    assert_eq!(ran.code, Some(0), "{}", ran.stderr);

    let mut after = config.documents();
    let key = after[0]["nodes"]["node-b"]["wireguard"]["private_key"].clone();
    let old = &before[0]["nodes"]["node-b"]["wireguard"]["private_key"];
    assert_ne!(&key, old);
    after[0]["nodes"]["node-b"]["wireguard"]["private_key"] = old.clone();
    assert_eq!(after, before);

    assert!(
        ran.stdout.contains("nodes.node-b.wireguard.private_key"),
        "{}",
        ran.stdout
    );
    assert!(
        !ran.stdout.contains(key.as_str().unwrap()),
        "{}",
        ran.stdout
    );
}

#[test]
fn rekey_of_an_unknown_node_is_refused_naming_the_nodes_and_writes_nothing() {
    let config = minted();
    let before = config.text();
    let ran = config.run(&["rekey", "--node", "node-x"]);
    assert_eq!(ran.code, Some(1));
    assert!(
        ran.stderr.contains("node-x") && ran.stderr.contains("router1"),
        "{}",
        ran.stderr
    );
    assert_eq!(config.text(), before);
}

/// As the loader lays it.
fn obfuscation_in_force(config: &Config, name: &str) -> sliplib_config::Obfuscation {
    let files = File::parse(&config.text()).unwrap();
    match &files.network().mesh.links[name].kind {
        LinkKind::Amneziawg(block) => block.obfuscation.clone().unwrap().0,
        LinkKind::Wireguard(_) => panic!("{name} is of type wireguard"),
    }
}

#[test]
fn mint_keeps_a_link_without_what_its_null_obfuscation_took_away() {
    let config = minted();
    edited(&config, |network| {
        network["mesh"]["links"]["a-b"]["amneziawg"]["obfuscation"] = Value::Null;
    });
    let ran = config.run(&["mint"]);
    assert_eq!(ran.code, Some(0), "{}", ran.stderr);
    let in_force = obfuscation_in_force(&config, "a-b");
    // The fields `mint` writes, and none of `mesh.amneziawg.obfuscation`'s.
    assert!(
        in_force.jc.is_some() && in_force.h4.is_some(),
        "{in_force:?}"
    );
    assert_eq!((in_force.s3, in_force.s4, in_force.i1), (None, None, None));
    assert_eq!(in_force.random_trailers, None);
    assert_eq!(config.run(&["check"]).code, Some(0));
}

#[test]
fn mint_and_rekey_write_a_key_under_a_wireguard_written_null() {
    let config = Config::holding(MESH);
    edited(&config, |network| {
        network["nodes"]["node-b"]["wireguard"] = Value::Null;
    });
    assert_eq!(config.run(&["mint"]).code, Some(0));
    assert!(config.documents()[0]["nodes"]["node-b"]["wireguard"]["private_key"].is_string());
    edited(&config, |network| {
        network["nodes"]["node-c"]["wireguard"] = Value::Null;
    });
    assert_eq!(config.run(&["rekey", "--node", "node-c"]).code, Some(0));
    assert!(config.documents()[0]["nodes"]["node-c"]["wireguard"]["private_key"].is_string());
}

#[test]
fn rekey_refuses_a_node_on_no_link_and_writes_nothing() {
    let config = minted();
    edited(&config, |network| {
        network["nodes"]["node-d"] =
            sliplib_yaml::from_str("{platform: talos, node_id: 0.0.0.5, underlay: {wan: {}}}")
                .unwrap();
    });
    let before = config.text();
    let ran = config.run(&["rekey", "--node", "node-d"]);
    assert_eq!(ran.code, Some(1), "{}", ran.stdout);
    assert!(ran.stderr.contains("node-d"), "{}", ran.stderr);
    assert_eq!(config.text(), before);
}
