//! A topology of format version 1 as the network the configs are computed from.

use crate::addressing;
use crate::model::{Endpoint, Family, Format, Link, MetricsPorts, Network, Node, V1Only};
use crate::mtu::DEFAULT_UPLINK_MTU;
use crate::secrets::unless_plain;
use sliplib_config::router::AnnounceEntry;
use sliplib_config::v1::mesh_config::{AddressFamily, MeshConfig, NodeEntry};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::num::NonZeroU16;

/// Why a topology of version 1, which reads, gives no network to compute from.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
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
    #[error("cluster.{0} is 0 - remove the field to serve no metrics")]
    MetricsPortZero(&'static str),
    #[error("node {node:?}: address_family {said:?} contradicts its endpoint {endpoint}")]
    FamilyContradicts {
        node: String,
        said: AddressFamily,
        endpoint: String,
    },
}

/// The label of the Kubernetes service range in a node's routing configuration.
const SERVICE_SUBNET_LABEL: &str = "k8s-services";

/// The name of the one class of routes a file of version 1 numbers.
const EGRESS_CLASS: &str = "egress";

/// An IPv4 and an IPv6 network, each as its address and prefix length.
type Networks = (Ipv4Addr, u8, Ipv6Addr, u8);

fn network(field: &'static str) -> impl Fn(sliplib_net::ParseError) -> Error {
    move |source| Error::Network { field, source }
}

fn loopback_networks(mesh: &MeshConfig) -> Result<Networks, Error> {
    let v4 = sliplib_net::parse_ipv4(&mesh.cluster.loopback_networks.ipv4)
        .map_err(network("cluster.loopback_networks.ipv4"))?;
    let v6 = sliplib_net::parse_ipv6(&mesh.cluster.loopback_networks.ipv6)
        .map_err(network("cluster.loopback_networks.ipv6"))?;
    Ok((v4.addr(), v4.prefix_len(), v6.addr(), v6.prefix_len()))
}

fn tunnel_networks(mesh: &MeshConfig) -> Result<Option<Networks>, Error> {
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
        addressing::ipv4_in(v4, v4_prefix, node_id),
        addressing::ipv6_in(v6, v6_prefix, node_id),
    )
}

fn family(family: AddressFamily) -> Family {
    match family {
        AddressFamily::Ipv4 => Family::Ipv4,
        AddressFamily::Ipv6 => Family::Ipv6,
    }
}

/// Where the node's peers dial it, over the family of its address, or the one `address_family`
/// says for a host name, IPv6 when it says none. An `address_family` that contradicts an address
/// is refused for a node on a link alone: no other is dialed, and version 1 refused no other.
fn endpoint(entry: &NodeEntry, on_a_link: bool) -> Result<Option<Endpoint>, Error> {
    let Some(written) = &entry.endpoint else {
        return Ok(None);
    };
    let family = match written.parse::<IpAddr>() {
        Ok(address) => {
            let of_address = if address.is_ipv4() {
                Family::Ipv4
            } else {
                Family::Ipv6
            };
            match entry.address_family {
                Some(said) if on_a_link && family(said) != of_address => {
                    return Err(Error::FamilyContradicts {
                        node: entry.name.clone(),
                        said,
                        endpoint: written.clone(),
                    });
                }
                _ => of_address,
            }
        }
        Err(_) => entry.address_family.map_or(Family::Ipv6, family),
    };
    Ok(Some(Endpoint {
        written: written.clone(),
        family,
    }))
}

/// A metrics port, where `field` names one: 0 serves nothing, as leaving it out does, so it is a
/// mistake.
fn metrics_port(port: Option<u16>, field: &'static str) -> Result<Option<NonZeroU16>, Error> {
    port.map(|port| NonZeroU16::new(port).ok_or(Error::MetricsPortZero(field)))
        .transpose()
}

fn node(
    mesh: &MeshConfig,
    entry: &NodeEntry,
    loopbacks: Networks,
    tunnels: Option<Networks>,
) -> Result<Node, Error> {
    let node_id: Ipv4Addr = entry.node_id.parse().map_err(|source| Error::NodeId {
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
        endpoint: endpoint(
            entry,
            mesh.mesh
                .links
                .iter()
                .any(|link| link.pair.contains(&entry.name)),
        )?,
        underlay_mtu: entry.uplink_mtu.unwrap_or(DEFAULT_UPLINK_MTU),
        underlay_mtu_key: "uplink_mtu".to_owned(),
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
pub fn lower(mesh: &MeshConfig) -> Result<Network, Error> {
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
            awg: metrics_port(cluster.awg_metrics_port, "awg_metrics_port")?,
            router: metrics_port(cluster.router_metrics_port, "router_metrics_port")?,
        },
        format: Format::V1,
        v1: Some(V1Only {
            pools: mesh.roadwarriors.clone(),
            obfuscation: mesh.obfuscation.clone(),
            bypass: mesh.bypass.clone(),
            bypass_refresh_interval_secs: cluster.bypass_refresh_interval_secs,
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two nodes, `a` dialed at an IPv4 literal; `cluster` adds lines to the cluster block, `a`
    /// fields to node `a`, and `links` is the list of links.
    fn topology(cluster: &str, a: &str, links: &str) -> MeshConfig {
        sliplib_yaml::from_str(&format!(
            "cluster:\n  bgp_as: 64512\n  loopback_networks: {{ipv4: 198.51.100.0/25, ipv6: \
             '3fff::/32'}}\n{cluster}nodes:\n  - {{name: a, node_id: 198.51.100.1, endpoint: \
             192.0.2.10{a}}}\n  - {{name: b, node_id: 198.51.100.2}}\nmesh:\n  links: {links}\n"
        ))
        .unwrap()
    }

    const LINKED: &str = "[{pair: [a, b], port: 51820, plain: true}]";

    #[test]
    fn every_node_of_version_1_needs_a_key_on_a_link_or_not() {
        let network = lower(&topology("", "", "[]")).unwrap();
        let resolved = crate::secrets::read(&network);
        for node in ["a", "b"] {
            let refusal = crate::render::node_configs(&network, node, &resolved).unwrap_err();
            assert!(refusal.missing_secret().is_some(), "{node}: {refusal:?}");
        }
    }

    #[cfg(feature = "mint")]
    #[test]
    fn mint_creates_a_key_for_every_node_of_version_1() {
        let network = lower(&topology("", "", "[]")).unwrap();
        let minted = crate::mint::create(&network).unwrap();
        let keyed: Vec<usize> = minted.node_keys.iter().map(|(node, _)| *node).collect();
        assert_eq!(keyed, [0, 1]);
    }

    #[test]
    fn a_metrics_port_of_zero_is_refused_by_its_field() {
        for field in ["awg_metrics_port", "router_metrics_port"] {
            let refusal = lower(&topology(&format!("  {field}: 0\n"), "", LINKED)).unwrap_err();
            assert!(
                matches!(refusal, Error::MetricsPortZero(named) if named == field),
                "{refusal:?}"
            );
        }
        let served = lower(&topology("  awg_metrics_port: 9586\n", "", LINKED)).unwrap();
        assert_eq!(served.metrics.awg.map(u16::from), Some(9586));
    }

    #[test]
    fn an_address_family_that_contradicts_the_endpoint_of_a_node_on_a_link_is_refused() {
        let said = ", address_family: ipv6";
        let refusal = lower(&topology("", said, LINKED)).unwrap_err();
        assert!(
            matches!(&refusal, Error::FamilyContradicts { node, said: AddressFamily::Ipv6, endpoint }
                if node == "a" && endpoint == "192.0.2.10"),
            "{refusal:?}"
        );
        // A node on no link is dialed by nobody, and its family is never counted.
        lower(&topology("", said, "[]")).unwrap();
    }
}
