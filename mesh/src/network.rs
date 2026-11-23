use crate::addressing;
use sliplib_config::v2::File;
use sliplib_config::v2::network::{Network, Node};
use std::net::{Ipv4Addr, Ipv6Addr};

/// Why files that read give no mesh.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("mesh.interface_prefix is not set - `slipmesh mint` writes it")]
    NoInterfacePrefix,
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("unknown node {0:?}")]
pub struct UnknownNode(pub String);

/// Under the node.
pub const PRIVATE_KEY: &str = "wireguard.private_key";

/// Its path in the files.
pub fn node_key(node: &str) -> String {
    format!("nodes.{node}.{PRIVATE_KEY}")
}

/// Its path in the files.
pub fn link_obfuscation(link: &str) -> String {
    format!("mesh.links.{link}.amneziawg.obfuscation")
}

/// The loader's network and its interface prefix.
#[derive(Debug, Clone, Copy)]
pub struct Mesh<'a> {
    network: &'a Network,
    interface_prefix: &'a str,
}

/// What a node's `node_id` gives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Addresses {
    pub loopback: (Ipv4Addr, Ipv6Addr),
    /// Of every link's interface on the node.
    pub link_local: Ipv6Addr,
    /// Of every link's interface, where the network gives links one.
    pub tunnel_ipv4: Option<Ipv4Addr>,
}

impl<'a> Mesh<'a> {
    pub fn of(files: &'a File) -> Result<Self, Error> {
        let network = files.network();
        let interface_prefix = network
            .mesh
            .interface_prefix
            .as_ref()
            .ok_or(Error::NoInterfacePrefix)?
            .as_str();
        Ok(Self {
            network,
            interface_prefix,
        })
    }

    pub fn network(&self) -> &'a Network {
        self.network
    }

    pub fn interface_prefix(&self) -> &'a str {
        self.interface_prefix
    }

    pub fn node(&self, name: &str) -> Result<&'a Node, UnknownNode> {
        self.network
            .nodes
            .get(name)
            .ok_or_else(|| UnknownNode(name.to_owned()))
    }

    /// Whether the node is on a link.
    pub fn needs_a_key(&self, node: &str) -> bool {
        self.network
            .mesh
            .links
            .values()
            .any(|link| link.ends.iter().any(|end| end.as_str() == node))
    }

    /// Distinct, none with a host part of zeros: the loader refuses a `node_id` that does not fit.
    pub fn addresses(&self, node: &Node) -> Addresses {
        let addressing = &self.network.addressing;
        let (ipv4, ipv6) = (addressing.loopback.ipv4.0, addressing.loopback.ipv6.0);
        let id = node.node_id;
        let ipv6_loopback = addressing::ipv6_in(ipv6.addr(), ipv6.prefix_len(), id);
        let tunnel = addressing.tunnel.as_ref();
        Addresses {
            loopback: (
                addressing::ipv4_in(ipv4.addr(), ipv4.prefix_len(), id),
                ipv6_loopback,
            ),
            link_local: tunnel.and_then(|tunnel| tunnel.link_local).map_or_else(
                || addressing::link_local_from_loopback(ipv6_loopback),
                |network| addressing::ipv6_in(network.0.addr(), network.0.prefix_len(), id),
            ),
            tunnel_ipv4: tunnel
                .and_then(|tunnel| tunnel.ipv4)
                .map(|network| addressing::ipv4_in(network.0.addr(), network.0.prefix_len(), id)),
        }
    }

    /// With the links' pattern appended unless listed, where the links carry IPv4: that announces
    /// them.
    pub fn direct_interfaces(&self, node: &Node) -> Vec<String> {
        let mut list = node.routing.direct_interfaces.clone();
        let tunnel = self.network.addressing.tunnel.as_ref();
        if tunnel.and_then(|tunnel| tunnel.ipv4).is_some() {
            let pattern = format!("{}*", self.interface_prefix);
            if !list.contains(&pattern) {
                list.push(pattern);
            }
        }
        list
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{example, example_files};

    fn addresses(mesh: &Mesh<'_>, node: &str) -> Addresses {
        mesh.addresses(mesh.node(node).unwrap())
    }

    fn direct_interfaces(mesh: &Mesh<'_>, node: &str) -> Vec<String> {
        mesh.direct_interfaces(mesh.node(node).unwrap())
    }

    #[test]
    fn a_node_gets_its_addresses_from_its_node_id() {
        let mesh = example("{}");
        let node_a = addresses(&mesh, "node-a");
        assert_eq!(node_a.loopback.0.to_string(), "198.51.100.1");
        assert_eq!(node_a.loopback.1.to_string(), "3fff:0:0:1::1");
        assert_eq!(node_a.link_local.to_string(), "fe80::1");
        assert_eq!(node_a.tunnel_ipv4.unwrap().to_string(), "198.51.100.129");
        let node_c = addresses(&mesh, "node-c");
        assert_eq!(node_c.loopback.0.to_string(), "198.51.100.3");
        assert_eq!(node_c.link_local.to_string(), "fe80::3");
    }

    #[test]
    fn the_link_local_is_the_node_id_in_the_host_part_of_its_network() {
        let own = example("{addressing: {tunnel: {link_local: 'fe80:0:0:1::/64'}}}");
        assert_eq!(
            addresses(&own, "node-b").link_local.to_string(),
            "fe80:0:0:1::2"
        );
        // A network that leaves fewer than 32 bits free keeps its own bits above the `node_id`.
        let narrow = example("{addressing: {tunnel: {link_local: 'fe80::1:0/120'}}}");
        assert_eq!(
            addresses(&narrow, "node-b").link_local.to_string(),
            "fe80::1:2"
        );
    }

    #[test]
    fn an_ipv6_loopback_network_longer_than_96_takes_a_node_id_that_fits_it() {
        let mesh = example(
            "{addressing: {loopback: {ipv4: 198.18.0.0/24, ipv6: '3fff:0:0:1::/120'}, tunnel: \
             {ipv4: null}}, nodes: {node-b: {node_id: 0.0.0.200}}}",
        );
        let node_b = addresses(&mesh, "node-b");
        assert_eq!(node_b.loopback.0.to_string(), "198.18.0.200");
        assert_eq!(node_b.loopback.1.to_string(), "3fff:0:0:1::c8");
        assert_eq!(
            addresses(&mesh, "node-a").loopback.1.to_string(),
            "3fff:0:0:1::1"
        );
    }

    #[test]
    fn without_a_link_local_network_the_links_take_the_link_local_of_the_loopback() {
        let mesh =
            example("{addressing: {loopback: {ipv6: '3fff::1:0/120'}, tunnel: {ipv4: null}}}");
        let node_c = addresses(&mesh, "node-c");
        assert_eq!(node_c.loopback.1.to_string(), "3fff::1:3");
        assert_eq!(node_c.link_local.to_string(), "fe80::1:3");
        assert_eq!(
            node_c.link_local,
            addressing::link_local_from_loopback(node_c.loopback.1)
        );
    }

    #[test]
    fn without_a_tunnel_network_a_link_has_no_ipv4_and_its_pattern_joins_no_list() {
        let mesh = example("{addressing: {tunnel: null}}");
        let node_a = addresses(&mesh, "node-a");
        assert_eq!(node_a.tunnel_ipv4, None);
        assert_eq!(node_a.link_local.to_string(), "fe80::1");
        assert_eq!(direct_interfaces(&mesh, "node-a"), ["lan0"]);
    }

    #[test]
    fn with_a_tunnel_ipv4_the_pattern_of_the_links_ends_every_nodes_list() {
        let mesh = example("{}");
        assert_eq!(direct_interfaces(&mesh, "node-a"), ["lan0", "mesh-*"]);
        assert_eq!(
            direct_interfaces(&mesh, "router1"),
            ["bridge-lan", "mesh-*"]
        );
    }

    #[test]
    fn a_pattern_already_in_a_list_keeps_its_place_and_is_not_added() {
        let mesh = example("{routing: {direct_interfaces: ['mesh-*', lan0]}}");
        assert_eq!(direct_interfaces(&mesh, "node-a"), ["mesh-*", "lan0"]);
    }

    #[test]
    fn the_pattern_follows_the_interface_prefix() {
        let mesh = example("{mesh: {interface_prefix: wg-}}");
        assert_eq!(direct_interfaces(&mesh, "node-a"), ["lan0", "wg-*"]);
        assert_eq!(mesh.interface_prefix(), "wg-");
    }

    #[test]
    fn files_without_the_interface_prefix_are_refused_naming_what_writes_it() {
        let files = example_files("{mesh: {interface_prefix: null}}");
        assert_eq!(Mesh::of(&files).unwrap_err(), Error::NoInterfacePrefix);
    }
}
