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
    /// Set, the daemon tracks the peer's handshake; what it is sent is
    /// [`PeerEntry::allowed_ips_or_all`].
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

    type IsTheRefusal = fn(&Invalid) -> bool;

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
    fn an_interface_takes_an_mtu_of_at_least_1280_with_ipv6_and_of_68_without() {
        for (addresses, mtu, floor) in [
            (&["fe80::1/64", "198.51.100.193/32"][..], 1280, None),
            (&["fe80::1/64"], 1279, Some(1280)),
            (&["198.51.100.193/32"], 68, None),
            (&["198.51.100.193/32"], 67, Some(68)),
        ] {
            let checked = validate(&with_mtu(addresses, mtu));
            let Some(floor) = floor else {
                checked.unwrap_or_else(|refused| panic!("{addresses:?} {mtu}: {refused}"));
                continue;
            };
            let refused = checked.expect_err(&format!("{addresses:?} {mtu}"));
            assert!(
                matches!(&refused, Invalid::MtuTooSmall { interface, mtu: refused_mtu, floor: least }
                    if interface == "mesh-a" && *refused_mtu == mtu && *least == floor),
                "{addresses:?} {mtu}: {refused:?}"
            );
            let message = refused.to_string();
            assert!(
                message.contains("\"mesh-a\"") && message.contains(&floor.to_string()),
                "{message}"
            );
        }
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
    mtu: 1420
    peers:
      - public_key: "BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB="
        name: east
        endpoint: "192.0.2.7:51820"
        persistent_keepalive: 20-30
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
    fn parses_a_full_config_from_yaml() {
        let yaml = FULL_CONFIG;
        let cfg: AwgConfig = sliplib_yaml::from_str(yaml).unwrap();
        assert_eq!(cfg.interfaces.len(), 2);
        let dialed = &cfg.interfaces[0].peers[0];
        assert_eq!(dialed.name.as_deref(), Some("east"));
        assert_eq!(dialed.endpoint.as_deref(), Some("192.0.2.7:51820"));
        assert_eq!(
            dialed.persistent_keepalive,
            Some(Range::new(20, 30).unwrap())
        );
        assert_eq!(dialed.allowed_ips, None);
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
        let written = sliplib_yaml::to_string(&cfg).unwrap();
        assert_eq!(sliplib_yaml::from_str::<AwgConfig>(&written).unwrap(), cfg);
    }

    #[test]
    fn a_peer_with_nothing_optional_set_is_written_with_its_public_key_alone() {
        let written = sliplib_yaml::to_value(waited_for(dialed(None))).unwrap();
        let keys: Vec<&str> = written["interfaces"][0]["peers"][0]
            .as_mapping()
            .unwrap()
            .keys()
            .map(|key| key.as_str().unwrap())
            .collect();
        assert_eq!(keys, ["public_key"]);
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
    fn rejects_a_second_interface_of_one_name_and_a_second_peer_of_one_key() {
        let peer = |allowed: &str| PeerEntry {
            public_key: "k".to_string(),
            name: None,
            endpoint: None,
            allowed_ips: Some(vec![Prefix::try_from(allowed.to_owned()).unwrap()]),
            persistent_keepalive: None,
        };
        let mut a = iface("a");
        a.peers = vec![peer("203.0.113.1/32"), peer("203.0.113.2/32")];
        let cases: [(&str, Vec<InterfaceEntry>, IsTheRefusal); 2] = [
            (
                "two interfaces named dup",
                vec![iface("dup"), iface("dup")],
                |refused| matches!(refused, Invalid::DuplicateInterface(name) if name == "dup"),
            ),
            ("two peers of the key k", vec![a], |refused| {
                matches!(refused, Invalid::DuplicatePeer { interface, public_key }
                    if interface == "a" && public_key == "k")
            }),
        ];
        for (case, interfaces, is_the_refusal) in cases {
            let refused = validate(&AwgConfig { interfaces }).expect_err(case);
            assert!(is_the_refusal(&refused), "{case}: {refused:?}");
        }
    }

    #[test]
    fn a_name_fills_ifnamsiz_and_one_byte_more_is_refused() {
        let named = |name: &str| {
            validate(&AwgConfig {
                interfaces: vec![iface(name)],
            })
        };
        named("fifteen-bytes-1").unwrap();
        let refused = named("sixteen-bytes-16").unwrap_err();
        assert!(
            matches!(&refused, Invalid::InterfaceNameTooLong(name) if name == "sixteen-bytes-16"),
            "{refused:?}"
        );
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

    #[test]
    fn a_dialed_peer_is_sent_its_own_keepalive_or_its_interfaces_and_one_waited_for_none() {
        let (dialed_peer, waited) = (true, false);
        let wide = Range::new(20, 30).unwrap();
        for (dials, own, obfuscating, sent) in [
            (dialed_peer, None, None, Range::from(25)),
            (dialed_peer, None, Some("jc"), wide),
            (dialed_peer, None, Some("keepalive_timeout"), wide),
            (dialed_peer, None, Some("random_trailers"), wide),
            (dialed_peer, Some(Range::from(7)), None, Range::from(7)),
            (
                dialed_peer,
                Some(Range::from(7)),
                Some("jc"),
                Range::from(7),
            ),
            (dialed_peer, Some(Range::from(0)), None, Range::from(0)),
            (
                dialed_peer,
                Some(Range::from(0)),
                Some("jc"),
                Range::from(0),
            ),
            (waited, None, None, NO_KEEPALIVE),
            (waited, None, Some("jc"), NO_KEEPALIVE),
            (waited, Some(Range::from(7)), None, NO_KEEPALIVE),
            (waited, Some(Range::from(7)), Some("jc"), NO_KEEPALIVE),
            (waited, Some(wide), None, NO_KEEPALIVE),
            (waited, Some(wide), Some("jc"), NO_KEEPALIVE),
        ] {
            let case = format!("dials: {dials}, own: {own:?}, obfuscated by: {obfuscating:?}");
            let mut config = dialed(own);
            if let Some(field) = obfuscating {
                config = obfuscated_by(field, config);
            }
            assert_eq!(
                config.interfaces[0].is_plain(),
                obfuscating.is_none(),
                "{case}"
            );
            if !dials {
                config = waited_for(config);
            }
            assert_eq!(keepalive_of_the_peer(&config), sent, "{case}");
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
