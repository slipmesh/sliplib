//! What is wrong in a network across its tree: what no one key can refuse by itself.

use super::network::{Addressing, Class, LinkKind, Mesh, Network, Node};
use super::{ClassName, Map, Name, Platform};
use crate::Range;
use sliplib_net::Ipv6Net;
use std::net::{Ipv4Addr, Ipv6Addr};

/// The link-local unicast range of IPv6.
const LINK_LOCAL: Ipv6Net = Ipv6Net::new_assert(Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, 0), 10);

/// The protocol numbers the kernel keeps for its own routes.
const KERNEL_PROTOCOLS: std::ops::RangeInclusive<u8> = 0..=4;

/// The protocol number of the routes to the clients of roadwarriors pools.
const POOL_PROTOCOL: u8 = 200;

/// Why a network that reads key by key is refused.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Invalid {
    #[error(
        "addressing.tunnel.link_local is {0}, outside fe80::/10, the range of link-local addresses - take a network inside it, or leave the key out for each node's link-local to be fe80:: and the low 32 bits of its IPv6 loopback"
    )]
    LinkLocalOutside(Ipv6Net),
    #[error("`nodes` is empty, and a network has at least one node")]
    NoNodes,
    #[error("no node is named `{0}`: the word is reserved - give the node another name")]
    ReservedName(Name),
    #[error(
        "the node {node:?} has {entries} `underlay` entries, and a node has exactly one: what it reaches the world through"
    )]
    Underlay { node: Name, entries: usize },
    #[error(
        "the nodes {first:?} and {second:?} both have `node_id` {id}, which is a node's identity - give each its own"
    )]
    NodeIdTwice {
        id: Ipv4Addr,
        first: Name,
        second: Name,
    },
    #[error(
        "the node {0:?} has `node_id` 0.0.0.0, which gives its loopbacks a host part of zeros - give it another"
    )]
    NodeIdZero(Name),
    #[error("the node {node:?} has `node_id` {id}, {}", too_wide(key, *host_bits))]
    NodeIdTooWide {
        node: Name,
        id: Ipv4Addr,
        key: &'static str,
        host_bits: u8,
    },
    #[error(
        "the link {link:?} names {end:?} in its `ends`, which is not a node; a link joins nodes, never a group"
    )]
    UnknownEnd { link: Name, end: Name },
    #[error(
        "the link {link:?} is of type `{kind}`, which `{platform}` does not serve, and its end {node:?} is a `{platform}` node - give the link a type both its ends serve"
    )]
    NotServed {
        link: Name,
        kind: &'static str,
        node: Name,
        platform: Platform,
    },
    #[error(
        "the links {first:?} and {second:?} both join {a:?} and {b:?}, and two nodes have one link between them - take one out"
    )]
    SecondLink {
        first: Name,
        second: Name,
        a: Name,
        b: Name,
    },
    #[error(
        "the links {first:?} and {second:?} both take port {port} on the node {node:?} - give each link of a node its own port"
    )]
    PortTwice {
        node: Name,
        port: u16,
        first: Name,
        second: Name,
    },
    #[error(
        "the route class {class:?} has `protocol` {protocol}, one of 0 to 4, which are the kernel's own - give it another number"
    )]
    KernelProtocol { class: ClassName, protocol: u8 },
    #[error(
        "the route class {class:?} has `protocol` {POOL_PROTOCOL}, which the routes to the clients of roadwarriors pools carry - give it another number"
    )]
    PoolProtocol { class: ClassName },
    #[error(
        "the route classes {first:?} and {second:?} both have `protocol` {protocol} - give each class its own number"
    )]
    ProtocolTwice {
        protocol: u8,
        first: ClassName,
        second: ClassName,
    },
    #[error(
        "{key}: the range {range} of `persistent_keepalive` starts at 0, and an interval drawn as 0 sends at once - start it at 1, or write `0` alone to send none"
    )]
    KeepaliveFromZero { key: String, range: Range<u16> },
}

/// What is wrong with a `node_id` too wide for `key`, a network of `host_bits` host bits, and what
/// to do about it.
fn too_wide(key: &str, host_bits: u8) -> String {
    match host_bits {
        0 => {
            format!("and {key} leaves no host bits for a `node_id` - take a shorter network there")
        }
        bits => format!(
            "whose bits do not fit the {bits} host bits of {key} - give it a `node_id` below \
             2^{bits}, or take a network there that leaves more"
        ),
    }
}

impl Network {
    /// Refuses what is wrong across the tree, which no one key refuses by itself: names, the
    /// ends and the ports of the links, the platforms that serve them, the route classes, the
    /// networks no address can come from, a `node_id` of `0.0.0.0` or one wider than a network it
    /// is added to, a keepalive that starts at zero.
    pub fn check(&self) -> Result<(), Invalid> {
        check(self)
    }
}

fn check(network: &Network) -> Result<(), Invalid> {
    addressing(&network.addressing)?;
    nodes(&network.nodes)?;
    node_ids_fit(network)?;
    links(network)?;
    classes(&network.routing.classes)?;
    keepalive(&network.mesh)
}

fn addressing(addressing: &Addressing) -> Result<(), Invalid> {
    let link_local = addressing
        .tunnel
        .as_ref()
        .and_then(|tunnel| tunnel.link_local)
        .map(|prefix| prefix.0);
    match link_local {
        Some(network) if !LINK_LOCAL.contains(&network) => Err(Invalid::LinkLocalOutside(network)),
        _ => Ok(()),
    }
}

/// Refuses a `node_id` with more significant bits than the host part of a network it is added
/// to holds: then two `node_id`s never give one address, and none gives a host part of zeros.
fn node_ids_fit(network: &Network) -> Result<(), Invalid> {
    let addressing = &network.addressing;
    let tunnel = addressing.tunnel.as_ref();
    let host_bits = [
        (
            "addressing.loopback.ipv4",
            Some(32 - addressing.loopback.ipv4.0.prefix_len()),
        ),
        (
            "addressing.loopback.ipv6",
            Some(128 - addressing.loopback.ipv6.0.prefix_len()),
        ),
        (
            "addressing.tunnel.ipv4",
            tunnel
                .and_then(|tunnel| tunnel.ipv4)
                .map(|network| 32 - network.0.prefix_len()),
        ),
        (
            "addressing.tunnel.link_local",
            tunnel
                .and_then(|tunnel| tunnel.link_local)
                .map(|network| 128 - network.0.prefix_len()),
        ),
    ];
    for (name, node) in &network.nodes {
        let significant = 32 - u32::from(node.node_id).leading_zeros();
        let narrower = host_bits
            .iter()
            .filter_map(|(key, bits)| Some((*key, (*bits)?)))
            .find(|(_, bits)| significant > u32::from(*bits));
        if let Some((key, host_bits)) = narrower {
            return Err(Invalid::NodeIdTooWide {
                node: name.clone(),
                id: node.node_id,
                key,
                host_bits,
            });
        }
    }
    Ok(())
}

fn nodes(nodes: &Map<Name, Node>) -> Result<(), Invalid> {
    if nodes.is_empty() {
        return Err(Invalid::NoNodes);
    }
    let mut ids: Map<Ipv4Addr, &Name> = Map::new();
    for (name, node) in nodes {
        if name.is_reserved() {
            return Err(Invalid::ReservedName(name.clone()));
        }
        if node.underlay.len() != 1 {
            return Err(Invalid::Underlay {
                node: name.clone(),
                entries: node.underlay.len(),
            });
        }
        if node.node_id == Ipv4Addr::UNSPECIFIED {
            return Err(Invalid::NodeIdZero(name.clone()));
        }
        if let Some(first) = ids.insert(node.node_id, name) {
            return Err(Invalid::NodeIdTwice {
                id: node.node_id,
                first: first.clone(),
                second: name.clone(),
            });
        }
    }
    Ok(())
}

/// Whether the tool of `platform` serves a link of this type.
fn serves(platform: Platform, kind: &LinkKind) -> bool {
    match (platform, kind) {
        (Platform::Talos, LinkKind::Wireguard(_) | LinkKind::Amneziawg(_))
        | (Platform::Routeros, LinkKind::Wireguard(_)) => true,
        (Platform::Routeros, LinkKind::Amneziawg(_)) => false,
    }
}

fn links(network: &Network) -> Result<(), Invalid> {
    let mut joined: Map<[&Name; 2], &Name> = Map::new();
    let mut ports: Map<(&Name, u16), &Name> = Map::new();
    for (name, link) in &network.mesh.links {
        for end in &link.ends {
            let Some(node) = network.nodes.get(end) else {
                return Err(Invalid::UnknownEnd {
                    link: name.clone(),
                    end: end.clone(),
                });
            };
            if !serves(node.platform, &link.kind) {
                return Err(Invalid::NotServed {
                    link: name.clone(),
                    kind: link.kind.name(),
                    node: end.clone(),
                    platform: node.platform,
                });
            }
        }
        let mut pair = [&link.ends[0], &link.ends[1]];
        pair.sort();
        if let Some(first) = joined.insert(pair, name) {
            return Err(Invalid::SecondLink {
                first: first.clone(),
                second: name.clone(),
                a: link.ends[0].clone(),
                b: link.ends[1].clone(),
            });
        }
        let port = link.kind.port().get();
        for end in &link.ends {
            if let Some(first) = ports.insert((end, port), name) {
                return Err(Invalid::PortTwice {
                    node: end.clone(),
                    port,
                    first: first.clone(),
                    second: name.clone(),
                });
            }
        }
    }
    Ok(())
}

fn classes(classes: &Map<ClassName, Class>) -> Result<(), Invalid> {
    let mut numbers: Map<u8, &ClassName> = Map::new();
    for (name, class) in classes {
        let protocol = class.protocol;
        if KERNEL_PROTOCOLS.contains(&protocol) {
            return Err(Invalid::KernelProtocol {
                class: name.clone(),
                protocol,
            });
        }
        if protocol == POOL_PROTOCOL {
            return Err(Invalid::PoolProtocol {
                class: name.clone(),
            });
        }
        if let Some(first) = numbers.insert(protocol, name) {
            return Err(Invalid::ProtocolTwice {
                protocol,
                first: first.clone(),
                second: name.clone(),
            });
        }
    }
    Ok(())
}

/// Only `amneziawg` takes a range, so only its keepalive can start at 0 without being 0.
fn keepalive(mesh: &Mesh) -> Result<(), Invalid> {
    let network = mesh
        .amneziawg
        .as_ref()
        .and_then(|defaults| defaults.persistent_keepalive)
        .map(|range| ("mesh.amneziawg".to_owned(), range));
    let of_links = mesh
        .links
        .iter()
        .filter_map(|(name, link)| match &link.kind {
            LinkKind::Amneziawg(block) => block
                .persistent_keepalive
                .map(|range| (format!("mesh.links.{name}.amneziawg"), range)),
            LinkKind::Wireguard(_) => None,
        });
    match network
        .into_iter()
        .chain(of_links)
        .find(|(_, range)| range.starts_at_zero())
    {
        Some((key, range)) => Err(Invalid::KeepaliveFromZero { key, range }),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::with;
    use super::*;

    fn refusal(overlay: &str) -> Invalid {
        check(&with(overlay).unwrap()).unwrap_err()
    }

    fn name(text: &str) -> Name {
        Name::try_from(text.to_owned()).unwrap()
    }

    fn class(text: &str) -> ClassName {
        ClassName::try_from(text.to_owned()).unwrap()
    }

    /// A node that reads, to be laid into the example under another name.
    const NODE: &str = "{platform: talos, node_id: 0.0.0.9, underlay: {wan: {}}}";

    #[test]
    fn the_example_of_the_specification_is_valid() {
        assert_eq!(with("{}").unwrap().check(), Ok(()));
    }

    #[test]
    fn a_network_refuses_through_its_check_what_the_checks_refuse() {
        let refused = with("{nodes: {node-c: {node_id: 0.0.0.1}}}").unwrap();
        assert!(refused.check().is_err());
    }

    #[test]
    fn a_network_without_nodes_is_refused() {
        let refusal = refusal("{nodes: {node-a: null, node-b: null, node-c: null, router1: null}}");
        assert_eq!(refusal, Invalid::NoNodes);
    }

    #[test]
    fn a_link_local_network_outside_the_link_local_range_is_refused() {
        for outside in ["3fff:0:0:2::/64", "fe00::/9"] {
            assert_eq!(
                refusal(&format!(
                    "{{addressing: {{tunnel: {{link_local: '{outside}'}}}}}}"
                )),
                Invalid::LinkLocalOutside(outside.parse().unwrap()),
            );
        }
        for inside in ["fe80::/10", "fe80:0:0:1::/64", "febf::/120"] {
            let network = with(&format!(
                "{{addressing: {{tunnel: {{link_local: '{inside}'}}}}}}"
            ))
            .unwrap();
            assert_eq!(check(&network), Ok(()), "{inside}");
        }
    }

    #[test]
    fn a_node_named_by_a_reserved_word_is_refused() {
        for word in ["all", "none"] {
            assert_eq!(
                refusal(&format!("{{nodes: {{{word}: {NODE}}}}}")),
                Invalid::ReservedName(name(word))
            );
        }
    }

    #[test]
    fn a_node_has_exactly_one_underlay_entry() {
        assert_eq!(
            refusal("{nodes: {node-b: {underlay: {lan: {}}}}}"),
            Invalid::Underlay {
                node: name("node-b"),
                entries: 2
            }
        );
        assert_eq!(
            refusal("{nodes: {node-b: {underlay: {wan: null}}}}"),
            Invalid::Underlay {
                node: name("node-b"),
                entries: 0
            }
        );
    }

    #[test]
    fn two_nodes_of_one_node_id_are_refused_naming_both() {
        assert_eq!(
            refusal("{nodes: {node-c: {node_id: 0.0.0.1}}}"),
            Invalid::NodeIdTwice {
                id: Ipv4Addr::new(0, 0, 0, 1),
                first: name("node-a"),
                second: name("node-c"),
            }
        );
    }

    #[test]
    fn a_node_id_of_zeros_is_refused_naming_the_node() {
        assert_eq!(
            refusal("{nodes: {node-b: {node_id: 0.0.0.0}}}"),
            Invalid::NodeIdZero(name("node-b"))
        );
    }

    fn too_wide(id: [u8; 4], key: &'static str, host_bits: u8) -> Invalid {
        Invalid::NodeIdTooWide {
            node: name("node-b"),
            id: Ipv4Addr::from(id),
            key,
            host_bits,
        }
    }

    #[test]
    fn a_node_id_one_bit_too_wide_for_a_network_it_is_added_to_is_refused_naming_it() {
        // The example's loopbacks are a /25 and a /64, its links' IPv4 a /25.
        let cases = [
            (
                "{nodes: {node-b: {node_id: 0.0.0.128}}}",
                too_wide([0, 0, 0, 128], "addressing.loopback.ipv4", 7),
            ),
            (
                "{addressing: {loopback: {ipv4: 198.18.0.0/15, ipv6: '3fff:0:0:1::/120'}, tunnel: \
                 {ipv4: null}}, nodes: {node-b: {node_id: 0.0.1.0}}}",
                too_wide([0, 0, 1, 0], "addressing.loopback.ipv6", 8),
            ),
            (
                "{addressing: {tunnel: {ipv4: 198.51.100.192/26}}, nodes: {node-b: {node_id: \
                 0.0.0.64}}}",
                too_wide([0, 0, 0, 64], "addressing.tunnel.ipv4", 6),
            ),
            (
                "{addressing: {tunnel: {ipv4: null, link_local: 'fe80::/122'}}, nodes: {node-b: \
                 {node_id: 0.0.0.64}}}",
                too_wide([0, 0, 0, 64], "addressing.tunnel.link_local", 6),
            ),
        ];
        for (overlay, expected) in cases {
            assert_eq!(refusal(overlay), expected, "{overlay}");
        }
    }

    #[test]
    fn a_node_id_at_the_limit_of_its_networks_is_taken() {
        let network = with(
            "{addressing: {loopback: {ipv4: 198.51.100.0/24, ipv6: '3fff:0:0:1::/120'}, tunnel: \
             {ipv4: null}}, nodes: {node-b: {node_id: 0.0.0.255}}}",
        )
        .unwrap();
        assert_eq!(check(&network), Ok(()));
    }

    #[test]
    fn a_network_that_leaves_no_host_bits_takes_no_node_id_and_says_so() {
        let cases = [
            (
                "{addressing: {loopback: {ipv4: 198.51.100.1/32}}}",
                "addressing.loopback.ipv4",
            ),
            (
                "{addressing: {loopback: {ipv6: '3fff::1/128'}}}",
                "addressing.loopback.ipv6",
            ),
        ];
        for (overlay, key) in cases {
            let refusal = refusal(overlay);
            assert_eq!(
                refusal,
                Invalid::NodeIdTooWide {
                    node: name("node-a"),
                    id: Ipv4Addr::new(0, 0, 0, 1),
                    key,
                    host_bits: 0,
                },
                "{overlay}"
            );
            assert_eq!(
                refusal.to_string(),
                format!(
                    "the node \"node-a\" has `node_id` 0.0.0.1, and {key} leaves no host bits for \
                     a `node_id` - take a shorter network there"
                )
            );
        }
    }

    #[test]
    fn a_network_of_32_host_bits_or_more_takes_any_node_id() {
        for ipv6 in ["3fff:0:0:1::/96", "3fff:0:0:1::/64"] {
            let network = with(&format!(
                "{{addressing: {{loopback: {{ipv4: 0.0.0.0/0, ipv6: '{ipv6}'}}, tunnel: {{ipv4: \
                 null}}}}, nodes: {{node-b: {{node_id: 255.255.255.255}}}}}}"
            ))
            .unwrap();
            assert_eq!(check(&network), Ok(()), "{ipv6}");
        }
    }

    #[test]
    fn a_node_id_too_wide_says_how_far_it_may_go() {
        assert_eq!(
            refusal("{nodes: {node-b: {node_id: 0.0.0.128}}}").to_string(),
            "the node \"node-b\" has `node_id` 0.0.0.128, whose bits do not fit the 7 host bits of \
             addressing.loopback.ipv4 - give it a `node_id` below 2^7, or take a network there that \
             leaves more"
        );
    }

    #[test]
    fn a_node_id_of_zeros_says_what_it_gives() {
        assert_eq!(
            refusal("{nodes: {node-b: {node_id: 0.0.0.0}}}").to_string(),
            "the node \"node-b\" has `node_id` 0.0.0.0, which gives its loopbacks a host part of \
             zeros - give it another"
        );
    }

    #[test]
    fn links_without_an_ipv4_network_take_any_node_id_there() {
        let overlay = |ipv4: &str| {
            format!(
                "{{addressing: {{tunnel: {{ipv4: {ipv4}}}}}, nodes: {{node-b: {{node_id: 0.0.0.100}}}}}}"
            )
        };
        assert_eq!(
            refusal(&overlay("198.51.100.192/26")),
            too_wide([0, 0, 0, 100], "addressing.tunnel.ipv4", 6)
        );
        assert_eq!(check(&with(&overlay("null")).unwrap()), Ok(()));
    }

    #[test]
    fn an_end_that_is_not_a_node_is_refused() {
        let refusal = refusal(
            "{mesh: {links: {a-x: {type: wireguard, ends: {node-a: {}, node-x: {}}, wireguard: {port: 1}}}}}",
        );
        assert_eq!(
            refusal,
            Invalid::UnknownEnd {
                link: name("a-x"),
                end: name("node-x")
            }
        );
    }

    #[test]
    fn a_link_of_a_type_one_of_its_ends_does_not_serve_is_refused() {
        let refusal = refusal(
            "{mesh: {links: {b-r1: {type: amneziawg, ends: {node-b: {}, router1: {}}, amneziawg: {port: 1}}}}}",
        );
        assert_eq!(
            refusal,
            Invalid::NotServed {
                link: name("b-r1"),
                kind: "amneziawg",
                node: name("router1"),
                platform: Platform::Routeros,
            }
        );
    }

    #[test]
    fn a_second_link_between_two_nodes_is_refused_whatever_the_order_of_its_ends() {
        let refusal = refusal(
            "{mesh: {links: {b-a: {type: wireguard, ends: {node-b: {}, node-a: {}}, wireguard: {port: 1}}}}}",
        );
        assert_eq!(
            refusal,
            Invalid::SecondLink {
                first: name("a-b"),
                second: name("b-a"),
                a: name("node-b"),
                b: name("node-a"),
            }
        );
    }

    #[test]
    fn a_port_two_links_of_one_node_take_is_refused() {
        // 51830 is the port of `a-b`; the new link shares `node-b` with it and nothing else.
        let refusal = refusal(
            "{mesh: {links: {b-c: {type: wireguard, ends: {node-c: {}, node-b: {}}, wireguard: {port: 51830}}}}}",
        );
        assert_eq!(
            refusal,
            Invalid::PortTwice {
                node: name("node-b"),
                port: 51830,
                first: name("a-b"),
                second: name("b-c"),
            }
        );
    }

    #[test]
    fn two_links_that_share_no_node_may_share_a_port() {
        // `a-r1` joins `node-a` and `router1`; this one joins the other two.
        let shared = with(
            "{mesh: {links: {b-c: {type: wireguard, ends: {node-b: {}, node-c: {}}, wireguard: {port: 51832}}}}}",
        )
        .unwrap();
        assert_eq!(check(&shared), Ok(()));
    }

    #[test]
    fn a_class_takes_no_number_of_the_kernel() {
        for protocol in [0, 4] {
            assert_eq!(
                refusal(&format!(
                    "{{routing: {{classes: {{egress: {{protocol: {protocol}}}}}}}}}"
                )),
                Invalid::KernelProtocol {
                    class: class("egress"),
                    protocol
                }
            );
        }
        let above = with("{routing: {classes: {egress: {protocol: 5}}}}").unwrap();
        assert_eq!(check(&above), Ok(()));
    }

    #[test]
    fn a_class_takes_not_the_number_of_the_pools_routes() {
        assert_eq!(
            refusal("{routing: {classes: {egress: {protocol: 200}}}}"),
            Invalid::PoolProtocol {
                class: class("egress")
            }
        );
        for beside in [199, 201] {
            let network = with(&format!(
                "{{routing: {{classes: {{egress: {{protocol: {beside}}}}}}}}}"
            ))
            .unwrap();
            assert_eq!(check(&network), Ok(()));
        }
    }

    #[test]
    fn two_classes_of_one_number_are_refused_naming_both() {
        assert_eq!(
            refusal("{routing: {classes: {transit: {protocol: 152}}}}"),
            Invalid::ProtocolTwice {
                protocol: 152,
                first: class("egress"),
                second: class("transit"),
            }
        );
    }

    #[test]
    fn a_keepalive_range_from_zero_is_refused_where_it_is_written() {
        let of_the_network = refusal("{mesh: {amneziawg: {persistent_keepalive: 0-30}}}");
        assert_eq!(
            of_the_network,
            Invalid::KeepaliveFromZero {
                key: "mesh.amneziawg".into(),
                range: Range::new(0, 30).unwrap(),
            }
        );
        let of_a_link =
            refusal("{mesh: {links: {a-b: {amneziawg: {persistent_keepalive: 0-30}}}}}");
        assert_eq!(
            of_a_link,
            Invalid::KeepaliveFromZero {
                key: "mesh.links.a-b.amneziawg".into(),
                range: Range::new(0, 30).unwrap(),
            }
        );
    }

    #[test]
    fn a_keepalive_of_zero_alone_and_a_range_from_one_are_valid() {
        for value in ["0", "1-30"] {
            let network = with(&format!(
                "{{mesh: {{amneziawg: {{persistent_keepalive: {value}}}}}}}"
            ))
            .unwrap();
            assert_eq!(check(&network), Ok(()));
        }
    }
}
