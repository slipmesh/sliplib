//! What is wrong in a network across its tree: what no one key can refuse by itself.

use super::network::{Class, LinkKind, Map, Mesh, Network, Node};
use super::{ALL, ClassName, NONE, Name, Platform};
use crate::Range;
use std::net::Ipv4Addr;

/// The protocol numbers the kernel keeps for its own routes.
const KERNEL_PROTOCOLS: std::ops::RangeInclusive<u8> = 0..=4;

/// The protocol number of the routes to the clients of roadwarriors pools.
const POOL_PROTOCOL: u8 = 200;

/// Why a network that reads key by key is refused.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Invalid {
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

pub(super) fn check(network: &Network) -> Result<(), Invalid> {
    nodes(&network.nodes)?;
    links(network)?;
    classes(&network.routing.classes)?;
    keepalive(&network.mesh)
}

fn nodes(nodes: &Map<Name, Node>) -> Result<(), Invalid> {
    if nodes.is_empty() {
        return Err(Invalid::NoNodes);
    }
    let mut ids: Map<Ipv4Addr, &Name> = Map::new();
    for (name, node) in nodes {
        if [ALL, NONE].contains(&name.as_str()) {
            return Err(Invalid::ReservedName(name.clone()));
        }
        if node.underlay.len() != 1 {
            return Err(Invalid::Underlay {
                node: name.clone(),
                entries: node.underlay.len(),
            });
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
        (Platform::Routeros, LinkKind::Amneziawg(_)) => false,
        (Platform::Talos, _) | (Platform::Routeros, LinkKind::Wireguard(_)) => true,
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
        assert_eq!(check(&with("{}").unwrap()), Ok(()));
    }

    #[test]
    fn a_network_without_nodes_is_refused() {
        // The links go too, or their ends would be refused first.
        let refusal = refusal(
            "{mesh: {links: null}, nodes: {node-a: null, node-b: null, node-c: null, router1: null}}",
        );
        assert_eq!(refusal, Invalid::NoNodes);
    }

    #[test]
    fn a_node_named_by_a_reserved_word_is_refused() {
        for word in ["all", "none"] {
            let refusal = refusal(&format!("{{nodes: {{{word}: {NODE}}}}}"));
            assert_eq!(refusal, Invalid::ReservedName(name(word)));
            assert!(refusal.to_string().contains(&format!("`{word}`")));
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
        let text = refusal.to_string();
        assert!(
            text.contains("`amneziawg`") && text.contains("`routeros`"),
            "{text}"
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
