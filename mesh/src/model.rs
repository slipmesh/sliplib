//! The network as the configs of its nodes are computed from it: every node with the addresses
//! derived for it, every link with what is in force on it. The files are lowered into this, and
//! nothing else is computed from.

use sliplib_config::router::{AnnounceEntry, BfdSettings};
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
}

/// How the files name a node's private key, under the node.
pub const PRIVATE_KEY: &str = "wireguard.private_key";

/// Where the files hold the key of `node`, as a message names it.
pub fn node_key(node: &str) -> String {
    format!("nodes.{node}.{PRIVATE_KEY}")
}

/// Where the files hold the obfuscation of `link`, as a message names it.
pub fn link_obfuscation(link: &Link) -> String {
    format!("mesh.links.{}.amneziawg.obfuscation", link.name)
}

/// A name that is not one of `nodes`.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("unknown node {0:?}")]
pub struct UnknownNode(pub String);

impl Network {
    pub fn node(&self, name: &str) -> Result<&Node, UnknownNode> {
        self.nodes
            .iter()
            .find(|node| node.name == name)
            .ok_or_else(|| UnknownNode(name.to_owned()))
    }

    /// Whether the node needs a private key: it is on a link.
    pub fn needs_a_key(&self, node: &str) -> bool {
        self.links
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
    /// Whose tool configures the node.
    pub platform: Platform,
    pub loopback: (Ipv4Addr, Ipv6Addr),
    /// The IPv6 link-local of every link's interface on the node.
    pub link_local: Ipv6Addr,
    /// The IPv4 address of every link's interface on the node, where the network gives links one.
    pub tunnel_ipv4: Option<Ipv4Addr>,
    /// The key the files write for the node, none where they write none.
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
