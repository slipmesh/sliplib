//! What every platform's `awg` daemon shares of its config.

use crate::{Obfuscation, Prefix, Range};
use serde::{Deserialize, Serialize};
use sliplib_net::mtu::MIN_IPV6_MTU;
use sliplib_net::{Ipv4Net, Ipv6Net};
use sliplib_secrets::wireguard::PrivateKey;
use std::collections::HashSet;

#[derive(Deserialize, Serialize, Debug, Default, PartialEq)]
pub struct AwgConfig {
    #[serde(default)]
    pub interfaces: Vec<InterfaceEntry>,
}

#[derive(Deserialize, Serialize, Debug, PartialEq)]
pub struct InterfaceEntry {
    /// At most 15 bytes: `IFNAMSIZ` is 16 with the NUL.
    pub name: String,
    pub listen_port: u16,
    #[serde(default)]
    pub addresses: Vec<Prefix>,
    /// The daemon generates and keeps none.
    pub private_key: PrivateKey,
    #[serde(default)]
    pub obfuscation: Obfuscation,
    /// Unset leaves the kernel's default for a new interface.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mtu: Option<u32>,
    #[serde(default)]
    pub peers: Vec<PeerEntry>,
}

#[derive(Deserialize, Serialize, Debug, PartialEq)]
pub struct PeerEntry {
    pub public_key: String,
    /// For display only: the public key is the identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    /// What counts is [`PeerEntry::allowed_ips_or_all`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_ips: Option<Vec<Prefix>>,
    /// Seconds, or a range the module draws from at every timer; `0` sends none. What counts is
    /// [`InterfaceEntry::keepalive_of`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persistent_keepalive: Option<Range<u16>>,
}

impl PeerEntry {
    /// Its own, or `0.0.0.0/0` and `::/0`.
    pub fn allowed_ips_or_all(&self) -> Vec<Prefix> {
        self.allowed_ips.clone().unwrap_or_else(|| {
            vec![
                Prefix(Ipv4Net::default().into()),
                Prefix(Ipv6Net::default().into()),
            ]
        })
    }
}

/// Of a dialed peer that sets none, on a plain interface: what WireGuard's tools suggest behind
/// NAT.
pub const PLAIN_KEEPALIVE: Range<u16> = Range::constant(25, 25);

/// On an obfuscated interface: drawn anew at every timer, so they are no clock to tell it by.
pub const OBFUSCATED_KEEPALIVE: Range<u16> = Range::constant(20, 30);

pub const NO_KEEPALIVE: Range<u16> = Range::constant(0, 0);

impl InterfaceEntry {
    /// No field of the obfuscation set.
    pub fn is_plain(&self) -> bool {
        self.obfuscation == Obfuscation::default()
    }

    /// None for a peer without an `endpoint`; else its own, or [`PLAIN_KEEPALIVE`] or
    /// [`OBFUSCATED_KEEPALIVE`].
    pub fn keepalive_of(&self, peer: &PeerEntry) -> Range<u16> {
        if peer.endpoint.is_none() {
            return NO_KEEPALIVE;
        }
        peer.persistent_keepalive.unwrap_or(if self.is_plain() {
            PLAIN_KEEPALIVE
        } else {
            OBFUSCATED_KEEPALIVE
        })
    }
}

/// What the daemon would refuse, or fail on while converging the node.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Invalid {
    #[error("interface name must not be empty")]
    EmptyInterfaceName,
    #[error("interface name {0:?} is longer than 15 bytes (IFNAMSIZ)")]
    InterfaceNameTooLong(String),
    #[error("duplicate interface name {0:?}")]
    DuplicateInterface(String),
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
}

pub fn validate(cfg: &AwgConfig) -> Result<(), Invalid> {
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
        let carries_ipv6 = iface
            .addresses
            .iter()
            .any(|prefix| prefix.0.addr().is_ipv6());
        if let Some(mtu) = iface.mtu {
            // RFC 8200, RFC 791: the kernel refuses less, once the daemon is converging.
            let floor = if carries_ipv6 { MIN_IPV6_MTU } else { 68 };
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
            // The module draws as written: 0 sends at once and at every re-arm.
            if let Some(range) = peer.persistent_keepalive
                && range.starts_at_zero()
            {
                return Err(Invalid::Keepalive {
                    interface: iface.name.clone(),
                    peer: peer.public_key.clone(),
                    range,
                });
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
            mtu: None,
            peers: vec![],
        }
    }

    fn with_mtu(addresses: &[&str], mtu: u32) -> AwgConfig {
        let mut iface = iface("mesh-a");
        iface.addresses = addresses
            .iter()
            .map(|a| Prefix::try_from(a.to_string()).unwrap())
            .collect();
        iface.mtu = Some(mtu);
        AwgConfig {
            interfaces: vec![iface],
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
            Some(vec![Prefix::try_from("203.0.113.5/32".to_owned()).unwrap()])
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
    fn an_address_or_allowed_ips_entry_that_is_not_a_cidr_is_refused_at_its_key() {
        for (field, key) in [
            ("addresses: [not-a-cidr]", "interfaces[0].addresses"),
            ("addresses: [198.51.100.1]", "interfaces[0].addresses"),
            (
                "peers: [{public_key: k, allowed_ips: [not-a-cidr]}]",
                "interfaces[0].peers[0].allowed_ips",
            ),
        ] {
            let yaml = format!(
                "interfaces:\n  - name: mesh-a\n    listen_port: 51820\n    private_key: \
                 \"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=\"\n    {field}\n"
            );
            let err = sliplib_yaml::from_str::<AwgConfig>(&yaml)
                .unwrap_err()
                .to_string();
            assert!(err.starts_with(&format!("{key}: ")), "{field}: {err}");
        }
    }

    #[test]
    fn a_peer_without_allowed_ips_takes_every_address_and_one_with_them_its_own() {
        let mut peer = PeerEntry {
            public_key: "k".to_owned(),
            name: None,
            endpoint: None,
            allowed_ips: None,
            persistent_keepalive: None,
        };
        let text = |list: Vec<Prefix>| list.iter().map(|p| p.0.to_string()).collect::<Vec<_>>();
        assert_eq!(text(peer.allowed_ips_or_all()), ["0.0.0.0/0", "::/0"]);
        peer.allowed_ips = Some(vec![Prefix::try_from("203.0.113.1/32".to_owned()).unwrap()]);
        assert_eq!(text(peer.allowed_ips_or_all()), ["203.0.113.1/32"]);
    }

    #[test]
    fn rejects_duplicate_interface_names() {
        let cfg = AwgConfig {
            interfaces: vec![iface("dup"), iface("dup")],
        };
        assert!(validate(&cfg).is_err());
    }

    #[test]
    fn rejects_a_name_longer_than_ifnamsiz() {
        let cfg = AwgConfig {
            interfaces: vec![iface("this-name-is-way-too-long")],
        };
        assert!(validate(&cfg).is_err());
    }

    #[test]
    fn accepts_a_name_that_fills_ifnamsiz() {
        let cfg = AwgConfig {
            interfaces: vec![iface("fifteen-bytes-1")],
        };
        validate(&cfg).unwrap();
    }

    #[test]
    fn rejects_duplicate_peer_public_key_within_one_interface() {
        let mut a = iface("a");
        a.peers.push(PeerEntry {
            public_key: "k".to_string(),
            name: None,
            endpoint: None,
            allowed_ips: Some(vec![Prefix::try_from("203.0.113.1/32".to_owned()).unwrap()]),
            persistent_keepalive: None,
        });
        a.peers.push(PeerEntry {
            public_key: "k".to_string(),
            name: None,
            endpoint: None,
            allowed_ips: Some(vec![Prefix::try_from("203.0.113.2/32".to_owned()).unwrap()]),
            persistent_keepalive: None,
        });
        let cfg = AwgConfig {
            interfaces: vec![a],
        };
        assert!(validate(&cfg).is_err());
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

    /// `config`, whose one peer is dialed, with that peer only waited for.
    fn waited_for(mut config: AwgConfig) -> AwgConfig {
        config.interfaces[0].peers[0].endpoint = None;
        config
    }

    fn keepalive_of_the_peer(config: &AwgConfig) -> Range<u16> {
        let interface = &config.interfaces[0];
        interface.keepalive_of(&interface.peers[0])
    }

    /// An interface is obfuscated by any field of its obfuscation, and by none in particular.
    fn obfuscated_by(field: &str, mut config: AwgConfig) -> AwgConfig {
        let obfuscation = &mut config.interfaces[0].obfuscation;
        match field {
            "jc" => obfuscation.jc = Some(4),
            "keepalive_timeout" => obfuscation.keepalive_timeout = Some(12.into()),
            "random_trailers" => obfuscation.random_trailers = Some(false),
            other => panic!("{other}"),
        }
        config
    }

    const OBFUSCATING_FIELDS: [&str; 3] = ["jc", "keepalive_timeout", "random_trailers"];

    #[test]
    fn a_dialed_peer_without_a_keepalive_takes_the_default_of_its_interface() {
        assert_eq!(keepalive_of_the_peer(&dialed(None)), Range::from(25));
        for field in OBFUSCATING_FIELDS {
            let obfuscated = obfuscated_by(field, dialed(None));
            assert!(!obfuscated.interfaces[0].is_plain(), "{field}");
            assert_eq!(
                keepalive_of_the_peer(&obfuscated),
                Range::new(20, 30).unwrap(),
                "{field}"
            );
        }
        assert!(dialed(None).interfaces[0].is_plain());
    }

    #[test]
    fn a_dialed_peers_own_keepalive_stands_whatever_its_interface_sets() {
        for own in [7, 0] {
            let plain = dialed(Some(Range::from(own)));
            assert_eq!(keepalive_of_the_peer(&plain), Range::from(own));
            let obfuscated = obfuscated_by("jc", dialed(Some(Range::from(own))));
            assert_eq!(keepalive_of_the_peer(&obfuscated), Range::from(own));
        }
    }

    #[test]
    fn a_peer_that_is_only_waited_for_is_sent_none_whatever_its_entry_sets() {
        for own in [
            None,
            Some(Range::from(7)),
            Some(Range::new(20, 30).unwrap()),
        ] {
            let plain = waited_for(dialed(own));
            assert_eq!(keepalive_of_the_peer(&plain), NO_KEEPALIVE, "{own:?}");
            let obfuscated = waited_for(obfuscated_by("jc", dialed(own)));
            assert_eq!(keepalive_of_the_peer(&obfuscated), NO_KEEPALIVE, "{own:?}");
        }
        assert_eq!(NO_KEEPALIVE, Range::from(0));
    }

    fn keepalive(written: &str) -> Option<Range<u16>> {
        Some(written.parse().unwrap())
    }

    #[test]
    fn a_keepalive_range_that_starts_at_zero_is_refused_naming_the_peer() {
        for config in [
            dialed(keepalive("0-1")),
            waited_for(dialed(keepalive("0-30"))),
        ] {
            let refused = validate(&config).unwrap_err();
            let Invalid::Keepalive {
                interface,
                peer,
                range,
            } = &refused
            else {
                panic!("{refused}");
            };
            assert_eq!((interface.as_str(), peer.as_str()), ("a", "k"));
            assert_eq!(
                Some(*range),
                config.interfaces[0].peers[0].persistent_keepalive
            );
        }
    }

    #[test]
    fn a_keepalive_of_zero_alone_or_a_range_from_one_is_accepted() {
        for written in ["0", "0-0", "1-5", "1-30"] {
            validate(&dialed(keepalive(written))).unwrap();
        }
    }
}
