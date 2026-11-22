//! A node's `awg` and `router` configs.

use crate::addressing;
use crate::model::{Network, Node, PRIVATE_KEY, UnknownNode};
use crate::mtu::MtuError;
use crate::secrets::{MissingSecret, unminted};
use sliplib_config::awg::{AwgConfig, InterfaceEntry, PeerEntry};
use sliplib_config::router::{BgpPeerEntry, NodeIdentity, RouterConfig};
use sliplib_config::{Obfuscation, ObfuscationError};
use sliplib_secrets::wireguard::KeyError;

/// Why a node has no configs. Every variant names the node and which of its two configs it is
/// about; the cause is its source.
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

impl Error {
    /// The key or obfuscation the node's configs lack, when that is what refused them: creating
    /// it cures the refusal. Only the `awg` config is computed from secrets.
    pub fn missing_secret(&self) -> Option<&MissingSecret> {
        match self {
            Self::RenderingAwg {
                source: RenderError::MissingSecret(missing),
                ..
            } => Some(missing),
            _ => None,
        }
    }
}

/// What a node's config cannot be computed from.
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

/// Full mesh minus self: every other node, addressed by its IPv6 loopback.
pub(crate) fn bgp_peers_for(mesh: &Network, node_name: &str) -> Vec<BgpPeerEntry> {
    mesh.nodes
        .iter()
        .filter(|n| n.name != node_name)
        .map(|n| BgpPeerEntry {
            name: n.name.clone(),
            address: n.loopback.1.to_string(),
        })
        .collect()
}

/// Every rendered node runs OSPF on the same two interface patterns: the mesh's and `router-lo`.
fn ospf_interfaces(mesh: &Network) -> Vec<String> {
    vec![mesh_glob(mesh), "router-lo".to_owned()]
}

/// Every mesh interface, as BIRD's `interface` clause and `direct_interfaces` match it.
fn mesh_glob(mesh: &Network) -> String {
    format!("{}*", mesh.interface_prefix)
}

pub(crate) fn render_router_config(mesh: &Network, node: &Node) -> RouterConfig {
    let (v4, v6) = node.loopback;
    RouterConfig {
        node: NodeIdentity {
            loopback_addresses: vec![format!("{v4}/32"), format!("{v6}/128")],
        },
        bgp_as: mesh.bgp_as,
        bgp_peers: bgp_peers_for(mesh, &node.name),
        bfd: mesh.bfd.clone(),
        ospf_interfaces: ospf_interfaces(mesh),
        direct_interfaces: node.direct_interfaces.clone(),
        learn: node.learn.clone(),
        announce: node.announce.clone(),
        bypass: None,
        metrics: None,
    }
}

fn key_of(node: &str) -> impl Fn(KeyError) -> RenderError {
    move |source| RenderError::Key {
        node: node.to_owned(),
        source,
    }
}

/// The node's mesh interfaces: one full-tunnel interface per link it is on, named after the peer
/// (`mesh-<peer>`). The name stays local - peers know each other by key - and must fit `IFNAMSIZ`,
/// which the awg check enforces.
fn mesh_interfaces_for(mesh: &Network, node_name: &str) -> Result<Vec<InterfaceEntry>> {
    let node = mesh.node(node_name)?;
    let own_private_key = node.private_key.as_ref();
    if own_private_key.is_none() && mesh.needs_a_key(node_name) {
        return Err(MissingSecret(format!("node {node_name:?}: {PRIVATE_KEY} is not set")).into());
    }
    // Decoded for every node, whether or not a link uses its key: where one does, the daemon
    // would otherwise be the first to decode it, and refuse a config it is already running on.
    if let Some(own_private_key) = own_private_key {
        own_private_key.public_key().map_err(key_of(node_name))?;
    }

    // Before the node's links: what refuses the MTU of the mesh stops a node on no link too.
    let mtu = crate::mtu::mesh_mtu(mesh)?;
    // Only a node that needs no key has none, and it is on no link.
    let Some(own_private_key) = own_private_key else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for link in &mesh.links {
        let peer_name = if link.ends[0] == node_name {
            &link.ends[1]
        } else if link.ends[1] == node_name {
            &link.ends[0]
        } else {
            continue;
        };
        let peer_node = mesh.node(peer_name)?;
        let peer_private_key = peer_node.private_key.as_ref().ok_or_else(|| {
            MissingSecret(format!("node {peer_name:?}: {PRIVATE_KEY} is not set"))
        })?;
        let obfuscation = minted_obfuscation(
            &link.obfuscation,
            link.plain,
            &format!("mesh link {}", link.name),
        )?;

        let mut addresses = vec![format!("{}/64", node.link_local)];
        if let Some(tunnel_v4) = node.tunnel_ipv4 {
            addresses.push(format!("{tunnel_v4}/32"));
        }

        let mut interface = InterfaceEntry {
            name: format!("{}{peer_name}", mesh.interface_prefix),
            listen_port: link.port,
            addresses,
            private_key: own_private_key.clone(),
            obfuscation,
            handshake_stale_secs: None,
            mtu,
            peers: vec![PeerEntry {
                public_key: peer_private_key.public_key().map_err(key_of(peer_name))?,
                name: Some(peer_name.clone()),
                endpoint: peer_node
                    .endpoint
                    .as_ref()
                    .map(|endpoint| addressing::endpoint(&endpoint.written, link.port)),
                allowed_ips: None,
                persistent_keepalive: None,
            }],
        };
        // The link's keepalive is written only where it changes what the peer is sent.
        let without = interface.keepalive_of(&interface.peers[0]);
        interface.peers[0].persistent_keepalive = link.persistent_keepalive;
        let with = interface.keepalive_of(&interface.peers[0]);
        if with == without {
            interface.peers[0].persistent_keepalive = None;
        }
        out.push(interface);
    }
    Ok(out)
}

pub(crate) fn render_awg_config(mesh: &Network, node_name: &str) -> Result<AwgConfig> {
    Ok(AwgConfig {
        interfaces: mesh_interfaces_for(mesh, node_name)?,
        metrics: None,
    })
}

/// A node's configs, as its daemons would take them.
#[derive(Debug)]
pub struct NodeConfigs {
    pub awg: AwgConfig,
    pub router: RouterConfig,
}

/// `host`'s configs from `model`, each checked the way the node would check it.
pub fn node_configs(model: &Network, host: &str) -> std::result::Result<NodeConfigs, Error> {
    let node = || host.to_owned();
    // Refused as the `awg` config, computed first, refuses a node it does not know.
    let found = model.node(host).map_err(|source| Error::RenderingAwg {
        node: node(),
        source: source.into(),
    })?;
    let awg_cfg = render_awg_config(model, host).map_err(|source| Error::RenderingAwg {
        node: node(),
        source,
    })?;
    sliplib_config::awg::validate(&awg_cfg).map_err(|source| Error::InvalidAwg {
        node: node(),
        source,
    })?;
    // Checked here rather than by awg: the daemon can only refuse a config it is already running
    // on, and a refusal there would leave every interface of the node down.
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
    let router_cfg = render_router_config(model, found);
    sliplib_config::router::validate(&router_cfg).map_err(|source| Error::InvalidRouter {
        node: node(),
        source,
    })?;
    Ok(NodeConfigs {
        awg: awg_cfg,
        router: router_cfg,
    })
}

/// Refuses one lacking a minted field, unless `plain`.
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
    Ok(obfuscation.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::test_support::example;

    fn configs_of(network: &Network, node: &str) -> std::result::Result<NodeConfigs, Error> {
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
        let peers: Vec<(&str, &str)> = peers
            .iter()
            .map(|peer| (peer.name.as_str(), peer.address.as_str()))
            .collect();
        assert_eq!(
            peers,
            [
                ("node-a", "3fff:0:0:1::1"),
                ("node-c", "3fff:0:0:1::3"),
                ("router1", "3fff:0:0:1::4"),
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
        assert_eq!(configs.router.ospf_interfaces, ["wg-*", "router-lo"]);
    }

    #[test]
    fn a_nodes_configs_are_its_rendered_awg_and_router_configs() {
        let network = example("{}");
        let configs = super::node_configs(&network, "node-a").unwrap();
        assert_eq!(
            configs.awg,
            super::render_awg_config(&network, "node-a").unwrap()
        );
        assert_eq!(
            configs.router,
            super::render_router_config(&network, network.node("node-a").unwrap())
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
            let missing = refusal.missing_secret().map(ToString::to_string);
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

    /// `refusal` is for the key of `owner`, which does not decode, and creating none cures it.
    fn assert_not_a_key(refusal: Error, owner: &str) {
        assert!(
            matches!(&refusal, Error::RenderingAwg { source: RenderError::Key { node, .. }, .. }
                if node == owner),
            "{refusal:?}"
        );
        assert!(refusal.missing_secret().is_none(), "{refusal:?}");
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
    fn a_link_without_any_obfuscation_is_refused_for_a_missing_secret_at_both_ends() {
        let network = example(
            "{mesh: {amneziawg: {obfuscation: null}, \
             links: {a-b: {amneziawg: {obfuscation: null}}}}}",
        );
        for node in ["node-a", "node-b"] {
            let refusal = configs_of(&network, node).unwrap_err();
            let missing = refusal.missing_secret().map(ToString::to_string);
            assert!(
                missing.is_some_and(|missing| missing.contains("a-b")),
                "{node}: {refusal:?}"
            );
        }
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
            assert!(refusal.missing_secret().is_none(), "{node}");
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
            assert!(refusal.missing_secret().is_some(), "{node}: {refusal:?}");
        }
    }
}
