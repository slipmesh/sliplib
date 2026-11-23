//! The commands on a file of format version 2: the example of its specification.

use crate::common::{Config, MESH, TALOS, edited, mint, minted, names_line};
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
fn mint_writes_the_reference_value_of_the_interface_prefix_and_mints_with_it() {
    let config = Config::holding(MESH);
    edited(&config, |network| {
        network["mesh"]
            .as_mapping_mut()
            .unwrap()
            .shift_remove("interface_prefix");
    });
    let ran = config.run(&["mint"]);
    assert_eq!(ran.code, Some(0), "{}", ran.stderr);
    assert_eq!(config.documents()[0]["mesh"]["interface_prefix"], "mesh-");
    assert!(
        ran.stdout.contains("mesh.interface_prefix"),
        "{}",
        ran.stdout
    );
    assert_eq!(config.run(&["check"]).code, Some(0));
}

#[test]
fn check_passes_the_minted_example_naming_each_node() {
    let config = minted();
    let checked = config.run(&["check"]);
    assert_eq!(checked.code, Some(0), "{}", checked.stderr);
    assert_eq!(
        checked.stdout,
        "node-a: ok\nnode-b: ok\nnode-c: ok\nrouter1: ok\n"
    );
    // The example turns `random_trailers` on, and what `mint` draws may stand where a warning
    // names it: nothing but a warning is told.
    for line in checked.stderr.lines() {
        assert!(line.starts_with("warning: "), "{line}");
    }
}

#[test]
fn check_refuses_every_node_that_lacks_a_secret_naming_what_creates_it() {
    let config = Config::holding(MESH);
    let checked = config.run(&["check"]);
    assert_eq!(checked.code, Some(1));
    assert_eq!(checked.stdout, "");
    let refusals: Vec<&str> = checked.stderr.lines().collect();
    assert_eq!(refusals.len(), 4, "{}", checked.stderr);
    for refusal in refusals {
        assert!(
            refusal.contains("wireguard.private_key")
                && refusal.ends_with(" - `slipmesh mint` creates it"),
            "{refusal}"
        );
    }
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
fn check_reads_the_block_of_a_platform_or_template_document_and_not_its_body() {
    // Without the documents of the example: it holds the one `platform` document of `talos`.
    let config = Config::holding(MESH);
    mint(&config);
    let bodies = "slipmesh: {kind: platform, platform: talos}\nlisten: true\n---\n\
                  slipmesh: {kind: platform, platform: routeros}\nmetrics: {}\n---\n\
                  slipmesh: {kind: template, platform: talos}\ndaemon: router\ncontent: x\n";
    std::fs::write(&config.path, format!("{}---\n{bodies}", config.text())).unwrap();
    let checked = config.run(&["check"]);
    assert_eq!(checked.code, Some(0), "{}", checked.stderr);
    assert!(checked.stdout.contains("node-a: ok"), "{}", checked.stdout);
}

#[test]
fn check_warns_about_a_link_that_loses_packets_and_still_passes() {
    let config = minted();
    edited(&config, |network| {
        network["mesh"]["links"]["a-b"]["amneziawg"]["obfuscation"]["h1"] = "100-199".into();
    });
    let checked = config.run(&["check"]);
    assert_eq!(checked.code, Some(0), "{}", checked.stderr);
    assert!(
        checked.stderr.starts_with("warning: ") && checked.stderr.contains("a-b"),
        "{}",
        checked.stderr
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
fn rekey_gives_a_key_to_a_node_that_had_none() {
    let config = Config::holding(MESH);
    let ran = config.run(&["rekey", "--node", "node-c"]);
    assert_eq!(ran.code, Some(0), "{}", ran.stderr);
    let written = config.documents();
    assert!(written[0]["nodes"]["node-c"]["wireguard"]["private_key"].is_string());
    assert!(written[0]["nodes"]["node-a"]["wireguard"].is_null());
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

#[test]
fn rekey_refuses_files_that_give_no_network_and_writes_nothing() {
    let config = minted();
    edited(&config, |network| {
        network["mesh"]
            .as_mapping_mut()
            .unwrap()
            .shift_remove("interface_prefix");
    });
    let before = config.text();
    let ran = config.run(&["rekey", "--node", "node-b"]);
    assert_eq!(ran.code, Some(1), "{}", ran.stdout);
    assert_eq!(config.text(), before);
}

#[test]
fn every_command_refuses_a_file_of_version_1_naming_how_to_convert_it_and_writes_nothing() {
    // The network document starts at line 3.
    let version_1 = "# version 1\n\nslipmesh: {kind: network}\ncluster:\n  bgp_as: 64512\n  \
        loopback_networks: {ipv4: 198.51.100.0/25, ipv6: '3fff::/32'}\n  \
        interface_prefix: {mesh: mesh-}\n  route_proto_id: {egress: 152}\nnodes:\n  - \
        {name: a, node_id: 198.51.100.1}\n";
    for command in [
        &["check"][..],
        &["mint"],
        &["rekey", "--node", "a"],
        &["fmt"],
    ] {
        let config = Config::holding(version_1);
        let ran = config.run(command);
        assert_eq!(ran.code, Some(1), "{command:?}: {}", ran.stdout);
        assert!(
            ran.stderr.contains("docs/slipmesh-yaml-v1-to-v2.md")
                && ran.stderr.contains("`kind: network`")
                && names_line(&ran.stderr, 3),
            "{command:?}: {}",
            ran.stderr
        );
        assert_eq!(config.text(), version_1, "{command:?}");
    }
}
