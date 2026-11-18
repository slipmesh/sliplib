//! Config shape: a flat list of interfaces, each with any number of peers. There is no
//! discriminator between "mesh-style" and "roadwarriors-style" interfaces - the difference is
//! purely behavioral, driven by whether a given peer's `allowed_ips` is set (see `PeerEntry`'s own
//! doc comment).

use crate::{MetricsConfig, MetricsError, Obfuscation, Range};
use serde::{Deserialize, Serialize};
use sliplib_net::parse as parse_cidr;
use sliplib_secrets::wireguard::PrivateKey;
use std::collections::HashSet;

#[derive(Deserialize, Serialize, Debug, Default, PartialEq)]
pub struct AwgConfig {
    #[serde(default)]
    pub interfaces: Vec<InterfaceEntry>,
    /// Absent means "do not listen" - a node not set up for scraping must not open a port because
    /// a default said so. Rendered rather than hand-written, and safe to render ahead of the
    /// binary that reads it: nothing here denies unknown fields, so an older
    /// `awg` ignores the section instead of refusing the config.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metrics: Option<MetricsConfig>,
}

#[derive(Deserialize, Serialize, Debug, PartialEq)]
pub struct InterfaceEntry {
    /// Valid Linux interface name (`IFNAMSIZ` is 16 bytes including the NUL terminator, so 15
    /// usable bytes) - no other naming convention required: the daemon does not tell its own
    /// interfaces by a name prefix.
    pub name: String,
    pub listen_port: u16,
    /// CIDR strings, IPv4 and IPv6 freely mixed, any count of either.
    #[serde(default)]
    pub addresses: Vec<String>,
    /// Base64 X25519 private key. Always supplied here - the daemon never generates or persists
    /// a key itself; whoever renders the machine config is responsible for
    /// giving a node its own per-interface key, or the same key across every node's config when a
    /// single shared identity is needed (e.g. so roaming clients see one consistent server
    /// identity regardless of which node they connect to).
    pub private_key: PrivateKey,
    #[serde(default)]
    pub obfuscation: Obfuscation,
    /// Only meaningful for peers that have an explicit `allowed_ips` (see `PeerEntry`) - ignored
    /// entirely for full-tunnel peers, which are never handshake-tracked. Defaults to 180s if unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handshake_stale_secs: Option<u64>,
    /// The interface's MTU. Unset leaves it as it is: the kernel's default for a new interface. The
    /// generator sets one MTU on every mesh interface, low enough that a full-size packet with
    /// its WireGuard and junk overhead still fits the underlay unfragmented.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mtu: Option<u32>,
    #[serde(default)]
    pub peers: Vec<PeerEntry>,
}

#[derive(Deserialize, Serialize, Debug, PartialEq)]
pub struct PeerEntry {
    pub public_key: String,
    /// Human-readable name, carried for display only: it reaches the `peer_name` metric label and
    /// nothing else. Never used to decide behaviour - the public key is the identity, and
    /// `allowed_ips` is the only discriminator between kinds of peer. A base64 key is what a
    /// dashboard would otherwise have to show, which nobody can read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    /// `None` => this peer gets the full-tunnel default (`0.0.0.0/0` + `::/0`) as its AllowedIPs,
    /// its handshake is never polled, and no kernel route is ever installed for it - this is what
    /// used to be a dedicated "mesh" interface (connectivity comes from the underlay routing
    /// protocol reaching over the tunnel once it's up, not from a per-peer route).
    ///
    /// `Some(cidrs)` => exactly these CIDRs become the peer's AllowedIPs, its handshake is polled
    /// at 1Hz, and a kernel route is installed per CIDR while the handshake stays fresh - this is
    /// what used to be a dedicated "roadwarriors" interface. An interface can freely mix peers of
    /// both kinds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_ips: Option<Vec<String>>,
    /// Seconds without traffic after which this node sends the peer a packet, so that a NAT on
    /// the way keeps its mapping: a number, or a range the kernel module draws an interval from
    /// each time it sets the timer. It counts only for a peer with an `endpoint`, which this node
    /// dials; `0` sends none. Unset, it is the default of what the interface speaks, which
    /// [`InterfaceEntry::keepalive_of`] gives.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persistent_keepalive: Option<Range<u16>>,
}

pub const DEFAULT_HANDSHAKE_STALE_SECS: u64 = 180;

/// The keepalive of a dialed peer whose entry sets none, on an interface that speaks plain
/// WireGuard: the interval WireGuard's own tools suggest behind a NAT.
pub const PLAIN_KEEPALIVE: Range<u16> = Range::constant(25, 25);

/// The same on an interface that obfuscates: around that interval, drawn anew at every timer, so
/// that the keepalives are no clock to tell the tunnel by.
pub const OBFUSCATED_KEEPALIVE: Range<u16> = Range::constant(20, 30);

impl InterfaceEntry {
    /// Whether the interface speaks plain WireGuard: it sets nothing of the obfuscation.
    pub fn is_plain(&self) -> bool {
        self.obfuscation == Obfuscation::default()
    }

    /// The keepalive of `peer`, one of this interface's: its own, or the default of what the
    /// interface speaks. Every reader of the config takes it from here, and whoever writes a
    /// config leaves out a keepalive equal to it.
    pub fn keepalive_of(&self, peer: &PeerEntry) -> Range<u16> {
        peer.persistent_keepalive.unwrap_or(if self.is_plain() {
            PLAIN_KEEPALIVE
        } else {
            OBFUSCATED_KEEPALIVE
        })
    }
}

/// What a daemon would refuse an `awg` config for, or fail on once it is converging the node.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Invalid {
    #[error(transparent)]
    Metrics(#[from] MetricsError),
    #[error("interface name must not be empty")]
    EmptyInterfaceName,
    #[error("interface name {0:?} is longer than 15 bytes (IFNAMSIZ)")]
    InterfaceNameTooLong(String),
    #[error("duplicate interface name {0:?}")]
    DuplicateInterface(String),
    #[error("interface {interface:?}: invalid address {cidr:?}: {error}")]
    Address {
        interface: String,
        cidr: String,
        error: sliplib_net::ParseError,
    },
    #[error("interface {interface:?}: mtu {mtu} is below {floor}, the least its addresses allow")]
    MtuTooSmall {
        interface: String,
        mtu: u32,
        floor: u32,
    },
    #[error("interface {interface:?}: duplicate peer public_key {public_key:?}")]
    DuplicatePeer {
        interface: String,
        public_key: String,
    },
    #[error(
        "interface {interface:?}, peer {peer:?}: persistent_keepalive is {range}; a range of it must start at 1 or above, and 0 alone sends none"
    )]
    Keepalive {
        interface: String,
        /// The peer's public key.
        peer: String,
        range: Range<u16>,
    },
    #[error("interface {interface:?}, peer {peer:?}: invalid allowed_ips entry {cidr:?}: {error}")]
    AllowedIp {
        interface: String,
        /// The peer's public key.
        peer: String,
        cidr: String,
        error: sliplib_net::ParseError,
    },
}

/// Pure validation, no I/O: interface names are non-empty, fit `IFNAMSIZ`, and are unique;
/// every `addresses`/`allowed_ips` entry is a parseable CIDR (either family); a peer's keepalive
/// range starts at 1 or above; `metrics.listen`, when present, is a parseable IPv4 `ip:port`.
pub fn validate(cfg: &AwgConfig) -> Result<(), Invalid> {
    if let Some(metrics) = &cfg.metrics {
        metrics.validate("metrics.listen")?;
    }

    let mut seen_names = HashSet::new();
    for iface in &cfg.interfaces {
        if iface.name.is_empty() {
            return Err(Invalid::EmptyInterfaceName);
        }
        if iface.name.len() > 15 {
            return Err(Invalid::InterfaceNameTooLong(iface.name.clone()));
        }
        if !seen_names.insert(iface.name.as_str()) {
            return Err(Invalid::DuplicateInterface(iface.name.clone()));
        }
        let mut carries_ipv6 = false;
        for cidr in &iface.addresses {
            let net = parse_cidr(cidr).map_err(|error| Invalid::Address {
                interface: iface.name.clone(),
                cidr: cidr.clone(),
                error,
            })?;
            carries_ipv6 |= net.addr().is_ipv6();
        }
        if let Some(mtu) = iface.mtu {
            // The smallest MTU each family allows a link (RFC 8200, RFC 791); the kernel refuses
            // less, and only once the daemon is already converging the node.
            let floor = if carries_ipv6 { 1280 } else { 68 };
            if mtu < floor {
                return Err(Invalid::MtuTooSmall {
                    interface: iface.name.clone(),
                    mtu,
                    floor,
                });
            }
        }
        let mut seen_peer_keys = HashSet::new();
        for peer in &iface.peers {
            if !seen_peer_keys.insert(peer.public_key.as_str()) {
                return Err(Invalid::DuplicatePeer {
                    interface: iface.name.clone(),
                    public_key: peer.public_key.clone(),
                });
            }
            // The module draws the interval out of the range as written, and a draw of 0 sends
            // a packet at once and again at every re-arm.
            if let Some(range) = peer.persistent_keepalive
                && !range.is_zero()
                && range.lo() < 1
            {
                return Err(Invalid::Keepalive {
                    interface: iface.name.clone(),
                    peer: peer.public_key.clone(),
                    range,
                });
            }
            for cidr in peer.allowed_ips.iter().flatten() {
                parse_cidr(cidr).map_err(|error| Invalid::AllowedIp {
                    interface: iface.name.clone(),
                    peer: peer.public_key.clone(),
                    cidr: cidr.clone(),
                    error,
                })?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn iface(name: &str) -> InterfaceEntry {
        InterfaceEntry {
            name: name.to_string(),
            listen_port: 51820,
            addresses: vec![],
            private_key: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".into(),
            obfuscation: Obfuscation::default(),
            handshake_stale_secs: None,
            mtu: None,
            peers: vec![],
        }
    }

    fn with_mtu(addresses: &[&str], mtu: u32) -> AwgConfig {
        let mut iface = iface("mesh-a");
        iface.addresses = addresses.iter().map(|a| a.to_string()).collect();
        iface.mtu = Some(mtu);
        AwgConfig {
            interfaces: vec![iface],
            metrics: None,
        }
    }

    #[test]
    fn an_interface_carrying_ipv6_takes_an_mtu_of_at_least_1280() {
        validate(&with_mtu(&["fe80::1/64", "198.51.100.193/32"], 1280)).unwrap();
        let err = validate(&with_mtu(&["fe80::1/64"], 1279))
            .unwrap_err()
            .to_string();
        assert!(err.contains("\"mesh-a\"") && err.contains("1280"), "{err}");
    }

    #[test]
    fn an_ipv4_only_interface_takes_an_mtu_of_at_least_68() {
        validate(&with_mtu(&["198.51.100.193/32"], 68)).unwrap();
        let err = validate(&with_mtu(&["198.51.100.193/32"], 67))
            .unwrap_err()
            .to_string();
        assert!(err.contains("\"mesh-a\"") && err.contains("68"), "{err}");
    }

    #[test]
    fn an_mtu_is_read_when_given_and_left_out_when_not() {
        let yaml = "interfaces:\n  - name: mesh-a\n    listen_port: 51820\n    private_key: \
                    \"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=\"\n    mtu: 1420\n";
        let cfg: AwgConfig = sliplib_yaml::from_str(yaml).unwrap();
        assert_eq!(cfg.interfaces[0].mtu, Some(1420));
        let unset = AwgConfig {
            interfaces: vec![iface("mesh-a")],
            metrics: None,
        };
        assert!(!sliplib_yaml::to_string(&unset).unwrap().contains("mtu"));
    }

    const FULL_CONFIG: &str = r#"
interfaces:
  - name: mesh-a1b2c3d4
    listen_port: 51820
    addresses: ["fe80::a1b2:c3d4/64"]
    private_key: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="
    obfuscation: {jc: 4, jmin: 40, jmax: 70}
    peers:
      - public_key: "BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB="
        endpoint: "192.0.2.7:51820"
  - name: rw-north
    listen_port: 51900
    addresses: ["203.0.113.1/27", "3fff:99::1/64"]
    private_key: "CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC="
    obfuscation:
      s1: 50
      s2: 100
      s3: 60
      s4: 20
      i1: "5-10"
      header_protection_key: "EEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEE="
      content_padding_addition: 128
      rekey_after_time: 120
      max_handshake_attempts: 90
    peers:
      - public_key: "DDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDD="
        allowed_ips: ["203.0.113.5/32"]
        # A key of earlier releases: read, and ignored.
        advanced_security: true
"#;

    #[test]
    fn debug_output_never_carries_a_secret() {
        let cfg: AwgConfig = sliplib_yaml::from_str(FULL_CONFIG).unwrap();
        let printed = format!("{cfg:?}");
        for secret in [
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
            "CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC=",
            "EEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEE=",
        ] {
            assert!(!printed.contains(secret), "{printed}");
        }
    }

    #[test]
    fn parses_a_full_config_from_yaml() {
        let yaml = FULL_CONFIG;
        let cfg: AwgConfig = sliplib_yaml::from_str(yaml).unwrap();
        assert_eq!(cfg.interfaces.len(), 2);
        assert_eq!(cfg.interfaces[0].peers[0].allowed_ips, None);
        assert_eq!(
            cfg.interfaces[1].peers[0].allowed_ips,
            Some(vec!["203.0.113.5/32".to_string()])
        );
        assert_eq!(cfg.interfaces[1].obfuscation.s3, Some(60));
        assert_eq!(cfg.interfaces[1].obfuscation.i1, Some("5-10".to_string()));
        assert_eq!(
            cfg.interfaces[1].obfuscation.header_protection_key,
            Some("EEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEE=".into())
        );
        assert_eq!(
            cfg.interfaces[1].obfuscation.content_padding_addition,
            Some(128.into())
        );
        assert_eq!(
            cfg.interfaces[1].obfuscation.max_handshake_attempts,
            Some(90.into())
        );
        validate(&cfg).unwrap();
    }

    #[test]
    fn rejects_duplicate_interface_names() {
        let cfg = AwgConfig {
            interfaces: vec![iface("dup"), iface("dup")],
            ..Default::default()
        };
        assert!(validate(&cfg).is_err());
    }

    #[test]
    fn rejects_a_name_longer_than_ifnamsiz() {
        let cfg = AwgConfig {
            interfaces: vec![iface("this-name-is-way-too-long")],
            ..Default::default()
        };
        assert!(validate(&cfg).is_err());
    }

    #[test]
    fn accepts_a_name_that_fills_ifnamsiz() {
        let cfg = AwgConfig {
            interfaces: vec![iface("fifteen-bytes-1")],
            ..Default::default()
        };
        validate(&cfg).unwrap();
    }

    #[test]
    fn rejects_a_malformed_address_cidr() {
        let mut a = iface("a");
        a.addresses.push("not-a-cidr".to_string());
        let cfg = AwgConfig {
            interfaces: vec![a],
            ..Default::default()
        };
        assert!(validate(&cfg).is_err());
    }

    #[test]
    fn rejects_a_malformed_allowed_ips_entry() {
        let mut a = iface("a");
        a.peers.push(PeerEntry {
            public_key: "k".to_string(),
            name: None,
            endpoint: None,
            allowed_ips: Some(vec!["not-a-cidr".to_string()]),
            persistent_keepalive: None,
        });
        let cfg = AwgConfig {
            interfaces: vec![a],
            ..Default::default()
        };
        assert!(validate(&cfg).is_err());
    }

    #[test]
    fn rejects_duplicate_peer_public_key_within_one_interface() {
        let mut a = iface("a");
        a.peers.push(PeerEntry {
            public_key: "k".to_string(),
            name: None,
            endpoint: None,
            allowed_ips: Some(vec!["203.0.113.1/32".to_string()]),
            persistent_keepalive: None,
        });
        a.peers.push(PeerEntry {
            public_key: "k".to_string(),
            name: None,
            endpoint: None,
            allowed_ips: Some(vec!["203.0.113.2/32".to_string()]),
            persistent_keepalive: None,
        });
        let cfg = AwgConfig {
            interfaces: vec![a],
            ..Default::default()
        };
        assert!(validate(&cfg).is_err());
    }

    fn with_metrics(listen: &str) -> AwgConfig {
        AwgConfig {
            interfaces: vec![],
            metrics: Some(MetricsConfig {
                listen: listen.to_string(),
            }),
        }
    }

    #[test]
    fn accepts_a_v4_metrics_listen_address() {
        validate(&with_metrics("198.51.100.1:9586")).unwrap();
    }

    #[test]
    fn rejects_a_wildcard_metrics_listen_address() {
        // The whole point of the address is that it exists only inside the overlay. Binding
        // 0.0.0.0 would put the endpoint on every interface the node has, public ones included.
        assert!(validate(&with_metrics("0.0.0.0:9586")).is_err());
    }

    #[test]
    fn rejects_port_zero() {
        // The kernel would pick an ephemeral port, which nothing could then scrape. "Do not
        // listen" is spelled by omitting the section, not by asking for port 0.
        assert!(validate(&with_metrics("198.51.100.1:0")).is_err());
    }

    #[test]
    fn rejects_a_malformed_metrics_listen_address() {
        assert!(validate(&with_metrics("198.51.100.1")).is_err());
        assert!(validate(&with_metrics("not-an-address")).is_err());
    }

    #[test]
    fn rejects_a_v6_metrics_listen_address() {
        // The address Prometheus discovers is the node's InternalIP, which is the v4 loopback;
        // a v6 listener would be bound to something nothing scrapes.
        assert!(validate(&with_metrics("[3fff::1]:9586")).is_err());
    }

    #[test]
    fn a_config_without_a_metrics_section_parses_and_validates() {
        let cfg: AwgConfig = sliplib_yaml::from_str("interfaces: []").unwrap();
        assert_eq!(cfg.metrics, None);
        validate(&cfg).unwrap();
    }

    #[test]
    fn a_metrics_section_round_trips_through_yaml() {
        let cfg = with_metrics("198.51.100.1:9586");
        let yaml = sliplib_yaml::to_string(&cfg).unwrap();
        assert_eq!(sliplib_yaml::from_str::<AwgConfig>(&yaml).unwrap(), cfg);
    }

    #[test]
    fn accepts_an_empty_config() {
        assert!(validate(&AwgConfig::default()).is_ok());
    }

    fn dialed(persistent_keepalive: Option<Range<u16>>) -> AwgConfig {
        let mut a = iface("a");
        a.peers.push(PeerEntry {
            public_key: "k".to_string(),
            name: None,
            endpoint: Some("192.0.2.1:51820".to_string()),
            allowed_ips: None,
            persistent_keepalive,
        });
        AwgConfig {
            interfaces: vec![a],
            ..Default::default()
        }
    }

    #[test]
    fn a_peers_keepalive_is_written_as_a_number_or_a_range_and_left_out_when_unset() {
        for (keepalive, written) in [
            (Some(Range::from(25)), Some("persistent_keepalive: 25\n")),
            (
                Some(Range::new(20, 30).unwrap()),
                Some("persistent_keepalive: 20-30\n"),
            ),
            (None, None),
        ] {
            let cfg = dialed(keepalive);
            let yaml = sliplib_yaml::to_string(&cfg).unwrap();
            match written {
                Some(line) => assert!(yaml.contains(line), "{yaml}"),
                None => assert!(!yaml.contains("persistent_keepalive"), "{yaml}"),
            }
            assert_eq!(sliplib_yaml::from_str::<AwgConfig>(&yaml).unwrap(), cfg);
        }
    }

    #[test]
    fn a_peer_without_a_keepalive_takes_the_default_of_what_its_interface_speaks() {
        let plain = dialed(None);
        let peer = &plain.interfaces[0].peers[0];
        assert_eq!(plain.interfaces[0].keepalive_of(peer), Range::from(25));

        let mut obfuscated = dialed(None);
        obfuscated.interfaces[0].obfuscation.jc = Some(4);
        assert_eq!(
            obfuscated.interfaces[0].keepalive_of(peer),
            Range::new(20, 30).unwrap()
        );
    }

    #[test]
    fn a_peers_own_keepalive_stands_whatever_its_interface_speaks() {
        let mut cfg = dialed(Some(Range::from(7)));
        for jc in [None, Some(4)] {
            cfg.interfaces[0].obfuscation.jc = jc;
            let interface = &cfg.interfaces[0];
            assert_eq!(interface.keepalive_of(&interface.peers[0]), Range::from(7));
        }
    }

    #[test]
    fn a_keepalive_range_that_starts_at_zero_is_refused_naming_the_peer() {
        let err = validate(&dialed(Some(Range::new(0, 30).unwrap())))
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("interface \"a\", peer \"k\": persistent_keepalive is 0-30"),
            "{err}"
        );
    }

    #[test]
    fn a_keepalive_of_zero_alone_or_a_range_from_one_is_accepted() {
        validate(&dialed(Some(Range::from(0)))).unwrap();
        validate(&dialed(Some(Range::new(1, 30).unwrap()))).unwrap();
    }
}
