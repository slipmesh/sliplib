//! A node's `awg` and `router` configs from the network: its mesh interfaces and peers, iBGP with
//! every other node, OSPF, announcements, roadwarriors pools and bypass lists. Every function takes
//! the network's secrets as [`crate::secrets::read`] gives them.

use crate::addressing;
use crate::model::{Network, Node};
use crate::mtu::MtuError;
use crate::secrets::{MissingSecret, ResolvedSecrets, link_key, unminted};
use sliplib_config::awg::{AwgConfig, InterfaceEntry, PeerEntry};
use sliplib_config::router::{
    BgpPeerEntry, BypassConfig, DEFAULT_BYPASS_REFRESH_INTERVAL_SECS, NodeIdentity, RouterConfig,
};
use sliplib_config::v1::mesh_config::UnknownNode;
use sliplib_config::v2::Platform;
use sliplib_config::{MetricsConfig, Obfuscation, ObfuscationError};
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
    #[error("rendering router config for node {node:?}")]
    RenderingRouter {
        node: String,
        #[source]
        source: RenderError,
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
    /// it cures the refusal.
    pub fn missing_secret(&self) -> Option<&MissingSecret> {
        match self {
            Self::RenderingAwg {
                source: RenderError::MissingSecret(missing),
                ..
            }
            | Self::RenderingRouter {
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
    /// The network at the key `field` of the file does not read as one.
    #[error("{field}")]
    Network {
        field: &'static str,
        #[source]
        source: sliplib_net::ParseError,
    },
    #[error("node {node:?}: node_id must be a valid IPv4-shaped identity")]
    NodeId {
        node: String,
        #[source]
        source: std::net::AddrParseError,
    },
    #[error("node {node:?}: {key}")]
    Key {
        node: String,
        /// How the files name a node's private key.
        key: &'static str,
        #[source]
        source: KeyError,
    },
    #[error(
        "node {node:?}: cluster.tunnel_networks is configured, so direct_interfaces must include {glob:?} - the tunnel IPv4 address is otherwise announced by nothing (OSPFv3 is IPv6-only) and is reachable on no other node"
    )]
    MeshPatternMissing { node: String, glob: String },
    #[error("cluster.{0} is 0 - remove the field to serve no metrics")]
    MetricsPortZero(&'static str),
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

/// The node's bypass rules, every `bypass` entry for it in one `BypassConfig`; `None` when it has
/// none.
pub(crate) fn bypass_for(mesh: &Network, node_name: &str) -> Option<BypassConfig> {
    let v1 = mesh.v1.as_ref()?;
    let entries: Vec<_> = v1.bypass.iter().filter(|b| b.node == node_name).collect();
    if entries.is_empty() {
        return None;
    }
    let mut include = Vec::new();
    let mut exclude = Vec::new();
    for entry in entries {
        include.extend(entry.include.iter().cloned());
        exclude.extend(entry.exclude.iter().cloned());
    }
    Some(BypassConfig {
        refresh_interval_secs: v1
            .bypass_refresh_interval_secs
            .unwrap_or(DEFAULT_BYPASS_REFRESH_INTERVAL_SECS),
        include,
        exclude,
    })
}

/// Every rendered node runs OSPF on the same two interface patterns: the mesh's and `router-lo`.
fn ospf_interfaces(mesh: &Network) -> Vec<String> {
    vec![mesh_glob(mesh), "router-lo".to_owned()]
}

/// Every mesh interface, as BIRD's `interface` clause and `direct_interfaces` match it.
fn mesh_glob(mesh: &Network) -> String {
    format!("{}*", mesh.interface_prefix)
}

pub(crate) fn render_router_config(mesh: &Network, node_name: &str) -> Result<RouterConfig> {
    let node = mesh.node(node_name)?;
    let (v4, v6) = node.loopback;
    let direct_interfaces = node.direct_interfaces.clone();
    // Refused rather than fixed up: OSPFv3 carries no IPv4, so the tunnel IPv4 address of a mesh
    // interface reaches other nodes only through `protocol direct` on the mesh glob. Without it,
    // replies masqueraded through that address never route back.
    let glob = mesh_glob(mesh);
    if node.tunnel_ipv4.is_some() && !direct_interfaces.contains(&glob) {
        return Err(RenderError::MeshPatternMissing {
            node: node_name.to_owned(),
            glob,
        });
    }
    Ok(RouterConfig {
        node: NodeIdentity {
            loopback_addresses: vec![format!("{v4}/32"), format!("{v6}/128")],
        },
        bgp_as: mesh.bgp_as,
        bgp_peers: bgp_peers_for(mesh, node_name),
        bfd: mesh.bfd.clone(),
        ospf_interfaces: ospf_interfaces(mesh),
        direct_interfaces,
        learn: node.learn.clone(),
        announce: node.announce.clone(),
        bypass: bypass_for(mesh, node_name),
        metrics: metrics_for(node, mesh.metrics.router, "router_metrics_port")?,
    })
}

fn key_of(node: &str, key: &'static str) -> impl Fn(KeyError) -> RenderError {
    move |source| RenderError::Key {
        node: node.to_owned(),
        key,
        source,
    }
}

/// The node's mesh interfaces: one full-tunnel interface per link it is on, named after the peer
/// (`mesh-<peer>`). The name stays local - peers know each other by key - and must fit `IFNAMSIZ`,
/// which the awg check enforces.
fn mesh_interfaces_for(
    mesh: &Network,
    node_name: &str,
    resolved: &ResolvedSecrets,
) -> Result<Vec<InterfaceEntry>> {
    let node = mesh.node(node_name)?;
    let key = mesh.keys.private_key;
    let own_private_key = resolved
        .mesh_private_keys
        .get(node_name)
        .ok_or_else(|| MissingSecret(format!("node {node_name:?}: {key} is not set")))?;
    // Decoded for every node, whether or not a link uses its key: where one does, the daemon
    // would otherwise be the first to decode it, and refuse a config it is already running on.
    own_private_key
        .public_key()
        .map_err(key_of(node_name, key))?;

    let mtu = crate::mtu::mesh_mtu(mesh, resolved)?;
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
        let peer_private_key = resolved
            .mesh_private_keys
            .get(peer_name)
            .ok_or_else(|| MissingSecret(format!("node {peer_name:?}: {key} is not set")))?;
        let obfuscation = minted_obfuscation(
            resolved.mesh_link_obfuscation.get(&link_key(&link.ends)),
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
                public_key: peer_private_key
                    .public_key()
                    .map_err(key_of(peer_name, key))?,
                name: Some(peer_name.clone()),
                endpoint: peer_node
                    .endpoint
                    .as_ref()
                    .map(|endpoint| addressing::endpoint(&endpoint.written, link.port)),
                allowed_ips: None,
                persistent_keepalive: None,
            }],
        };
        // Only a dialed peer is sent keepalives, and the one every reader takes for a peer that
        // sets none is left out.
        let peer = &interface.peers[0];
        let default = interface.keepalive_of(peer);
        let own = link
            .persistent_keepalive
            .filter(|own| peer.endpoint.is_some() && *own != default);
        interface.peers[0].persistent_keepalive = own;
        out.push(interface);
    }
    Ok(out)
}

/// This node's roadwarriors-pool interfaces: one `InterfaceEntry` per pool that lists this node in
/// `node_hostnames`, tracked (`allowed_ips` set) peers built straight from `clients`.
fn roadwarrior_interfaces_for(
    mesh: &Network,
    node_name: &str,
    resolved: &ResolvedSecrets,
) -> Result<Vec<InterfaceEntry>> {
    mesh.v1
        .iter()
        .flat_map(|v1| &v1.pools)
        .filter(|pool| pool.node_hostnames.iter().any(|h| h == node_name))
        .map(|pool| {
            let private_key = resolved
                .roadwarrior_private_keys
                .get(&pool.name)
                .cloned()
                .ok_or_else(|| {
                    MissingSecret(format!(
                        "roadwarriors pool {:?}: private_key is not set",
                        pool.name
                    ))
                })?;
            let mut addresses = vec![pool.address.clone()];
            addresses.extend(pool.address6.clone());
            Ok(InterfaceEntry {
                name: format!("rw-{}", pool.name),
                listen_port: pool.listen_port,
                addresses,
                private_key,
                obfuscation: minted_obfuscation(
                    resolved.roadwarrior_obfuscation.get(&pool.name),
                    pool.plain,
                    &format!("roadwarriors pool {:?}", pool.name),
                )?,
                handshake_stale_secs: pool.handshake_stale_secs,
                mtu: None,
                peers: pool
                    .clients
                    .iter()
                    .map(|c| PeerEntry {
                        public_key: c.public_key.clone(),
                        name: Some(c.name.clone()),
                        endpoint: None,
                        allowed_ips: Some(c.allowed_ips.clone()),
                        persistent_keepalive: None,
                    })
                    .collect(),
            })
        })
        .collect()
}

pub(crate) fn render_awg_config(
    mesh: &Network,
    node_name: &str,
    resolved: &ResolvedSecrets,
) -> Result<AwgConfig> {
    let mut interfaces = mesh_interfaces_for(mesh, node_name, resolved)?;
    interfaces.extend(roadwarrior_interfaces_for(mesh, node_name, resolved)?);
    Ok(AwgConfig {
        interfaces,
        metrics: metrics_for(mesh.node(node_name)?, mesh.metrics.awg, "awg_metrics_port")?,
    })
}

/// A node's configs, as its daemons would take them.
#[derive(Debug)]
pub struct NodeConfigs {
    pub awg: AwgConfig,
    pub router: RouterConfig,
}

/// `host`'s configs from `model`, each checked the way the node would check it.
pub fn node_configs(
    model: &Network,
    host: &str,
    resolved: &ResolvedSecrets,
) -> std::result::Result<NodeConfigs, Error> {
    let node = || host.to_owned();
    let awg_cfg =
        render_awg_config(model, host, resolved).map_err(|source| Error::RenderingAwg {
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
    let router_cfg =
        render_router_config(model, host).map_err(|source| Error::RenderingRouter {
            node: node(),
            source,
        })?;
    sliplib_config::router::validate(&router_cfg).map_err(|source| Error::InvalidRouter {
        node: node(),
        source,
    })?;
    Ok(NodeConfigs {
        awg: awg_cfg,
        router: router_cfg,
    })
}

/// The obfuscation read for `what`, refused when none was read and, unless `plain`, when it lacks
/// a minted field.
fn minted_obfuscation(
    read: Option<&Obfuscation>,
    plain: bool,
    what: &str,
) -> std::result::Result<Obfuscation, MissingSecret> {
    let obfuscation = read
        .cloned()
        .ok_or_else(|| MissingSecret(format!("{what}: obfuscation is not set")))?;
    let unminted = if plain {
        Vec::new()
    } else {
        unminted(&obfuscation)
    };
    if !unminted.is_empty() {
        return Err(MissingSecret(format!(
            "{what}: obfuscation lacks {}",
            unminted.join(", ")
        )));
    }
    Ok(obfuscation)
}

/// A metrics listener for this node, when the port says there is one, on the node's own IPv4
/// loopback - the address the router daemon brings up, so no listener can drift from it.
fn metrics_for(
    node: &Node,
    port: Option<u16>,
    field: &'static str,
) -> Result<Option<MetricsConfig>> {
    // The listeners are those of the daemons the Talos generator installs. A file of version 1
    // names no platform, and every node of it gets them.
    let served = node
        .platform
        .is_none_or(|platform| platform == Platform::Talos);
    let Some(port) = port.filter(|_| served) else {
        return Ok(None);
    };
    // Refused here, not by the daemon: unset already serves nothing, so 0 is a mistake, better
    // caught where it was written than on every node at once.
    if port == 0 {
        return Err(RenderError::MetricsPortZero(field));
    }
    Ok(Some(MetricsConfig {
        listen: format!("{}:{port}", node.loopback.0),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sliplib_config::v1::mesh_config::MeshConfig;
    use std::net::{Ipv4Addr, Ipv6Addr};

    // The fixtures are topologies of format version 1. Each function under test is reached
    // through that version's lowering, as a tool reaches it.
    fn lowered(mesh: &MeshConfig) -> Result<Network> {
        crate::v1::lower(mesh)
    }

    fn node_loopbacks(mesh: &MeshConfig, node: &str) -> Result<(Ipv4Addr, Ipv6Addr)> {
        Ok(lowered(mesh)?.node(node)?.loopback)
    }

    fn node_tunnel_addresses(
        mesh: &MeshConfig,
        node: &str,
    ) -> Result<Option<(Ipv4Addr, Ipv6Addr)>> {
        let network = lowered(mesh)?;
        let node = network.node(node)?;
        Ok(node.tunnel_ipv4.map(|v4| (v4, node.link_local)))
    }

    fn bgp_peers_for(mesh: &MeshConfig, node: &str) -> Result<Vec<BgpPeerEntry>> {
        Ok(super::bgp_peers_for(&lowered(mesh)?, node))
    }

    fn bypass_for(mesh: &MeshConfig, node: &str) -> Option<BypassConfig> {
        super::bypass_for(&lowered(mesh).unwrap(), node)
    }

    fn render_router_config(mesh: &MeshConfig, node: &str) -> Result<RouterConfig> {
        super::render_router_config(&lowered(mesh)?, node)
    }

    fn mesh_interfaces_for(
        mesh: &MeshConfig,
        node: &str,
        resolved: &ResolvedSecrets,
    ) -> Result<Vec<InterfaceEntry>> {
        super::mesh_interfaces_for(&lowered(mesh)?, node, resolved)
    }

    fn roadwarrior_interfaces_for(
        mesh: &MeshConfig,
        node: &str,
        resolved: &ResolvedSecrets,
    ) -> Result<Vec<InterfaceEntry>> {
        super::roadwarrior_interfaces_for(&lowered(mesh)?, node, resolved)
    }

    fn render_awg_config(
        mesh: &MeshConfig,
        node: &str,
        resolved: &ResolvedSecrets,
    ) -> Result<AwgConfig> {
        super::render_awg_config(&lowered(mesh)?, node, resolved)
    }

    fn node_configs(
        mesh: &MeshConfig,
        node: &str,
        resolved: &ResolvedSecrets,
    ) -> std::result::Result<NodeConfigs, Error> {
        super::node_configs(&lowered(mesh).unwrap(), node, resolved)
    }

    fn three_node_mesh_yaml() -> &'static str {
        r#"
cluster:
  bgp_as: 64512
  loopback_networks: {ipv4: "198.51.100.0/25", ipv6: "3fff::/32"}
  bypass_refresh_interval_secs: 3600
nodes:
  - {name: a, node_id: "198.51.100.1"}
  - {name: b, node_id: "198.51.100.2"}
  - {name: c, node_id: "198.51.100.3"}
mesh:
  links:
    - {pair: [a, b], port: 51820}
bypass:
  - node: a
    include:
      - kind: literal
        prefixes: [{net: "192.0.2.0/24"}]
"#
    }

    #[test]
    fn node_loopbacks_ors_node_id_onto_cluster_networks() {
        let mesh: MeshConfig = sliplib_yaml::from_str(three_node_mesh_yaml()).unwrap();
        let (v4, v6) = node_loopbacks(&mesh, "a").unwrap();
        assert_eq!(v4.to_string(), "198.51.100.1");
        // node_id 198.51.100.1 is all 32 bits, 0xc6336401, not just its last octet.
        assert_eq!(v6.to_string(), "3fff::c633:6401");
    }

    #[test]
    fn node_loopbacks_rejects_unknown_node() {
        let mesh: MeshConfig = sliplib_yaml::from_str(three_node_mesh_yaml()).unwrap();
        assert!(node_loopbacks(&mesh, "nonexistent").is_err());
    }

    #[test]
    fn bgp_peers_for_excludes_self_and_covers_full_mesh() {
        let mesh: MeshConfig = sliplib_yaml::from_str(three_node_mesh_yaml()).unwrap();
        let peers = bgp_peers_for(&mesh, "a").unwrap();
        let names: Vec<&str> = peers.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"b"));
        assert!(names.contains(&"c"));
        assert!(!names.contains(&"a"));
    }

    #[test]
    fn bgp_peers_for_addresses_are_ipv6_loopbacks() {
        let mesh: MeshConfig = sliplib_yaml::from_str(three_node_mesh_yaml()).unwrap();
        let peers = bgp_peers_for(&mesh, "a").unwrap();
        let b = peers.iter().find(|p| p.name == "b").unwrap();
        assert_eq!(b.address, "3fff::c633:6402");
    }

    #[test]
    fn bypass_for_filters_by_node() {
        let mesh: MeshConfig = sliplib_yaml::from_str(three_node_mesh_yaml()).unwrap();
        assert!(bypass_for(&mesh, "a").is_some());
        assert!(bypass_for(&mesh, "b").is_none());
    }

    #[test]
    fn bypass_for_converts_literal_prefixes() {
        let mesh: MeshConfig = sliplib_yaml::from_str(three_node_mesh_yaml()).unwrap();
        let bypass = bypass_for(&mesh, "a").unwrap();
        assert_eq!(bypass.include.len(), 1);
        assert_eq!(bypass.include[0].kind, "literal");
        assert_eq!(
            bypass.include[0].prefixes.as_ref().unwrap()[0].net,
            "192.0.2.0/24"
        );
        assert_eq!(bypass.refresh_interval_secs, 3600);
    }

    #[test]
    fn render_router_config_is_valid() {
        let mesh: MeshConfig = sliplib_yaml::from_str(three_node_mesh_yaml()).unwrap();
        let cfg = render_router_config(&mesh, "a").unwrap();
        sliplib_config::router::validate(&cfg).unwrap();
        assert_eq!(cfg.bgp_as, 64512);
        assert_eq!(
            cfg.ospf_interfaces,
            vec!["mesh-*".to_string(), "router-lo".to_string()]
        );
        assert!(cfg.bypass.is_some());
    }

    #[test]
    fn render_router_config_direct_interfaces_has_no_default() {
        let mesh: MeshConfig = sliplib_yaml::from_str(three_node_mesh_yaml()).unwrap();
        let cfg_a = render_router_config(&mesh, "a").unwrap();
        let cfg_b = render_router_config(&mesh, "b").unwrap();
        assert!(cfg_a.direct_interfaces.is_empty());
        assert!(cfg_b.direct_interfaces.is_empty());
    }

    #[test]
    fn render_router_config_omits_mesh_glob_from_direct_interfaces_when_tunnel_networks_not_configured()
     {
        let mesh: MeshConfig = sliplib_yaml::from_str(three_node_mesh_yaml()).unwrap();
        let cfg = render_router_config(&mesh, "a").unwrap();
        assert!(!cfg.direct_interfaces.iter().any(|s| s == "mesh-*"));
    }

    #[test]
    fn render_router_config_uses_explicit_cluster_direct_interfaces() {
        let mut mesh: MeshConfig = sliplib_yaml::from_str(three_node_mesh_yaml()).unwrap();
        mesh.cluster.direct_interfaces = vec!["cni0".to_string(), "extra0".to_string()];
        let cfg = render_router_config(&mesh, "a").unwrap();
        assert_eq!(
            cfg.direct_interfaces,
            vec!["cni0".to_string(), "extra0".to_string()]
        );
    }

    #[test]
    fn render_router_config_per_node_direct_interfaces_overrides_not_merges_the_global_default() {
        let mut mesh: MeshConfig = sliplib_yaml::from_str(three_node_mesh_yaml()).unwrap();
        mesh.nodes[0].direct_interfaces = Some(vec!["home".to_string()]);
        let cfg_a = render_router_config(&mesh, "a").unwrap();
        let cfg_b = render_router_config(&mesh, "b").unwrap();
        assert_eq!(cfg_a.direct_interfaces, vec!["home".to_string()]);
        assert!(cfg_b.direct_interfaces.is_empty());
    }

    #[test]
    fn render_router_config_per_node_override_replaces_a_nonempty_global() {
        let mut mesh: MeshConfig = sliplib_yaml::from_str(three_node_mesh_yaml()).unwrap();
        mesh.cluster.direct_interfaces = vec!["mesh-*".to_string()];
        mesh.nodes[0].direct_interfaces = Some(vec!["home".to_string()]);
        let cfg_a = render_router_config(&mesh, "a").unwrap();
        let cfg_b = render_router_config(&mesh, "b").unwrap();
        assert_eq!(cfg_a.direct_interfaces, vec!["home".to_string()]);
        assert_eq!(cfg_b.direct_interfaces, vec!["mesh-*".to_string()]);
    }

    #[test]
    fn render_router_config_announce_is_empty_by_default() {
        let mesh: MeshConfig = sliplib_yaml::from_str(three_node_mesh_yaml()).unwrap();
        let cfg = render_router_config(&mesh, "a").unwrap();
        assert!(cfg.announce.is_empty());
    }

    #[test]
    fn render_router_config_announces_the_configured_service_subnet_on_every_node() {
        let mut mesh: MeshConfig = sliplib_yaml::from_str(three_node_mesh_yaml()).unwrap();
        mesh.cluster.service_subnet = Some("203.0.113.192/26".to_string());
        let cfg_a = render_router_config(&mesh, "a").unwrap();
        let cfg_b = render_router_config(&mesh, "b").unwrap();
        for cfg in [&cfg_a, &cfg_b] {
            assert_eq!(cfg.announce.len(), 1);
            assert_eq!(cfg.announce[0].net, "203.0.113.192/26");
            assert_eq!(cfg.announce[0].label.as_deref(), Some("k8s-services"));
        }
    }

    #[test]
    fn render_router_config_learn_is_empty_by_default() {
        let mesh: MeshConfig = sliplib_yaml::from_str(three_node_mesh_yaml()).unwrap();
        let cfg = render_router_config(&mesh, "a").unwrap();
        assert!(cfg.learn.is_empty());
    }

    #[test]
    fn render_router_config_learns_the_configured_pod_subnet_on_every_node() {
        let mut mesh: MeshConfig = sliplib_yaml::from_str(three_node_mesh_yaml()).unwrap();
        mesh.cluster.pod_subnet = Some("203.0.113.128/26".to_string());
        let cfg_a = render_router_config(&mesh, "a").unwrap();
        let cfg_b = render_router_config(&mesh, "b").unwrap();
        for cfg in [&cfg_a, &cfg_b] {
            assert_eq!(cfg.learn, vec!["203.0.113.128/26".to_string()]);
        }
    }

    /// `pod_subnet` and `direct_interfaces` are independent: `learn` alone carries the node's
    /// podCIDR.
    #[test]
    fn render_router_config_learns_pod_subnet_with_direct_interfaces_emptied() {
        let mut mesh: MeshConfig = sliplib_yaml::from_str(three_node_mesh_yaml()).unwrap();
        mesh.cluster.pod_subnet = Some("203.0.113.128/26".to_string());
        mesh.cluster.direct_interfaces = vec![];
        let cfg = render_router_config(&mesh, "a").unwrap();
        assert_eq!(cfg.learn, vec!["203.0.113.128/26".to_string()]);
        assert!(!cfg.direct_interfaces.iter().any(|s| s == "cni*"));
    }

    #[test]
    fn render_router_config_omits_bypass_for_a_node_with_none() {
        let mesh: MeshConfig = sliplib_yaml::from_str(three_node_mesh_yaml()).unwrap();
        let cfg = render_router_config(&mesh, "b").unwrap();
        assert!(cfg.bypass.is_none());
    }

    fn mesh_and_roadwarriors_yaml() -> &'static str {
        r#"
cluster:
  bgp_as: 64512
  loopback_networks: {ipv4: "198.51.100.0/25", ipv6: "3fff::/32"}
nodes:
  - {name: a, node_id: "198.51.100.1"}
  - {name: b, node_id: "198.51.100.2", endpoint: "b.example.com"}
  - {name: c, node_id: "198.51.100.3"}
mesh:
  links:
    - {pair: [a, b], port: 51820}
roadwarriors:
  - name: north
    node_hostnames: [a]
    address: "203.0.113.1/27"
    listen_port: 51900
    clients:
      - name: alice
        public_key: "client-pubkey"
        allowed_ips: ["203.0.113.5/32"]
"#
    }

    fn resolved_for(mesh: &MeshConfig) -> ResolvedSecrets {
        crate::test_support::minted(mesh)
    }

    #[test]
    fn every_mesh_interface_takes_the_mesh_mtu_and_a_pool_interface_none() {
        let mesh: MeshConfig = sliplib_yaml::from_str(mesh_and_roadwarriors_yaml()).unwrap();
        let resolved = resolved_for(&mesh);
        let cfg = render_awg_config(&mesh, "a", &resolved).unwrap();
        let mtu = |name: &str| cfg.interfaces.iter().find(|i| i.name == name).unwrap().mtu;
        // a-b is dialed at b's hostname, IPv6 when unsaid: 1500 - 32 - 8 - 40.
        assert_eq!(mtu("mesh-b"), Some(1420));
        assert_eq!(mtu("rw-north"), None);
    }

    #[test]
    fn an_ipv6_endpoint_is_dialed_in_brackets() {
        let yaml = mesh_and_roadwarriors_yaml().replace("b.example.com", "2001:db8::11");
        let mesh: MeshConfig = sliplib_yaml::from_str(&yaml).unwrap();
        let ifaces = mesh_interfaces_for(&mesh, "a", &resolved_for(&mesh)).unwrap();
        assert_eq!(
            ifaces[0].peers[0].endpoint.as_deref(),
            Some("[2001:db8::11]:51820")
        );
    }

    #[test]
    fn mesh_interfaces_for_names_the_interface_after_the_peers_node_name() {
        let mesh: MeshConfig = sliplib_yaml::from_str(mesh_and_roadwarriors_yaml()).unwrap();
        let resolved = resolved_for(&mesh);
        let ifaces = mesh_interfaces_for(&mesh, "a", &resolved).unwrap();
        assert_eq!(ifaces.len(), 1);
        assert_eq!(ifaces[0].name, "mesh-b");
        assert_eq!(ifaces[0].listen_port, 51820);
    }

    #[test]
    fn mesh_interfaces_for_is_full_tunnel_with_no_allowed_ips() {
        let mesh: MeshConfig = sliplib_yaml::from_str(mesh_and_roadwarriors_yaml()).unwrap();
        let resolved = resolved_for(&mesh);
        let ifaces = mesh_interfaces_for(&mesh, "a", &resolved).unwrap();
        assert_eq!(ifaces[0].peers[0].allowed_ips, None);
    }

    #[test]
    fn mesh_interfaces_for_builds_endpoint_from_node_host_and_link_port() {
        let mesh: MeshConfig = sliplib_yaml::from_str(mesh_and_roadwarriors_yaml()).unwrap();
        let resolved = resolved_for(&mesh);
        let a_ifaces = mesh_interfaces_for(&mesh, "a", &resolved).unwrap();
        assert_eq!(
            a_ifaces[0].peers[0].endpoint,
            Some("b.example.com:51820".to_string())
        );
        let b_ifaces = mesh_interfaces_for(&mesh, "b", &resolved).unwrap();
        assert_eq!(b_ifaces[0].peers[0].endpoint, None);
    }

    #[test]
    fn mesh_interfaces_for_peer_public_key_matches_the_peers_resolved_private_key() {
        let mesh: MeshConfig = sliplib_yaml::from_str(mesh_and_roadwarriors_yaml()).unwrap();
        let resolved = resolved_for(&mesh);
        let ifaces = mesh_interfaces_for(&mesh, "a", &resolved).unwrap();
        let expected_public = resolved.mesh_private_keys["b"].public_key().unwrap();
        assert_eq!(ifaces[0].peers[0].public_key, expected_public);
    }

    fn mesh_and_roadwarriors_with_tunnel_networks_yaml() -> &'static str {
        r#"
cluster:
  bgp_as: 64512
  loopback_networks: {ipv4: "198.51.100.0/25", ipv6: "3fff::/32"}
  tunnel_networks: {ipv4: "198.51.100.192/26", ipv6: "3fff:1::/120"}
nodes:
  - {name: a, node_id: "198.51.100.1"}
  - {name: b, node_id: "198.51.100.2", endpoint: "b.example.com"}
mesh:
  links:
    - {pair: [a, b], port: 51820}
"#
    }

    #[test]
    fn node_tunnel_addresses_is_none_when_not_configured() {
        let mesh: MeshConfig = sliplib_yaml::from_str(mesh_and_roadwarriors_yaml()).unwrap();
        assert!(node_tunnel_addresses(&mesh, "a").unwrap().is_none());
    }

    #[test]
    fn node_tunnel_addresses_ors_node_id_onto_cluster_networks() {
        let mesh: MeshConfig =
            sliplib_yaml::from_str(mesh_and_roadwarriors_with_tunnel_networks_yaml()).unwrap();
        let (v4, v6) = node_tunnel_addresses(&mesh, "a").unwrap().unwrap();
        assert_eq!(v4.to_string(), "198.51.100.193");
        assert_eq!(v6.to_string(), "3fff:1::1");
    }

    #[test]
    fn mesh_interfaces_for_uses_tunnel_ipv6_as_the_sole_link_local_when_configured() {
        let mesh: MeshConfig =
            sliplib_yaml::from_str(mesh_and_roadwarriors_with_tunnel_networks_yaml()).unwrap();
        let resolved = resolved_for(&mesh);
        let ifaces = mesh_interfaces_for(&mesh, "a", &resolved).unwrap();
        assert_eq!(
            ifaces[0].addresses,
            vec!["3fff:1::1/64".to_string(), "198.51.100.193/32".to_string()]
        );
    }

    #[test]
    fn mesh_interfaces_for_uses_the_loopback_derived_link_local_when_not_configured() {
        let mesh: MeshConfig = sliplib_yaml::from_str(mesh_and_roadwarriors_yaml()).unwrap();
        let resolved = resolved_for(&mesh);
        let ifaces = mesh_interfaces_for(&mesh, "a", &resolved).unwrap();
        assert_eq!(ifaces[0].addresses, vec!["fe80::c633:6401/64".to_string()]);
    }

    #[test]
    fn render_router_config_rejects_tunnel_networks_without_the_mesh_glob() {
        let mesh: MeshConfig =
            sliplib_yaml::from_str(mesh_and_roadwarriors_with_tunnel_networks_yaml()).unwrap();
        let err = render_router_config(&mesh, "a").unwrap_err().to_string();
        assert!(err.contains("mesh-*"), "unexpected error: {err}");
        assert!(err.contains("tunnel_networks"), "unexpected error: {err}");
    }

    #[test]
    fn render_router_config_does_not_duplicate_mesh_glob_if_already_present() {
        let mut mesh: MeshConfig =
            sliplib_yaml::from_str(mesh_and_roadwarriors_with_tunnel_networks_yaml()).unwrap();
        mesh.cluster.direct_interfaces = vec!["mesh-*".to_string()];
        let cfg = render_router_config(&mesh, "a").unwrap();
        assert_eq!(
            cfg.direct_interfaces
                .iter()
                .filter(|s| *s == "mesh-*")
                .count(),
            1
        );
    }

    #[test]
    fn the_mesh_interface_prefix_names_the_interfaces_ospf_and_the_tunnel_check() {
        let mut mesh: MeshConfig =
            sliplib_yaml::from_str(mesh_and_roadwarriors_with_tunnel_networks_yaml()).unwrap();
        mesh.cluster.interface_prefix.mesh = Some("wg-".to_owned());
        let ifaces = mesh_interfaces_for(&mesh, "a", &resolved_for(&mesh)).unwrap();
        assert_eq!(ifaces[0].name, "wg-b");
        let err = render_router_config(&mesh, "a").unwrap_err().to_string();
        assert!(err.contains("\"wg-*\""), "{err}");
        mesh.cluster.direct_interfaces = vec!["wg-*".to_owned()];
        let cfg = render_router_config(&mesh, "a").unwrap();
        assert_eq!(cfg.ospf_interfaces, ["wg-*", "router-lo"]);
    }

    #[test]
    fn mesh_interfaces_for_a_node_with_no_links_is_empty() {
        let mesh: MeshConfig = sliplib_yaml::from_str(mesh_and_roadwarriors_yaml()).unwrap();
        let resolved = resolved_for(&mesh);
        assert!(
            mesh_interfaces_for(&mesh, "c", &resolved)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn roadwarrior_interfaces_for_builds_a_tracked_peer_per_client() {
        let mesh: MeshConfig = sliplib_yaml::from_str(mesh_and_roadwarriors_yaml()).unwrap();
        let resolved = resolved_for(&mesh);
        let ifaces = roadwarrior_interfaces_for(&mesh, "a", &resolved).unwrap();
        assert_eq!(ifaces.len(), 1);
        assert_eq!(ifaces[0].name, "rw-north");
        assert_eq!(ifaces[0].listen_port, 51900);
        assert_eq!(ifaces[0].peers[0].public_key, "client-pubkey");
        assert_eq!(
            ifaces[0].peers[0].allowed_ips,
            Some(vec!["203.0.113.5/32".to_string()])
        );
    }

    #[test]
    fn roadwarrior_interfaces_for_a_pool_without_a_resolved_key_is_an_error() {
        let mesh: MeshConfig = sliplib_yaml::from_str(mesh_and_roadwarriors_yaml()).unwrap();
        let mut resolved = resolved_for(&mesh);
        resolved.roadwarrior_private_keys.clear();
        let err = format!(
            "{:#}",
            roadwarrior_interfaces_for(&mesh, "a", &resolved).unwrap_err()
        );
        says(&err, &["roadwarriors pool \"north\"", "private_key"]);
    }

    #[test]
    fn roadwarrior_interfaces_for_a_node_not_in_node_hostnames_is_empty() {
        let mesh: MeshConfig = sliplib_yaml::from_str(mesh_and_roadwarriors_yaml()).unwrap();
        let resolved = resolved_for(&mesh);
        assert!(
            roadwarrior_interfaces_for(&mesh, "c", &resolved)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn render_awg_config_combines_mesh_and_roadwarrior_interfaces_and_is_valid() {
        let mesh: MeshConfig = sliplib_yaml::from_str(mesh_and_roadwarriors_yaml()).unwrap();
        let resolved = resolved_for(&mesh);
        let cfg = render_awg_config(&mesh, "a", &resolved).unwrap();
        sliplib_config::awg::validate(&cfg).unwrap();
        assert_eq!(cfg.interfaces.len(), 2);
    }

    #[test]
    fn every_rendered_peer_carries_the_name_it_has_in_mesh_yaml() {
        // Straight from slipmesh.yaml: the node on the far end of a link, the client's own name in
        // a pool. It only ever reaches the `peer_name` metric label - nothing decides on it.
        let mesh: MeshConfig = sliplib_yaml::from_str(mesh_and_roadwarriors_yaml()).unwrap();
        let resolved = resolved_for(&mesh);
        let cfg = render_awg_config(&mesh, "a", &resolved).unwrap();

        let mesh_iface = cfg.interfaces.iter().find(|i| i.name == "mesh-b").unwrap();
        assert_eq!(mesh_iface.peers[0].name.as_deref(), Some("b"));

        let rw_iface = cfg
            .interfaces
            .iter()
            .find(|i| i.name == "rw-north")
            .unwrap();
        assert_eq!(rw_iface.peers[0].name.as_deref(), Some("alice"));
    }

    #[test]
    fn render_router_config_runs_the_exporter_only_on_a_configured_port() {
        let base = mesh_and_roadwarriors_yaml();
        let mesh: MeshConfig = sliplib_yaml::from_str(base).unwrap();
        assert_eq!(render_router_config(&mesh, "a").unwrap().metrics, None);

        let yaml = base.replace(
            "  bgp_as: 64512",
            "  bgp_as: 64512
  router_metrics_port: 9324",
        );
        let mesh: MeshConfig = sliplib_yaml::from_str(&yaml).unwrap();
        let listen = render_router_config(&mesh, "a")
            .unwrap()
            .metrics
            .unwrap()
            .listen;
        assert!(listen.ends_with(":9324"), "unexpected listen {listen}");
        assert!(
            listen.starts_with(&node_loopbacks(&mesh, "a").unwrap().0.to_string()),
            "listen {listen} is not this node's own v4 loopback"
        );

        let yaml = base.replace(
            "  bgp_as: 64512",
            "  bgp_as: 64512
  router_metrics_port: 0",
        );
        let mesh: MeshConfig = sliplib_yaml::from_str(&yaml).unwrap();
        assert!(render_router_config(&mesh, "a").is_err());
    }

    #[test]
    fn render_awg_config_refuses_a_metrics_port_of_zero() {
        let yaml = mesh_and_roadwarriors_yaml()
            .replace("  bgp_as: 64512", "  bgp_as: 64512\n  awg_metrics_port: 0");
        let mesh: MeshConfig = sliplib_yaml::from_str(&yaml).unwrap();
        let resolved = resolved_for(&mesh);
        assert!(render_awg_config(&mesh, "a", &resolved).is_err());
    }

    #[test]
    fn render_awg_config_has_no_metrics_section_when_no_port_is_configured() {
        let mesh: MeshConfig = sliplib_yaml::from_str(mesh_and_roadwarriors_yaml()).unwrap();
        let resolved = resolved_for(&mesh);
        let cfg = render_awg_config(&mesh, "a", &resolved).unwrap();
        assert_eq!(cfg.metrics, None);
    }

    #[test]
    fn render_awg_config_binds_metrics_to_this_node_s_own_v4_loopback() {
        let yaml = mesh_and_roadwarriors_yaml().replace(
            "  bgp_as: 64512",
            "  bgp_as: 64512\n  awg_metrics_port: 9586",
        );
        let mesh: MeshConfig = sliplib_yaml::from_str(&yaml).unwrap();
        let resolved = resolved_for(&mesh);

        // The same address `node_loopbacks` gives the router daemon as node.loopback_addresses, so
        // the two cannot drift: node b's own identity, not node a's, not the network's.
        let cfg = render_awg_config(&mesh, "b", &resolved).unwrap();
        assert_eq!(
            cfg.metrics.as_ref().map(|m| m.listen.as_str()),
            Some("198.51.100.2:9586")
        );
        sliplib_config::awg::validate(&cfg).unwrap();
    }

    fn refusal(mesh: &MeshConfig, resolved: &ResolvedSecrets, node: &str) -> String {
        format!(
            "{:#}",
            anyhow::Error::new(render_awg_config(mesh, node, resolved).unwrap_err())
        )
    }

    fn says(err: &str, parts: &[&str]) {
        for part in parts {
            assert!(err.contains(part), "{part:?} missing from: {err}");
        }
    }

    #[test]
    fn a_node_without_its_own_key_is_refused_naming_it_and_the_key() {
        let mesh: MeshConfig = sliplib_yaml::from_str(mesh_and_roadwarriors_yaml()).unwrap();
        let mut resolved = resolved_for(&mesh);
        resolved.mesh_private_keys.remove("a");
        says(
            &refusal(&mesh, &resolved, "a"),
            &["node \"a\"", "mesh_private_key"],
        );
    }

    #[test]
    fn a_node_whose_peer_has_no_key_is_refused_naming_the_peer_and_the_key() {
        let mesh: MeshConfig = sliplib_yaml::from_str(mesh_and_roadwarriors_yaml()).unwrap();
        let mut resolved = resolved_for(&mesh);
        resolved.mesh_private_keys.remove("b");
        says(
            &refusal(&mesh, &resolved, "a"),
            &["node \"b\"", "mesh_private_key"],
        );
    }

    const NOT_A_KEY: &str = "mesh_private_key: key is not valid base64";

    #[test]
    fn a_node_whose_own_key_does_not_decode_is_refused_naming_it_and_the_key() {
        let mesh: MeshConfig = sliplib_yaml::from_str(mesh_and_roadwarriors_yaml()).unwrap();
        for (node, links) in [("a", "one link"), ("c", "no link")] {
            let mut resolved = resolved_for(&mesh);
            resolved
                .mesh_private_keys
                .insert(node.to_owned(), "not a key".into());
            let refusal = refusal(&mesh, &resolved, node);
            assert!(
                refusal.contains(&format!("node {node:?}: {NOT_A_KEY}")),
                "{links}: {refusal}"
            );
        }
    }

    #[test]
    fn a_node_whose_peers_key_does_not_decode_is_refused_naming_the_peer_and_the_key() {
        let mesh: MeshConfig = sliplib_yaml::from_str(mesh_and_roadwarriors_yaml()).unwrap();
        let mut resolved = resolved_for(&mesh);
        resolved
            .mesh_private_keys
            .insert("b".to_owned(), "not a key".into());
        says(
            &refusal(&mesh, &resolved, "a"),
            &[&format!("node \"b\": {NOT_A_KEY}")],
        );
    }

    fn partial() -> Obfuscation {
        Obfuscation {
            jc: Some(4),
            s4: Some(12),
            ..Obfuscation::default()
        }
    }

    const UNMINTED: &str = "jmin, jmax, s1, s2, h1, h2, h3, h4";

    #[test]
    fn a_link_with_part_of_its_obfuscation_is_refused_naming_the_link_and_what_it_lacks() {
        let mesh: MeshConfig = sliplib_yaml::from_str(mesh_and_roadwarriors_yaml()).unwrap();
        let mut resolved = resolved_for(&mesh);
        resolved
            .mesh_link_obfuscation
            .insert("a|b".to_owned(), partial());
        says(
            &refusal(&mesh, &resolved, "b"),
            &["mesh link a-b", UNMINTED],
        );
    }

    #[test]
    fn a_pool_with_part_of_its_obfuscation_is_refused_naming_the_pool_and_what_it_lacks() {
        let mesh: MeshConfig = sliplib_yaml::from_str(mesh_and_roadwarriors_yaml()).unwrap();
        let mut resolved = resolved_for(&mesh);
        resolved
            .roadwarrior_obfuscation
            .insert("north".to_owned(), partial());
        says(
            &refusal(&mesh, &resolved, "a"),
            &["roadwarriors pool \"north\"", UNMINTED],
        );
    }

    #[test]
    fn a_link_or_pool_with_no_obfuscation_read_is_refused_naming_it() {
        let mesh: MeshConfig = sliplib_yaml::from_str(mesh_and_roadwarriors_yaml()).unwrap();
        let mut resolved = resolved_for(&mesh);
        resolved.mesh_link_obfuscation.clear();
        says(
            &refusal(&mesh, &resolved, "b"),
            &["mesh link a-b: obfuscation is not set"],
        );
        let mut resolved = resolved_for(&mesh);
        resolved.roadwarrior_obfuscation.clear();
        says(
            &refusal(&mesh, &resolved, "a"),
            &["roadwarriors pool \"north\": obfuscation is not set"],
        );
    }

    #[test]
    fn a_plain_link_and_pool_render_without_obfuscation() {
        let mut mesh: MeshConfig = sliplib_yaml::from_str(mesh_and_roadwarriors_yaml()).unwrap();
        mesh.mesh.links[0].plain = true;
        mesh.roadwarriors[0].plain = true;
        let cfg = render_awg_config(&mesh, "a", &resolved_for(&mesh)).unwrap();
        for iface in &cfg.interfaces {
            assert_eq!(iface.obfuscation, Obfuscation::default(), "{}", iface.name);
        }
    }

    #[test]
    fn node_configs_are_the_rendered_ones() {
        let mesh: MeshConfig = sliplib_yaml::from_str(mesh_and_roadwarriors_yaml()).unwrap();
        let resolved = resolved_for(&mesh);
        let configs = node_configs(&mesh, "a", &resolved).unwrap();
        assert_eq!(
            configs.awg,
            render_awg_config(&mesh, "a", &resolved).unwrap()
        );
        assert_eq!(configs.router, render_router_config(&mesh, "a").unwrap());
    }

    fn node_configs_refusal(yaml: &str, resolved: Option<ResolvedSecrets>) -> String {
        let mesh: MeshConfig = sliplib_yaml::from_str(yaml).unwrap();
        let resolved = resolved.unwrap_or_else(|| resolved_for(&mesh));
        format!(
            "{:#}",
            anyhow::Error::new(node_configs(&mesh, "a", &resolved).unwrap_err())
        )
    }

    #[test]
    fn node_configs_refuse_an_awg_config_that_does_not_render() {
        let mesh: MeshConfig = sliplib_yaml::from_str(mesh_and_roadwarriors_yaml()).unwrap();
        let mut resolved = resolved_for(&mesh);
        resolved.mesh_private_keys.remove("a");
        let err = node_configs_refusal(mesh_and_roadwarriors_yaml(), Some(resolved));
        says(&err, &["rendering awg config for node \"a\""]);
    }

    #[test]
    fn node_configs_refuse_an_awg_config_the_daemon_refuses() {
        let yaml = mesh_and_roadwarriors_yaml()
            .replace("name: b,", "name: a-long-name,")
            .replace("pair: [a, b]", "pair: [a, a-long-name]");
        says(
            &node_configs_refusal(&yaml, None),
            &["rendered awg config for node \"a\" is invalid"],
        );
    }

    #[test]
    fn node_configs_refuse_an_obfuscation_the_kernel_module_refuses() {
        let mesh: MeshConfig = sliplib_yaml::from_str(mesh_and_roadwarriors_yaml()).unwrap();
        let mut resolved = resolved_for(&mesh);
        let clash = Obfuscation {
            h2: Some(5.into()),
            ..crate::test_support::minted_obfuscation()
        };
        resolved
            .mesh_link_obfuscation
            .insert("a|b".to_owned(), clash);
        let err = node_configs_refusal(mesh_and_roadwarriors_yaml(), Some(resolved));
        says(
            &err,
            &[
                "node \"a\"",
                "interface \"mesh-b\"",
                "kernel module refuses",
            ],
        );
    }

    #[test]
    fn node_configs_refuse_a_router_config_that_does_not_render() {
        let yaml = mesh_and_roadwarriors_yaml().replace(
            "  loopback_networks:",
            "  tunnel_networks: {ipv4: \"198.51.100.128/25\", ipv6: \"fe80::/64\"}\n  loopback_networks:",
        );
        says(
            &node_configs_refusal(&yaml, None),
            &["rendering router config for node \"a\"", "mesh-*"],
        );
    }

    #[test]
    fn node_configs_refuse_a_router_config_the_daemon_refuses() {
        let yaml = mesh_and_roadwarriors_yaml().replace("bgp_as: 64512", "bgp_as: 0");
        says(
            &node_configs_refusal(&yaml, None),
            &["rendered router config for node \"a\" is invalid", "bgp_as"],
        );
    }

    /// Whether what refuses node `a` of `mesh` with `resolved` is marked as a missing secret.
    fn is_a_missing_secret(mesh: &MeshConfig, resolved: &ResolvedSecrets) -> bool {
        node_configs(mesh, "a", resolved)
            .unwrap_err()
            .missing_secret()
            .is_some()
    }

    #[test]
    fn a_refusal_for_a_key_or_obfuscation_the_file_lacks_is_marked_as_a_missing_secret() {
        let mesh: MeshConfig = sliplib_yaml::from_str(mesh_and_roadwarriors_yaml()).unwrap();
        type Take = fn(&mut ResolvedSecrets);
        let cases: [(&str, Take); 6] = [
            ("the node's own key", |r| {
                r.mesh_private_keys.remove("a");
            }),
            ("a peer's key", |r| {
                r.mesh_private_keys.remove("b");
            }),
            ("a pool's key", |r| r.roadwarrior_private_keys.clear()),
            ("a link's obfuscation in part", |r| {
                r.mesh_link_obfuscation.insert("a|b".to_owned(), partial());
            }),
            ("a pool's obfuscation in part", |r| {
                r.roadwarrior_obfuscation
                    .insert("north".to_owned(), partial());
            }),
            ("a link's obfuscation", |r| r.mesh_link_obfuscation.clear()),
        ];
        for (lacking, take) in cases {
            let mut resolved = resolved_for(&mesh);
            take(&mut resolved);
            assert!(is_a_missing_secret(&mesh, &resolved), "{lacking}");
        }
    }

    #[test]
    fn a_refusal_that_no_secret_cures_is_not_marked_as_a_missing_secret() {
        let unrendered = mesh_and_roadwarriors_yaml()
            .replace("bgp_as: 64512", "bgp_as: 64512\n  awg_metrics_port: 0");
        let invalid = mesh_and_roadwarriors_yaml().replace("bgp_as: 64512", "bgp_as: 0");
        for yaml in [unrendered, invalid] {
            let mesh: MeshConfig = sliplib_yaml::from_str(&yaml).unwrap();
            assert!(!is_a_missing_secret(&mesh, &resolved_for(&mesh)), "{yaml}");
        }

        let mesh: MeshConfig = sliplib_yaml::from_str(mesh_and_roadwarriors_yaml()).unwrap();
        let mut resolved = resolved_for(&mesh);
        let clash = Obfuscation {
            h2: Some(5.into()),
            ..crate::test_support::minted_obfuscation()
        };
        resolved
            .mesh_link_obfuscation
            .insert("a|b".to_owned(), clash);
        assert!(!is_a_missing_secret(&mesh, &resolved));

        let mut resolved = resolved_for(&mesh);
        resolved
            .mesh_private_keys
            .insert("a".to_owned(), "not a key".into());
        assert!(!is_a_missing_secret(&mesh, &resolved));
    }

    /// Fixed 32-byte keys, plain text once decoded, so none can be mistaken for a real one.
    fn written_key(label: &str) -> String {
        let mut bytes = [0u8; 32];
        for (b, l) in bytes.iter_mut().zip(label.bytes().cycle()) {
            *b = l;
        }
        sliplib_secrets::wireguard::encode_key(&bytes)
    }

    #[test]
    fn a_file_with_every_secret_written_renders_both_ends_of_a_link_alike() {
        let yaml = format!(
            r#"
cluster:
  bgp_as: 64512
  loopback_networks: {{ipv4: "198.51.100.0/25", ipv6: "3fff::/32"}}
obfuscation: {{h1: 101, h2: 102, h3: 103, h4: 104}}
nodes:
  - {{name: a, node_id: "198.51.100.1", endpoint: "192.0.2.10", mesh_private_key: "{a}"}}
  - {{name: b, node_id: "198.51.100.2", endpoint: "192.0.2.11", mesh_private_key: "{b}"}}
mesh:
  links:
    - pair: [a, b]
      port: 51820
      obfuscation: {{jc: 4, jmin: 10, jmax: 50, s1: 15, s2: 16}}
"#,
            a = written_key("node-a"),
            b = written_key("node-b"),
        );
        let mesh: MeshConfig = sliplib_yaml::from_str(&yaml).unwrap();
        let resolved = crate::secrets::read(&lowered(&mesh).unwrap());
        let iface = |node: &str| {
            let configs = node_configs(&mesh, node, &resolved).unwrap();
            let [iface] = <[_; 1]>::try_from(configs.awg.interfaces).unwrap();
            iface
        };
        let (on_a, on_b) = (iface("a"), iface("b"));
        let expected = Obfuscation {
            jc: Some(4),
            jmin: Some(10),
            jmax: Some(50),
            s1: Some(15),
            s2: Some(16),
            h1: Some(101.into()),
            h2: Some(102.into()),
            h3: Some(103.into()),
            h4: Some(104.into()),
            ..Obfuscation::default()
        };
        assert_eq!(on_a.obfuscation, expected);
        assert_eq!(on_b.obfuscation, expected);
        assert_eq!(
            on_a.peers[0].public_key,
            on_b.private_key.public_key().unwrap()
        );
        assert_eq!(
            on_b.peers[0].public_key,
            on_a.private_key.public_key().unwrap()
        );
    }

    #[test]
    fn a_secret_of_a_node_link_or_pool_the_rendered_node_does_not_touch_is_not_refused() {
        let yaml = mesh_and_roadwarriors_yaml()
            .replace(
                "    - {pair: [a, b], port: 51820}",
                "    - {pair: [a, b], port: 51820}\n    - {pair: [a, c], port: 51821}",
            )
            .replace(
                "{name: c, node_id: \"198.51.100.3\"}",
                "{name: c, node_id: \"198.51.100.3\", endpoint: \"c.example.com\"}",
            );
        let mesh: MeshConfig = sliplib_yaml::from_str(&yaml).unwrap();
        let mut resolved = resolved_for(&mesh);
        assert!(resolved.mesh_private_keys.remove("c").is_some());
        assert!(resolved.mesh_link_obfuscation.remove("a|c").is_some());
        assert!(resolved.roadwarrior_private_keys.remove("north").is_some());
        assert!(resolved.roadwarrior_obfuscation.remove("north").is_some());
        node_configs(&mesh, "b", &resolved).unwrap();
    }
}
