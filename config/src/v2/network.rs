//! The `network` document of format version 2: the one tree of addressing, routing, links and
//! nodes. Every type refuses a key it does not know, at any depth.
//!
//! As the loader returns it, a node's `routing` and the block of a link's type hold what is in
//! force there: the node's and the link's own values laid over the network's.

use super::{ClassName, Name, Platform};
use crate::router::{BFD_DEFAULT_MIN_RX_MS, BFD_DEFAULT_MIN_TX_MS, BFD_DEFAULT_MULTIPLIER};
use crate::{Obfuscation, Range};
use indexmap::IndexMap;
use serde::{Deserialize, Deserializer};
use sliplib_net::{Ipv4Net, Ipv6Net};
use sliplib_secrets::wireguard::PrivateKey;
use sliplib_yaml::Value;
use std::net::{IpAddr, Ipv4Addr};
use std::num::{NonZeroU8, NonZeroU16, NonZeroU32};

/// What is named, by its name, in the order written: the order reaches the nodes' configs.
pub type Map<K, V> = IndexMap<K, V>;

/// A key written with `null` reads as the key left out.
fn absent_when_null<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Network {
    pub addressing: Addressing,
    pub routing: Routing,
    pub mesh: Mesh,
    pub nodes: Map<Name, Node>,
}

/// An IPv4 network as the files write it, host bits kept.
#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(try_from = "String")]
pub struct Prefix4(pub Ipv4Net);

/// An IPv6 network as the files write it, host bits kept.
#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(try_from = "String")]
pub struct Prefix6(pub Ipv6Net);

impl TryFrom<String> for Prefix4 {
    type Error = sliplib_net::ParseError;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        sliplib_net::parse_ipv4(&text).map(Self)
    }
}

impl TryFrom<String> for Prefix6 {
    type Error = sliplib_net::ParseError;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        sliplib_net::parse_ipv6(&text).map(Self)
    }
}

/// The networks a node's addresses are derived from by its `node_id`.
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
    /// Present, every link has a BFD session; absent, none has.
    #[serde(default)]
    pub bfd: Option<Bfd>,
    #[serde(default, deserialize_with = "absent_when_null")]
    pub direct_interfaces: Vec<String>,
    #[serde(default, deserialize_with = "absent_when_null")]
    pub learn: Vec<Prefix4>,
    #[serde(default, deserialize_with = "absent_when_null")]
    pub announce: Vec<Prefix4>,
    #[serde(default, deserialize_with = "absent_when_null")]
    pub classes: Map<ClassName, Class>,
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Bgp {
    /// The AS number of the mesh, under which the nodes speak iBGP to each other.
    #[serde(rename = "as")]
    pub number: NonZeroU32,
}

// A default of zero would not compile: `unwrap` is evaluated with the constant.
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

/// A named protocol number for routes that something other than the router installs in a node's
/// kernel.
#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Class {
    pub protocol: u8,
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Node {
    pub platform: Platform,
    /// An identity in the form of an IPv4 address, from which the node's addresses are derived.
    pub node_id: Ipv4Addr,
    /// What the node reaches the world through, by name.
    pub underlay: Map<Name, Underlay>,
    #[serde(default)]
    pub wireguard: Option<NodeWireguard>,
    /// The node's own values of the network's `routing`.
    #[serde(default)]
    pub routing: Option<NodeRouting>,
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Underlay {
    /// Absent for a node behind NAT or without a fixed address: it is never dialed.
    #[serde(default)]
    pub endpoint: Option<Endpoint>,
    #[serde(default)]
    pub mtu: Option<u32>,
}

/// The family the links to a node are dialed over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Ipv4,
    Ipv6,
}

/// Where a node's peers reach it: an address of the family its key names, or a host name dialed
/// over that family. It carries no port, since each link adds its own.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(try_from = "EndpointKeys")]
pub struct Endpoint {
    pub family: Family,
    /// The address or the host name, as written.
    pub host: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EndpointKeys {
    #[serde(default)]
    ipv4: Option<String>,
    #[serde(default)]
    ipv6: Option<String>,
}

/// What is wrong with an `endpoint`.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum EndpointError {
    #[error("an `endpoint` names one family, `ipv4` or `ipv6`, and this one names none")]
    NoFamily,
    #[error("an `endpoint` names one family, and this one names both `ipv4` and `ipv6`")]
    BothFamilies,
    #[error("the address {host} stands under `{key}` and is of the other family")]
    OtherFamily { host: String, key: &'static str },
    #[error(
        "{0:?} is neither an IP address nor a host name; an `endpoint` takes no port and no brackets, since each link adds its own port"
    )]
    NotAHost(String),
}

/// Whether `text` is a host name as RFC 1123 takes one: labels of letters, digits and `-` joined
/// by dots, none starting or ending with `-`, and the last not all digits.
fn is_host_name(text: &str) -> bool {
    let labels: Vec<&str> = text.split('.').collect();
    text.len() <= 253
        && labels.iter().all(|label| {
            (1..=63).contains(&label.len())
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                && !label.starts_with('-')
                && !label.ends_with('-')
        })
        && labels
            .last()
            .is_some_and(|last| !last.bytes().all(|byte| byte.is_ascii_digit()))
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
        match host.parse::<IpAddr>() {
            Ok(address) if address.is_ipv4() == (family == Family::Ipv4) => {}
            Ok(_) => return Err(EndpointError::OtherFamily { host, key }),
            Err(_) if is_host_name(&host) => {}
            Err(_) => return Err(EndpointError::NotAHost(host)),
        }
        Ok(Self { family, host })
    }
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct NodeWireguard {
    /// The node's one identity on every link of type `wireguard` or `amneziawg`.
    #[serde(default)]
    pub private_key: Option<PrivateKey>,
}

/// A node's own lists. Left out, a list is the network's; as the loader returns it, every list
/// is the one in force.
#[derive(Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct NodeRouting {
    #[serde(default)]
    pub direct_interfaces: Option<Vec<String>>,
    #[serde(default)]
    pub learn: Option<Vec<Prefix4>>,
    #[serde(default)]
    pub announce: Option<Vec<Prefix4>>,
}

/// What every interface a tool creates for a link starts with.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(try_from = "String")]
pub struct InterfacePrefix(String);

/// What keeps a text from being an interface prefix.
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
        // The kernel refuses `/`, `:` and whitespace in an interface name, and a routing daemon
        // reads the prefix in a pattern, where `*`, `?` and `[` would match other interfaces.
        if !prefix
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        {
            return Err(InterfacePrefixError::Character(prefix));
        }
        Ok(Self(prefix))
    }
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Mesh {
    /// Required by every tool, and written by `slipmesh mint`: a file may be read without it.
    #[serde(default)]
    pub interface_prefix: Option<InterfacePrefix>,
    #[serde(default)]
    pub mtu: Option<u32>,
    #[serde(default)]
    pub wireguard: Option<WireguardDefaults>,
    #[serde(default)]
    pub amneziawg: Option<AmneziawgDefaults>,
    #[serde(default, deserialize_with = "absent_when_null")]
    pub links: Map<Name, Link>,
}

/// What every link of type `wireguard` starts from.
#[derive(Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct WireguardDefaults {
    #[serde(default)]
    pub persistent_keepalive: Option<u16>,
}

/// What every link of type `amneziawg` starts from.
#[derive(Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct AmneziawgDefaults {
    #[serde(default)]
    pub persistent_keepalive: Option<Range<u16>>,
    #[serde(default)]
    pub obfuscation: Option<StrictObfuscation>,
}

/// The keys of a link of type `wireguard`.
#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct WireguardLink {
    /// The UDP port both ends listen on.
    pub port: NonZeroU16,
    #[serde(default)]
    pub persistent_keepalive: Option<u16>,
}

/// The keys of a link of type `amneziawg`.
#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AmneziawgLink {
    /// The UDP port both ends listen on.
    pub port: NonZeroU16,
    #[serde(default)]
    pub persistent_keepalive: Option<Range<u16>>,
    #[serde(default)]
    pub obfuscation: Option<StrictObfuscation>,
}

/// What carries a link, with the keys of that type.
#[derive(Debug, Clone, PartialEq)]
pub enum LinkKind {
    Wireguard(WireguardLink),
    Amneziawg(Box<AmneziawgLink>),
}

/// A link: the two nodes it joins, and what carries it.
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

/// The keys of a link's end. None today.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct End {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LinkKeys {
    r#type: LinkType,
    ends: Map<Name, End>,
    #[serde(default)]
    wireguard: Option<WireguardLink>,
    #[serde(default)]
    amneziawg: Option<AmneziawgLink>,
}

/// What is wrong with a link by its own keys.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum LinkError {
    #[error("a link of type `{0}` is a tunnel and has exactly two ends, and this one has {1}")]
    Ends(&'static str, usize),
    #[error("a link of type `{0}` has no `{0}` block, which holds its port")]
    NoBlock(&'static str),
    #[error("a link of type `{link}` has a `{block}` block, the block of another type")]
    OtherBlock {
        link: &'static str,
        block: &'static str,
    },
}

impl TryFrom<LinkKeys> for Link {
    type Error = LinkError;

    fn try_from(keys: LinkKeys) -> Result<Self, LinkError> {
        const WIREGUARD: &str = "wireguard";
        const AMNEZIAWG: &str = "amneziawg";
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

/// The obfuscation of a link as the files hold it: read like the daemons' own, but refusing a
/// key that is not one of its fields, which a daemon passes over.
#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(try_from = "Value")]
pub struct StrictObfuscation(pub Obfuscation);

/// Every field of [`Obfuscation`], as the files name it.
pub(crate) const OBFUSCATION_FIELDS: [&str; 25] = [
    "jc",
    "jmin",
    "jmax",
    "s1",
    "s2",
    "s3",
    "s4",
    "h1",
    "h2",
    "h3",
    "h4",
    "i1",
    "i2",
    "i3",
    "i4",
    "i5",
    "header_protection_key",
    "content_padding_addition",
    "rekey_after_time",
    "rekey_timeout",
    "reject_after_time",
    "keepalive_timeout",
    "max_handshake_attempts",
    "random_trailers",
    "disable_cookies",
];

impl TryFrom<Value> for StrictObfuscation {
    type Error = String;

    fn try_from(value: Value) -> Result<Self, String> {
        let unknown = value
            .as_mapping()
            .into_iter()
            .flatten()
            .filter_map(|(key, _)| key.as_str())
            .find(|key| !OBFUSCATION_FIELDS.contains(key));
        if let Some(key) = unknown {
            return Err(format!(
                "unknown field `{key}`, expected one of the obfuscation fields"
            ));
        }
        sliplib_yaml::from_value(value)
            .map(Self)
            .map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The network of the specification's example, its `slipmesh:` block taken off.
    fn example() -> Value {
        let mut value: Value =
            sliplib_yaml::from_str(include_str!("../../../docs/example/mesh.yaml")).unwrap();
        value.as_mapping_mut().unwrap().shift_remove("slipmesh");
        value
    }

    fn network(value: Value) -> Result<Network, String> {
        sliplib_yaml::from_value(value).map_err(|error| error.to_string())
    }

    /// The example with `overlay` laid over it.
    fn with(overlay: &str) -> Result<Network, String> {
        let overlay: Value = sliplib_yaml::from_str(overlay).unwrap();
        network(sliplib_yaml::merge(example(), &overlay))
    }

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
        assert_eq!(
            endpoint("node-b"),
            Some(Endpoint {
                family: Family::Ipv4,
                host: "node-b.example.com".to_owned()
            })
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
            let refused = with(overlay).unwrap_err();
            assert!(refused.contains("`color`"), "{overlay}: {refused}");
        }
    }

    #[test]
    fn what_is_only_planned_is_a_key_the_format_does_not_have() {
        for (overlay, key) in [
            (
                "{addressing: {tunnel: {ipv6: \"3fff:0:0:9::/64\"}}}",
                "ipv6",
            ),
            ("{routing: {ospf: {cost: 10}}}", "ospf"),
            ("{routing: {bgp: {sessions: {}}}}", "sessions"),
            ("{routing: {classes: {egress: {table: 152}}}}", "table"),
            ("{mesh: {wireguard: {fwmark: \"0x10\"}}}", "fwmark"),
            ("{mesh: {links: {a-b: {mtu: 1400}}}}", "mtu"),
            (
                "{mesh: {links: {a-b: {routing: {ospf: {cost: 5}}}}}}",
                "routing",
            ),
            (
                "{mesh: {links: {a-b: {ends: {node-a: {interface: wg9}}}}}}",
                "interface",
            ),
            (
                "{mesh: {links: {a-r1: {wireguard: {preshared_key: x}}}}}",
                "preshared_key",
            ),
        ] {
            let refused = with(overlay).unwrap_err();
            assert!(
                refused.contains(&format!("`{key}`")),
                "{overlay}: {refused}"
            );
        }
        let refused = with("{mesh: {links: {a-b: {type: ethernet}}}}").unwrap_err();
        assert!(refused.contains("ethernet"), "{refused}");
    }

    #[test]
    fn a_required_key_left_out_is_missed_by_name() {
        for (overlay, key) in [
            ("{addressing: null}", "addressing"),
            ("{addressing: {loopback: {ipv6: null}}}", "ipv6"),
            ("{routing: {bgp: {as: null}}}", "as"),
            (
                "{routing: {classes: {egress: {protocol: null}}}}",
                "protocol",
            ),
            ("{nodes: {node-a: {platform: null}}}", "platform"),
            ("{nodes: {node-a: {node_id: null}}}", "node_id"),
            ("{nodes: {node-a: {underlay: null}}}", "underlay"),
            ("{mesh: {links: {a-b: {type: null}}}}", "type"),
            ("{mesh: {links: {a-b: {ends: null}}}}", "ends"),
            ("{mesh: {links: {a-b: {amneziawg: {port: null}}}}}", "port"),
        ] {
            let refused = with(overlay).unwrap_err();
            assert!(
                refused.contains(&format!("missing field `{key}`")),
                "{overlay}: {refused}"
            );
        }
    }

    #[test]
    fn a_value_outside_what_its_key_takes_is_refused() {
        for overlay in [
            "{routing: {bgp: {as: 0}}}",
            "{routing: {bgp: {as: 4294967296}}}",
            "{routing: {bfd: {min_rx_ms: 0}}}",
            "{routing: {bfd: {multiplier: 256}}}",
            "{routing: {classes: {egress: {protocol: 256}}}}",
            "{routing: {classes: {-egress: {protocol: 7}}}}",
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
            "{mesh: {links: {a-r1: {wireguard: {persistent_keepalive: 20-30}}}}}",
            "{mesh: {wireguard: {persistent_keepalive: 65536}}}",
            "{mesh: {amneziawg: {obfuscation: {h1: 9-1}}}}",
        ] {
            assert!(with(overlay).is_err(), "{overlay}");
        }
    }

    #[test]
    fn a_list_written_as_null_is_a_list_left_out() {
        let network = with(
            "{routing: {direct_interfaces: null, learn: null, announce: null, classes: null}, \
              mesh: {links: null}}",
        )
        .unwrap();
        assert!(network.routing.direct_interfaces.is_empty());
        assert!(network.routing.learn.is_empty() && network.routing.announce.is_empty());
        assert!(network.routing.classes.is_empty() && network.mesh.links.is_empty());
    }

    #[test]
    fn bfd_written_empty_is_on_with_its_defaults() {
        assert_eq!(with("{routing: {bfd: null}}").unwrap().routing.bfd, None);
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
    fn an_endpoint_is_an_address_of_its_keys_family_or_a_host_name() {
        for (yaml, family, host) in [
            ("{ipv4: 192.0.2.1}", Family::Ipv4, "192.0.2.1"),
            ("{ipv6: \"2001:db8::1\"}", Family::Ipv6, "2001:db8::1"),
            (
                "{ipv4: node-b.example.com}",
                Family::Ipv4,
                "node-b.example.com",
            ),
            ("{ipv6: node-b}", Family::Ipv6, "node-b"),
        ] {
            assert_eq!(
                endpoint(yaml).unwrap(),
                Endpoint {
                    family,
                    host: host.to_owned()
                }
            );
        }
    }

    #[test]
    fn an_endpoint_of_the_other_family_of_both_or_of_none_is_refused() {
        for (yaml, said) in [
            ("{ipv6: 192.0.2.1}", "other family"),
            ("{ipv4: \"2001:db8::1\"}", "other family"),
            ("{ipv4: 192.0.2.1, ipv6: \"2001:db8::1\"}", "both"),
            ("{}", "names none"),
        ] {
            let refused = endpoint(yaml).unwrap_err();
            assert!(refused.contains(said), "{yaml}: {refused}");
        }
    }

    #[test]
    fn an_endpoint_with_a_port_or_brackets_or_no_host_name_is_refused() {
        for host in [
            "192.0.2.1:51820",
            "\"[2001:db8::1]\"",
            "\"[2001:db8::1]:51820\"",
            "node-b.example.com:51820",
            "-node.example.com",
            "node_b.example.com",
            "node..example.com",
            "192.0.2",
            "\"\"",
        ] {
            let refused = endpoint(&format!("{{ipv4: {host}}}")).unwrap_err();
            assert!(
                refused.contains("neither an IP address nor a host name"),
                "{host}: {refused}"
            );
        }
    }

    fn link(yaml: &str) -> Result<Link, String> {
        sliplib_yaml::from_str(yaml).map_err(|error| error.to_string())
    }

    #[test]
    fn a_link_holds_the_block_of_its_own_type_and_no_other() {
        for (yaml, said) in [
            (
                "{type: wireguard, ends: {a: {}, b: {}}}",
                "no `wireguard` block",
            ),
            (
                "{type: amneziawg, ends: {a: {}, b: {}}}",
                "no `amneziawg` block",
            ),
            (
                "{type: wireguard, ends: {a: {}, b: {}}, amneziawg: {port: 1}}",
                "has a `amneziawg` block",
            ),
            (
                "{type: amneziawg, ends: {a: {}, b: {}}, amneziawg: {port: 1}, wireguard: {port: 1}}",
                "has a `wireguard` block",
            ),
        ] {
            let refused = link(yaml).unwrap_err();
            assert!(refused.contains(said), "{yaml}: {refused}");
        }
    }

    #[test]
    fn a_tunnel_has_exactly_two_ends() {
        for (ends, count) in [
            ("{a: {}}", "has 1"),
            ("{a: {}, b: {}, c: {}}", "has 3"),
            ("{}", "has 0"),
        ] {
            let refused = link(&format!(
                "{{type: wireguard, ends: {ends}, wireguard: {{port: 1}}}}"
            ))
            .unwrap_err();
            assert!(refused.contains(count), "{ends}: {refused}");
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
        let written = sliplib_yaml::to_value(&every_field).unwrap();
        let fields: Vec<&str> = written
            .as_mapping()
            .unwrap()
            .keys()
            .map(|key| key.as_str().unwrap())
            .collect();
        assert_eq!(fields, OBFUSCATION_FIELDS);
        let read: StrictObfuscation = sliplib_yaml::from_value(written).unwrap();
        assert_eq!(read.0, every_field);
    }
}
