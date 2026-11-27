//! A node's `awg` and `router` configs.

use crate::addressing;
use crate::mtu::MtuError;
use crate::network::{Mesh, PRIVATE_KEY, UnknownNode};
use crate::secrets::{MissingSecret, unminted};
use sliplib_config::awg::{AwgConfig, InterfaceEntry, PeerEntry};
use sliplib_config::router::{
    AnnounceEntry, BfdSettings, BgpPeerEntry, InterfacePattern, Loopbacks, NodeIdentity,
    RouterConfig,
};
use sliplib_config::v2::Name;
use sliplib_config::v2::network::{LinkKind, Node};
use sliplib_config::{Obfuscation, ObfuscationError, Prefix, Prefix4, Prefix6, Range};
use sliplib_net::Ipv6Net;
use sliplib_secrets::wireguard::KeyError;
use std::net::IpAddr;

/// Each names the node and which config; the cause is its source.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("rendering awg config for node {node:?}")]
    RenderingAwg {
        node: String,
        #[source]
        source: RenderError,
    },
    #[error("rendered awg config for node {node:?} is invalid")]
    InvalidAwg {
        node: String,
        #[source]
        source: sliplib_config::awg::Invalid,
    },
    #[error("node {node:?}, interface {interface:?}: the kernel module refuses this obfuscation")]
    Obfuscation {
        node: String,
        interface: String,
        #[source]
        source: ObfuscationError,
    },
    #[error("rendered router config for node {node:?} is invalid")]
    InvalidRouter {
        node: String,
        #[source]
        source: sliplib_config::router::Invalid,
    },
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RenderError {
    #[error(transparent)]
    MissingSecret(#[from] MissingSecret),
    #[error(transparent)]
    UnknownNode(#[from] UnknownNode),
    #[error(transparent)]
    Mtu(#[from] MtuError),
    #[error("node {node:?}: {PRIVATE_KEY}")]
    Key {
        node: String,
        #[source]
        source: KeyError,
    },
}

type Result<T> = std::result::Result<T, RenderError>;

/// Every other node, at its IPv6 loopback.
pub(crate) fn bgp_peers_for(mesh: &Mesh<'_>, node_name: &str) -> Vec<BgpPeerEntry> {
    mesh.network()
        .nodes
        .iter()
        .filter(|(name, _)| name.as_str() != node_name)
        .map(|(name, node)| BgpPeerEntry {
            name: name.to_string(),
            address: mesh.addresses(node).loopback.1,
        })
        .collect()
}

fn ospf_interfaces(mesh: &Mesh<'_>) -> Vec<InterfacePattern> {
    vec![mesh.links_pattern(), InterfacePattern::router_loopback()]
}

pub(crate) fn render_router_config(mesh: &Mesh<'_>, name: &str, node: &Node) -> RouterConfig {
    let (v4, v6) = mesh.addresses(node).loopback;
    let routing = &mesh.network().routing;
    RouterConfig {
        node: NodeIdentity {
            loopback_addresses: Loopbacks {
                ipv4: Prefix4(v4.into()),
                ipv6: Prefix6(v6.into()),
            },
        },
        bgp_as: routing.bgp.number.get(),
        bgp_peers: bgp_peers_for(mesh, name),
        bfd: routing.bfd.as_ref().map(|bfd| BfdSettings {
            min_rx_ms: bfd.min_rx_ms.get(),
            min_tx_ms: bfd.min_tx_ms.get(),
            multiplier: bfd.multiplier.get(),
        }),
        ospf_interfaces: ospf_interfaces(mesh),
        direct_interfaces: mesh.direct_interfaces(node),
        learn: node.routing.learn.clone(),
        announce: node
            .routing
            .announce
            .iter()
            .map(|&net| AnnounceEntry { net, label: None })
            .collect(),
        bypass: None,
    }
}

fn key_of(node: &str) -> impl Fn(KeyError) -> RenderError {
    move |source| RenderError::Key {
        node: node.to_owned(),
        source,
    }
}

/// One per link, named after the peer: peers know each other by key.
fn mesh_interfaces_for(
    mesh: &Mesh<'_>,
    node_name: &str,
    node: &Node,
) -> Result<Vec<InterfaceEntry>> {
    let own_private_key = node.private_key();
    if own_private_key.is_none() && mesh.needs_a_key(node_name) {
        return Err(MissingSecret(format!("node {node_name:?}: {PRIVATE_KEY} is not set")).into());
    }
    // Decoded whether or not a link uses it: else the daemon would refuse it while running.
    if let Some(own_private_key) = own_private_key {
        own_private_key.public_key().map_err(key_of(node_name))?;
    }

    // Before the links: what refuses the mesh MTU stops a node on no link too.
    let mtu = crate::mtu::mesh_mtu(mesh)?;
    // Only a node on no link has none.
    let Some(own_private_key) = own_private_key else {
        return Ok(Vec::new());
    };
    let own = mesh.addresses(node);
    let mut out = Vec::new();
    for (link_name, link) in &mesh.network().mesh.links {
        let Some(peer_name) = link.other_end(node_name).map(Name::as_str) else {
            continue;
        };
        let peer_node = mesh.node(peer_name)?;
        let peer_private_key = peer_node.private_key().ok_or_else(|| {
            MissingSecret(format!("node {peer_name:?}: {PRIVATE_KEY} is not set"))
        })?;
        let obfuscation = minted_obfuscation(
            &link.kind.obfuscation(),
            matches!(link.kind, LinkKind::Wireguard(_)),
            &format!("mesh link {link_name}"),
        )?;
        let port = link.kind.port().get();

        // 64 is a prefix length an IPv6 address takes, so the host's is never the one used.
        let link_local = Ipv6Net::new(own.link_local, 64).unwrap_or(own.link_local.into());
        let mut addresses = vec![Prefix(link_local.into())];
        if let Some(tunnel_v4) = own.tunnel_ipv4 {
            addresses.push(Prefix(IpAddr::V4(tunnel_v4).into()));
        }

        let mut interface = InterfaceEntry {
            name: format!("{}{peer_name}", mesh.interface_prefix()),
            listen_port: port,
            addresses,
            private_key: own_private_key.clone(),
            obfuscation,
            mtu,
            peers: vec![PeerEntry {
                public_key: peer_private_key.public_key().map_err(key_of(peer_name))?,
                name: Some(peer_name.to_owned()),
                endpoint: peer_node
                    .underlay()
                    .and_then(|underlay| underlay.endpoint.as_ref())
                    .map(|endpoint| addressing::endpoint(endpoint.host(), port)),
                allowed_ips: None,
                persistent_keepalive: link.kind.persistent_keepalive(),
            }],
        };
        interface.peers[0].persistent_keepalive =
            resolved_keepalive(&interface, &interface.peers[0]);
        out.push(interface);
    }
    Ok(out)
}

/// What the daemon sends `peer` of `interface`, written out; none to a peer it does not dial.
fn resolved_keepalive(interface: &InterfaceEntry, peer: &PeerEntry) -> Option<Range<u16>> {
    peer.endpoint
        .is_some()
        .then(|| interface.keepalive_of(peer))
}

pub(crate) fn render_awg_config(
    mesh: &Mesh<'_>,
    node_name: &str,
    node: &Node,
) -> Result<AwgConfig> {
    Ok(AwgConfig {
        interfaces: mesh_interfaces_for(mesh, node_name, node)?,
    })
}

#[derive(Debug)]
pub struct NodeConfigs {
    pub awg: AwgConfig,
    pub router: RouterConfig,
}

/// Each checked as its node would.
pub fn node_configs(mesh: &Mesh<'_>, host: &str) -> std::result::Result<NodeConfigs, Error> {
    let node = || host.to_owned();
    // As the `awg` config, computed first, would refuse it.
    let found = mesh.node(host).map_err(|source| Error::RenderingAwg {
        node: node(),
        source: source.into(),
    })?;
    let awg_cfg = render_awg_config(mesh, host, found).map_err(|source| Error::RenderingAwg {
        node: node(),
        source,
    })?;
    sliplib_config::awg::validate(&awg_cfg).map_err(|source| Error::InvalidAwg {
        node: node(),
        source,
    })?;
    // Here: a refusal in the running daemon would leave every interface down.
    for iface in &awg_cfg.interfaces {
        iface
            .obfuscation
            .validate()
            .map_err(|source| Error::Obfuscation {
                node: node(),
                interface: iface.name.clone(),
                source,
            })?;
    }
    let router_cfg = render_router_config(mesh, host, found);
    sliplib_config::router::validate(&router_cfg).map_err(|source| Error::InvalidRouter {
        node: node(),
        source,
    })?;
    Ok(NodeConfigs {
        awg: awg_cfg,
        router: router_cfg,
    })
}

/// Refuses one lacking a minted field, unless `plain`; else gives it with what the daemon sends
/// for each value left unset.
fn minted_obfuscation(
    obfuscation: &Obfuscation,
    plain: bool,
    what: &str,
) -> std::result::Result<Obfuscation, MissingSecret> {
    let unminted = if plain {
        Vec::new()
    } else {
        unminted(obfuscation)
    };
    if !unminted.is_empty() {
        return Err(MissingSecret(format!(
            "{what}: obfuscation lacks {}",
            unminted.join(", ")
        )));
    }
    Ok(if plain {
        obfuscation.clone()
    } else {
        as_sent(obfuscation.clone())
    })
}

/// `0`, the module's "not set", and `false` for a switch. The decoys and the header protection key
/// are sent only when set, and the minted fields are always set.
fn as_sent(obfuscation: Obfuscation) -> Obfuscation {
    let zero = || Some(Range::from(0));
    Obfuscation {
        s3: obfuscation.s3.or(Some(0)),
        s4: obfuscation.s4.or(Some(0)),
        content_padding_addition: obfuscation.content_padding_addition.or_else(zero),
        rekey_after_time: obfuscation.rekey_after_time.or_else(zero),
        rekey_timeout: obfuscation.rekey_timeout.or_else(zero),
        reject_after_time: obfuscation.reject_after_time.or_else(zero),
        keepalive_timeout: obfuscation.keepalive_timeout.or_else(zero),
        max_handshake_attempts: obfuscation.max_handshake_attempts.or_else(zero),
        random_trailers: obfuscation.random_trailers.or(Some(false)),
        disable_cookies: obfuscation.disable_cookies.or(Some(false)),
        ..obfuscation
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sliplib_config::Range;

    use crate::test_support::{NODE_D, example};

    fn missing_secret(refusal: &Error) -> Option<&MissingSecret> {
        match refusal {
            Error::RenderingAwg {
                source: RenderError::MissingSecret(missing),
                ..
            } => Some(missing),
            _ => None,
        }
    }

    fn configs_of(network: &Mesh<'_>, node: &str) -> std::result::Result<NodeConfigs, Error> {
        super::node_configs(network, node)
    }

    #[test]
    fn an_unknown_node_gets_no_configs() {
        let refusal = configs_of(&example("{}"), "node-x").unwrap_err();
        assert!(
            matches!(&refusal, Error::RenderingAwg { node, .. } if node == "node-x"),
            "{refusal:?}"
        );
    }

    #[test]
    fn a_node_peers_over_bgp_with_every_other_node_at_its_ipv6_loopback() {
        let network = example("{}");
        let peers = super::bgp_peers_for(&network, "node-b");
        let peers: Vec<(&str, String)> = peers
            .iter()
            .map(|peer| (peer.name.as_str(), peer.address.to_string()))
            .collect();
        assert_eq!(
            peers,
            [
                ("node-a", "3fff:0:0:1::1".to_owned()),
                ("node-c", "3fff:0:0:1::3".to_owned()),
                ("router1", "3fff:0:0:1::4".to_owned()),
            ]
        );
    }

    #[test]
    fn a_peer_dialed_over_ipv6_is_dialed_in_brackets() {
        // Both ends of a link are dialed over one family.
        let network = example(
            "{nodes: {node-a: {underlay: {wan: {endpoint: {ipv4: null, ipv6: '2001:db8::1'}}}}, \
             node-b: {underlay: {wan: {endpoint: {ipv4: null, ipv6: '2001:db8::11'}}}}, \
             router1: {underlay: {wan: {endpoint: {ipv4: null, ipv6: '2001:db8::4'}}}}}}",
        );
        let configs = configs_of(&network, "node-a").unwrap();
        let interface = configs
            .awg
            .interfaces
            .iter()
            .find(|interface| interface.name == "mesh-node-b")
            .unwrap();
        assert_eq!(
            interface.peers[0].endpoint.as_deref(),
            Some("[2001:db8::11]:51830")
        );
    }

    #[test]
    fn the_interface_prefix_names_the_interfaces_and_what_ospf_runs_on() {
        let configs = configs_of(&example("{mesh: {interface_prefix: wg-}}"), "node-a").unwrap();
        let names: Vec<&str> = configs
            .awg
            .interfaces
            .iter()
            .map(|interface| interface.name.as_str())
            .collect();
        assert_eq!(names, ["wg-node-b", "wg-node-c", "wg-router1"]);
        let ospf: Vec<&str> = configs
            .router
            .ospf_interfaces
            .iter()
            .map(InterfacePattern::as_str)
            .collect();
        assert_eq!(ospf, ["wg-*", "router-lo"]);
    }

    #[test]
    fn a_nodes_configs_are_its_rendered_awg_and_router_configs() {
        let network = example("{}");
        let configs = super::node_configs(&network, "node-a").unwrap();
        assert_eq!(
            configs.awg,
            super::render_awg_config(&network, "node-a", network.node("node-a").unwrap()).unwrap()
        );
        assert_eq!(
            configs.router,
            super::render_router_config(&network, "node-a", network.node("node-a").unwrap())
        );
    }

    #[test]
    fn a_node_whose_own_key_or_a_peers_is_not_written_is_refused_for_a_missing_secret() {
        let network = example("{nodes: {node-b: {wireguard: null}}}");
        for node in ["node-b", "node-a"] {
            let refusal = configs_of(&network, node).unwrap_err();
            assert!(
                matches!(&refusal, Error::RenderingAwg { node: refused, .. } if refused == node),
                "{node}: {refusal:?}"
            );
            let missing = missing_secret(&refusal).map(ToString::to_string);
            assert!(
                missing.is_some_and(|missing| missing.contains("node-b")
                    && missing.contains("wireguard.private_key")),
                "{node}: {refusal:?}"
            );
        }
        configs_of(&network, "node-c").unwrap();
    }

    #[test]
    fn a_key_that_does_not_decode_refuses_its_node_and_its_peers_and_no_secret_cures_it() {
        let network = example("{nodes: {node-b: {wireguard: {private_key: not a key}}}}");
        for node in ["node-b", "node-a"] {
            assert_not_a_key(configs_of(&network, node).unwrap_err(), "node-b");
        }
        configs_of(&network, "node-c").unwrap();
    }

    /// `owner`'s key does not decode, and no secret cures it.
    fn assert_not_a_key(refusal: Error, owner: &str) {
        assert!(
            matches!(&refusal, Error::RenderingAwg { source: RenderError::Key { node, .. }, .. }
                if node == owner),
            "{refusal:?}"
        );
        assert!(missing_secret(&refusal).is_none(), "{refusal:?}");
        let cause = sliplib_secrets::wireguard::decode_key("not a key").unwrap_err();
        let chain = format!("{:#}", anyhow::Error::new(refusal));
        assert!(chain.contains(&cause.to_string()), "{chain}");
    }

    // Every node's own key is decoded, whether or not a link uses it.
    #[test]
    fn a_key_that_does_not_decode_refuses_a_node_on_no_link_too() {
        let network = example(
            "{nodes: {node-d: {platform: talos, node_id: 0.0.0.5, underlay: {wan: {}}, \
             wireguard: {private_key: not a key}}}}",
        );
        assert_not_a_key(configs_of(&network, "node-d").unwrap_err(), "node-d");
        configs_of(&network, "node-a").unwrap();
    }

    #[test]
    fn an_obfuscation_the_kernel_module_refuses_refuses_both_ends_and_no_secret_cures_it() {
        let network = example("{mesh: {links: {a-b: {amneziawg: {obfuscation: {h2: 1101}}}}}}");
        for (node, interface) in [("node-a", "mesh-node-b"), ("node-b", "mesh-node-a")] {
            let refusal = configs_of(&network, node).unwrap_err();
            assert!(
                matches!(&refusal, Error::Obfuscation { node: refused, interface: on, .. }
                    if refused == node && on == interface),
                "{node}: {refusal:?}"
            );
            assert!(missing_secret(&refusal).is_none(), "{node}");
        }
    }

    #[test]
    fn a_secret_of_a_node_or_link_the_rendered_node_does_not_touch_is_not_refused() {
        let network = example(
            "{nodes: {node-c: {wireguard: null}}, \
             mesh: {links: {a-c: {amneziawg: {obfuscation: null}}}}}",
        );
        configs_of(&network, "node-b").unwrap();
        configs_of(&network, "router1").unwrap();
        // The two ends of `a-c` need what the overlay took.
        for node in ["node-a", "node-c"] {
            let refusal = configs_of(&network, node).unwrap_err();
            assert!(missing_secret(&refusal).is_some(), "{node}: {refusal:?}");
        }
    }

    #[test]
    fn a_node_on_no_link_is_configured_without_a_key() {
        let network = example(&format!("{{nodes: {{node-d: {NODE_D}}}}}"));
        let configs = node_configs(&network, "node-d").unwrap();
        assert!(configs.awg.interfaces.is_empty());
        assert_eq!(configs.router.bgp_peers.len(), 4);
    }

    /// With a keyed node of `name` on a link to `node-a`.
    fn with_a_node_named(name: &str) -> Mesh<'static> {
        let key = sliplib_secrets::wireguard::PrivateKey::generate();
        example(&format!(
            "{{nodes: {{{name}: {{platform: talos, node_id: 0.0.0.5, underlay: {{wan: {{}}}}, \
             wireguard: {{private_key: {}}}}}}}, mesh: {{links: {{a-x: {{type: wireguard, \
             ends: {{node-a: {{}}, {name}: {{}}}}, wireguard: {{port: 51840}}}}}}}}}}",
            key.expose()
        ))
    }

    #[test]
    fn a_name_too_long_for_an_interface_is_refused_at_the_other_end_of_its_link() {
        use sliplib_config::awg::Invalid;
        let fits = with_a_node_named("node-abcde");
        node_configs(&fits, "node-a").unwrap();
        let long = with_a_node_named("node-abcdef");
        node_configs(&long, "node-abcdef").unwrap();
        let refusal = node_configs(&long, "node-a").unwrap_err();
        assert!(
            matches!(&refusal, crate::render::Error::InvalidAwg { node, source: Invalid::InterfaceNameTooLong(name) }
                if node == "node-a" && name == "mesh-node-abcdef"),
            "{refusal:?}"
        );
    }

    fn mesh_interface(network: &Mesh<'_>, node: &str, peer: &str) -> InterfaceEntry {
        node_configs(network, node)
            .unwrap()
            .awg
            .interfaces
            .into_iter()
            .find(|interface| interface.name == format!("mesh-{peer}"))
            .unwrap()
    }

    fn peer_keepalive(network: &Mesh<'_>, node: &str, peer: &str) -> Option<Range<u16>> {
        mesh_interface(network, node, peer).peers[0].persistent_keepalive
    }

    #[test]
    fn a_dialed_peer_gets_the_keepalive_of_its_link_and_one_nobody_dials_none() {
        let network = example("{}");
        // `node-c` dials `node-a`; nobody dials `node-c`, which has no endpoint.
        assert_eq!(
            peer_keepalive(&network, "node-c", "node-a"),
            Some(Range::new(15, 20).unwrap())
        );
        assert_eq!(peer_keepalive(&network, "node-a", "node-c"), None);
    }

    #[test]
    fn a_dialed_peer_on_a_link_that_sets_none_gets_what_the_daemon_gives_its_type() {
        use sliplib_config::awg::{OBFUSCATED_KEEPALIVE, PLAIN_KEEPALIVE};
        let network = example("{}");
        assert_eq!(
            peer_keepalive(&network, "node-a", "node-b"),
            Some(OBFUSCATED_KEEPALIVE)
        );
        assert_eq!(
            peer_keepalive(&network, "node-a", "router1"),
            Some(PLAIN_KEEPALIVE)
        );
    }

    #[test]
    fn a_keepalive_set_on_a_type_is_written_as_set() {
        let of_their_own = example(
            "{mesh: {wireguard: {persistent_keepalive: 10}, amneziawg: {persistent_keepalive: 25}}}",
        );
        assert_eq!(
            peer_keepalive(&of_their_own, "node-a", "router1"),
            Some(Range::from(10))
        );
        assert_eq!(
            peer_keepalive(&of_their_own, "node-a", "node-b"),
            Some(Range::from(25))
        );
    }

    fn refused(network: &Mesh<'_>, node: &str) -> crate::render::Error {
        node_configs(network, node).unwrap_err()
    }

    #[test]
    fn a_node_on_no_link_is_still_stopped_by_what_stops_every_node() {
        let network = example(&format!(
            "{{mesh: {{mtu: 9000}}, nodes: {{node-d: {NODE_D}}}}}"
        ));
        let refusal = refused(&network, "node-d");
        assert!(
            matches!(
                &refusal,
                crate::render::Error::RenderingAwg {
                    source: crate::render::RenderError::Mtu(crate::mtu::MtuError::AboveLimit {
                        mtu: 9000,
                        ..
                    }),
                    ..
                }
            ),
            "{refusal:?}"
        );
    }

    #[test]
    fn every_field_of_a_links_obfuscation_reaches_both_its_ends() {
        let every_field = "{jc: 4, jmin: 40, jmax: 400, s1: 64, s2: 48, s3: 40, s4: 16, h1: 1101, \
            h2: 2202, h3: 3303, h4: 4404-4499, i1: '<b 0xabcd>', i2: '<c>', i3: '<t>', i4: '<r 8>', \
            i5: '<rd 6>', header_protection_key: ccccccccccccccccccccccccccccccccccccccccccc=, \
            content_padding_addition: 0-32, rekey_after_time: 100-140, rekey_timeout: 6, \
            reject_after_time: 170, keepalive_timeout: 12, max_handshake_attempts: 20, \
            random_trailers: true, disable_cookies: true}";
        let written: Obfuscation = sliplib_yaml::from_str(every_field).unwrap();
        let fields = sliplib_yaml::to_value(&written).unwrap();
        assert_eq!(fields.as_mapping().unwrap().len(), 25, "a field is not set");

        let network = example(&format!(
            "{{mesh: {{links: {{a-b: {{amneziawg: {{obfuscation: {every_field}}}}}}}}}}}"
        ));
        for (node, peer) in [("node-a", "node-b"), ("node-b", "node-a")] {
            assert_eq!(
                mesh_interface(&network, node, peer).obfuscation,
                written,
                "{node}"
            );
        }
    }

    #[test]
    fn an_amneziawg_interface_carries_what_the_daemon_sends_for_a_value_the_file_leaves_unset() {
        let network = example(
            "{mesh: {amneziawg: {obfuscation: {s3: null, s4: null, random_trailers: null, \
             disable_cookies: null}}, links: {a-b: {amneziawg: {obfuscation: \
             {header_protection_key: null}}}}}}",
        );
        let obfuscation = mesh_interface(&network, "node-a", "node-b").obfuscation;
        let zero = Some(Range::from(0));
        assert_eq!((obfuscation.s3, obfuscation.s4), (Some(0), Some(0)));
        for (name, value) in [
            (
                "content_padding_addition",
                obfuscation.content_padding_addition,
            ),
            ("rekey_after_time", obfuscation.rekey_after_time),
            ("rekey_timeout", obfuscation.rekey_timeout),
            ("reject_after_time", obfuscation.reject_after_time),
            ("keepalive_timeout", obfuscation.keepalive_timeout),
            ("max_handshake_attempts", obfuscation.max_handshake_attempts),
        ] {
            assert_eq!(value, zero, "{name}");
        }
        assert_eq!(
            (obfuscation.random_trailers, obfuscation.disable_cookies),
            (Some(false), Some(false))
        );
        // Sent only when set.
        assert_eq!(obfuscation.header_protection_key, None);
        assert_eq!(obfuscation.i1.as_deref(), Some("<b 0xabcd><r 16>"));
    }

    #[test]
    fn a_wireguard_interface_carries_no_obfuscation_and_keeps_the_plain_keepalive() {
        let interface = mesh_interface(&example("{}"), "node-a", "router1");
        assert_eq!(interface.obfuscation, Obfuscation::default());
        assert_eq!(
            interface.peers[0].persistent_keepalive,
            Some(sliplib_config::awg::PLAIN_KEEPALIVE)
        );
    }

    #[test]
    fn a_link_that_lacks_a_field_mint_writes_is_refused_when_its_nodes_are_computed() {
        for overlay in [
            "{mesh: {links: {a-b: {amneziawg: {obfuscation: {jc: null}}}}}}",
            "{mesh: {amneziawg: {obfuscation: null}, \
             links: {a-b: {amneziawg: {obfuscation: null}}}}}",
        ] {
            let unminted = example(overlay);
            let refusal = node_configs(&unminted, "node-b").expect_err(overlay);
            let missing = missing_secret(&refusal).map(ToString::to_string);
            assert!(
                missing.is_some_and(|missing| missing.contains("a-b") && missing.contains("jc")),
                "{overlay}: {refusal:?}"
            );
        }
    }

    #[test]
    fn every_link_has_a_bfd_session_where_the_block_is_and_none_without_it() {
        let with = configs_of(&example("{}"), "node-a").unwrap();
        assert_eq!(
            with.router.bfd,
            Some(BfdSettings {
                min_rx_ms: 300,
                min_tx_ms: 300,
                multiplier: 5
            })
        );
        let without = configs_of(&example("{routing: {bfd: null}}"), "node-a").unwrap();
        assert_eq!(without.router.bfd, None);
    }
}
