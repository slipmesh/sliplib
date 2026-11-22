//! The files of format version 2 as the network the configs are computed from: every node with
//! the addresses its `node_id` gives it, every link with what the loader found in force on it.

use crate::addressing;
use crate::model::{Endpoint, Link, MetricsPorts, Network, Node};
use crate::mtu::DEFAULT_UPLINK_MTU;
use sliplib_config::router::{AnnounceEntry, BfdSettings};
use sliplib_config::v2::File;
use sliplib_config::v2::network::{self, LinkKind};
use sliplib_config::{Obfuscation, Range};

/// Why the files, which read, give no network to compute from.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("mesh.interface_prefix is not set - `slipmesh mint` writes it")]
    NoInterfacePrefix,
}

/// `files` as a network. The loader's rule that every `node_id` fits each network it is added to,
/// and is not `0.0.0.0`, leaves the derived addresses distinct and none with a host part of zeros.
pub fn lower(files: &File) -> Result<Network, Error> {
    let written = files.network();
    let prefix = written
        .mesh
        .interface_prefix
        .as_ref()
        .ok_or(Error::NoInterfacePrefix)?
        .as_str();
    let loopback = &written.addressing.loopback;
    let tunnel = written.addressing.tunnel.as_ref();
    let link_local = tunnel
        .and_then(|tunnel| tunnel.link_local)
        .map(|prefix| prefix.0);
    let tunnel_ipv4 = tunnel.and_then(|tunnel| tunnel.ipv4).map(|prefix| prefix.0);
    // The links' interfaces are announced if and only if they carry an IPv4 address, so their
    // pattern joins a node's list here and is never written by hand.
    let pattern = tunnel_ipv4.map(|_| format!("{prefix}*"));

    let nodes: Vec<Node> = written
        .nodes
        .iter()
        .map(|(name, node)| {
            let id = node.node_id;
            let (ipv4, ipv6) = (loopback.ipv4.0, loopback.ipv6.0);
            // The loader leaves exactly one entry.
            let underlay = node.underlay.first().map(|(_, underlay)| underlay);
            let mut direct_interfaces = node.routing.direct_interfaces.clone();
            if let Some(pattern) = &pattern
                && !direct_interfaces.contains(pattern)
            {
                direct_interfaces.push(pattern.clone());
            }
            let ipv6_loopback = addressing::ipv6_in(ipv6.addr(), ipv6.prefix_len(), id);
            Node {
                name: name.to_string(),
                platform: node.platform,
                loopback: (
                    addressing::ipv4_in(ipv4.addr(), ipv4.prefix_len(), id),
                    ipv6_loopback,
                ),
                link_local: link_local.map_or_else(
                    || addressing::link_local_from_loopback(ipv6_loopback),
                    |network| addressing::ipv6_in(network.addr(), network.prefix_len(), id),
                ),
                tunnel_ipv4: tunnel_ipv4
                    .map(|net| addressing::ipv4_in(net.addr(), net.prefix_len(), id)),
                private_key: node
                    .wireguard
                    .as_ref()
                    .and_then(|wireguard| wireguard.private_key.clone()),
                endpoint: underlay
                    .and_then(|underlay| underlay.endpoint.as_ref())
                    .map(|endpoint| Endpoint {
                        written: endpoint.host().to_owned(),
                        family: endpoint.family(),
                    }),
                underlay_mtu: underlay
                    .and_then(|underlay| underlay.mtu)
                    .unwrap_or(DEFAULT_UPLINK_MTU),
                direct_interfaces,
                learn: prefixes(&node.routing.learn),
                announce: prefixes(&node.routing.announce)
                    .into_iter()
                    .map(|net| AnnounceEntry { net, label: None })
                    .collect(),
            }
        })
        .collect();

    Ok(Network {
        nodes,
        links: written
            .mesh
            .links
            .iter()
            .map(|(name, link)| {
                let (plain, obfuscation, persistent_keepalive) = match &link.kind {
                    LinkKind::Wireguard(block) => (
                        true,
                        Obfuscation::default(),
                        block.persistent_keepalive.map(Range::from),
                    ),
                    LinkKind::Amneziawg(block) => (
                        false,
                        block
                            .obfuscation
                            .as_ref()
                            .map(|obfuscation| obfuscation.0.clone())
                            .unwrap_or_default(),
                        block.persistent_keepalive,
                    ),
                };
                Link {
                    name: name.to_string(),
                    ends: link.ends.clone().map(|end| end.to_string()),
                    port: link.kind.port().get(),
                    plain,
                    obfuscation,
                    persistent_keepalive,
                }
            })
            .collect(),
        bgp_as: written.routing.bgp.number.get(),
        bfd: written.routing.bfd.as_ref().map(|bfd| BfdSettings {
            min_rx_ms: bfd.min_rx_ms.get(),
            min_tx_ms: bfd.min_tx_ms.get(),
            multiplier: bfd.multiplier.get(),
        }),
        interface_prefix: prefix.to_owned(),
        mtu: written.mesh.mtu,
        classes: written
            .routing
            .classes
            .iter()
            .map(|(name, class)| (name.to_string(), class.protocol))
            .collect(),
        metrics: MetricsPorts::default(),
    })
}

fn prefixes(list: &[network::Prefix4]) -> Vec<String> {
    list.iter().map(|prefix| prefix.0.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Family;
    use crate::render::node_configs;
    use crate::secrets;
    use sliplib_config::v2::Platform;

    use crate::test_support::{example as lowered, example_files as files};

    fn refusal(overlay: &str) -> Error {
        lower(&files(overlay)).unwrap_err()
    }

    fn link<'a>(network: &'a Network, name: &str) -> &'a Link {
        network.links.iter().find(|link| link.name == name).unwrap()
    }

    fn range(lo: u16, hi: u16) -> Range<u16> {
        Range::new(lo, hi).unwrap()
    }

    #[test]
    fn a_node_gets_its_addresses_from_its_node_id() {
        let network = lowered("{}");
        let node = network.node("node-a").unwrap();
        assert_eq!(node.loopback.0.to_string(), "198.51.100.1");
        assert_eq!(node.loopback.1.to_string(), "3fff:0:0:1::1");
        assert_eq!(node.link_local.to_string(), "fe80::1");
        assert_eq!(node.tunnel_ipv4.unwrap().to_string(), "198.51.100.129");
        let other = network.node("node-c").unwrap();
        assert_eq!(other.loopback.0.to_string(), "198.51.100.3");
        assert_eq!(other.link_local.to_string(), "fe80::3");
    }

    #[test]
    fn the_link_local_is_the_node_id_in_the_host_part_of_its_network() {
        let own = lowered("{addressing: {tunnel: {link_local: 'fe80:0:0:1::/64'}}}");
        assert_eq!(
            own.node("node-b").unwrap().link_local.to_string(),
            "fe80:0:0:1::2"
        );
        // A network that leaves fewer than 32 bits free keeps its own bits above the `node_id`.
        let narrow = lowered("{addressing: {tunnel: {link_local: 'fe80::1:0/120'}}}");
        assert_eq!(
            narrow.node("node-b").unwrap().link_local.to_string(),
            "fe80::1:2"
        );
    }

    #[test]
    fn without_a_tunnel_network_a_link_has_no_ipv4_and_its_pattern_joins_no_list() {
        let network = lowered("{addressing: {tunnel: null}}");
        let node = network.node("node-a").unwrap();
        assert_eq!(node.tunnel_ipv4, None);
        assert_eq!(node.link_local.to_string(), "fe80::1");
        assert_eq!(node.direct_interfaces, ["lan0"]);
    }

    #[test]
    fn with_a_tunnel_ipv4_the_pattern_of_the_links_ends_every_nodes_list() {
        let network = lowered("{}");
        assert_eq!(
            network.node("node-a").unwrap().direct_interfaces,
            ["lan0", "mesh-*"]
        );
        assert_eq!(
            network.node("router1").unwrap().direct_interfaces,
            ["bridge-lan", "mesh-*"]
        );
    }

    #[test]
    fn a_pattern_already_in_a_list_keeps_its_place_and_is_not_added() {
        let network = lowered("{routing: {direct_interfaces: ['mesh-*', lan0]}}");
        assert_eq!(
            network.node("node-a").unwrap().direct_interfaces,
            ["mesh-*", "lan0"]
        );
    }

    #[test]
    fn the_pattern_follows_the_interface_prefix() {
        let network = lowered("{mesh: {interface_prefix: wg-}}");
        assert_eq!(
            network.node("node-a").unwrap().direct_interfaces,
            ["lan0", "wg-*"]
        );
        assert_eq!(network.interface_prefix, "wg-");
    }

    #[test]
    fn a_node_announces_and_learns_what_is_in_force_for_it() {
        let network = lowered("{}");
        let node = network.node("node-a").unwrap();
        assert_eq!(node.learn, ["203.0.113.0/26"]);
        assert_eq!(
            node.announce,
            [AnnounceEntry {
                net: "203.0.113.64/26".into(),
                label: None
            }]
        );
        let router = network.node("router1").unwrap();
        assert!(router.learn.is_empty() && router.announce.is_empty());
    }

    #[test]
    fn a_node_is_dialed_at_its_endpoint_over_the_family_of_its_key() {
        let network = lowered(
            "{nodes: {node-b: {underlay: {wan: {endpoint: {ipv4: null, ipv6: node-b.example.com}}}}}}",
        );
        assert_eq!(
            network.node("node-a").unwrap().endpoint,
            Some(Endpoint {
                written: "192.0.2.1".into(),
                family: Family::Ipv4,
            })
        );
        assert_eq!(
            network.node("node-b").unwrap().endpoint,
            Some(Endpoint {
                written: "node-b.example.com".into(),
                family: Family::Ipv6,
            })
        );
        assert_eq!(network.node("node-c").unwrap().endpoint, None);
    }

    #[test]
    fn a_node_has_the_mtu_of_its_underlay_or_that_of_ethernet() {
        let network = lowered("{}");
        assert_eq!(network.node("node-c").unwrap().underlay_mtu, 1420);
        assert_eq!(network.node("node-a").unwrap().underlay_mtu, 1500);
    }

    #[test]
    fn a_node_has_its_platform_and_the_key_written_for_it() {
        let network = lowered("{nodes: {node-b: {wireguard: null}}}");
        assert_eq!(network.node("node-a").unwrap().platform, Platform::Talos);
        assert_eq!(
            network.node("router1").unwrap().platform,
            Platform::Routeros
        );
        assert!(network.node("node-a").unwrap().private_key.is_some());
        assert!(network.node("node-b").unwrap().private_key.is_none());
    }

    #[test]
    fn a_link_of_type_amneziawg_has_the_obfuscation_in_force_on_it() {
        let network = lowered("{}");
        let a_b = link(&network, "a-b");
        assert!(!a_b.plain);
        assert_eq!(a_b.ends, ["node-a", "node-b"]);
        assert_eq!(a_b.port, 51830);
        // Its own, and the network's.
        assert_eq!(a_b.obfuscation.jc, Some(4));
        assert_eq!(a_b.obfuscation.s4, Some(16));
        assert_eq!(a_b.persistent_keepalive, None);
        assert_eq!(
            link(&network, "a-c").persistent_keepalive,
            Some(range(15, 20))
        );
    }

    #[test]
    fn a_link_of_type_wireguard_is_plain_and_carries_no_obfuscation() {
        let network = lowered("{mesh: {wireguard: {persistent_keepalive: 10}}}");
        let a_r1 = link(&network, "a-r1");
        assert!(a_r1.plain);
        assert_eq!(a_r1.obfuscation, Obfuscation::default());
        assert_eq!(a_r1.persistent_keepalive, Some(Range::from(10)));
        assert_eq!(a_r1.port, 51832);
    }

    #[test]
    fn what_is_one_for_the_network_is_carried_over() {
        let network = lowered("{}");
        assert_eq!(network.bgp_as, 64500);
        assert_eq!(
            network.bfd,
            Some(BfdSettings {
                min_rx_ms: 300,
                min_tx_ms: 300,
                multiplier: 5
            })
        );
        assert_eq!(network.mtu, Some(1320));
        assert_eq!(network.classes, [("egress".to_owned(), 152)]);
        let names: Vec<&str> = network
            .nodes
            .iter()
            .map(|node| node.name.as_str())
            .collect();
        assert_eq!(names, ["node-a", "node-b", "node-c", "router1"]);
        let links: Vec<&str> = network
            .links
            .iter()
            .map(|link| link.name.as_str())
            .collect();
        assert_eq!(links, ["a-b", "a-c", "a-r1"]);
    }

    #[test]
    fn without_its_block_there_is_no_bfd() {
        assert_eq!(lowered("{routing: {bfd: null}}").bfd, None);
    }

    #[test]
    fn files_without_the_interface_prefix_are_refused_naming_what_writes_it() {
        assert_eq!(
            refusal("{mesh: {interface_prefix: null}}"),
            Error::NoInterfacePrefix
        );
    }

    #[test]
    fn an_ipv6_loopback_network_longer_than_96_takes_a_node_id_that_fits_it() {
        // What v0.19's `ipv6_loopback` derived: the network's bits and the 8 of 0.0.0.200.
        let network = lowered(
            "{addressing: {loopback: {ipv4: 198.18.0.0/24, ipv6: '3fff:0:0:1::/120'}, tunnel: \
             {ipv4: null}}, nodes: {node-b: {node_id: 0.0.0.200}}}",
        );
        let node = network.node("node-b").unwrap();
        assert_eq!(node.loopback.0.to_string(), "198.18.0.200");
        assert_eq!(node.loopback.1.to_string(), "3fff:0:0:1::c8");
        assert_eq!(
            network.node("node-a").unwrap().loopback.1.to_string(),
            "3fff:0:0:1::1"
        );
    }

    #[test]
    fn without_a_link_local_network_the_links_take_the_link_local_of_the_loopback() {
        // As v0.19 gave them: fe80:: and the low 32 bits of the IPv6 loopback, the network's own
        // bits there among them.
        let network =
            lowered("{addressing: {loopback: {ipv6: '3fff::1:0/120'}, tunnel: {ipv4: null}}}");
        let node = network.node("node-c").unwrap();
        assert_eq!(node.loopback.1.to_string(), "3fff::1:3");
        assert_eq!(node.link_local.to_string(), "fe80::1:3");
        assert_eq!(
            node.link_local,
            addressing::link_local_from_loopback(node.loopback.1)
        );
    }

    #[test]
    fn the_mesh_mtu_of_the_example_is_below_the_least_limit_of_its_links_as_its_comment_says() {
        let mtu = |overlay: &str| {
            let network = lowered(overlay);
            crate::mtu::mesh_mtu(&network, &secrets::read(&network)).unwrap()
        };
        assert_eq!(mtu("{}"), Some(1320));
        assert_eq!(mtu("{mesh: {mtu: null}}"), Some(1344));
    }

    fn mtu_refusal(overlay: &str) -> crate::mtu::MtuError {
        let network = lowered(overlay);
        crate::mtu::mesh_mtu(&network, &secrets::read(&network)).unwrap_err()
    }

    #[test]
    fn an_mtu_of_zero_is_refused() {
        use crate::mtu::MtuError;
        let underlay = mtu_refusal("{nodes: {node-b: {underlay: {wan: {mtu: 0}}}}}");
        assert!(
            matches!(&underlay, MtuError::AboveLimit { limit: 0, link, .. } if link == "a-b"),
            "{underlay:?}"
        );
        let mesh = mtu_refusal("{mesh: {mtu: 0}}");
        assert!(
            matches!(mesh, MtuError::BelowMinimum { mtu: 0 }),
            "{mesh:?}"
        );
    }

    /// A node of `talos` without a key, to be laid on no link.
    const NODE_D: &str = "{platform: talos, node_id: 0.0.0.5, underlay: {wan: {}}}";

    #[test]
    fn a_node_on_no_link_is_configured_without_a_key() {
        let network = lowered(&format!("{{nodes: {{node-d: {NODE_D}}}}}"));
        let configs = node_configs(&network, "node-d", &secrets::read(&network)).unwrap();
        assert!(configs.awg.interfaces.is_empty());
        assert_eq!(configs.router.bgp_peers.len(), 4);
        // A node on a link still needs its own.
        let keyless = lowered("{nodes: {node-b: {wireguard: null}}}");
        let refusal = node_configs(&keyless, "node-b", &secrets::read(&keyless)).unwrap_err();
        assert!(refusal.missing_secret().is_some(), "{refusal:?}");
    }

    #[cfg(feature = "mint")]
    #[test]
    fn mint_creates_a_key_for_a_node_on_a_link_alone() {
        let network = lowered(&format!(
            "{{nodes: {{node-b: {{wireguard: null}}, node-d: {NODE_D}}}}}"
        ));
        let minted = crate::mint::create(&network).unwrap();
        let keyed: Vec<&str> = minted
            .node_keys
            .iter()
            .map(|(index, _)| network.nodes[*index].name.as_str())
            .collect();
        assert_eq!(keyed, ["node-b"]);
    }

    /// The example with a node of `name`, keyed, on a link to `node-a`.
    fn with_a_node_named(name: &str) -> Network {
        let key = sliplib_secrets::wireguard::PrivateKey::generate();
        lowered(&format!(
            "{{nodes: {{{name}: {{platform: talos, node_id: 0.0.0.5, underlay: {{wan: {{}}}}, \
             wireguard: {{private_key: {}}}}}}}, mesh: {{links: {{a-x: {{type: wireguard, \
             ends: {{node-a: {{}}, {name}: {{}}}}, wireguard: {{port: 51840}}}}}}}}}}",
            key.expose()
        ))
    }

    #[test]
    fn a_name_too_long_for_an_interface_is_refused_at_the_other_end_of_its_link() {
        use sliplib_config::awg::Invalid;
        let fits = with_a_node_named("node-abcde");
        node_configs(&fits, "node-a", &secrets::read(&fits)).unwrap();
        let long = with_a_node_named("node-abcdef");
        let resolved = secrets::read(&long);
        node_configs(&long, "node-abcdef", &resolved).unwrap();
        let refusal = node_configs(&long, "node-a", &resolved).unwrap_err();
        assert!(
            matches!(&refusal, crate::render::Error::InvalidAwg { node, source: Invalid::InterfaceNameTooLong(name) }
                if node == "node-a" && name == "mesh-node-abcdef"),
            "{refusal:?}"
        );
    }

    fn peer_keepalive(network: &Network, node: &str, peer: &str) -> Option<Range<u16>> {
        let configs = node_configs(network, node, &secrets::read(network)).unwrap();
        let interface = configs
            .awg
            .interfaces
            .into_iter()
            .find(|interface| interface.name == format!("mesh-{peer}"))
            .unwrap();
        interface.peers[0].persistent_keepalive
    }

    #[test]
    fn every_node_of_the_example_gets_its_configs() {
        let network = lowered("{}");
        let resolved = secrets::read(&network);
        for node in &network.nodes {
            node_configs(&network, &node.name, &resolved).unwrap();
        }
    }

    #[test]
    fn a_dialed_peer_gets_the_keepalive_of_its_link_where_that_is_not_the_default() {
        let network = lowered("{}");
        // `node-c` dials `node-a`; nobody dials `node-c`, which has no endpoint.
        assert_eq!(
            peer_keepalive(&network, "node-c", "node-a"),
            Some(range(15, 20))
        );
        assert_eq!(peer_keepalive(&network, "node-a", "node-c"), None);
        // A link that sets none.
        assert_eq!(peer_keepalive(&network, "node-a", "node-b"), None);
    }

    #[test]
    fn a_keepalive_equal_to_the_default_of_its_type_is_left_out() {
        let of_the_defaults = lowered(
            "{mesh: {wireguard: {persistent_keepalive: 25}, amneziawg: {persistent_keepalive: 20-30}, links: {a-c: {amneziawg: {persistent_keepalive: null}}}}}",
        );
        assert_eq!(peer_keepalive(&of_the_defaults, "node-a", "router1"), None);
        assert_eq!(peer_keepalive(&of_the_defaults, "node-a", "node-b"), None);
        let of_their_own = lowered(
            "{mesh: {wireguard: {persistent_keepalive: 10}, amneziawg: {persistent_keepalive: 25}}}",
        );
        assert_eq!(
            peer_keepalive(&of_their_own, "node-a", "router1"),
            Some(Range::from(10))
        );
        assert_eq!(
            peer_keepalive(&of_their_own, "node-a", "node-b"),
            Some(Range::from(25))
        );
    }

    #[test]
    fn the_metrics_ports_reach_the_nodes_of_talos_alone() {
        let mut network = lowered("{}");
        network.metrics = MetricsPorts {
            awg: std::num::NonZeroU16::new(9586),
            router: std::num::NonZeroU16::new(9324),
        };
        let resolved = secrets::read(&network);
        let talos = node_configs(&network, "node-a", &resolved).unwrap();
        assert_eq!(talos.awg.metrics.unwrap().listen, "198.51.100.1:9586");
        assert_eq!(talos.router.metrics.unwrap().listen, "198.51.100.1:9324");
        let routeros = node_configs(&network, "router1", &resolved).unwrap();
        assert!(routeros.awg.metrics.is_none() && routeros.router.metrics.is_none());
    }

    fn refused(network: &Network, node: &str) -> crate::render::Error {
        node_configs(network, node, &secrets::read(network)).unwrap_err()
    }

    #[test]
    fn a_refusal_names_the_keys_as_the_files_write_them() {
        let keyless = lowered("{nodes: {node-b: {wireguard: null}}}");
        let refusal = refused(&keyless, "node-a");
        let missing = refusal.missing_secret().map(ToString::to_string);
        assert!(
            missing.is_some_and(|missing| missing.contains(crate::model::PRIVATE_KEY)),
            "{refusal:?}"
        );
        let above = refused(&lowered("{mesh: {mtu: 9000}}"), "node-a");
        assert!(
            matches!(
                &above,
                crate::render::Error::RenderingAwg {
                    source: crate::render::RenderError::Mtu(crate::mtu::MtuError::AboveLimit {
                        mtu: 9000,
                        ..
                    }),
                    ..
                }
            ),
            "{above:?}"
        );
    }

    #[test]
    fn a_node_on_no_link_is_still_stopped_by_what_stops_every_node() {
        let network = lowered(&format!(
            "{{mesh: {{mtu: 9000}}, nodes: {{node-d: {NODE_D}}}}}"
        ));
        let refusal = refused(&network, "node-d");
        assert!(
            matches!(
                &refusal,
                crate::render::Error::RenderingAwg {
                    source: crate::render::RenderError::Mtu(crate::mtu::MtuError::AboveLimit {
                        mtu: 9000,
                        ..
                    }),
                    ..
                }
            ),
            "{refusal:?}"
        );
    }

    #[test]
    fn every_field_of_a_links_obfuscation_reaches_both_its_ends() {
        let every_field = "{jc: 4, jmin: 40, jmax: 400, s1: 64, s2: 48, s3: 40, s4: 16, h1: 1101, \
            h2: 2202, h3: 3303, h4: 4404-4499, i1: '<b 0xabcd>', i2: '<c>', i3: '<t>', i4: '<r 8>', \
            i5: '<rd 6>', header_protection_key: ccccccccccccccccccccccccccccccccccccccccccc=, \
            content_padding_addition: 0-32, rekey_after_time: 100-140, rekey_timeout: 6, \
            reject_after_time: 170, keepalive_timeout: 12, max_handshake_attempts: 20, \
            random_trailers: true, disable_cookies: true}";
        let written: Obfuscation = sliplib_yaml::from_str(every_field).unwrap();
        let fields = sliplib_yaml::to_value(&written).unwrap();
        assert_eq!(fields.as_mapping().unwrap().len(), 25, "a field is not set");

        let network = lowered(&format!(
            "{{mesh: {{links: {{a-b: {{amneziawg: {{obfuscation: {every_field}}}}}}}}}}}"
        ));
        let resolved = secrets::read(&network);
        for (node, peer) in [("node-a", "node-b"), ("node-b", "node-a")] {
            let configs = node_configs(&network, node, &resolved).unwrap();
            let interface = configs
                .awg
                .interfaces
                .iter()
                .find(|interface| interface.name == format!("mesh-{peer}"))
                .unwrap();
            assert_eq!(interface.obfuscation, written, "{node}");
        }
    }

    #[test]
    fn a_link_that_lacks_a_field_mint_writes_is_refused_when_its_nodes_are_computed() {
        let unminted = lowered("{mesh: {links: {a-b: {amneziawg: {obfuscation: {jc: null}}}}}}");
        let refusal = refused(&unminted, "node-b");
        let missing = refusal.missing_secret().map(ToString::to_string);
        assert!(
            missing.is_some_and(|missing| missing.contains("a-b") && missing.contains("jc")),
            "{refusal:?}"
        );
    }
}
