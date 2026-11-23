//! The `router` daemon's config, which the RouterOS tool reads too. Nothing is discovered.

use crate::v2::is_name_character;
use crate::v2::network::InterfacePrefix;
use crate::{Prefix4, Prefix6};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use sliplib_net::IpNet;
use std::collections::HashSet;
use std::net::{IpAddr, Ipv6Addr};

/// BIRD's defaults call a session dead after ~500 ms, too short over long tunnels where a stall is
/// normal and a flap costs more than slow convergence. 300 ms, multiplier 5: ~1.5 s, against
/// OSPF's 40 s.
pub const BFD_DEFAULT_MIN_RX_MS: u32 = 300;
pub const BFD_DEFAULT_MIN_TX_MS: u32 = 300;
pub const BFD_DEFAULT_MULTIPLIER: u8 = 5;

fn default_bfd_min_rx_ms() -> u32 {
    BFD_DEFAULT_MIN_RX_MS
}
fn default_bfd_min_tx_ms() -> u32 {
    BFD_DEFAULT_MIN_TX_MS
}
fn default_bfd_multiplier() -> u8 {
    BFD_DEFAULT_MULTIPLIER
}

/// Only what both BIRD and RouterOS take: one end set otherwise would differ from the other. No
/// authentication: the packets travel inside WireGuard.
#[derive(Deserialize, Serialize, Debug, PartialEq, Clone)]
pub struct BfdSettings {
    #[serde(default = "default_bfd_min_rx_ms")]
    pub min_rx_ms: u32,
    #[serde(default = "default_bfd_min_tx_ms")]
    pub min_tx_ms: u32,
    #[serde(default = "default_bfd_multiplier")]
    pub multiplier: u8,
}

impl Default for BfdSettings {
    fn default() -> Self {
        Self {
            min_rx_ms: BFD_DEFAULT_MIN_RX_MS,
            min_tx_ms: BFD_DEFAULT_MIN_TX_MS,
            multiplier: BFD_DEFAULT_MULTIPLIER,
        }
    }
}

/// The interface that carries the node's loopbacks: OSPF runs on it as a stub, with no BFD.
pub const ROUTER_LOOPBACK: &str = "router-lo";

/// An interface name, or a prefix and `*`: what BIRD's `interface` clause and RouterOS's interface
/// names both match alike.
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(try_from = "String", into = "String")]
pub struct InterfacePattern(String);

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum InterfacePatternError {
    #[error("an interface pattern must not be empty")]
    Empty,
    #[error(
        "interface pattern {pattern:?} holds {character:?}: write an interface name of ASCII letters, digits, `-`, `_` and `.`, or a prefix of them and `*`"
    )]
    Character { pattern: String, character: char },
}

impl InterfacePattern {
    /// Every name that starts with `prefix`.
    pub fn every_name_from(prefix: &InterfacePrefix) -> Self {
        Self(format!("{}*", prefix.as_str()))
    }

    pub fn router_loopback() -> Self {
        Self(ROUTER_LOOPBACK.to_owned())
    }

    pub fn is_router_loopback(&self) -> bool {
        self.0 == ROUTER_LOOPBACK
    }

    pub fn matches(&self, name: &str) -> bool {
        match self.0.strip_suffix('*') {
            Some(prefix) => name.starts_with(prefix),
            None => self.0 == name,
        }
    }

    /// The one name it matches, if it has no `*`.
    pub fn name(&self) -> Option<&str> {
        (!self.0.ends_with('*')).then_some(self.0.as_str())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for InterfacePattern {
    type Error = InterfacePatternError;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        if text.is_empty() {
            return Err(InterfacePatternError::Empty);
        }
        let body = text.strip_suffix('*').unwrap_or(&text);
        if let Some(character) = body.chars().find(|&c| !is_name_character(c)) {
            return Err(InterfacePatternError::Character {
                pattern: text,
                character,
            });
        }
        Ok(Self(text))
    }
}

impl From<InterfacePattern> for String {
    fn from(pattern: InterfacePattern) -> Self {
        pattern.0
    }
}

#[derive(Deserialize, Serialize, Debug, PartialEq)]
pub struct RouterConfig {
    pub node: NodeIdentity,
    pub bgp_as: u32,
    #[serde(default)]
    pub bgp_peers: Vec<BgpPeerEntry>,
    /// BFD on the OSPF links; the RouterOS tool reads it too.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bfd: Option<BfdSettings>,
    #[serde(default)]
    pub ospf_interfaces: Vec<InterfacePattern>,
    /// `protocol direct` sources, whose connected routes go out over iBGP as `router-lo`'s do.
    #[serde(default)]
    pub direct_interfaces: Vec<InterfacePattern>,
    /// A kernel route at or inside one is announced over iBGP.
    #[serde(default)]
    pub learn: Vec<Prefix4>,
    #[serde(default)]
    pub announce: Vec<AnnounceEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bypass: Option<BypassConfig>,
}

#[derive(Deserialize, Serialize, Debug, PartialEq)]
pub struct NodeIdentity {
    pub loopback_addresses: Loopbacks,
}

/// The IPv4 one is BIRD's router id and `krt_prefsrc`, the IPv6 one the iBGP source. Written as a
/// list of the two; read from one, a prefix length left out is the host's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Loopbacks {
    pub ipv4: Prefix4,
    pub ipv6: Prefix6,
}

impl Serialize for Loopbacks {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        (self.ipv4, self.ipv6).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Loopbacks {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let (mut ipv4, mut ipv6) = (Vec::new(), Vec::new());
        for text in Vec::<String>::deserialize(deserializer)? {
            let net = loopback(&text)
                .map_err(|error| de::Error::custom(format!("loopback_addresses: {error}")))?;
            match net {
                IpNet::V4(net) => ipv4.push(net),
                IpNet::V6(net) => ipv6.push(net),
            }
        }
        match (ipv4.as_slice(), ipv6.as_slice()) {
            ([v4], [v6]) => Ok(Self {
                ipv4: Prefix4(*v4),
                ipv6: Prefix6(*v6),
            }),
            _ => Err(de::Error::custom(format!(
                "loopback_addresses holds {} IPv4 and {} IPv6 addresses, and takes one of each",
                ipv4.len(),
                ipv6.len()
            ))),
        }
    }
}

fn loopback(text: &str) -> Result<IpNet, String> {
    if text.contains('/') {
        return sliplib_net::parse(text).map_err(|error| error.to_string());
    }
    text.parse::<IpAddr>()
        .map(IpNet::from)
        .map_err(|error| format!("{text:?} is not an IP address: {error}"))
}

#[derive(Deserialize, Serialize, Debug, PartialEq)]
pub struct BgpPeerEntry {
    pub name: String,
    /// As BIRD's `neighbor` takes it; RFC 8950 carries IPv4 over the session.
    pub address: Ipv6Addr,
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
pub struct AnnounceEntry {
    pub net: Prefix4,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Deserialize, Serialize, Debug, PartialEq)]
pub struct BypassConfig {
    #[serde(default = "default_bypass_refresh_interval_secs")]
    pub refresh_interval_secs: u64,
    pub include: Vec<BypassSourceEntry>,
    #[serde(default)]
    pub exclude: Vec<BypassSourceEntry>,
}

pub const DEFAULT_BYPASS_REFRESH_INTERVAL_SECS: u64 = 24 * 60 * 60;

fn default_bypass_refresh_interval_secs() -> u64 {
    DEFAULT_BYPASS_REFRESH_INTERVAL_SECS
}

/// Resolved by the daemon: `asn` and `geoip` through RIPEstat, `literal` as written, `dns` by its A
/// records.
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
pub struct BypassSourceEntry {
    /// `asn`, `literal`, `geoip` or `dns`, checked by [`validate`].
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asns: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefixes: Option<Vec<LabeledPrefix>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    /// `dns` only; the list shares `label`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hostnames: Option<Vec<String>>,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
pub struct LabeledPrefix {
    pub net: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// What the daemon would refuse, or fail on once running.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Invalid {
    #[error("bgp_as must not be 0 (reserved, RFC 7607)")]
    BgpAsZero,
    #[error("bfd: min_rx_ms and min_tx_ms must be greater than 0")]
    BfdIntervalZero,
    #[error("bfd: multiplier must be greater than 0")]
    BfdMultiplierZero,
    #[error("bgp_peers: name must not be empty")]
    EmptyPeerName,
    #[error("bgp_peers: duplicate name {0:?}")]
    DuplicatePeer(String),
    #[error("bypass.refresh_interval_secs must be greater than 0")]
    BypassRefreshZero,
    #[error("literal bypass source requires `prefixes`")]
    LiteralWithoutPrefixes,
    #[error("bypass: invalid literal prefix {net:?}: {error}")]
    LiteralPrefix {
        net: String,
        error: sliplib_net::ParseError,
    },
    #[error("{kind} bypass source requires `{field}`")]
    SourceLacks {
        kind: &'static str,
        field: &'static str,
    },
    #[error("unknown bypass source kind: {0:?}")]
    UnknownSourceKind(String),
}

/// No I/O: a `literal` source's prefixes are checked, the others' only once resolved.
pub fn validate(cfg: &RouterConfig) -> Result<(), Invalid> {
    // RFC 7607: BIRD would refuse it only when a session starts. Private-use numbers are taken.
    if cfg.bgp_as == 0 {
        return Err(Invalid::BgpAsZero);
    }
    if let Some(bfd) = &cfg.bfd {
        // BIRD refuses a zero, and the daemon would learn so only at `birdc configure`.
        if bfd.min_rx_ms == 0 || bfd.min_tx_ms == 0 {
            return Err(Invalid::BfdIntervalZero);
        }
        if bfd.multiplier == 0 {
            return Err(Invalid::BfdMultiplierZero);
        }
    }

    let mut seen_names = HashSet::new();
    for peer in &cfg.bgp_peers {
        if peer.name.is_empty() {
            return Err(Invalid::EmptyPeerName);
        }
        if !seen_names.insert(peer.name.as_str()) {
            return Err(Invalid::DuplicatePeer(peer.name.clone()));
        }
    }

    if let Some(bypass) = &cfg.bypass {
        // `tokio::time::interval` panics on a zero period.
        if bypass.refresh_interval_secs == 0 {
            return Err(Invalid::BypassRefreshZero);
        }
        for entry in bypass.include.iter().chain(bypass.exclude.iter()) {
            validate_bypass_entry(entry)?;
        }
    }

    Ok(())
}

fn validate_bypass_entry(entry: &BypassSourceEntry) -> Result<(), Invalid> {
    let lacks = |kind, field| Invalid::SourceLacks { kind, field };
    match entry.kind.as_str() {
        "literal" => {
            let prefixes = entry
                .prefixes
                .as_ref()
                .ok_or(Invalid::LiteralWithoutPrefixes)?;
            for p in prefixes {
                sliplib_net::parse_ipv4(&p.net).map_err(|error| Invalid::LiteralPrefix {
                    net: p.net.clone(),
                    error,
                })?;
            }
        }
        "asn" => {
            entry.label.as_ref().ok_or(lacks("asn", "label"))?;
            entry.asns.as_ref().ok_or(lacks("asn", "asns"))?;
        }
        "dns" => {
            entry.label.as_ref().ok_or(lacks("dns", "label"))?;
            entry.hostnames.as_ref().ok_or(lacks("dns", "hostnames"))?;
        }
        "geoip" => {
            entry.country.as_ref().ok_or(lacks("geoip", "country"))?;
        }
        other => return Err(Invalid::UnknownSourceKind(other.to_owned())),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_yaml() -> &'static str {
        r#"
node:
  loopback_addresses: ["198.51.100.1/32", "3fff::1/128"]
bgp_as: 64512
"#
    }

    #[test]
    fn validate_rejects_each_bfd_setting_of_zero_alone() {
        let refusal = |setting: &str| {
            let yaml = format!("{}bfd:\n  {setting}: 0\n", minimal_yaml());
            validate(&sliplib_yaml::from_str(&yaml).unwrap()).unwrap_err()
        };
        assert!(matches!(refusal("min_rx_ms"), Invalid::BfdIntervalZero));
        assert!(matches!(refusal("min_tx_ms"), Invalid::BfdIntervalZero));
        assert!(matches!(refusal("multiplier"), Invalid::BfdMultiplierZero));
    }

    /// BIRD refuses a zero interval.
    #[test]
    fn validate_rejects_a_bfd_interval_of_zero() {
        let yaml = format!(
            "{}bfd:
  min_rx_ms: 0
  min_tx_ms: 300
",
            minimal_yaml()
        );
        let cfg: RouterConfig = sliplib_yaml::from_str(&yaml).unwrap();
        assert_eq!(cfg.bfd.as_ref().unwrap().multiplier, BFD_DEFAULT_MULTIPLIER);
        let err = validate(&cfg).unwrap_err().to_string();
        assert!(err.contains("min_rx_ms"), "unexpected error: {err}");
    }

    #[test]
    fn parses_a_minimal_config() {
        let cfg: RouterConfig = sliplib_yaml::from_str(minimal_yaml()).unwrap();
        assert_eq!(cfg.bgp_as, 64512);
        assert!(cfg.bgp_peers.is_empty());
        assert!(cfg.bypass.is_none());
        validate(&cfg).unwrap();
    }

    #[test]
    fn parses_a_full_config() {
        let yaml = r#"
node:
  loopback_addresses: ["198.51.100.1/32", "3fff::1/128"]
bgp_as: 64512
bgp_peers:
  - name: east
    address: 3fff::2
  - name: west
    address: 3fff::3
ospf_interfaces: ["mesh-*", "router-lo"]
learn: ["203.0.113.0/27"]
announce:
  - net: "203.0.113.192/26"
    label: "k8s-services"
bypass:
  refresh_interval_secs: 3600
  include:
    - kind: asn
      label: "some vendor"
      asns: ["AS15169"]
  exclude:
    - kind: literal
      prefixes: [{net: "192.0.2.0/24"}]
"#;
        let cfg: RouterConfig = sliplib_yaml::from_str(yaml).unwrap();
        validate(&cfg).unwrap();
        assert_eq!(cfg.bgp_peers.len(), 2);
        assert_eq!(cfg.bypass.as_ref().unwrap().refresh_interval_secs, 3600);
    }

    #[test]
    fn rejects_duplicate_peer_names() {
        let mut cfg: RouterConfig = sliplib_yaml::from_str(minimal_yaml()).unwrap();
        cfg.bgp_peers = vec![
            BgpPeerEntry {
                name: "east".to_string(),
                address: "3fff::2".parse().unwrap(),
            },
            BgpPeerEntry {
                name: "east".to_string(),
                address: "3fff::3".parse().unwrap(),
            },
        ];
        assert!(validate(&cfg).is_err());
    }

    fn with_loopbacks(list: &str) -> Result<RouterConfig, String> {
        sliplib_yaml::from_str(&format!(
            "node:\n  loopback_addresses: {list}\nbgp_as: 64512\n"
        ))
        .map_err(|error| error.to_string())
    }

    #[test]
    fn the_loopbacks_read_as_one_of_each_family_in_either_order() {
        for list in [
            "[198.51.100.1/32, 3fff::1/128]",
            "[3fff::1/128, 198.51.100.1/32]",
        ] {
            let loopbacks = with_loopbacks(list).unwrap().node.loopback_addresses;
            assert_eq!(loopbacks.ipv4.0.to_string(), "198.51.100.1/32", "{list}");
            assert_eq!(loopbacks.ipv6.0.to_string(), "3fff::1/128", "{list}");
        }
    }

    #[test]
    fn a_loopback_without_a_prefix_length_is_the_hosts_and_one_with_it_keeps_it() {
        let loopbacks = with_loopbacks("[198.51.100.1, 3fff::1/64]")
            .unwrap()
            .node
            .loopback_addresses;
        assert_eq!(loopbacks.ipv4.0.to_string(), "198.51.100.1/32");
        assert_eq!(loopbacks.ipv6.0.to_string(), "3fff::1/64");
        let ipv6 = with_loopbacks("[198.51.100.1/25, 3fff::1]").unwrap();
        assert_eq!(
            ipv6.node.loopback_addresses.ipv6.0.to_string(),
            "3fff::1/128"
        );
        assert_eq!(
            ipv6.node.loopback_addresses.ipv4.0.to_string(),
            "198.51.100.1/25"
        );
    }

    #[test]
    fn loopbacks_other_than_one_of_each_family_are_refused_at_their_key() {
        for list in [
            "[3fff::1/128]",
            "[198.51.100.1/32]",
            "[198.51.100.1/32, 198.51.100.2/32, 3fff::1/128]",
            "[198.51.100.1/32, 3fff::1/128, 3fff::2/128]",
            "[198.51.100.1/32, not-an-address]",
        ] {
            let err = with_loopbacks(list).unwrap_err();
            assert!(
                err.starts_with("node: loopback_addresses") && err.contains("at line 2"),
                "{list}: {err}"
            );
        }
    }

    #[test]
    fn the_loopbacks_are_written_as_read() {
        let cfg = with_loopbacks("[3fff::1/128, 198.51.100.1/32]").unwrap();
        assert_eq!(
            sliplib_yaml::to_string(&cfg.node).unwrap(),
            "loopback_addresses:\n- 198.51.100.1/32\n- 3fff::1/128\n"
        );
    }

    #[test]
    fn a_peer_address_learn_or_announce_of_the_wrong_family_is_refused_at_its_key() {
        for (yaml, key) in [
            (
                "bgp_peers: [{name: east, address: 198.51.100.2}]",
                "bgp_peers[0].address",
            ),
            ("learn: [not-a-cidr]", "learn"),
            ("learn: [\"3fff::/64\"]", "learn"),
            ("announce: [{net: \"3fff::/64\"}]", "announce[0]"),
        ] {
            let text = format!("{}{yaml}\n", minimal_yaml());
            let err = sliplib_yaml::from_str::<RouterConfig>(&text)
                .unwrap_err()
                .to_string();
            assert!(err.starts_with(&format!("{key}: ")), "{yaml}: {err}");
        }
    }

    #[test]
    fn rejects_unknown_bypass_kind() {
        let mut cfg: RouterConfig = sliplib_yaml::from_str(minimal_yaml()).unwrap();
        cfg.bypass = Some(BypassConfig {
            refresh_interval_secs: default_bypass_refresh_interval_secs(),
            include: vec![BypassSourceEntry {
                kind: "bogus".to_string(),
                label: None,
                asns: None,
                prefixes: None,
                country: None,
                hostnames: None,
            }],
            exclude: vec![],
        });
        assert!(validate(&cfg).is_err());
    }

    #[test]
    fn rejects_as0() {
        let mut cfg: RouterConfig = sliplib_yaml::from_str(minimal_yaml()).unwrap();
        cfg.bgp_as = 0;
        assert!(validate(&cfg).is_err());
    }

    #[test]
    fn rejects_zero_bypass_refresh_interval() {
        let mut cfg: RouterConfig = sliplib_yaml::from_str(minimal_yaml()).unwrap();
        cfg.bypass = Some(BypassConfig {
            refresh_interval_secs: 0,
            include: vec![],
            exclude: vec![],
        });
        assert!(validate(&cfg).is_err());
    }

    #[test]
    fn rejects_literal_bypass_without_prefixes() {
        let mut cfg: RouterConfig = sliplib_yaml::from_str(minimal_yaml()).unwrap();
        cfg.bypass = Some(BypassConfig {
            refresh_interval_secs: default_bypass_refresh_interval_secs(),
            include: vec![BypassSourceEntry {
                kind: "literal".to_string(),
                label: None,
                asns: None,
                prefixes: None,
                country: None,
                hostnames: None,
            }],
            exclude: vec![],
        });
        assert!(validate(&cfg).is_err());
    }

    #[test]
    fn accepts_empty_ospf_interfaces_and_learn() {
        let cfg: RouterConfig = sliplib_yaml::from_str(minimal_yaml()).unwrap();
        assert!(cfg.ospf_interfaces.is_empty());
        assert!(cfg.learn.is_empty());
        validate(&cfg).unwrap();
    }

    fn pattern(text: &str) -> Result<InterfacePattern, InterfacePatternError> {
        InterfacePattern::try_from(text.to_owned())
    }

    #[test]
    fn a_pattern_with_a_star_matches_every_name_it_starts_and_a_name_only_itself() {
        let links = pattern("mesh-*").unwrap();
        assert!(links.matches("mesh-east"));
        assert!(links.matches("mesh-"));
        assert!(!links.matches("lan0"));
        assert_eq!(links.name(), None);
        let lan = pattern("lan0").unwrap();
        assert!(lan.matches("lan0"));
        assert!(!lan.matches("lan01"));
        assert_eq!(lan.name(), Some("lan0"));
    }

    #[test]
    fn a_pattern_takes_the_characters_of_a_name_and_a_star_at_the_end_only() {
        for (text, character) in [
            ("mesh-?", '?'),
            ("me*sh", '*'),
            ("**", '*'),
            ("192.0.2.0/24", '/'),
            ("a\"b", '"'),
            ("lan 0", ' '),
            ("lan\n0", '\n'),
            ("lan:0", ':'),
        ] {
            assert_eq!(
                pattern(text),
                Err(InterfacePatternError::Character {
                    pattern: text.to_owned(),
                    character
                }),
                "{text}"
            );
        }
        assert_eq!(pattern(""), Err(InterfacePatternError::Empty));
        for text in ["mesh-*", "*", "eth0.100", "bridge_lan"] {
            assert!(pattern(text).is_ok(), "{text}");
        }
    }

    #[test]
    fn the_router_loopback_is_known_by_its_entry_not_by_a_name_each_tool_spells() {
        assert!(InterfacePattern::router_loopback().is_router_loopback());
        assert_eq!(
            InterfacePattern::router_loopback().name(),
            Some(ROUTER_LOOPBACK)
        );
        assert!(pattern("router-lo").unwrap().is_router_loopback());
        assert!(!pattern("router-*").unwrap().is_router_loopback());
        assert!(!pattern("mesh-a").unwrap().is_router_loopback());
    }

    #[test]
    fn a_pattern_is_written_as_read() {
        let yaml = format!("{}ospf_interfaces: [mesh-*, router-lo]\n", minimal_yaml());
        let cfg: RouterConfig = sliplib_yaml::from_str(&yaml).unwrap();
        assert_eq!(
            sliplib_yaml::to_string(&cfg.ospf_interfaces).unwrap(),
            "- mesh-*\n- router-lo\n"
        );
    }

    #[test]
    fn bfd_settings_left_out_take_the_defaults() {
        let yaml = format!("{}bfd: {{}}\n", minimal_yaml());
        let cfg: RouterConfig = sliplib_yaml::from_str(&yaml).unwrap();
        assert_eq!(cfg.bfd, Some(BfdSettings::default()));
    }

    #[test]
    fn a_bypass_without_an_interval_is_resolved_again_daily() {
        let yaml = format!("{}bypass:\n  include: []\n", minimal_yaml());
        let cfg: RouterConfig = sliplib_yaml::from_str(&yaml).unwrap();
        assert_eq!(cfg.bypass.unwrap().refresh_interval_secs, 86_400);
    }
}
