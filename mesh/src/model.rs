//! The network as the configs of its nodes are computed from it, whatever version of the format
//! it was written in: every node with the addresses derived for it, every link with what is in
//! force on it. Each version of the format is lowered into this, and nothing else is computed
//! from.
//!
//! It refuses nothing: it can hold every file of version 1 that reads. What only one version
//! refuses is refused where that version is read and lowered.

use crate::render::RenderError;
use sliplib_config::router::{AnnounceEntry, BfdSettings};
use sliplib_config::slipmesh_file::SlipmeshFile;
use sliplib_config::v1::mesh_config::{AddressFamily, BypassEntry, RoadwarriorPool, UnknownNode};
use sliplib_config::v2::Platform;
use sliplib_config::{Obfuscation, Range};
use sliplib_secrets::wireguard::PrivateKey;
use std::net::{Ipv4Addr, Ipv6Addr};

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
    pub keys: Keys,
    /// What only a file of version 1 holds.
    pub v1: Option<V1Only>,
}

/// How a message names the keys of the files, which differ between the versions of the format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Keys {
    /// A node's private key, under the node.
    pub private_key: &'static str,
    /// The MTU set for the mesh.
    pub mtu: &'static str,
    /// The MTU of a node's underlay, under the node.
    pub underlay_mtu: &'static str,
}

impl Keys {
    pub const V1: Self = Self {
        private_key: "mesh_private_key",
        mtu: "cluster.mesh_mtu",
        underlay_mtu: "uplink_mtu",
    };
    pub const V2: Self = Self {
        private_key: "wireguard.private_key",
        mtu: "mesh.mtu",
        underlay_mtu: "underlay.<entry>.mtu",
    };
}

impl Network {
    pub fn node(&self, name: &str) -> Result<&Node, UnknownNode> {
        self.nodes
            .iter()
            .find(|node| node.name == name)
            .ok_or_else(|| UnknownNode(name.to_owned()))
    }
}

/// The ports a node's daemons serve their metrics on, on its own IPv4 loopback. None, no
/// listener.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct MetricsPorts {
    pub awg: Option<u16>,
    pub router: Option<u16>,
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
    pub direct_interfaces: Vec<String>,
    pub learn: Vec<String>,
    pub announce: Vec<AnnounceEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    /// The address or host name as the files write it, which a peer dials with the link's port.
    pub written: String,
    /// The family the files say it is dialed over, where they say one.
    pub stated_family: Option<AddressFamily>,
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
    V1(#[from] RenderError),
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
    fn a_file_of_either_version_is_lowered_by_its_own() {
        let of_version_1 = lowered(VERSION_1).unwrap();
        assert_eq!(of_version_1.keys, Keys::V1);
        assert!(of_version_1.v1.is_some());
        let of_version_2 = lowered(VERSION_2).unwrap();
        assert_eq!(of_version_2.keys, Keys::V2);
        assert!(of_version_2.v1.is_none());
    }

    #[test]
    fn a_refusal_reads_as_the_lowering_of_the_version_gives_it() {
        let of_version_1 =
            lowered(&VERSION_1.replace("198.51.100.0/25", "198.51.100.0")).unwrap_err();
        let of_version_1 = format!("{:#}", anyhow::Error::new(of_version_1));
        assert!(
            of_version_1.starts_with("cluster.loopback_networks.ipv4: "),
            "{of_version_1}"
        );
        let of_version_2 =
            lowered(&VERSION_2.replace("  interface_prefix: mesh-\n", "")).unwrap_err();
        assert_eq!(
            of_version_2.to_string(),
            "mesh.interface_prefix is not set - `slipmesh mint` writes it"
        );
    }
}
