//! The files of format version 2 as the network the configs are computed from: every node with
//! the addresses its `node_id` gives it, every link with what the loader found in force on it.

use crate::addressing;
use crate::model::{Endpoint, Format, Link, MetricsPorts, Network, Node};
use crate::mtu::DEFAULT_UPLINK_MTU;
use sliplib_config::router::{AnnounceEntry, BfdSettings};
use sliplib_config::v2::File;
use sliplib_config::v2::network::{self, LinkKind};
use sliplib_config::{Obfuscation, Range};
use sliplib_net::{IpNet, Ipv6Net};
use std::net::{IpAddr, Ipv6Addr};

/// The network of the link-local addresses when `addressing.tunnel.link_local` is left out.
const DEFAULT_LINK_LOCAL: Ipv6Net =
    Ipv6Net::new_assert(Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, 0), 64);

/// The prefix length a link's interface carries its link-local with.
const LINK_LOCAL_PREFIX_LEN: u8 = 64;

/// Why the files, which read, give no network to compute from.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("mesh.interface_prefix is not set - `slipmesh mint` writes it")]
    NoInterfacePrefix,
    #[error(
        "the nodes {first:?} and {second:?} both get the {what} {address}: their `node_id`s give the same bits in the host part of {key} - change one `node_id`, or take a network that leaves room for both"
    )]
    SameAddress {
        what: &'static str,
        key: &'static str,
        address: IpAddr,
        first: String,
        second: String,
    },
    #[error(
        "the node {node:?} gets the link-local address {address}/{LINK_LOCAL_PREFIX_LEN}, whose host part is all zeros: that is the Subnet-Router anycast address, which no interface takes - give the node a `node_id` that leaves a bit set there, or take an `addressing.tunnel.link_local` where it does"
    )]
    AnycastLinkLocal { node: String, address: Ipv6Addr },
}

/// What a derived address is, for a refusal: its name and the key of the network it is from.
struct Derived {
    what: &'static str,
    key: &'static str,
}

const IPV4_LOOPBACK: Derived = Derived {
    what: "IPv4 loopback",
    key: "addressing.loopback.ipv4",
};
const LINK_LOCAL: Derived = Derived {
    what: "link-local address",
    key: "addressing.tunnel.link_local",
};
const TUNNEL_IPV4: Derived = Derived {
    what: "IPv4 address on its links",
    key: "addressing.tunnel.ipv4",
};

/// Refuses two nodes that got one address: `of` gives a node's, where it has one. The IPv6
/// loopbacks need no such look: the loader refuses a loopback network longer than /96 and two
/// nodes of one `node_id`, so each loopback holds a whole `node_id` of its own.
fn distinct(
    nodes: &[Node],
    derived: &Derived,
    of: impl Fn(&Node) -> Option<IpAddr>,
) -> Result<(), Error> {
    for (index, node) in nodes.iter().enumerate() {
        let Some(address) = of(node) else { continue };
        if let Some(first) = nodes[..index]
            .iter()
            .find(|other| of(other) == Some(address))
        {
            return Err(Error::SameAddress {
                what: derived.what,
                key: derived.key,
                address,
                first: first.name.clone(),
                second: node.name.clone(),
            });
        }
    }
    Ok(())
}

/// `files` as a network. Refuses what only the derived addresses show: two nodes with one
/// address, and a link-local no interface takes.
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
        .map_or(DEFAULT_LINK_LOCAL, |prefix| prefix.0);
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
            let (entry, underlay) = node
                .underlay
                .first()
                .map_or(("", None), |(entry, underlay)| {
                    (entry.as_str(), Some(underlay))
                });
            let mut direct_interfaces = node.routing.direct_interfaces.clone();
            if let Some(pattern) = &pattern
                && !direct_interfaces.contains(pattern)
            {
                direct_interfaces.push(pattern.clone());
            }
            Node {
                name: name.to_string(),
                platform: Some(node.platform),
                loopback: (
                    addressing::ipv4_in(ipv4.addr(), ipv4.prefix_len(), id),
                    addressing::ipv6_in(ipv6.addr(), ipv6.prefix_len(), id),
                ),
                link_local: addressing::ipv6_in(link_local.addr(), link_local.prefix_len(), id),
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
                underlay_mtu_key: format!("underlay.{entry}.mtu"),
                direct_interfaces,
                learn: prefixes(&node.routing.learn),
                announce: prefixes(&node.routing.announce)
                    .into_iter()
                    .map(|net| AnnounceEntry { net, label: None })
                    .collect(),
            }
        })
        .collect();

    distinct(&nodes, &IPV4_LOOPBACK, |node| Some(node.loopback.0.into()))?;
    distinct(&nodes, &LINK_LOCAL, |node| Some(node.link_local.into()))?;
    distinct(&nodes, &TUNNEL_IPV4, |node| {
        node.tunnel_ipv4.map(IpAddr::from)
    })?;
    for node in &nodes {
        let on_the_link = Ipv6Net::new_assert(node.link_local, LINK_LOCAL_PREFIX_LEN);
        if sliplib_net::reserved(IpNet::V6(on_the_link)).is_some() {
            return Err(Error::AnycastLinkLocal {
                node: node.name.clone(),
                address: node.link_local,
            });
        }
    }

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
        format: Format::V2,
    })
}

fn prefixes(list: &[network::Prefix4]) -> Vec<String> {
    list.iter().map(|prefix| prefix.0.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Family, Format};
    use crate::render::node_configs;
    use crate::secrets;
    use sliplib_config::v2::Platform;
    use sliplib_yaml::{Value, merge};

    /// The example of the specification, its secrets laid over its mesh as the two files are
    /// read, and `overlay` over both.
    fn files(overlay: &str) -> File {
        let layers = [
            include_str!("../../docs/example/mesh.yaml"),
            include_str!("../../docs/example/secrets.yaml"),
            overlay,
        ];
        let tree = layers.iter().fold(Value::Null, |tree, layer| {
            merge(tree, &sliplib_yaml::from_str::<Value>(layer).unwrap())
        });
        File::parse(&sliplib_yaml::to_string(&tree).unwrap()).unwrap()
    }

    fn lowered(overlay: &str) -> Network {
        lower(&files(overlay)).unwrap()
    }

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
        // A network that leaves fewer than 32 bits free takes only the low bits of `node_id`.
        let narrow = lowered(
            "{addressing: {tunnel: {link_local: 'fe80::1:0/120'}}, nodes: {node-b: {node_id: 0.0.1.2}}}",
        );
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
        assert_eq!(
            network.node("node-a").unwrap().platform,
            Some(Platform::Talos)
        );
        assert_eq!(
            network.node("router1").unwrap().platform,
            Some(Platform::Routeros)
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
    fn two_nodes_with_one_ipv4_loopback_are_refused_naming_both() {
        // In a /25 the node_id gives its low 7 bits: 129 and 1 are the same there.
        let refusal = refusal("{nodes: {node-b: {node_id: 0.0.0.129}}}");
        assert_eq!(
            refusal,
            Error::SameAddress {
                what: "IPv4 loopback",
                key: "addressing.loopback.ipv4",
                address: "198.51.100.1".parse().unwrap(),
                first: "node-a".into(),
                second: "node-b".into(),
            }
        );
    }

    #[test]
    fn two_nodes_with_one_link_local_are_refused() {
        // 65 and 1 differ in the 7 bits of the loopback and agree in the 6 of this network.
        let refusal = refusal(
            "{addressing: {tunnel: {ipv4: null, link_local: 'fe80::/122'}}, nodes: {node-b: {node_id: 0.0.0.65}}}",
        );
        assert_eq!(
            refusal,
            Error::SameAddress {
                what: "link-local address",
                key: "addressing.tunnel.link_local",
                address: "fe80::1".parse().unwrap(),
                first: "node-a".into(),
                second: "node-b".into(),
            }
        );
    }

    #[test]
    fn two_nodes_with_one_ipv4_address_on_their_links_are_refused() {
        let refusal = refusal(
            "{addressing: {tunnel: {ipv4: 198.51.100.192/26}}, nodes: {node-b: {node_id: 0.0.0.65}}}",
        );
        assert_eq!(
            refusal,
            Error::SameAddress {
                what: "IPv4 address on its links",
                key: "addressing.tunnel.ipv4",
                address: "198.51.100.193".parse().unwrap(),
                first: "node-a".into(),
                second: "node-b".into(),
            }
        );
    }

    #[test]
    fn a_link_local_with_a_host_part_of_zeros_is_refused() {
        let narrow = "addressing: {tunnel: {link_local: 'fe80::/120'}}";
        let refusal = refusal(&format!(
            "{{{narrow}, nodes: {{node-b: {{node_id: 0.0.1.0}}}}}}"
        ));
        assert_eq!(
            refusal,
            Error::AnycastLinkLocal {
                node: "node-b".into(),
                address: "fe80::".parse().unwrap(),
            }
        );
        // The same node_id in the default network leaves a bit set.
        let wide = lowered("{nodes: {node-b: {node_id: 0.0.1.0}}}");
        assert_eq!(
            wide.node("node-b").unwrap().link_local.to_string(),
            "fe80::100"
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
    fn an_mtu_of_zero_is_refused_by_its_key() {
        use crate::mtu::{LinkError, MtuError};
        let underlay = mtu_refusal("{nodes: {node-b: {underlay: {wan: {mtu: 0}}}}}");
        assert!(
            matches!(&underlay, MtuError::Link { link, source: LinkError::UnderlayBelowMinimum { node, key, mtu: 0, .. } }
                if link == "a-b" && node == "node-b" && key == "underlay.wan.mtu"),
            "{underlay:?}"
        );
        let mesh = mtu_refusal("{mesh: {mtu: 0}}");
        assert!(
            matches!(mesh, MtuError::BelowMinimum { key, mtu: 0 } if key == Format::V2.mtu()),
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
    fn a_refusal_names_the_keys_as_version_2_writes_them() {
        let keyless = lowered("{nodes: {node-b: {wireguard: null}}}");
        let refusal = refused(&keyless, "node-a");
        let missing = refusal.missing_secret().map(ToString::to_string);
        assert!(
            missing.is_some_and(|missing| missing.contains(Format::V2.private_key())),
            "{refusal:?}"
        );
        let above = refused(&lowered("{mesh: {mtu: 9000}}"), "node-a");
        assert!(
            matches!(
                &above,
                crate::render::Error::RenderingAwg {
                    source: crate::render::RenderError::Mtu(crate::mtu::MtuError::AboveLimit { key, mtu: 9000, .. }),
                    ..
                } if *key == Format::V2.mtu()
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
