//! A topology of format version 1 as the network the configs are computed from.

use crate::addressing;
use crate::model::{Endpoint, Keys, Link, MetricsPorts, Network, Node, V1Only};
use crate::mtu::DEFAULT_UPLINK_MTU;
use crate::render::RenderError;
use crate::secrets::unless_plain;
use sliplib_config::mesh_config::{MeshConfig, NodeEntry};
use sliplib_config::router::AnnounceEntry;
use std::net::{Ipv4Addr, Ipv6Addr};

/// The label of the Kubernetes service range in a node's routing configuration.
const SERVICE_SUBNET_LABEL: &str = "k8s-services";

/// The name of the one class of routes a file of version 1 numbers.
const EGRESS_CLASS: &str = "egress";

/// An IPv4 and an IPv6 network, each as its address and prefix length.
type Networks = (Ipv4Addr, u8, Ipv6Addr, u8);

fn network(field: &'static str) -> impl Fn(sliplib_net::ParseError) -> RenderError {
    move |source| RenderError::Network { field, source }
}

fn loopback_networks(mesh: &MeshConfig) -> Result<Networks, RenderError> {
    let v4 = sliplib_net::parse_ipv4(&mesh.cluster.loopback_networks.ipv4)
        .map_err(network("cluster.loopback_networks.ipv4"))?;
    let v6 = sliplib_net::parse_ipv6(&mesh.cluster.loopback_networks.ipv6)
        .map_err(network("cluster.loopback_networks.ipv6"))?;
    Ok((v4.addr(), v4.prefix_len(), v6.addr(), v6.prefix_len()))
}

fn tunnel_networks(mesh: &MeshConfig) -> Result<Option<Networks>, RenderError> {
    let Some(tn) = &mesh.cluster.tunnel_networks else {
        return Ok(None);
    };
    let v4 = sliplib_net::parse_ipv4(&tn.ipv4).map_err(network("cluster.tunnel_networks.ipv4"))?;
    let v6 = sliplib_net::parse_ipv6(&tn.ipv6).map_err(network("cluster.tunnel_networks.ipv6"))?;
    Ok(Some((
        v4.addr(),
        v4.prefix_len(),
        v6.addr(),
        v6.prefix_len(),
    )))
}

/// The two addresses `node_id` gives in `networks`: itself in the host part of each.
fn addresses((v4, v4_prefix, v6, v6_prefix): Networks, node_id: Ipv4Addr) -> (Ipv4Addr, Ipv6Addr) {
    (
        addressing::ipv4_loopback(v4, v4_prefix, node_id),
        addressing::ipv6_loopback(v6, v6_prefix, node_id),
    )
}

fn node(
    mesh: &MeshConfig,
    entry: &NodeEntry,
    loopbacks: Networks,
    tunnels: Option<Networks>,
) -> Result<Node, RenderError> {
    let node_id: Ipv4Addr = entry
        .node_id
        .parse()
        .map_err(|source| RenderError::NodeId {
            node: entry.name.clone(),
            source,
        })?;
    let loopback = addresses(loopbacks, node_id);
    // With `tunnel_networks`, its IPv6 half is the interface's only link-local, replacing the one
    // derived from the loopback: OSPFv3 takes any link-scoped address, and with two it is unclear
    // which one BIRD sends from.
    let (link_local, tunnel_ipv4) = match tunnels.map(|tunnels| addresses(tunnels, node_id)) {
        Some((v4, v6)) => (v6, Some(v4)),
        None => (addressing::link_local_from_loopback(loopback.1), None),
    };
    let cluster = &mesh.cluster;
    Ok(Node {
        name: entry.name.clone(),
        platform: None,
        loopback,
        link_local,
        tunnel_ipv4,
        private_key: entry.mesh_private_key.clone(),
        endpoint: entry.endpoint.as_ref().map(|written| Endpoint {
            written: written.clone(),
            stated_family: entry.address_family,
        }),
        underlay_mtu: entry.uplink_mtu.unwrap_or(DEFAULT_UPLINK_MTU),
        // The node's own list replaces the cluster's. Nothing is added to it: what a node
        // announces has to be readable in the file.
        direct_interfaces: entry
            .direct_interfaces
            .clone()
            .unwrap_or_else(|| cluster.direct_interfaces.clone()),
        // The pod range, on every node. A non-empty `learn` also re-announces what it learns
        // over iBGP; the two are one mechanism.
        learn: cluster.pod_subnet.iter().cloned().collect(),
        announce: cluster
            .service_subnet
            .iter()
            .map(|net| AnnounceEntry {
                net: net.clone(),
                label: Some(SERVICE_SUBNET_LABEL.to_owned()),
            })
            .collect(),
    })
}

/// `mesh` as a network. Refuses what no address can be derived from: a loopback or tunnel
/// network that does not read.
pub fn lower(mesh: &MeshConfig) -> Result<Network, RenderError> {
    let loopbacks = loopback_networks(mesh)?;
    let tunnels = tunnel_networks(mesh)?;
    let cluster = &mesh.cluster;
    Ok(Network {
        nodes: mesh
            .nodes
            .iter()
            .map(|entry| node(mesh, entry, loopbacks, tunnels))
            .collect::<Result<_, _>>()?,
        links: mesh
            .mesh
            .links
            .iter()
            .map(|link| Link {
                name: format!("{}-{}", link.pair[0], link.pair[1]),
                ends: link.pair.clone(),
                port: link.port,
                plain: link.plain,
                obfuscation: unless_plain(link.plain, &link.obfuscation, &mesh.obfuscation),
                persistent_keepalive: None,
            })
            .collect(),
        bgp_as: cluster.bgp_as,
        bfd: mesh.bfd.enable.then(|| mesh.bfd.settings.clone()),
        interface_prefix: cluster.mesh_interface_prefix().to_owned(),
        mtu: cluster.mesh_mtu,
        classes: vec![(EGRESS_CLASS.to_owned(), cluster.egress_route_proto_id())],
        metrics: MetricsPorts {
            awg: cluster.awg_metrics_port,
            router: cluster.router_metrics_port,
        },
        keys: Keys::V1,
        v1: Some(V1Only {
            pools: mesh.roadwarriors.clone(),
            obfuscation: mesh.obfuscation.clone(),
            bypass: mesh.bypass.clone(),
            bypass_refresh_interval_secs: cluster.bypass_refresh_interval_secs,
        }),
    })
}
