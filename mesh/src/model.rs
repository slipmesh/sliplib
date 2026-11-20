//! The network as the configs of its nodes are computed from it, whatever version of the format
//! it was written in: every node with the addresses derived for it, every link with what is in
//! force on it. Each version of the format is lowered into this, and nothing else is computed
//! from.
//!
//! It refuses nothing: it can hold every file of version 1 that reads. What only one version
//! refuses is refused where that version is read and lowered.

use crate::secrets::link_key;
use sliplib_config::router::{AnnounceEntry, BfdSettings};
use sliplib_config::slipmesh_file::SlipmeshFile;
use sliplib_config::v1::mesh_config::{BypassEntry, RoadwarriorPool, UnknownNode};
use sliplib_config::v2::Platform;
pub use sliplib_config::v2::network::Family;
use sliplib_config::v2::platform::TalosSettings;
use sliplib_config::{Obfuscation, Range};
use sliplib_secrets::wireguard::PrivateKey;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::num::NonZeroU16;

#[derive(Debug)]
pub struct Network {
    /// In the order written, which a node's iBGP peers follow.
    pub nodes: Vec<Node>,
    /// In the order written, which a node's interfaces follow.
    pub links: Vec<Link>,
    pub bgp_as: u32,
    /// Present, every link has a BFD session.
    pub bfd: Option<BfdSettings>,
    /// What the name of every link's interface starts with.
    pub interface_prefix: String,
    /// The MTU the files set for the mesh, which may only lower the one computed.
    pub mtu: Option<u32>,
    /// The classes of routes by name, each with its protocol number.
    pub classes: Vec<(String, u8)>,
    pub metrics: MetricsPorts,
    pub format: Format,
}

/// The version of the format a network was read from, where what is computed or said of it
/// depends on that: how a message names a key, which nodes have one, and what only version 1
/// holds.
#[derive(Debug)]
pub enum Format {
    V1(Box<V1Only>),
    V2,
}

impl Format {
    /// What only a file of version 1 holds.
    pub fn v1(&self) -> Option<&V1Only> {
        match self {
            Self::V1(v1) => Some(v1),
            Self::V2 => None,
        }
    }

    /// How the files name a node's private key, under the node.
    pub fn private_key(&self) -> &'static str {
        match self {
            Self::V1(_) => "mesh_private_key",
            Self::V2 => "wireguard.private_key",
        }
    }

    /// How the files name the MTU set for the mesh.
    pub fn mtu(&self) -> &'static str {
        match self {
            Self::V1(_) => "cluster.mesh_mtu",
            Self::V2 => "mesh.mtu",
        }
    }

    /// How the files name the network the links' IPv4 addresses come from.
    pub fn tunnel_ipv4(&self) -> &'static str {
        match self {
            Self::V1(_) => "cluster.tunnel_networks",
            Self::V2 => "addressing.tunnel.ipv4",
        }
    }

    /// Whether every node has a key: version 2 has one created only for a node on a link.
    pub fn keys_every_node(&self) -> bool {
        matches!(self, Self::V1(_))
    }

    /// Where the files hold the key of `node`, as a message names it.
    pub fn node_key(&self, node: &str) -> String {
        match self {
            Self::V1(_) => format!("nodes[{node}].mesh_private_key"),
            Self::V2 => format!("nodes.{node}.wireguard.private_key"),
        }
    }

    /// Where the files hold the obfuscation of `link`, as a message names it. Version 1 gives a
    /// link no name, and it is named by its two nodes, sorted.
    pub fn link_obfuscation(&self, link: &Link) -> String {
        match self {
            Self::V1(_) => format!("mesh.links[{}].obfuscation", link_key(&link.ends)),
            Self::V2 => format!("mesh.links.{}.amneziawg.obfuscation", link.name),
        }
    }
}

impl Network {
    pub fn node(&self, name: &str) -> Result<&Node, UnknownNode> {
        self.nodes
            .iter()
            .find(|node| node.name == name)
            .ok_or_else(|| UnknownNode(name.to_owned()))
    }

    /// Whether the node needs a private key: it is on a link, or the version says every node does.
    pub fn needs_a_key(&self, node: &str) -> bool {
        self.format.keys_every_node()
            || self
                .links
                .iter()
                .any(|link| link.ends.iter().any(|end| end == node))
    }
}

/// The ports a node's daemons serve their metrics on, on its own IPv4 loopback. None, no
/// listener.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct MetricsPorts {
    pub awg: Option<NonZeroU16>,
    pub router: Option<NonZeroU16>,
}

impl From<&TalosSettings> for MetricsPorts {
    fn from(settings: &TalosSettings) -> Self {
        let metrics = settings.metrics.as_ref();
        Self {
            awg: metrics
                .and_then(|metrics| metrics.mesh.as_ref())
                .map(|listener| listener.port),
            router: metrics
                .and_then(|metrics| metrics.router.as_ref())
                .map(|listener| listener.port),
        }
    }
}

#[derive(Debug)]
pub struct Node {
    pub name: String,
    /// Whose tool configures the node. A file of version 1 does not say.
    pub platform: Option<Platform>,
    pub loopback: (Ipv4Addr, Ipv6Addr),
    /// The IPv6 link-local of every link's interface on the node.
    pub link_local: Ipv6Addr,
    /// The IPv4 address of every link's interface on the node, where the network gives links one.
    pub tunnel_ipv4: Option<Ipv4Addr>,
    pub private_key: Option<PrivateKey>,
    /// Where the node's peers dial it. None, it is never dialed.
    pub endpoint: Option<Endpoint>,
    pub underlay_mtu: u32,
    /// How a message names the key of `underlay_mtu`, under the node.
    pub underlay_mtu_key: String,
    pub direct_interfaces: Vec<String>,
    pub learn: Vec<String>,
    pub announce: Vec<AnnounceEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    /// The address or host name as the files write it, which a peer dials with the link's port.
    pub written: String,
    /// The family it is dialed over.
    pub family: Family,
}

#[derive(Debug)]
pub struct Link {
    /// How a message names the link.
    pub name: String,
    /// The names of the two nodes it joins.
    pub ends: [String; 2],
    /// The UDP port both ends listen on.
    pub port: u16,
    /// Plain WireGuard: it carries no obfuscation and needs none created.
    pub plain: bool,
    /// The obfuscation in force on the link, empty on a plain one.
    pub obfuscation: Obfuscation,
    /// The keepalive the files set for the link. None, each end takes the default of what its
    /// interface speaks.
    pub persistent_keepalive: Option<Range<u16>>,
}

/// What a file of version 1 describes besides the mesh.
#[derive(Debug)]
pub struct V1Only {
    pub pools: Vec<RoadwarriorPool>,
    /// The obfuscation a pool starts from.
    pub obfuscation: Obfuscation,
    pub bypass: Vec<BypassEntry>,
    pub bypass_refresh_interval_secs: Option<u64>,
}

/// Why a file that reads gives no network to compute from, as the lowering of its version says
/// it.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error(transparent)]
    V1(#[from] crate::v1::Error),
    #[error(transparent)]
    V2(#[from] crate::v2::Error),
}

/// `file` as the network its nodes' configs are computed from, whichever version it is of.
pub fn lower(file: &SlipmeshFile) -> Result<Network, Error> {
    Ok(match file {
        SlipmeshFile::V1(file) => crate::v1::lower(file.topology())?,
        SlipmeshFile::V2(file) => crate::v2::lower(file)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const VERSION_1: &str = "slipmesh: {kind: network}\ncluster:\n  bgp_as: 64500\n  \
        loopback_networks: {ipv4: 198.51.100.0/25, ipv6: '3fff:0:0:1::/64'}\nnodes:\n  - {name: \
        node-a, node_id: 0.0.0.1}\n";
    const VERSION_2: &str = include_str!("../../docs/example/mesh.yaml");

    fn lowered(raw: &str) -> Result<Network, Error> {
        lower(&SlipmeshFile::parse(raw).unwrap())
    }

    #[test]
    fn the_metrics_ports_are_those_of_the_settings_of_talos() {
        let settings = |yaml: &str| -> TalosSettings { sliplib_yaml::from_str(yaml).unwrap() };
        let port = NonZeroU16::new;
        assert_eq!(
            MetricsPorts::from(&settings(
                "metrics: {mesh: {port: 9586}, router: {port: 9324}}"
            )),
            MetricsPorts {
                awg: port(9586),
                router: port(9324)
            }
        );
        assert_eq!(
            MetricsPorts::from(&settings("metrics: {router: {port: 9324}}")),
            MetricsPorts {
                awg: None,
                router: port(9324)
            }
        );
        assert_eq!(MetricsPorts::from(&settings("{}")), MetricsPorts::default());
    }

    #[test]
    fn a_file_of_either_version_is_lowered_by_its_own() {
        let of_version_1 = lowered(VERSION_1).unwrap();
        assert!(matches!(of_version_1.format, Format::V1(_)));
        let of_version_2 = lowered(VERSION_2).unwrap();
        assert!(matches!(of_version_2.format, Format::V2));
    }

    #[test]
    fn a_refusal_is_the_one_the_lowering_of_the_version_gives() {
        let of_version_1 =
            lowered(&VERSION_1.replace("198.51.100.0/25", "198.51.100.0")).unwrap_err();
        assert!(
            matches!(
                of_version_1,
                Error::V1(crate::v1::Error::Network {
                    field: "cluster.loopback_networks.ipv4",
                    ..
                })
            ),
            "{of_version_1:?}"
        );
        let of_version_2 =
            lowered(&VERSION_2.replace("  interface_prefix: mesh-\n", "")).unwrap_err();
        assert!(
            matches!(of_version_2, Error::V2(crate::v2::Error::NoInterfacePrefix)),
            "{of_version_2:?}"
        );
    }
}
