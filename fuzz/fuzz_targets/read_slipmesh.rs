#![no_main]

use libfuzzer_sys::fuzz_target;
use serde::de::DeserializeOwned;
use sliplib_config::v2::network::LinkKind;
use sliplib_config::v2::{Aim, File, Name};
use sliplib_config::{Obfuscation, Range};
use sliplib_mesh::model::Network;
use sliplib_yaml::{Mapping, MultiDoc, Value};
use std::net::{IpAddr, Ipv4Addr};

/// What the loader holds of a document's aim: it reaches nodes, each of the document's platform.
fn assert_aimed(files: &File, aim: &Aim) {
    assert!(!aim.nodes().is_empty(), "{aim} reaches no node");
    for node in aim.nodes() {
        let platform = files.network().nodes[node].platform;
        assert_eq!(platform, aim.platform(), "{aim} reaches {node}");
    }
}

/// A node is given each document of `all` whose aim holds it, once, by rank and then as read.
fn assert_given(node: &str, given: &[usize], all: &[&Aim]) {
    let mut reaching: Vec<usize> = (0..all.len())
        .filter(|&index| all[index].nodes().iter().any(|name| name.as_str() == node))
        .collect();
    reaching.sort_by_key(|&index| (all[index].rank(), index));
    assert_eq!(given, reaching, "{node}");
}

fn index_of<T>(all: &[T], one: &T) -> usize {
    all.iter().position(|each| std::ptr::eq(each, one)).unwrap()
}

/// The entry of `mapping` whose key reads as `name`.
fn entry<'a>(mapping: Option<&'a Value>, name: &Name) -> Option<&'a Value> {
    mapping?.as_mapping()?.iter().find_map(|(key, value)| {
        let key: Name = sliplib_yaml::from_value(key.clone()).ok()?;
        (key == *name).then_some(value)
    })
}

/// What is in force where `own`, as written, stands over `network`: its value, nothing where it
/// is `null`, the network's where it is left out.
fn laid<T: DeserializeOwned>(own: Option<&Value>, network: Option<T>) -> Option<T> {
    match own {
        Some(Value::Null) => None,
        Some(value) => Some(sliplib_yaml::from_value(value.clone()).unwrap()),
        None => network,
    }
}

/// The fields of `own`, as written, laid one by one over those of `network`.
fn laid_fields(own: &Mapping, network: Option<&Obfuscation>) -> Obfuscation {
    let mut fields = match network.map(sliplib_yaml::to_value) {
        Some(Ok(Value::Mapping(fields))) => fields,
        _ => Mapping::new(),
    };
    for (key, value) in own {
        if value.is_null() {
            fields.shift_remove(key);
        } else {
            fields.insert(key.clone(), value.clone());
        }
    }
    sliplib_yaml::from_value(Value::Mapping(fields)).unwrap()
}

/// A node holds its own lists of `routing` where it wrote them and the network's where it did not,
/// and a link the keepalive and the obfuscation of its type unless it wrote its own or `null`.
fn assert_in_force(files: &File, written: &Value) {
    let network = files.network();
    for (name, node) in &network.nodes {
        let own = entry(written.get("nodes"), name).and_then(|node| node.get("routing"));
        let own = |key: &str| {
            own.filter(|own| own.is_mapping())
                .and_then(|own| own.get(key))
        };
        let lists = &network.routing;
        let expected = laid(
            own("direct_interfaces"),
            Some(lists.direct_interfaces.clone()),
        );
        assert_eq!(
            node.routing.direct_interfaces,
            expected.unwrap_or_default(),
            "{name}"
        );
        let expected = laid(own("learn"), Some(lists.learn.clone()));
        assert_eq!(node.routing.learn, expected.unwrap_or_default(), "{name}");
        let expected = laid(own("announce"), Some(lists.announce.clone()));
        assert_eq!(
            node.routing.announce,
            expected.unwrap_or_default(),
            "{name}"
        );
    }
    let links = written.get("mesh").and_then(|mesh| mesh.get("links"));
    for (name, link) in &network.mesh.links {
        let own = entry(links, name).and_then(|written| written.get(link.kind.name()));
        let own = |key: &str| own.and_then(|own| own.get(key));
        match &link.kind {
            LinkKind::Wireguard(block) => {
                let defaults = network.mesh.wireguard.as_ref();
                let expected = laid(
                    own("persistent_keepalive"),
                    defaults.and_then(|defaults| defaults.persistent_keepalive),
                );
                assert_eq!(block.persistent_keepalive, expected, "{name}");
            }
            LinkKind::Amneziawg(block) => {
                let defaults = network.mesh.amneziawg.as_ref();
                let expected: Option<Range<u16>> = laid(
                    own("persistent_keepalive"),
                    defaults.and_then(|defaults| defaults.persistent_keepalive),
                );
                assert_eq!(block.persistent_keepalive, expected, "{name}");
                let of_type = defaults
                    .and_then(|defaults| defaults.obfuscation.as_ref())
                    .map(|obfuscation| &obfuscation.0);
                let expected = match own("obfuscation") {
                    Some(Value::Null) => None,
                    Some(Value::Mapping(fields)) => Some(laid_fields(fields, of_type)),
                    Some(other) => panic!("{name}: an obfuscation read from {other:?}"),
                    None => of_type.cloned(),
                };
                let held = block.obfuscation.as_ref().map(|obfuscation| &obfuscation.0);
                assert_eq!(held, expected.as_ref(), "{name}");
            }
        }
    }
}

/// The `network` document as written, its block kept: the one of the files, since they read.
fn written_network(text: &str) -> Value {
    let yaml = MultiDoc::parse(text).unwrap();
    let is_network = |document: &&sliplib_yaml::Document| {
        let block = document.value.get("slipmesh");
        block
            .and_then(|block| block.get("kind"))
            .and_then(Value::as_str)
            == Some("network")
    };
    yaml.documents()
        .iter()
        .find(is_network)
        .unwrap()
        .value
        .clone()
}

/// The host bits of `address` in a network of `prefix_len`, out of `width` bits.
fn host_bits(address: u128, prefix_len: u8, width: u8) -> u128 {
    let host = width - prefix_len;
    if host == 0 {
        0
    } else {
        address & (u128::MAX >> (128 - u32::from(host)))
    }
}

/// Every address the lowering derived is the network it is derived in with the whole `node_id` in
/// its host bits: a node's loopbacks, its link-local, and the IPv4 of its links where the links
/// carry one. Without a network for the link-locals, a node's is `fe80::` and the low 32 bits of
/// its IPv6 loopback. With `node_id`s unique, not `0.0.0.0` and fitting, the loader's rule, every
/// kind of address is distinct between nodes and none has a host part of zeros.
fn assert_derived(files: &File, network: &Network) {
    let written = files.network();
    let addressing = &written.addressing;
    let tunnel = addressing.tunnel.as_ref();
    let link_local = tunnel
        .and_then(|tunnel| tunnel.link_local)
        .map(|prefix| prefix.0);
    let tunnel_ipv4 = tunnel.and_then(|tunnel| tunnel.ipv4).map(|prefix| prefix.0);
    let (ipv4, ipv6) = (addressing.loopback.ipv4.0, addressing.loopback.ipv6.0);
    for node in &network.nodes {
        let name = &node.name;
        let id = u128::from(u32::from(
            written
                .nodes
                .iter()
                .find(|(key, _)| key.as_str() == name)
                .unwrap()
                .1
                .node_id,
        ));
        assert_ne!(id, 0, "{name}");
        let v4_host = |address: Ipv4Addr, prefix_len| {
            host_bits(u128::from(u32::from(address)), prefix_len, 32)
        };
        assert!(ipv4.contains(&node.loopback.0), "{name}");
        assert_eq!(v4_host(node.loopback.0, ipv4.prefix_len()), id, "{name}");
        assert!(ipv6.contains(&node.loopback.1), "{name}");
        assert_eq!(
            host_bits(u128::from(node.loopback.1), ipv6.prefix_len(), 128),
            id,
            "{name}"
        );
        match link_local {
            Some(net) => {
                assert!(net.contains(&node.link_local), "{name}");
                assert_eq!(
                    host_bits(u128::from(node.link_local), net.prefix_len(), 128),
                    id
                );
            }
            None => assert_eq!(
                u128::from(node.link_local),
                (0xfe80_u128 << 112) | (u128::from(node.loopback.1) & 0xffff_ffff),
                "{name}"
            ),
        }
        match (tunnel_ipv4, node.tunnel_ipv4) {
            (Some(net), Some(address)) => {
                assert!(net.contains(&address), "{name}");
                assert_eq!(v4_host(address, net.prefix_len()), id, "{name}");
            }
            (None, None) => {}
            other => panic!("{name}: the IPv4 of its links {other:?}"),
        }
    }
    let distinct = |kind: &str, addresses: Vec<IpAddr>| {
        let mut seen = std::collections::BTreeSet::new();
        for address in addresses {
            assert!(seen.insert(address), "two nodes with the {kind} {address}");
        }
    };
    let nodes = &network.nodes;
    distinct(
        "IPv4 loopback",
        nodes.iter().map(|n| n.loopback.0.into()).collect(),
    );
    distinct(
        "IPv6 loopback",
        nodes.iter().map(|n| n.loopback.1.into()).collect(),
    );
    distinct(
        "link-local",
        nodes.iter().map(|n| n.link_local.into()).collect(),
    );
    distinct(
        "IPv4 on the links",
        nodes
            .iter()
            .filter_map(|n| n.tunnel_ipv4.map(IpAddr::from))
            .collect(),
    );
}

/// What a tool reads of a file of format version 2, and what the loader promises of it.
fn assert_read(files: &File, text: &str) {
    assert_in_force(files, &written_network(text));
    let _ = files.talos_settings();
    let _ = files.routeros_settings();
    let templates: Vec<&Aim> = files.templates().iter().map(|t| t.aim()).collect();
    let patches: Vec<&Aim> = files.patches().iter().map(|p| p.aim()).collect();
    for template in files.templates() {
        let _ = template.read();
    }
    for aim in templates.iter().chain(&patches) {
        assert_aimed(files, aim);
    }
    for node in files.network().nodes.keys() {
        let node = node.as_str();
        let given: Vec<usize> = files
            .templates_for(node)
            .iter()
            .map(|template| index_of(files.templates(), *template))
            .collect();
        assert_given(node, &given, &templates);
        let given: Vec<usize> = files
            .patches_for(node)
            .iter()
            .map(|patch| index_of(files.patches(), *patch))
            .collect();
        assert_given(node, &given, &patches);
    }
}

// Everything a tool reads of the files, up to the configs of every node: the text may be refused at
// any step, none of the steps may panic, and what is not refused holds what each step promises of
// it.
fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(files) = File::parse(text) else {
        return;
    };
    assert_read(&files, text);
    // The loader has refused everything else a network can be refused for.
    let network = match sliplib_mesh::v2::lower(&files) {
        Ok(network) => network,
        Err(sliplib_mesh::v2::Error::NoInterfacePrefix) => {
            assert!(files.network().mesh.interface_prefix.is_none());
            return;
        }
        Err(other) => panic!("lowering refused what the loader took: {other}"),
    };
    assert_derived(&files, &network);
    let resolved = sliplib_mesh::secrets::read(&network);
    for node in &network.nodes {
        let _ = sliplib_mesh::render::node_configs(&network, &node.name, &resolved);
    }
});
