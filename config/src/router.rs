//! The `router` daemon's config, which the RouterOS tool reads too. Nothing is discovered.

use serde::{Deserialize, Serialize};
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

#[derive(Deserialize, Serialize, Debug, PartialEq)]
pub struct RouterConfig {
    pub node: NodeIdentity,
    pub bgp_as: u32,
    #[serde(default)]
    pub bgp_peers: Vec<BgpPeerEntry>,
    /// BFD on the OSPF links; the RouterOS tool reads it too.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bfd: Option<BfdSettings>,
    /// BIRD's `interface` clause: a name, a glob (`"mesh-*"`) or a CIDR of an interface's address.
    #[serde(default)]
    pub ospf_interfaces: Vec<String>,
    /// As `ospf_interfaces`: `protocol direct` sources, whose connected routes go out over iBGP as
    /// `router-lo`'s do.
    #[serde(default)]
    pub direct_interfaces: Vec<String>,
    /// IPv4 CIDRs: a kernel route at or inside one is announced over iBGP.
    #[serde(default)]
    pub learn: Vec<String>,
    #[serde(default)]
    pub announce: Vec<AnnounceEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bypass: Option<BypassConfig>,
}

#[derive(Deserialize, Serialize, Debug, PartialEq)]
pub struct NodeIdentity {
    /// One IPv4, BIRD's router id and `krt_prefsrc`, and one IPv6, the iBGP source; a prefix length
    /// left out is the host's.
    pub loopback_addresses: Vec<String>,
}

#[derive(Deserialize, Serialize, Debug, PartialEq)]
pub struct BgpPeerEntry {
    pub name: String,
    /// One bare IPv6 address, as BIRD's `neighbor` takes; RFC 8950 carries IPv4 over the session.
    pub address: String,
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
pub struct AnnounceEntry {
    pub net: String,
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

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LoopbackAddressError {
    #[error(transparent)]
    Cidr(#[from] sliplib_net::ParseError),
    #[error("{text:?} is not a valid IP address: {error}")]
    NotAnAddress {
        text: String,
        error: std::net::AddrParseError,
    },
}

/// `"<addr>/<prefix>"`, or a bare address as the host's `/32` or `/128`.
pub fn parse_loopback_address(s: &str) -> Result<(IpAddr, u8), LoopbackAddressError> {
    if s.contains('/') {
        let net = sliplib_net::parse(s)?;
        return Ok((net.addr(), net.prefix_len()));
    }
    let addr: IpAddr = s
        .parse()
        .map_err(|error| LoopbackAddressError::NotAnAddress {
            text: s.to_owned(),
            error,
        })?;
    let prefix = if addr.is_ipv4() { 32 } else { 128 };
    Ok((addr, prefix))
}

/// What the daemon would refuse, or fail on once running.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Invalid {
    #[error("bgp_as must not be 0 (reserved, RFC 7607)")]
    BgpAsZero,
    #[error("node.loopback_addresses: {0}")]
    LoopbackAddress(LoopbackAddressError),
    #[error("node.loopback_addresses must contain exactly one IPv4 address, found {0}")]
    Ipv4Loopbacks(usize),
    #[error("node.loopback_addresses must contain exactly one IPv6 address, found {0}")]
    Ipv6Loopbacks(usize),
    #[error("bfd: min_rx_ms and min_tx_ms must be greater than 0")]
    BfdIntervalZero,
    #[error("bfd: multiplier must be greater than 0")]
    BfdMultiplierZero,
    #[error("bgp_peers: name must not be empty")]
    EmptyPeerName,
    #[error("bgp_peers: duplicate name {0:?}")]
    DuplicatePeer(String),
    #[error("bgp_peers {name:?}: address {address:?} is not a valid IPv6 address: {error}")]
    PeerAddress {
        name: String,
        address: String,
        error: std::net::AddrParseError,
    },
    #[error("ospf_interfaces: entry must not be empty")]
    EmptyOspfInterface,
    #[error("learn: invalid IPv4 CIDR {cidr:?}: {error}")]
    Learn {
        cidr: String,
        error: sliplib_net::ParseError,
    },
    #[error("announce: invalid IPv4 CIDR {cidr:?}: {error}")]
    Announce {
        cidr: String,
        error: sliplib_net::ParseError,
    },
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
    let mut v4_count = 0;
    let mut v6_count = 0;
    for addr in &cfg.node.loopback_addresses {
        let (addr, _prefix) = parse_loopback_address(addr).map_err(Invalid::LoopbackAddress)?;
        match addr {
            IpAddr::V4(_) => v4_count += 1,
            IpAddr::V6(_) => v6_count += 1,
        }
    }
    if v4_count != 1 {
        return Err(Invalid::Ipv4Loopbacks(v4_count));
    }
    if v6_count != 1 {
        return Err(Invalid::Ipv6Loopbacks(v6_count));
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
        peer.address
            .parse::<Ipv6Addr>()
            .map_err(|error| Invalid::PeerAddress {
                name: peer.name.clone(),
                address: peer.address.clone(),
                error,
            })?;
    }

    if cfg.ospf_interfaces.iter().any(String::is_empty) {
        return Err(Invalid::EmptyOspfInterface);
    }

    for cidr in &cfg.learn {
        sliplib_net::parse_ipv4(cidr).map_err(|error| Invalid::Learn {
            cidr: cidr.clone(),
            error,
        })?;
    }

    for entry in &cfg.announce {
        sliplib_net::parse_ipv4(&entry.net).map_err(|error| Invalid::Announce {
            cidr: entry.net.clone(),
            error,
        })?;
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
    fn parse_loopback_address_defaults_prefix_by_family() {
        assert_eq!(
            parse_loopback_address("198.51.100.1").unwrap(),
            ("198.51.100.1".parse().unwrap(), 32)
        );
        assert_eq!(
            parse_loopback_address("3fff::1").unwrap(),
            ("3fff::1".parse().unwrap(), 128)
        );
    }

    #[test]
    fn parse_loopback_address_accepts_explicit_prefix() {
        assert_eq!(
            parse_loopback_address("198.51.100.1/25").unwrap(),
            ("198.51.100.1".parse().unwrap(), 25)
        );
    }

    #[test]
    fn rejects_missing_ipv4_loopback() {
        let mut cfg: RouterConfig = sliplib_yaml::from_str(minimal_yaml()).unwrap();
        cfg.node.loopback_addresses = vec!["3fff::1/128".to_string()];
        assert!(validate(&cfg).is_err());
    }

    #[test]
    fn rejects_missing_ipv6_loopback() {
        let mut cfg: RouterConfig = sliplib_yaml::from_str(minimal_yaml()).unwrap();
        cfg.node.loopback_addresses = vec!["198.51.100.1/32".to_string()];
        assert!(validate(&cfg).is_err());
    }

    #[test]
    fn rejects_duplicate_peer_names() {
        let mut cfg: RouterConfig = sliplib_yaml::from_str(minimal_yaml()).unwrap();
        cfg.bgp_peers = vec![
            BgpPeerEntry {
                name: "east".to_string(),
                address: "3fff::2".to_string(),
            },
            BgpPeerEntry {
                name: "east".to_string(),
                address: "3fff::3".to_string(),
            },
        ];
        assert!(validate(&cfg).is_err());
    }

    #[test]
    fn rejects_ipv4_peer_address() {
        let mut cfg: RouterConfig = sliplib_yaml::from_str(minimal_yaml()).unwrap();
        cfg.bgp_peers = vec![BgpPeerEntry {
            name: "east".to_string(),
            address: "198.51.100.2".to_string(),
        }];
        assert!(validate(&cfg).is_err());
    }

    #[test]
    fn rejects_malformed_learn_cidr() {
        let mut cfg: RouterConfig = sliplib_yaml::from_str(minimal_yaml()).unwrap();
        cfg.learn = vec!["not-a-cidr".to_string()];
        assert!(validate(&cfg).is_err());
    }

    #[test]
    fn rejects_malformed_announce_cidr() {
        let mut cfg: RouterConfig = sliplib_yaml::from_str(minimal_yaml()).unwrap();
        cfg.announce = vec![AnnounceEntry {
            net: "not-a-cidr".to_string(),
            label: None,
        }];
        assert!(validate(&cfg).is_err());
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
