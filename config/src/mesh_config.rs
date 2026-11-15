//! The topology `slipmesh.yaml` describes: the cluster's settings, its nodes, the mesh links
//! between them, roadwarriors pools and bypass entries, with `validate`, the checks across them.
//! One schema for every concern, since they all name the same nodes and addresses.

use crate::Obfuscation;
use crate::router::{BfdSettings, BypassSourceEntry};
use serde::Deserialize;
use sliplib_secrets::wireguard::PrivateKey;
use sliplib_yaml::Value;
use std::collections::HashSet;
use std::net::Ipv4Addr;

#[derive(Deserialize, Debug, Default, PartialEq)]
pub struct BfdConfig {
    #[serde(default)]
    pub enable: bool,
    /// Flattened, so `bfd:` reads as one block: `enable`, then any of the settings, which default
    /// as `BfdSettings` says.
    #[serde(default, flatten)]
    pub settings: BfdSettings,
}

pub(crate) const DEFAULT_MESH_INTERFACE_PREFIX: &str = "mesh-";

pub(crate) const DEFAULT_EGRESS_ROUTE_PROTO_ID: u8 = 152;

/// Strict for the same reason `ClusterConfig` is.
#[derive(Deserialize, Debug, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct InterfacePrefix {
    #[serde(default)]
    pub mesh: Option<String>,
}

/// Strict for the same reason `ClusterConfig` is.
#[derive(Deserialize, Debug, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RouteProtoId {
    #[serde(default)]
    pub egress: Option<u8>,
}

impl MeshConfig {
    pub fn node(&self, name: &str) -> anyhow::Result<&NodeEntry> {
        self.nodes
            .iter()
            .find(|n| n.name == name)
            .ok_or_else(|| anyhow::anyhow!("unknown node {name:?}"))
    }
}

impl ClusterConfig {
    pub fn mesh_interface_prefix(&self) -> &str {
        self.interface_prefix
            .mesh
            .as_deref()
            .unwrap_or(DEFAULT_MESH_INTERFACE_PREFIX)
    }

    pub fn egress_route_proto_id(&self) -> u8 {
        self.route_proto_id
            .egress
            .unwrap_or(DEFAULT_EGRESS_ROUTE_PROTO_ID)
    }
}

/// Strict for the same reason `ClusterConfig` is.
#[derive(Deserialize, Debug, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MeshConfig {
    pub cluster: ClusterConfig,
    /// Off unless asked for: BFD trades constant control traffic on every mesh link for detecting a
    /// dead link in seconds rather than at OSPF's dead timer.
    #[serde(default)]
    pub bfd: BfdConfig,
    #[serde(default)]
    pub obfuscation: Obfuscation,
    #[serde(default)]
    pub nodes: Vec<NodeEntry>,
    #[serde(default)]
    pub mesh: MeshTopology,
    #[serde(default)]
    pub roadwarriors: Vec<RoadwarriorPool>,
    #[serde(default)]
    pub bypass: Vec<BypassEntry>,
}

/// Strict, unlike the daemons' configs: a daemon may be older than a field it is given and ignores
/// it, but this file is read by tools built with its schema, so an unknown key is a mistake.
/// Ignored, a renamed key would stop rendering what it configures on every node at once.
#[derive(Deserialize, Debug, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ClusterConfig {
    #[serde(default)]
    pub bgp_as: u32,
    /// The MTU of every mesh interface, at most the least limit of the mesh's links; that limit
    /// when unset.
    #[serde(default)]
    pub mesh_mtu: Option<u32>,
    #[serde(default)]
    pub interface_prefix: InterfacePrefix,
    /// The `proto` of the kernel routes of each kind, as `ip route` shows it.
    #[serde(default)]
    pub route_proto_id: RouteProtoId,
    #[serde(default)]
    pub loopback_networks: LoopbackNetworks,
    #[serde(default)]
    pub bypass_refresh_interval_secs: Option<u64>,
    /// TCP ports every node serves Prometheus metrics on, one per daemon since two processes cannot
    /// bind one, each on the node's own IPv4 loopback. Unset, that daemon serves no metrics.
    #[serde(default)]
    pub awg_metrics_port: Option<u16>,
    #[serde(default)]
    pub router_metrics_port: Option<u16>,
    /// Interfaces whose connected prefixes every node announces (BIRD's `protocol direct`). One
    /// that a node lacks matches nothing there. Empty announces nothing: no interface is implied.
    ///
    /// Must include the mesh glob whenever `tunnel_networks` is set, or `sliplib-mesh` refuses to
    /// render the node's router config. A node's podCIDR comes through `pod_subnet`, not from here.
    #[serde(default)]
    pub direct_interfaces: Vec<String>,
    /// Kubernetes' ClusterIP range, announced from every node. Stated here because no daemon reads
    /// the Kubernetes API; it is fixed when the cluster is created. Unset announces none.
    #[serde(default)]
    pub service_subnet: Option<String>,
    /// Kubernetes' pod range. Every node learns the kernel routes inside it and announces them,
    /// which is how its own podCIDR reaches iBGP: every CNI routes a node's podCIDR, but not every
    /// CNI puts an address of it on an interface, so `direct_interfaces` cannot carry it. A range,
    /// because which part of it a node gets is decided at runtime. Unset learns nothing.
    #[serde(default)]
    pub pod_subnet: Option<String>,
    /// A second address pair derived from `node_id`, put on every mesh interface. The IPv4 half
    /// gives masquerade a source address there, where there is no IPv4 otherwise. The IPv6 half is
    /// link-local and replaces the one derived from the loopback, so OSPFv3 has one link-local per
    /// interface to send from. Unset, a mesh interface carries the loopback-derived link-local
    /// alone.
    #[serde(default)]
    pub tunnel_networks: Option<TunnelNetworks>,
}

#[derive(Deserialize, Debug, Default, PartialEq)]
pub struct LoopbackNetworks {
    #[serde(default)]
    pub ipv4: String,
    #[serde(default)]
    pub ipv6: String,
}

#[derive(Deserialize, Debug, Default, PartialEq)]
pub struct TunnelNetworks {
    #[serde(default)]
    pub ipv4: String,
    #[serde(default)]
    pub ipv6: String,
}

#[derive(Deserialize, Debug, PartialEq)]
pub struct NodeEntry {
    pub name: String,
    /// An IPv4-shaped identity, not an address: its bits fill the host part of
    /// `cluster.loopback_networks` to give the node its loopback addresses.
    pub node_id: String,
    /// The host or IP a peer dials, without a port: each mesh link adds its own. Unset for a node
    /// behind NAT, which can still be on a link - only its other end is dialed.
    #[serde(default)]
    pub endpoint: Option<String>,
    /// One identity for every mesh link of the node.
    #[serde(default)]
    pub mesh_private_key: Option<PrivateKey>,
    /// Replaces, not extends, `cluster.direct_interfaces` for this node; unset means the cluster's
    /// list.
    #[serde(default)]
    pub direct_interfaces: Option<Vec<String>>,
    /// The MTU of the node's uplink, which bounds every link it is on; 1500 when unset. Set it on a
    /// node whose uplink is narrower, such as one behind a tunnel of its provider's.
    #[serde(default)]
    pub uplink_mtu: Option<u32>,
    /// The family a peer dials `endpoint` over, when `endpoint` is a hostname; IPv6 when unset. An
    /// IP literal says its family itself, and an `address_family` that contradicts it is an
    /// error.
    #[serde(default)]
    pub address_family: Option<AddressFamily>,
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AddressFamily {
    Ipv4,
    Ipv6,
}

#[derive(Deserialize, Debug, Default, PartialEq)]
pub struct MeshTopology {
    #[serde(default)]
    pub links: Vec<MeshLink>,
}

#[derive(Deserialize, Debug, PartialEq)]
pub struct MeshLink {
    /// Two names of `nodes`; their order carries no meaning.
    pub pair: [String; 2],
    /// The UDP port of both ends. Never generated: which port a link takes is the operator's
    /// decision.
    pub port: u16,
    #[serde(default)]
    pub obfuscation: Obfuscation,
    /// Stock WireGuard, no AmneziaWG obfuscation, for a peer that cannot speak AmneziaWG, such as
    /// RouterOS. The link carries no obfuscation, whatever the topology's default says.
    #[serde(default)]
    pub plain: bool,
}

/// Strict for the same reason `ClusterConfig` is.
#[derive(Deserialize, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RoadwarriorPool {
    pub name: String,
    /// The nodes that terminate the pool, each on an interface of its own.
    pub node_hostnames: Vec<String>,
    pub address: String,
    #[serde(default)]
    pub address6: Option<String>,
    pub listen_port: u16,
    #[serde(default)]
    pub handshake_stale_secs: Option<u64>,
    /// One identity for the pool on every node of `node_hostnames`.
    #[serde(default)]
    pub private_key: Option<PrivateKey>,
    /// Stock WireGuard, for clients of a stock WireGuard app; as [`MeshLink::plain`].
    #[serde(default)]
    pub plain: bool,
    #[serde(default)]
    pub obfuscation: Obfuscation,
    /// The `DNS =` of the pool's client configs; no server config carries it. Unset, a client gets
    /// the cluster's CoreDNS address, `.10` of `cluster.service_subnet`.
    #[serde(default)]
    pub dns: Option<String>,
    #[serde(default)]
    pub clients: Vec<RoadwarriorClient>,
}

/// A client is written down by one of its keys: `public_key` when its private key stays with the
/// client, `private_key` when the file keeps it - the public key is then derived from it, so the
/// two cannot disagree.
#[derive(Deserialize, Debug, PartialEq)]
#[serde(try_from = "ClientEntry")]
pub struct RoadwarriorClient {
    pub name: String,
    pub public_key: String,
    pub private_key: Option<PrivateKey>,
    pub allowed_ips: Vec<String>,
    pub advanced_security: bool,
}

#[derive(Deserialize)]
struct ClientEntry {
    name: String,
    #[serde(default)]
    public_key: Option<String>,
    #[serde(default)]
    private_key: Option<PrivateKey>,
    #[serde(default)]
    allowed_ips: Vec<String>,
    #[serde(default)]
    advanced_security: bool,
}

impl TryFrom<ClientEntry> for RoadwarriorClient {
    type Error = String;

    fn try_from(entry: ClientEntry) -> Result<Self, Self::Error> {
        let name = entry.name;
        let public_key = match (entry.public_key, &entry.private_key) {
            (Some(public), None) => public,
            (None, Some(private)) => private.public_key().map_err(|e| {
                format!("client {name:?}: private_key: {:#}", anyhow::Error::new(e))
            })?,
            (None, None) => {
                return Err(format!("client {name:?}: needs public_key or private_key"));
            }
            (Some(_), Some(_)) => {
                return Err(format!(
                    "client {name:?}: public_key or private_key, not both"
                ));
            }
        };
        Ok(Self {
            name,
            public_key,
            private_key: entry.private_key,
            allowed_ips: entry.allowed_ips,
            advanced_security: entry.advanced_security,
        })
    }
}

#[derive(Deserialize, Debug, PartialEq)]
pub struct BypassEntry {
    pub node: String,
    #[serde(default)]
    pub include: Vec<BypassSourceEntry>,
    #[serde(default)]
    pub exclude: Vec<BypassSourceEntry>,
}

/// The keys of the network document that `topology` leaves out and that are written down with
/// their default, so that the values published to the nodes stand in `slipmesh.yaml`. The mesh MTU
/// is not one of them: written down, it would no longer follow the links.
pub fn missing_defaults(topology: &MeshConfig) -> Vec<(&'static str, Value)> {
    let cluster = &topology.cluster;
    let mut missing = Vec::new();
    if cluster.interface_prefix.mesh.is_none() {
        missing.push((
            "cluster.interface_prefix.mesh",
            DEFAULT_MESH_INTERFACE_PREFIX.into(),
        ));
    }
    if cluster.route_proto_id.egress.is_none() {
        missing.push((
            "cluster.route_proto_id.egress",
            u64::from(DEFAULT_EGRESS_ROUTE_PROTO_ID).into(),
        ));
    }
    missing
}

/// The checks across the topology that its types alone cannot make; the configs rendered from it
/// are the daemons' checks to make.
pub fn validate(cfg: &MeshConfig) -> anyhow::Result<()> {
    anyhow::ensure!(!cfg.nodes.is_empty(), "nodes must not be empty");
    let prefix = cfg.cluster.mesh_interface_prefix();
    anyhow::ensure!(
        !prefix.is_empty(),
        "cluster.interface_prefix.mesh must not be empty: it is what tells a mesh interface"
    );
    // The kernel refuses `/`, `:` and whitespace in an interface name, and BIRD reads the prefix
    // in a glob, where `*`, `?` and `[` would match interfaces that are not the mesh's.
    anyhow::ensure!(
        prefix
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')),
        "cluster.interface_prefix.mesh {prefix:?} may hold only ASCII letters, digits, `-`, `_` and \
         `.`, the characters an interface name and its glob both take as they are"
    );
    // 0..=4 are the kernel's own: unspec, redirect, kernel, boot and static (`RTPROT_*`).
    let egress = cfg.cluster.egress_route_proto_id();
    anyhow::ensure!(
        egress > 4,
        "cluster.route_proto_id.egress {egress} is one the kernel keeps for its own routes (0-4)"
    );

    let mut seen_node_names = HashSet::new();
    let mut seen_node_ids = HashSet::new();
    for node in &cfg.nodes {
        anyhow::ensure!(!node.name.is_empty(), "nodes[].name must not be empty");
        anyhow::ensure!(
            seen_node_names.insert(node.name.as_str()),
            "duplicate node name {:?}",
            node.name
        );
        let node_id: Ipv4Addr = node.node_id.parse().map_err(|e| {
            anyhow::anyhow!(
                "node {:?}: node_id {:?} is not a valid IPv4-shaped identity: {e}",
                node.name,
                node.node_id
            )
        })?;
        anyhow::ensure!(
            seen_node_ids.insert(node_id),
            "node {:?}: node_id {:?} is already used by another node - each node needs a distinct \
             node_id, or they'll collide onto the same loopback address",
            node.name,
            node.node_id
        );
    }
    let known_node = |name: &str| seen_node_names.contains(name);

    let mut ports_by_node: std::collections::HashMap<&str, HashSet<u16>> =
        std::collections::HashMap::new();

    for link in &cfg.mesh.links {
        for name in &link.pair {
            anyhow::ensure!(
                known_node(name),
                "mesh.links: pair references unknown node {name:?}"
            );
        }
        anyhow::ensure!(
            link.pair[0] != link.pair[1],
            "mesh.links: pair {:?} links a node to itself",
            link.pair
        );
        for name in &link.pair {
            anyhow::ensure!(
                ports_by_node
                    .entry(name.as_str())
                    .or_default()
                    .insert(link.port),
                "node {name:?}: port {} used by more than one mesh link/roadwarriors pool",
                link.port
            );
        }
    }

    let mut seen_pool_names = HashSet::new();
    for pool in &cfg.roadwarriors {
        anyhow::ensure!(
            seen_pool_names.insert(pool.name.as_str()),
            "duplicate roadwarriors pool name {:?}",
            pool.name
        );
        anyhow::ensure!(
            !pool.node_hostnames.is_empty(),
            "roadwarriors pool {:?}: node_hostnames must not be empty",
            pool.name
        );
        for name in &pool.node_hostnames {
            anyhow::ensure!(
                known_node(name),
                "roadwarriors pool {:?}: node_hostnames references unknown node {name:?}",
                pool.name
            );
            anyhow::ensure!(
                ports_by_node
                    .entry(name.as_str())
                    .or_default()
                    .insert(pool.listen_port),
                "node {name:?}: port {} used by more than one mesh link/roadwarriors pool",
                pool.listen_port
            );
        }
        let mut seen_client_keys = HashSet::new();
        for client in &pool.clients {
            anyhow::ensure!(
                seen_client_keys.insert(client.public_key.as_str()),
                "roadwarriors pool {:?}: duplicate client public_key {:?}",
                pool.name,
                client.public_key
            );
        }
    }

    for entry in &cfg.bypass {
        anyhow::ensure!(
            known_node(&entry.node),
            "bypass: entry references unknown node {:?}",
            entry.node
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_defaults_are_the_prefix_and_protocol_keys_left_out_each_with_its_default() {
        let cfg: MeshConfig = sliplib_yaml::from_str(minimal_yaml()).unwrap();
        assert_eq!(
            missing_defaults(&cfg),
            [
                ("cluster.interface_prefix.mesh", Value::from("mesh-")),
                ("cluster.route_proto_id.egress", Value::from(152u64)),
            ]
        );

        let given = |setting: &str| -> MeshConfig {
            let yaml =
                minimal_yaml().replace("  bgp_as: 64512\n", &format!("  bgp_as: 64512\n{setting}"));
            sliplib_yaml::from_str(&yaml).unwrap()
        };
        let keys = |cfg: &MeshConfig| -> Vec<&str> {
            missing_defaults(cfg)
                .into_iter()
                .map(|(key, _)| key)
                .collect()
        };
        assert_eq!(
            keys(&given("  interface_prefix: {mesh: wg-}\n")),
            ["cluster.route_proto_id.egress"]
        );
        assert_eq!(
            keys(&given("  route_proto_id: {egress: 160}\n")),
            ["cluster.interface_prefix.mesh"]
        );
        assert!(
            keys(&given(
                "  interface_prefix: {mesh: wg-}\n  route_proto_id: {egress: 160}\n"
            ))
            .is_empty()
        );
    }

    #[test]
    fn the_prefix_and_protocol_keys_are_read_when_given() {
        let yaml = minimal_yaml().replace(
            "  bgp_as: 64512\n",
            "  bgp_as: 64512\n  interface_prefix: {mesh: wg-}\n  route_proto_id: {egress: 160}\n",
        );
        let cfg: MeshConfig = sliplib_yaml::from_str(&yaml).unwrap();
        assert_eq!(cfg.cluster.mesh_interface_prefix(), "wg-");
        assert_eq!(cfg.cluster.egress_route_proto_id(), 160);
        validate(&cfg).unwrap();
    }

    #[test]
    fn an_empty_mesh_prefix_is_refused() {
        let mut cfg: MeshConfig = sliplib_yaml::from_str(minimal_yaml()).unwrap();
        cfg.cluster.interface_prefix.mesh = Some(String::new());
        let err = validate(&cfg).unwrap_err().to_string();
        assert!(err.contains("cluster.interface_prefix.mesh"), "{err}");
    }

    #[test]
    fn a_mesh_prefix_an_interface_name_or_its_glob_cannot_carry_is_refused() {
        let mut cfg: MeshConfig = sliplib_yaml::from_str(minimal_yaml()).unwrap();
        for prefix in ["mesh*", "me sh", "a/b", "a:b"] {
            cfg.cluster.interface_prefix.mesh = Some(prefix.to_owned());
            let err = validate(&cfg).unwrap_err().to_string();
            assert!(
                err.contains("cluster.interface_prefix.mesh"),
                "{prefix}: {err}"
            );
        }
        cfg.cluster.interface_prefix.mesh = Some("wg_1.x-".to_owned());
        validate(&cfg).unwrap();
    }

    #[test]
    fn a_route_protocol_the_kernel_keeps_for_itself_is_refused() {
        let mut cfg: MeshConfig = sliplib_yaml::from_str(minimal_yaml()).unwrap();
        cfg.cluster.route_proto_id.egress = Some(4);
        let err = validate(&cfg).unwrap_err().to_string();
        assert!(err.contains("cluster.route_proto_id.egress"), "{err}");
        cfg.cluster.route_proto_id.egress = Some(5);
        validate(&cfg).unwrap();
    }

    fn minimal_yaml() -> &'static str {
        r#"
cluster:
  bgp_as: 64512
  loopback_networks: {ipv4: "198.51.100.0/25", ipv6: "3fff::/32"}
nodes:
  - name: a
    node_id: "198.51.100.1"
  - name: b
    node_id: "198.51.100.2"
"#
    }

    #[test]
    fn debug_output_never_carries_a_secret() {
        let [node, header, pool, client] =
            ["A", "E", "I", "M"].map(|c| format!("{}=", c.repeat(43)));
        let yaml = format!(
            "{}    mesh_private_key: \"{node}\"
obfuscation: {{header_protection_key: \"{header}\"}}
roadwarriors:
  - name: rw
    node_hostnames: [a]
    address: 203.0.113.1/27
    listen_port: 51900
    private_key: \"{pool}\"
    clients:
      - {{name: laptop, private_key: \"{client}\"}}
",
            minimal_yaml()
        );
        let cfg: MeshConfig = sliplib_yaml::from_str(&yaml).unwrap();
        let printed = format!("{cfg:?}");
        for secret in [node, header, pool, client] {
            assert!(!printed.contains(&secret), "{printed}");
        }
    }

    #[test]
    fn a_client_needs_exactly_one_of_its_keys() {
        let neither = sliplib_yaml::from_str::<RoadwarriorClient>("{name: laptop}").unwrap_err();
        assert!(neither.to_string().contains("laptop"), "{neither}");

        let private = PrivateKey::generate();
        let public = private.public_key().unwrap();
        let both = sliplib_yaml::from_str::<RoadwarriorClient>(&format!(
            "{{name: laptop, public_key: \"{public}\", private_key: \"{}\"}}",
            private.expose()
        ))
        .unwrap_err();
        assert!(both.to_string().contains("not both"), "{both}");
    }

    #[test]
    fn an_unknown_top_level_key_is_refused() {
        let yaml = format!(
            "{}roadwarrior: []
",
            minimal_yaml()
        );
        let err = sliplib_yaml::from_str::<MeshConfig>(&yaml).unwrap_err();
        assert!(err.to_string().contains("roadwarrior"), "{err}");
    }

    #[test]
    fn an_unknown_roadwarriors_pool_key_is_refused() {
        let yaml = "name: plain
node_hostnames: [a]
address: 203.0.113.1/27
listen_port: 51820
listen-port: 1
";
        let err = sliplib_yaml::from_str::<RoadwarriorPool>(yaml).unwrap_err();
        assert!(err.to_string().contains("listen-port"), "{err}");
    }

    #[test]
    fn rejects_empty_nodes() {
        let cfg = MeshConfig::default();
        assert!(validate(&cfg).is_err());
    }

    #[test]
    fn rejects_duplicate_node_names() {
        let mut cfg: MeshConfig = sliplib_yaml::from_str(minimal_yaml()).unwrap();
        cfg.nodes[1].name = "a".to_string();
        assert!(validate(&cfg).is_err());
    }

    #[test]
    fn rejects_duplicate_node_id() {
        // Two nodes of one node_id would share a loopback address.
        let mut cfg: MeshConfig = sliplib_yaml::from_str(minimal_yaml()).unwrap();
        cfg.nodes[1].node_id = cfg.nodes[0].node_id.clone();
        let err = validate(&cfg).unwrap_err();
        assert!(err.to_string().contains("node_id"), "error was: {err}");
    }

    #[test]
    fn rejects_a_node_id_that_is_not_a_valid_ipv4_address() {
        let mut cfg: MeshConfig = sliplib_yaml::from_str(minimal_yaml()).unwrap();
        cfg.nodes[0].node_id = "not-an-ip".to_string();
        assert!(validate(&cfg).is_err());
    }

    #[test]
    fn rejects_mesh_link_referencing_unknown_node() {
        let mut cfg: MeshConfig = sliplib_yaml::from_str(minimal_yaml()).unwrap();
        cfg.mesh.links.push(MeshLink {
            pair: ["a".to_string(), "ghost".to_string()],
            port: 51820,
            obfuscation: Obfuscation::default(),
            plain: false,
        });
        assert!(validate(&cfg).is_err());
    }

    #[test]
    fn rejects_mesh_link_to_self() {
        let mut cfg: MeshConfig = sliplib_yaml::from_str(minimal_yaml()).unwrap();
        cfg.mesh.links.push(MeshLink {
            pair: ["a".to_string(), "a".to_string()],
            port: 51820,
            obfuscation: Obfuscation::default(),
            plain: false,
        });
        assert!(validate(&cfg).is_err());
    }

    #[test]
    fn accepts_two_links_on_the_same_node_with_different_ports() {
        let mut cfg: MeshConfig = sliplib_yaml::from_str(minimal_yaml()).unwrap();
        cfg.nodes.push(NodeEntry {
            name: "c".to_string(),
            node_id: "198.51.100.3".to_string(),
            endpoint: None,
            mesh_private_key: None,
            direct_interfaces: None,
            uplink_mtu: None,
            address_family: None,
        });
        cfg.mesh.links.push(MeshLink {
            pair: ["a".to_string(), "b".to_string()],
            port: 51820,
            obfuscation: Obfuscation::default(),
            plain: false,
        });
        cfg.mesh.links.push(MeshLink {
            pair: ["a".to_string(), "c".to_string()],
            port: 51821,
            obfuscation: Obfuscation::default(),
            plain: false,
        });
        validate(&cfg).unwrap();
    }

    #[test]
    fn rejects_duplicate_port_on_the_same_node() {
        let mut cfg: MeshConfig = sliplib_yaml::from_str(minimal_yaml()).unwrap();
        cfg.nodes.push(NodeEntry {
            name: "c".to_string(),
            node_id: "198.51.100.3".to_string(),
            endpoint: None,
            mesh_private_key: None,
            direct_interfaces: None,
            uplink_mtu: None,
            address_family: None,
        });
        cfg.mesh.links.push(MeshLink {
            pair: ["a".to_string(), "b".to_string()],
            port: 51820,
            obfuscation: Obfuscation::default(),
            plain: false,
        });
        cfg.mesh.links.push(MeshLink {
            pair: ["a".to_string(), "c".to_string()],
            port: 51820,
            obfuscation: Obfuscation::default(),
            plain: false,
        });
        assert!(validate(&cfg).is_err());
    }

    #[test]
    fn rejects_roadwarriors_pool_referencing_unknown_node() {
        let mut cfg: MeshConfig = sliplib_yaml::from_str(minimal_yaml()).unwrap();
        cfg.roadwarriors.push(RoadwarriorPool {
            name: "north".to_string(),
            node_hostnames: vec!["ghost".to_string()],
            address: "203.0.113.1/27".to_string(),
            address6: None,
            listen_port: 51820,
            handshake_stale_secs: None,
            private_key: None,
            plain: false,
            obfuscation: Obfuscation::default(),
            dns: None,
            clients: vec![],
        });
        assert!(validate(&cfg).is_err());
    }

    #[test]
    fn rejects_roadwarriors_pool_with_empty_node_hostnames() {
        let mut cfg: MeshConfig = sliplib_yaml::from_str(minimal_yaml()).unwrap();
        cfg.roadwarriors.push(RoadwarriorPool {
            name: "north".to_string(),
            node_hostnames: vec![],
            address: "203.0.113.1/27".to_string(),
            address6: None,
            listen_port: 51820,
            handshake_stale_secs: None,
            private_key: None,
            plain: false,
            obfuscation: Obfuscation::default(),
            dns: None,
            clients: vec![],
        });
        assert!(validate(&cfg).is_err());
    }

    #[test]
    fn rejects_duplicate_roadwarrior_client_public_key_in_one_pool() {
        let mut cfg: MeshConfig = sliplib_yaml::from_str(minimal_yaml()).unwrap();
        cfg.roadwarriors.push(RoadwarriorPool {
            name: "north".to_string(),
            node_hostnames: vec!["a".to_string()],
            address: "203.0.113.1/27".to_string(),
            address6: None,
            listen_port: 51820,
            handshake_stale_secs: None,
            private_key: None,
            plain: false,
            obfuscation: Obfuscation::default(),
            dns: None,
            clients: vec![
                RoadwarriorClient {
                    name: "alice".to_string(),
                    public_key: "k".to_string(),
                    private_key: None,
                    allowed_ips: vec!["203.0.113.5/32".to_string()],
                    advanced_security: false,
                },
                RoadwarriorClient {
                    name: "bob".to_string(),
                    public_key: "k".to_string(),
                    private_key: None,
                    allowed_ips: vec!["203.0.113.6/32".to_string()],
                    advanced_security: false,
                },
            ],
        });
        assert!(validate(&cfg).is_err());
    }

    #[test]
    fn rejects_bypass_entry_referencing_unknown_node() {
        let mut cfg: MeshConfig = sliplib_yaml::from_str(minimal_yaml()).unwrap();
        cfg.bypass.push(BypassEntry {
            node: "ghost".to_string(),
            include: vec![],
            exclude: vec![],
        });
        assert!(validate(&cfg).is_err());
    }

    #[test]
    fn accepts_roadwarriors_and_mesh_sharing_a_node_with_distinct_ports() {
        let mut cfg: MeshConfig = sliplib_yaml::from_str(minimal_yaml()).unwrap();
        cfg.mesh.links.push(MeshLink {
            pair: ["a".to_string(), "b".to_string()],
            port: 51820,
            obfuscation: Obfuscation::default(),
            plain: false,
        });
        cfg.roadwarriors.push(RoadwarriorPool {
            name: "north".to_string(),
            node_hostnames: vec!["a".to_string()],
            address: "203.0.113.1/27".to_string(),
            address6: None,
            listen_port: 51821,
            handshake_stale_secs: None,
            private_key: None,
            plain: false,
            obfuscation: Obfuscation::default(),
            dns: None,
            clients: vec![],
        });
        validate(&cfg).unwrap();
    }

    #[test]
    fn a_node_is_found_by_its_name_and_an_unknown_one_refused() {
        let cfg: MeshConfig = sliplib_yaml::from_str(minimal_yaml()).unwrap();
        assert_eq!(cfg.node("b").unwrap().node_id, "198.51.100.2");
        assert!(cfg.node("c").is_err());
    }
}
