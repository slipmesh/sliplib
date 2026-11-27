//! What no one key of a network refuses by itself.

use super::network::{Addressing, Class, LinkKind, Mesh, Network, Node, Rule};
use super::{ClassName, Map, Name, Platform};
use crate::Range;
use sliplib_net::Ipv6Net;
use std::net::{Ipv4Addr, Ipv6Addr};

const LINK_LOCAL: Ipv6Net = Ipv6Net::new_assert(Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, 0), 10);

/// The kernel's own.
const KERNEL_PROTOCOLS: std::ops::RangeInclusive<u8> = 0..=4;

/// The kernel's own: unspecified, `default`, `main` and `local`.
const KERNEL_TABLES: [u8; 4] = [0, 253, 254, 255];

/// The routes to pool clients'.
const POOL_PROTOCOL: u8 = 200;

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
    #[error(
        "the node {node:?} has `node_id` {id}, whose bits do not fit the {host_bits} host bits of {key} - give it a `node_id` below 2^{host_bits}, or take a network there that leaves more"
    )]
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
        "the route class {class:?} has `table` {table}, one the kernel keeps for itself (0 unspecified, 253 default, 254 main, 255 local) - give it another from 1 to 252, or leave the key out for main"
    )]
    KernelTable { class: ClassName, table: u8 },
    #[error(
        "the rule {:?} of the route class {:?} and the rule {:?} of the route class {:?} both have priority {priority} - give each rule its own",
        .first.1, .first.0, .second.1, .second.0
    )]
    PriorityTwice {
        priority: u32,
        first: (ClassName, Name),
        second: (ClassName, Name),
    },
    #[error(
        "{key}: the range {range} of `persistent_keepalive` starts at 0, and an interval drawn as 0 sends at once - start it at 1, or write `0` alone to send none"
    )]
    KeepaliveFromZero { key: String, range: Range<u16> },
}

impl Network {
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

/// A `node_id` that fits each network's host part gives distinct addresses, none of host part 0.
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
    let mut priorities: Map<u32, (&ClassName, &Name)> = Map::new();
    for (name, class) in classes {
        if let Some(table) = class.table
            && KERNEL_TABLES.contains(&table)
        {
            return Err(Invalid::KernelTable {
                class: name.clone(),
                table,
            });
        }
        for (rule, Rule { priority, .. }) in &class.rules {
            if let Some((first_class, first)) = priorities.insert(*priority, (name, rule)) {
                return Err(Invalid::PriorityTwice {
                    priority: *priority,
                    first: (first_class.clone(), first.clone()),
                    second: (name.clone(), rule.clone()),
                });
            }
        }
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

/// Only `amneziawg`'s is a range, which can start at 0 without being 0.
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

    fn checked(overlay: &str) -> Result<(), Invalid> {
        check(&with(overlay).unwrap())
    }

    fn name(text: &str) -> Name {
        Name::try_from(text.to_owned()).unwrap()
    }

    fn class(text: &str) -> ClassName {
        ClassName::try_from(text.to_owned()).unwrap()
    }

    /// To be laid into the example under another name.
    const NODE: &str = "{platform: talos, node_id: 0.0.0.9, underlay: {wan: {}}}";

    #[test]
    fn a_network_whose_nodes_break_a_rule_is_refused_naming_it() {
        let underlay = |entries| Invalid::Underlay {
            node: name("node-b"),
            entries,
        };
        for (overlay, refused) in [
            (
                "{nodes: {node-a: null, node-b: null, node-c: null, router1: null}}".to_owned(),
                Invalid::NoNodes,
            ),
            (
                format!("{{nodes: {{all: {NODE}}}}}"),
                Invalid::ReservedName(name("all")),
            ),
            (
                format!("{{nodes: {{none: {NODE}}}}}"),
                Invalid::ReservedName(name("none")),
            ),
            (
                "{nodes: {node-b: {underlay: {lan: {}}}}}".to_owned(),
                underlay(2),
            ),
            (
                "{nodes: {node-b: {underlay: {wan: null}}}}".to_owned(),
                underlay(0),
            ),
            (
                "{nodes: {node-c: {node_id: 0.0.0.1}}}".to_owned(),
                Invalid::NodeIdTwice {
                    id: Ipv4Addr::new(0, 0, 0, 1),
                    first: name("node-a"),
                    second: name("node-c"),
                },
            ),
            (
                "{nodes: {node-b: {node_id: 0.0.0.0}}}".to_owned(),
                Invalid::NodeIdZero(name("node-b")),
            ),
        ] {
            assert_eq!(refusal(&overlay), refused, "{overlay}");
        }
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

    fn too_wide(id: [u8; 4], key: &'static str, host_bits: u8) -> Invalid {
        Invalid::NodeIdTooWide {
            node: name("node-b"),
            id: Ipv4Addr::from(id),
            key,
            host_bits,
        }
    }

    #[test]
    fn a_node_id_is_taken_at_the_limit_of_its_networks_and_refused_one_bit_wider() {
        // The example's loopbacks are a /25 and a /64, its links' IPv4 a /25.
        let cases = [
            (
                "{nodes: {node-b: {node_id: 0.0.0.128}}}",
                Err(too_wide([0, 0, 0, 128], "addressing.loopback.ipv4", 7)),
            ),
            (
                "{addressing: {loopback: {ipv4: 198.18.0.0/15, ipv6: '3fff:0:0:1::/120'}, tunnel: \
                 {ipv4: null}}, nodes: {node-b: {node_id: 0.0.1.0}}}",
                Err(too_wide([0, 0, 1, 0], "addressing.loopback.ipv6", 8)),
            ),
            (
                "{addressing: {tunnel: {ipv4: 198.51.100.192/26}}, nodes: {node-b: {node_id: \
                 0.0.0.64}}}",
                Err(too_wide([0, 0, 0, 64], "addressing.tunnel.ipv4", 6)),
            ),
            (
                "{addressing: {tunnel: {ipv4: null, link_local: 'fe80::/122'}}, nodes: {node-b: \
                 {node_id: 0.0.0.64}}}",
                Err(too_wide([0, 0, 0, 64], "addressing.tunnel.link_local", 6)),
            ),
            (
                "{addressing: {loopback: {ipv4: 198.51.100.0/24, ipv6: '3fff:0:0:1::/120'}, tunnel: \
                 {ipv4: null}}, nodes: {node-b: {node_id: 0.0.0.255}}}",
                Ok(()),
            ),
        ];
        for (overlay, expected) in cases {
            assert_eq!(checked(overlay), expected, "{overlay}");
        }
    }

    #[test]
    fn a_link_is_refused_for_its_ends_its_type_its_pair_or_its_port() {
        for (overlay, expected) in [
            (
                "{mesh: {links: {a-x: {type: wireguard, ends: {node-a: {}, node-x: {}}, wireguard: {port: 1}}}}}",
                Err(Invalid::UnknownEnd {
                    link: name("a-x"),
                    end: name("node-x"),
                }),
            ),
            (
                "{mesh: {links: {b-r1: {type: amneziawg, ends: {node-b: {}, router1: {}}, amneziawg: {port: 1}}}}}",
                Err(Invalid::NotServed {
                    link: name("b-r1"),
                    kind: "amneziawg",
                    node: name("router1"),
                    platform: Platform::Routeros,
                }),
            ),
            // Whatever the order of its ends.
            (
                "{mesh: {links: {b-a: {type: wireguard, ends: {node-b: {}, node-a: {}}, wireguard: {port: 1}}}}}",
                Err(Invalid::SecondLink {
                    first: name("a-b"),
                    second: name("b-a"),
                    a: name("node-b"),
                    b: name("node-a"),
                }),
            ),
            // 51830 is the port of `a-b`; the new link shares `node-b` with it and nothing else.
            (
                "{mesh: {links: {b-c: {type: wireguard, ends: {node-c: {}, node-b: {}}, wireguard: {port: 51830}}}}}",
                Err(Invalid::PortTwice {
                    node: name("node-b"),
                    port: 51830,
                    first: name("a-b"),
                    second: name("b-c"),
                }),
            ),
            // 51832 is the port of `a-r1`, which joins `node-a` and `router1`; this one joins the
            // other two.
            (
                "{mesh: {links: {b-c: {type: wireguard, ends: {node-b: {}, node-c: {}}, wireguard: {port: 51832}}}}}",
                Ok(()),
            ),
        ] {
            assert_eq!(checked(overlay), expected, "{overlay}");
        }
    }

    #[test]
    fn a_class_takes_no_number_of_the_kernel_of_the_pools_routes_or_of_another_class() {
        let of_the_kernel = |protocol| {
            Err(Invalid::KernelProtocol {
                class: class("egress"),
                protocol,
            })
        };
        for (class_name, protocol, expected) in [
            ("egress", 0, of_the_kernel(0)),
            ("egress", 4, of_the_kernel(4)),
            ("egress", 5, Ok(())),
            ("egress", 199, Ok(())),
            (
                "egress",
                200,
                Err(Invalid::PoolProtocol {
                    class: class("egress"),
                }),
            ),
            ("egress", 201, Ok(())),
            (
                "transit",
                152,
                Err(Invalid::ProtocolTwice {
                    protocol: 152,
                    first: class("egress"),
                    second: class("transit"),
                }),
            ),
        ] {
            let overlay =
                format!("{{routing: {{classes: {{{class_name}: {{protocol: {protocol}}}}}}}}}");
            assert_eq!(checked(&overlay), expected, "{overlay}");
        }
    }

    #[test]
    fn a_class_takes_no_table_the_kernel_keeps() {
        for table in [0, 253, 254, 255] {
            assert_eq!(
                refusal(&format!(
                    "{{routing: {{classes: {{egress: {{table: {table}}}}}}}}}"
                )),
                Invalid::KernelTable {
                    class: class("egress"),
                    table
                }
            );
        }
        for table in [1, 252] {
            let network = with(&format!(
                "{{routing: {{classes: {{egress: {{table: {table}}}}}}}}}"
            ))
            .unwrap();
            assert_eq!(check(&network), Ok(()));
        }
    }

    #[test]
    fn two_rules_of_one_priority_are_refused_naming_both_in_any_classes() {
        assert_eq!(
            refusal("{routing: {classes: {egress: {rules: {again: {priority: 1000}}}}}}"),
            Invalid::PriorityTwice {
                priority: 1000,
                first: (class("egress"), name("uplink-replies")),
                second: (class("egress"), name("again")),
            }
        );
        assert_eq!(
            refusal(
                "{routing: {classes: {transit: {protocol: 153, rules: {back: {priority: 1001}}}}}}"
            ),
            Invalid::PriorityTwice {
                priority: 1001,
                first: (class("egress"), name("egress")),
                second: (class("transit"), name("back")),
            }
        );
    }

    #[test]
    fn a_keepalive_range_from_zero_is_refused_where_it_is_written_and_no_other() {
        let from_zero = |key: &str| {
            Err(Invalid::KeepaliveFromZero {
                key: key.into(),
                range: Range::new(0, 30).unwrap(),
            })
        };
        for (overlay, expected) in [
            (
                "{mesh: {amneziawg: {persistent_keepalive: 0-30}}}",
                from_zero("mesh.amneziawg"),
            ),
            (
                "{mesh: {links: {a-b: {amneziawg: {persistent_keepalive: 0-30}}}}}",
                from_zero("mesh.links.a-b.amneziawg"),
            ),
            ("{mesh: {amneziawg: {persistent_keepalive: 0}}}", Ok(())),
            ("{mesh: {amneziawg: {persistent_keepalive: 1-30}}}", Ok(())),
        ] {
            assert_eq!(checked(overlay), expected, "{overlay}");
        }
    }
}
