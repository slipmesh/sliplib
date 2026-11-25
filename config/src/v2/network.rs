//! The `network` document. Every type refuses an unknown key. From the loader, a node's `routing`
//! and a link's block hold what is in force: their own laid over the network's.

use super::name::is_name_character;
use super::{ClassName, Map, Name, Platform, unique_keys};
use crate::router::{
    BFD_DEFAULT_MIN_RX_MS, BFD_DEFAULT_MIN_TX_MS, BFD_DEFAULT_MULTIPLIER, InterfacePattern,
};
use crate::{Obfuscation, Prefix4, Prefix6, Range};
use serde::{Deserialize, Deserializer};
use sliplib_secrets::wireguard::PrivateKey;
use std::net::{IpAddr, Ipv4Addr};
use std::num::{NonZeroU8, NonZeroU16, NonZeroU32};

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Network {
    pub addressing: Addressing,
    pub routing: Routing,
    pub mesh: Mesh,
    #[serde(deserialize_with = "unique_keys")]
    pub nodes: Map<Name, Node>,
}

/// What a node's `node_id` is added to.
#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Addressing {
    pub loopback: Loopback,
    #[serde(default)]
    pub tunnel: Option<Tunnel>,
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Loopback {
    pub ipv4: Prefix4,
    pub ipv6: Prefix6,
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Tunnel {
    #[serde(default)]
    pub ipv4: Option<Prefix4>,
    #[serde(default)]
    pub link_local: Option<Prefix6>,
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Routing {
    pub bgp: Bgp,
    /// Present, a BFD session on every link.
    #[serde(default)]
    pub bfd: Option<Bfd>,
    #[serde(default)]
    pub direct_interfaces: Vec<InterfacePattern>,
    #[serde(default)]
    pub learn: Vec<Prefix4>,
    #[serde(default)]
    pub announce: Vec<Prefix4>,
    #[serde(default, deserialize_with = "unique_keys")]
    pub classes: Map<ClassName, Class>,
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Bgp {
    #[serde(rename = "as")]
    pub number: NonZeroU32,
}

// A zero default fails to compile: `unwrap` runs with the constant.
const DEFAULT_MIN_RX_MS: NonZeroU32 = NonZeroU32::new(BFD_DEFAULT_MIN_RX_MS).unwrap();
const DEFAULT_MIN_TX_MS: NonZeroU32 = NonZeroU32::new(BFD_DEFAULT_MIN_TX_MS).unwrap();
const DEFAULT_MULTIPLIER: NonZeroU8 = NonZeroU8::new(BFD_DEFAULT_MULTIPLIER).unwrap();

fn default_min_rx_ms() -> NonZeroU32 {
    DEFAULT_MIN_RX_MS
}

fn default_min_tx_ms() -> NonZeroU32 {
    DEFAULT_MIN_TX_MS
}

fn default_multiplier() -> NonZeroU8 {
    DEFAULT_MULTIPLIER
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Bfd {
    #[serde(default = "default_min_rx_ms")]
    pub min_rx_ms: NonZeroU32,
    #[serde(default = "default_min_tx_ms")]
    pub min_tx_ms: NonZeroU32,
    #[serde(default = "default_multiplier")]
    pub multiplier: NonZeroU8,
}

/// The protocol number of routes something other than the router installs.
#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Class {
    pub protocol: u8,
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Node {
    pub platform: Platform,
    /// An identity in IPv4 form, which the node's addresses are derived from.
    pub node_id: Ipv4Addr,
    #[serde(deserialize_with = "unique_keys")]
    pub underlay: Map<Name, Underlay>,
    #[serde(default)]
    pub wireguard: Option<NodeWireguard>,
    #[serde(default)]
    pub routing: NodeRouting,
}

impl Node {
    /// The loader leaves one.
    pub fn underlay(&self) -> Option<&Underlay> {
        self.underlay.values().next()
    }

    pub fn private_key(&self) -> Option<&PrivateKey> {
        self.wireguard
            .as_ref()
            .and_then(|wireguard| wireguard.private_key.as_ref())
    }
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Underlay {
    /// Absent, never dialed.
    #[serde(default)]
    pub endpoint: Option<Endpoint>,
    #[serde(default)]
    pub mtu: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Ipv4,
    Ipv6,
}

/// An address of its key's family, or a name taken as written and dialed over it; no port.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(try_from = "EndpointKeys")]
pub struct Endpoint {
    family: Family,
    host: String,
}

impl Endpoint {
    pub fn family(&self) -> Family {
        self.family
    }

    /// As written.
    pub fn host(&self) -> &str {
        &self.host
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EndpointKeys {
    #[serde(default)]
    ipv4: Option<String>,
    #[serde(default)]
    ipv6: Option<String>,
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum EndpointError {
    #[error("an `endpoint` names one family, `ipv4` or `ipv6`, and this one names none")]
    NoFamily,
    #[error("an `endpoint` names one family, and this one names both `ipv4` and `ipv6`")]
    BothFamilies,
    #[error("the address {host} stands under `{key}` and is of the other family")]
    OtherFamily { host: String, key: &'static str },
}

impl TryFrom<EndpointKeys> for Endpoint {
    type Error = EndpointError;

    fn try_from(keys: EndpointKeys) -> Result<Self, EndpointError> {
        let (family, key, host) = match (keys.ipv4, keys.ipv6) {
            (Some(host), None) => (Family::Ipv4, "ipv4", host),
            (None, Some(host)) => (Family::Ipv6, "ipv6", host),
            (None, None) => return Err(EndpointError::NoFamily),
            (Some(_), Some(_)) => return Err(EndpointError::BothFamilies),
        };
        if host
            .parse::<IpAddr>()
            .is_ok_and(|address| address.is_ipv4() != (family == Family::Ipv4))
        {
            return Err(EndpointError::OtherFamily { host, key });
        }
        Ok(Self { family, host })
    }
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct NodeWireguard {
    /// One identity on every link.
    #[serde(default)]
    pub private_key: Option<PrivateKey>,
}

/// From the loader, in force: its own, empty where written `null`, else the network's.
#[derive(Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct NodeRouting {
    #[serde(default)]
    pub direct_interfaces: Vec<InterfacePattern>,
    #[serde(default)]
    pub learn: Vec<Prefix4>,
    #[serde(default)]
    pub announce: Vec<Prefix4>,
}

/// What every link's interface name starts with.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(try_from = "String")]
pub struct InterfacePrefix(String);

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum InterfacePrefixError {
    #[error("mesh.interface_prefix must not be empty: it is what tells a link's interface")]
    Empty,
    #[error(
        "mesh.interface_prefix {0:?} may hold only ASCII letters, digits, `-`, `_` and `.`, the characters an interface name and its pattern both take as they are"
    )]
    Character(String),
}

impl InterfacePrefix {
    /// What `slipmesh mint` writes where the files set none; no tool falls back to it.
    pub const REFERENCE: &str = "mesh-";

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for InterfacePrefix {
    type Error = InterfacePrefixError;

    fn try_from(prefix: String) -> Result<Self, Self::Error> {
        if prefix.is_empty() {
            return Err(InterfacePrefixError::Empty);
        }
        // The kernel refuses `/`, `:` and whitespace; in a routing pattern `*` and `?` would match
        // other interfaces.
        if !prefix.chars().all(is_name_character) {
            return Err(InterfacePrefixError::Character(prefix));
        }
        Ok(Self(prefix))
    }
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Mesh {
    /// Every tool needs it and `slipmesh mint` writes it, so a file reads without it.
    #[serde(default)]
    pub interface_prefix: Option<InterfacePrefix>,
    #[serde(default)]
    pub mtu: Option<u32>,
    #[serde(default)]
    pub wireguard: Option<WireguardDefaults>,
    #[serde(default)]
    pub amneziawg: Option<AmneziawgDefaults>,
    #[serde(default, deserialize_with = "unique_keys")]
    pub links: Map<Name, Link>,
}

#[derive(Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct WireguardDefaults {
    #[serde(default)]
    pub persistent_keepalive: Option<u16>,
}

#[derive(Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct AmneziawgDefaults {
    #[serde(default)]
    pub persistent_keepalive: Option<Range<u16>>,
    #[serde(default)]
    pub obfuscation: Option<StrictObfuscation>,
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct WireguardLink {
    /// Both ends listen on it.
    pub port: NonZeroU16,
    #[serde(default)]
    pub persistent_keepalive: Option<u16>,
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AmneziawgLink {
    /// Both ends listen on it.
    pub port: NonZeroU16,
    #[serde(default)]
    pub persistent_keepalive: Option<Range<u16>>,
    #[serde(default)]
    pub obfuscation: Option<StrictObfuscation>,
}

pub(super) const WIREGUARD: &str = "wireguard";
pub(super) const AMNEZIAWG: &str = "amneziawg";

#[derive(Debug, Clone, PartialEq)]
pub enum LinkKind {
    Wireguard(WireguardLink),
    Amneziawg(Box<AmneziawgLink>),
}

impl LinkKind {
    /// As the files name it.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Wireguard(_) => WIREGUARD,
            Self::Amneziawg(_) => AMNEZIAWG,
        }
    }

    /// Both ends listen on it.
    pub fn port(&self) -> NonZeroU16 {
        match self {
            Self::Wireguard(block) => block.port,
            Self::Amneziawg(block) => block.port,
        }
    }

    /// Empty on `wireguard`.
    pub fn obfuscation(&self) -> Obfuscation {
        match self {
            Self::Wireguard(_) => Obfuscation::default(),
            Self::Amneziawg(block) => block
                .obfuscation
                .as_ref()
                .map(|obfuscation| obfuscation.0.clone())
                .unwrap_or_default(),
        }
    }

    pub fn persistent_keepalive(&self) -> Option<Range<u16>> {
        match self {
            Self::Wireguard(block) => block.persistent_keepalive.map(Range::from),
            Self::Amneziawg(block) => block.persistent_keepalive,
        }
    }
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(try_from = "LinkKeys")]
pub struct Link {
    pub ends: [Name; 2],
    pub kind: LinkKind,
}

#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
enum LinkType {
    Wireguard,
    Amneziawg,
}

/// None today.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct End {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LinkKeys {
    r#type: LinkType,
    #[serde(deserialize_with = "unique_keys")]
    ends: Map<Name, End>,
    #[serde(default)]
    wireguard: Option<WireguardLink>,
    #[serde(default)]
    amneziawg: Option<AmneziawgLink>,
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum LinkError {
    #[error("a link of type `{0}` is a tunnel and has exactly two ends, and this one has {1}")]
    Ends(&'static str, usize),
    #[error("a link of type `{0}` has no `{0}` block, which holds its port")]
    NoBlock(&'static str),
    #[error("a link of type `{link}` has the block `{block}`, which is another type's")]
    OtherBlock {
        link: &'static str,
        block: &'static str,
    },
}

impl Link {
    pub fn has_end(&self, node: &str) -> bool {
        self.ends.iter().any(|end| end.as_str() == node)
    }

    /// None where `node` is not an end.
    pub fn other_end(&self, node: &str) -> Option<&Name> {
        let [a, b] = &self.ends;
        if a.as_str() == node {
            Some(b)
        } else if b.as_str() == node {
            Some(a)
        } else {
            None
        }
    }
}

impl TryFrom<LinkKeys> for Link {
    type Error = LinkError;

    fn try_from(keys: LinkKeys) -> Result<Self, LinkError> {
        let (name, kind) = match (keys.r#type, keys.wireguard, keys.amneziawg) {
            (LinkType::Wireguard, Some(block), None) => (WIREGUARD, LinkKind::Wireguard(block)),
            (LinkType::Amneziawg, None, Some(block)) => {
                (AMNEZIAWG, LinkKind::Amneziawg(Box::new(block)))
            }
            (LinkType::Wireguard, None, None) => return Err(LinkError::NoBlock(WIREGUARD)),
            (LinkType::Amneziawg, None, None) => return Err(LinkError::NoBlock(AMNEZIAWG)),
            (LinkType::Wireguard, _, Some(_)) => {
                return Err(LinkError::OtherBlock {
                    link: WIREGUARD,
                    block: AMNEZIAWG,
                });
            }
            (LinkType::Amneziawg, Some(_), _) => {
                return Err(LinkError::OtherBlock {
                    link: AMNEZIAWG,
                    block: WIREGUARD,
                });
            }
        };
        let count = keys.ends.len();
        let ends: Vec<Name> = keys.ends.into_keys().collect();
        let ends: [Name; 2] = ends.try_into().map_err(|_| LinkError::Ends(name, count))?;
        Ok(Self { ends, kind })
    }
}

/// Refusing an unknown key, which a daemon passes over.
#[derive(Debug, Clone, PartialEq)]
pub struct StrictObfuscation(pub Obfuscation);

impl<'de> Deserialize<'de> for StrictObfuscation {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        sliplib_yaml::strict(deserializer).map(Self)
    }
}

#[cfg(test)]
mod tests {
    use super::super::NameError;
    use super::super::testing::{example, left_out, network, with};
    use super::*;
    use sliplib_yaml::Value;

    #[test]
    fn the_example_of_the_specification_reads() {
        let network = network(example()).unwrap();
        let nodes: Vec<&str> = network.nodes.keys().map(Name::as_str).collect();
        assert_eq!(nodes, ["node-a", "node-b", "node-c", "router1"]);
        let links: Vec<&str> = network.mesh.links.keys().map(Name::as_str).collect();
        assert_eq!(links, ["a-b", "a-c", "a-r1"]);

        assert_eq!(network.routing.bgp.number.get(), 64500);
        assert_eq!(network.routing.classes.len(), 1);
        assert_eq!(network.mesh.interface_prefix.unwrap().as_str(), "mesh-");
        assert_eq!(network.mesh.mtu, Some(1320));
        assert_eq!(network.nodes["router1"].platform, Platform::Routeros);

        let endpoint = |node: &str| network.nodes[node].underlay["wan"].endpoint.clone();
        let node_b = endpoint("node-b").unwrap();
        assert_eq!(
            (node_b.family(), node_b.host()),
            (Family::Ipv4, "node-b.example.com")
        );
        assert_eq!(endpoint("node-c"), None);
        assert_eq!(network.nodes["node-c"].underlay["wan"].mtu, Some(1420));

        let LinkKind::Amneziawg(a_c) = &network.mesh.links["a-c"].kind else {
            panic!("a-c is of type amneziawg");
        };
        assert_eq!(a_c.port.get(), 51831);
        assert_eq!(a_c.persistent_keepalive, Some(Range::new(15, 20).unwrap()));
        assert!(matches!(
            network.mesh.links["a-r1"].kind,
            LinkKind::Wireguard(_)
        ));
        assert_eq!(
            network.mesh.links["a-b"].ends.each_ref().map(Name::as_str),
            ["node-a", "node-b"]
        );
    }

    #[test]
    fn a_key_the_format_does_not_have_is_refused_at_every_depth() {
        for overlay in [
            "{color: red}",
            "{addressing: {color: red}}",
            "{addressing: {loopback: {color: red}}}",
            "{addressing: {tunnel: {color: red}}}",
            "{routing: {color: red}}",
            "{routing: {bgp: {color: red}}}",
            "{routing: {bfd: {color: red}}}",
            "{routing: {classes: {egress: {color: red}}}}",
            "{nodes: {node-a: {color: red}}}",
            "{nodes: {node-a: {underlay: {wan: {color: red}}}}}",
            "{nodes: {node-a: {underlay: {wan: {endpoint: {color: red}}}}}}",
            "{nodes: {node-a: {wireguard: {color: red}}}}",
            "{nodes: {node-a: {routing: {color: red}}}}",
            "{mesh: {color: red}}",
            "{mesh: {wireguard: {color: red}}}",
            "{mesh: {amneziawg: {color: red}}}",
            "{mesh: {amneziawg: {obfuscation: {color: red}}}}",
            "{mesh: {links: {a-b: {color: red}}}}",
            "{mesh: {links: {a-b: {ends: {node-a: {color: red}}}}}}",
            "{mesh: {links: {a-b: {amneziawg: {color: red}}}}}",
            "{mesh: {links: {a-b: {amneziawg: {obfuscation: {color: red}}}}}}",
            "{mesh: {links: {a-r1: {wireguard: {color: red}}}}}",
        ] {
            assert!(with(overlay).is_err(), "{overlay}");
        }
    }

    #[test]
    fn what_is_only_planned_is_a_key_the_format_does_not_have() {
        for overlay in [
            "{addressing: {tunnel: {ipv6: \"3fff:0:0:9::/64\"}}}",
            "{routing: {ospf: {cost: 10}}}",
            "{routing: {bgp: {sessions: {}}}}",
            "{routing: {classes: {egress: {table: 152}}}}",
            "{mesh: {wireguard: {fwmark: \"0x10\"}}}",
            "{mesh: {links: {a-b: {mtu: 1400}}}}",
            "{mesh: {links: {a-b: {routing: {ospf: {cost: 5}}}}}}",
            "{mesh: {links: {a-b: {ends: {node-a: {interface: wg9}}}}}}",
            "{mesh: {links: {a-r1: {wireguard: {preshared_key: x}}}}}",
            "{mesh: {links: {a-b: {type: ethernet}}}}",
        ] {
            assert!(with(overlay).is_err(), "{overlay}");
        }
    }

    #[test]
    fn a_required_key_left_out_is_refused() {
        for path in [
            &["addressing"][..],
            &["addressing", "loopback", "ipv6"],
            &["routing", "bgp", "as"],
            &["routing", "classes", "egress", "protocol"],
            &["nodes", "node-a", "platform"],
            &["nodes", "node-a", "node_id"],
            &["nodes", "node-a", "underlay"],
            &["mesh", "links", "a-b", "type"],
            &["mesh", "links", "a-b", "ends"],
            &["mesh", "links", "a-b", "amneziawg", "port"],
        ] {
            assert!(left_out(path).is_err(), "{path:?}");
        }
    }

    #[test]
    fn a_value_outside_what_its_key_takes_is_refused() {
        for overlay in [
            "{routing: {bgp: {as: 0}}}",
            "{routing: {bgp: {as: 4294967296}}}",
            "{routing: {bfd: {min_rx_ms: 0}}}",
            "{routing: {bfd: {min_tx_ms: 0}}}",
            "{routing: {bfd: {multiplier: 0}}}",
            "{routing: {bfd: {multiplier: 256}}}",
            "{routing: {classes: {egress: {protocol: 256}}}}",
            "{routing: {classes: {-egress: {protocol: 7}}}}",
            "{routing: {direct_interfaces: [\"lan?\"]}}",
            "{nodes: {node-a: {routing: {direct_interfaces: [198.51.100.0/25]}}}}",
            "{routing: {learn: [\"3fff::/64\"]}}",
            "{routing: {announce: [203.0.113.64]}}",
            "{addressing: {loopback: {ipv4: \"3fff::/64\"}}}",
            "{addressing: {tunnel: {link_local: 198.51.100.0/25}}}",
            "{nodes: {node-a: {node_id: one}}}",
            "{nodes: {node-a: {platform: openwrt}}}",
            "{nodes: {node a: {platform: talos, node_id: 0.0.0.9, underlay: {wan: {}}}}}",
            "{mesh: {interface_prefix: \"\"}}",
            "{mesh: {interface_prefix: \"mesh*\"}}",
            "{mesh: {links: {a-b: {amneziawg: {port: 0}}}}}",
            "{mesh: {links: {a-b: {amneziawg: {port: 65536}}}}}",
            "{mesh: {links: {a-r1: {wireguard: {port: 0}}}}}",
            "{mesh: {links: {a-r1: {wireguard: {persistent_keepalive: 20-30}}}}}",
            "{mesh: {wireguard: {persistent_keepalive: 65536}}}",
            "{mesh: {amneziawg: {obfuscation: {h1: 9-1}}}}",
        ] {
            assert!(with(overlay).is_err(), "{overlay}");
        }
    }

    #[test]
    fn bfd_written_empty_is_on_with_its_defaults() {
        let mut value = example();
        value["routing"]["bfd"] = Value::Mapping(Default::default());
        let bfd = network(value).unwrap().routing.bfd.unwrap();
        assert_eq!(
            (
                bfd.min_rx_ms.get(),
                bfd.min_tx_ms.get(),
                bfd.multiplier.get()
            ),
            (300, 300, 5)
        );
    }

    fn endpoint(yaml: &str) -> Result<Endpoint, String> {
        sliplib_yaml::from_str(yaml).map_err(|error| error.to_string())
    }

    #[test]
    fn an_endpoint_is_an_address_of_its_keys_family_or_a_name() {
        for (yaml, family, host) in [
            ("{ipv4: 192.0.2.1}", Family::Ipv4, "192.0.2.1"),
            ("{ipv6: \"2001:db8::1\"}", Family::Ipv6, "2001:db8::1"),
            (
                "{ipv4: node-b.example.com}",
                Family::Ipv4,
                "node-b.example.com",
            ),
            ("{ipv6: node-b}", Family::Ipv6, "node-b"),
            (
                "{ipv4: node_b.example.com}",
                Family::Ipv4,
                "node_b.example.com",
            ),
        ] {
            let read = endpoint(yaml).unwrap();
            assert_eq!((read.family(), read.host()), (family, host), "{yaml}");
        }
    }

    #[test]
    fn an_endpoint_of_the_other_family_of_both_or_of_none_is_refused() {
        let other_family = |host: &str, key| EndpointError::OtherFamily {
            host: host.to_owned(),
            key,
        };
        for (yaml, refusal) in [
            ("{ipv6: 192.0.2.1}", other_family("192.0.2.1", "ipv6")),
            (
                "{ipv4: \"2001:db8::1\"}",
                other_family("2001:db8::1", "ipv4"),
            ),
            (
                "{ipv4: 192.0.2.1, ipv6: \"2001:db8::1\"}",
                EndpointError::BothFamilies,
            ),
            ("{}", EndpointError::NoFamily),
        ] {
            let refused = endpoint(yaml).unwrap_err();
            assert!(refused.contains(&refusal.to_string()), "{yaml}: {refused}");
        }
    }

    fn link(yaml: &str) -> Result<Link, String> {
        sliplib_yaml::from_str(yaml).map_err(|error| error.to_string())
    }

    #[test]
    fn a_link_tells_its_ends_and_each_ends_other_end() {
        let link = link("{type: wireguard, ends: {a: {}, b: {}}, wireguard: {port: 1}}").unwrap();
        assert!(link.has_end("a") && link.has_end("b") && !link.has_end("c"));
        assert_eq!(link.other_end("a").map(Name::as_str), Some("b"));
        assert_eq!(link.other_end("b").map(Name::as_str), Some("a"));
        assert_eq!(link.other_end("c"), None);
    }

    #[test]
    fn a_link_gives_its_obfuscation_and_keepalive_an_empty_one_on_wireguard() {
        let amneziawg = link(
            "{type: amneziawg, ends: {a: {}, b: {}}, amneziawg: {port: 1, persistent_keepalive: \
             15-20, obfuscation: {s4: 16}}}",
        )
        .unwrap();
        assert_eq!(amneziawg.kind.obfuscation().s4, Some(16));
        assert_eq!(
            amneziawg.kind.persistent_keepalive(),
            Some(Range::new(15, 20).unwrap())
        );
        let unset = link("{type: amneziawg, ends: {a: {}, b: {}}, amneziawg: {port: 1}}").unwrap();
        assert_eq!(unset.kind.obfuscation(), Obfuscation::default());
        assert_eq!(unset.kind.persistent_keepalive(), None);
        let wireguard = link(
            "{type: wireguard, ends: {a: {}, b: {}}, wireguard: {port: 1, persistent_keepalive: 10}}",
        )
        .unwrap();
        assert_eq!(wireguard.kind.obfuscation(), Obfuscation::default());
        assert_eq!(wireguard.kind.persistent_keepalive(), Some(Range::from(10)));
    }

    #[test]
    fn a_node_gives_its_one_underlay_entry_and_the_key_written_for_it() {
        let key = PrivateKey::generate();
        let network = with(&format!(
            "{{nodes: {{node-a: {{wireguard: {{private_key: {}}}}}}}}}",
            key.expose()
        ))
        .unwrap();
        assert_eq!(network.nodes["node-c"].underlay().unwrap().mtu, Some(1420));
        assert_eq!(network.nodes["node-a"].private_key(), Some(&key));
        assert_eq!(network.nodes["node-c"].private_key(), None);
    }

    #[test]
    fn a_link_holds_the_block_of_its_own_type_and_no_other() {
        for (yaml, refusal) in [
            (
                "{type: wireguard, ends: {a: {}, b: {}}}",
                LinkError::NoBlock(WIREGUARD),
            ),
            (
                "{type: amneziawg, ends: {a: {}, b: {}}}",
                LinkError::NoBlock(AMNEZIAWG),
            ),
            (
                "{type: wireguard, ends: {a: {}, b: {}}, amneziawg: {port: 1}}",
                LinkError::OtherBlock {
                    link: WIREGUARD,
                    block: AMNEZIAWG,
                },
            ),
            (
                "{type: amneziawg, ends: {a: {}, b: {}}, amneziawg: {port: 1}, wireguard: {port: 1}}",
                LinkError::OtherBlock {
                    link: AMNEZIAWG,
                    block: WIREGUARD,
                },
            ),
        ] {
            let refused = link(yaml).unwrap_err();
            assert!(refused.contains(&refusal.to_string()), "{yaml}: {refused}");
        }
    }

    #[test]
    fn a_tunnel_has_exactly_two_ends() {
        for (ends, count) in [("{a: {}}", 1), ("{a: {}, b: {}, c: {}}", 3), ("{}", 0)] {
            let refused = link(&format!(
                "{{type: wireguard, ends: {ends}, wireguard: {{port: 1}}}}"
            ))
            .unwrap_err();
            let refusal = LinkError::Ends(WIREGUARD, count).to_string();
            assert!(refused.contains(&refusal), "{ends}: {refused}");
        }
    }

    #[test]
    fn every_field_of_the_obfuscation_is_one_the_files_may_name() {
        let every_field = Obfuscation {
            jc: Some(1),
            jmin: Some(1),
            jmax: Some(1),
            s1: Some(1),
            s2: Some(1),
            s3: Some(1),
            s4: Some(1),
            h1: Some(5.into()),
            h2: Some(6.into()),
            h3: Some(7.into()),
            h4: Some(8.into()),
            i1: Some(String::new()),
            i2: Some(String::new()),
            i3: Some(String::new()),
            i4: Some(String::new()),
            i5: Some(String::new()),
            header_protection_key: Some("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".into()),
            content_padding_addition: Some(1.into()),
            rekey_after_time: Some(1.into()),
            rekey_timeout: Some(1.into()),
            reject_after_time: Some(1.into()),
            keepalive_timeout: Some(1.into()),
            max_handshake_attempts: Some(1.into()),
            random_trailers: Some(true),
            disable_cookies: Some(true),
        };
        let written = sliplib_yaml::to_string(&every_field).unwrap();
        assert_eq!(obfuscation(&written).unwrap().0, every_field);
    }

    fn obfuscation(yaml: &str) -> Result<StrictObfuscation, String> {
        sliplib_yaml::from_str(yaml).map_err(|error| error.to_string())
    }

    #[test]
    fn a_key_the_obfuscation_does_not_have_is_refused_naming_it() {
        assert!(obfuscation("{jc: 4}").is_ok());
        let refused = obfuscation("{jc: 4, color: red}").unwrap_err();
        assert!(refused.contains("`color`"), "{refused}");
    }

    #[test]
    fn a_key_of_the_obfuscation_that_is_not_a_text_is_refused() {
        for yaml in ["{0: 4}", "{true: 4}", "{jc: 4, 1: 5}"] {
            assert!(obfuscation(yaml).is_err(), "{yaml} from the text");
            let value: Value = sliplib_yaml::from_str(yaml).unwrap();
            let read: Result<StrictObfuscation, _> = sliplib_yaml::from_value(value);
            assert!(read.is_err(), "{yaml} from a value");
        }
    }

    /// To be laid into the example under a name.
    const NODE: &str = "{platform: talos, node_id: 0.0.0.9, underlay: {wan: {}}}";

    // YAML reads `1.0` and `1` as one name: two entries would be one, the later replacing the
    // earlier.
    #[test]
    fn two_keys_of_one_mapping_of_the_network_that_read_as_one_name_are_refused() {
        let node = NODE.replace("0.0.0.9", "0.0.0.10");
        let link = "{type: wireguard, ends: {node-a: {}, router1: {}}, wireguard: {port: 1}}";
        for overlay in [
            format!("{{nodes: {{1.0: {NODE}, 1: {node}}}}}"),
            "{routing: {classes: {1.0: {protocol: 160}, 1: {protocol: 161}}}}".to_owned(),
            "{nodes: {node-a: {underlay: {1.0: {}, 1: {}}}}}".to_owned(),
            format!("{{mesh: {{links: {{1.0: {link}, 1: {link}}}}}}}"),
            "{mesh: {links: {a-b: {ends: {1.0: {}, 1: {}}}}}}".to_owned(),
        ] {
            let refused = with(&overlay).unwrap_err();
            let twice = NameError::Twice("1".to_owned()).to_string();
            assert!(refused.contains(&twice), "{overlay}: {refused}");
        }
    }
}
