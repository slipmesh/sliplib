//! The secrets `slipmesh.yaml` leaves unset, generated: a node's mesh private key, a link's or a
//! pool's obfuscation, a pool's private key. What was generated comes back as `Minted`, for the
//! caller to write into the field it belongs to - the next run finds it there and generates
//! nothing.
//!
//! The whole topology at once, not per node: a link's two ends share one obfuscation, and a node's
//! config needs its peers' keys.

use crate::secrets::{self, link_key};
use sliplib_config::Obfuscation;
use sliplib_config::mesh_config::MeshConfig;
use sliplib_secrets::wireguard::PrivateKey;

mod obfuscation_gen;

/// What a run generated, by the entry it belongs to.
#[derive(Default)]
pub struct Minted {
    /// By the node's position in `nodes`.
    pub node_keys: Vec<(usize, PrivateKey)>,
    /// By the link's position in `mesh.links`; only the fields generated.
    pub link_obfuscation: Vec<(usize, Obfuscation)>,
    pub pools: Vec<PoolMinted>,
    /// How a message names each value.
    names: Vec<String>,
}

pub struct PoolMinted {
    pub name: String,
    pub private_key: Option<PrivateKey>,
    /// Only the fields generated.
    pub obfuscation: Option<Obfuscation>,
}

impl Minted {
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    /// Where each value goes, the way a message names it.
    pub fn routes(&self) -> &[String] {
        &self.names
    }
}

/// Generates the minted fields `obfuscation` lacks and returns them, `None` when it lacked none.
fn mint_obfuscation(obfuscation: &mut Obfuscation) -> Option<Obfuscation> {
    secrets::fill(obfuscation, &obfuscation_gen::generate())
}

/// Whatever secrets `topology` lacks, generated, each by the entry it belongs to. A `plain` link
/// or pool gets no obfuscation generated: it would turn "no obfuscation" back into an obfuscated
/// one.
pub fn create(topology: &MeshConfig) -> Minted {
    let mut written = secrets::read(topology);
    let mut minted = Minted::default();

    for (index, node) in topology.nodes.iter().enumerate() {
        if written.mesh_private_keys.contains_key(&node.name) {
            continue;
        }
        minted.node_keys.push((index, PrivateKey::generate()));
        minted
            .names
            .push(format!("nodes[{}].mesh_private_key", node.name));
    }

    for (index, link) in topology.mesh.links.iter().enumerate() {
        if link.plain {
            continue;
        }
        let pair = link_key(&link.pair);
        let obfuscation = written
            .mesh_link_obfuscation
            .entry(pair.clone())
            .or_default();
        if let Some(generated) = mint_obfuscation(obfuscation) {
            minted.link_obfuscation.push((index, generated));
            minted.names.push(format!("mesh.links[{pair}].obfuscation"));
        }
    }

    for pool in &topology.roadwarriors {
        let private_key =
            (!written.roadwarrior_private_keys.contains_key(&pool.name)).then(PrivateKey::generate);
        if private_key.is_some() {
            minted
                .names
                .push(format!("roadwarriors[{}].private_key", pool.name));
        }
        let obfuscation = if pool.plain {
            None
        } else {
            mint_obfuscation(
                written
                    .roadwarrior_obfuscation
                    .entry(pool.name.clone())
                    .or_default(),
            )
        };
        if obfuscation.is_some() {
            minted
                .names
                .push(format!("roadwarriors[{}].obfuscation", pool.name));
        }
        if private_key.is_some() || obfuscation.is_some() {
            minted.pools.push(PoolMinted {
                name: pool.name.clone(),
                private_key,
                obfuscation,
            });
        }
    }

    minted
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The 3.1 switches are never invented: they are not among the minted fields, so an
    /// unconfigured mesh stays as it was.
    #[test]
    fn mint_obfuscation_carries_the_31_switches_and_never_generates_them() {
        let mut resolved = Obfuscation {
            random_trailers: Some(true),
            ..Obfuscation::default()
        };
        let generated = mint_obfuscation(&mut resolved).unwrap();
        assert_eq!(resolved.random_trailers, Some(true));
        assert_eq!(resolved.disable_cookies, None);
        assert_eq!(generated.random_trailers, None);
    }

    #[test]
    fn mint_obfuscation_generates_when_nothing_else_is_set() {
        let mut resolved = Obfuscation::default();
        let generated = mint_obfuscation(&mut resolved).unwrap();
        assert!(resolved.jc.is_some());
        assert!(resolved.h1.is_some());
        assert_eq!(resolved, generated);
    }

    #[test]
    fn mint_obfuscation_reports_as_generated_only_what_no_layer_set() {
        let mut resolved = Obfuscation {
            jc: Some(9),
            h1: Some(42.into()),
            ..Obfuscation::default()
        };
        let generated = mint_obfuscation(&mut resolved).unwrap();
        assert_eq!((generated.jc, generated.h1), (None, None));
        assert_eq!(generated.jmin, resolved.jmin);
        assert!(generated.jmin.is_some());
    }

    fn topology() -> MeshConfig {
        sliplib_yaml::from_str(
            r#"
cluster:
  bgp_as: 64512
  loopback_networks: {ipv4: "198.51.100.0/25", ipv6: "3fff::/32"}
nodes:
  - {name: a, node_id: "198.51.100.1"}
  - {name: b, node_id: "198.51.100.2"}
mesh:
  links:
    - {pair: [b, a], port: 51820}
roadwarriors:
  - name: obfuscated
    node_hostnames: [a]
    address: "203.0.113.1/27"
    listen_port: 51900
    obfuscation: {jc: 4, jmin: 10, jmax: 50, s1: 1, s2: 2, h1: 5, h2: 6, h3: 7, h4: 8}
  - name: plain
    node_hostnames: [b]
    address: "203.0.113.33/27"
    listen_port: 51901
    plain: true
"#,
        )
        .unwrap()
    }

    #[test]
    fn a_node_with_a_key_written_gets_none() {
        let mut topology = topology();
        topology.nodes[0].mesh_private_key = Some("explicit-key".into());
        let minted = create(&topology);
        assert_eq!(minted.node_keys.len(), 1);
        assert_eq!(minted.node_keys[0].0, 1);
    }

    #[test]
    fn a_plain_link_gets_no_obfuscation() {
        let mut topology = topology();
        topology.mesh.links[0].plain = true;
        assert!(create(&topology).link_obfuscation.is_empty());
    }

    #[test]
    fn a_link_that_is_not_plain_gets_the_minted_fields_it_lacks() {
        let minted = create(&topology());
        let (index, generated) = &minted.link_obfuscation[0];
        assert_eq!(*index, 0);
        assert!(secrets::unminted(generated).is_empty());
    }

    #[test]
    fn a_plain_pool_gets_its_key_and_no_obfuscation() {
        let minted = create(&topology());
        let plain = minted.pools.iter().find(|p| p.name == "plain").unwrap();
        assert!(plain.private_key.is_some() && plain.obfuscation.is_none());
    }

    #[test]
    fn a_pool_that_is_not_plain_gets_the_minted_fields_it_lacks() {
        let mut topology = topology();
        topology.roadwarriors[0].obfuscation = Obfuscation::default();
        let minted = create(&topology);
        let generated = minted.pools[0].obfuscation.as_ref().unwrap();
        assert!(secrets::unminted(generated).is_empty());
    }

    #[test]
    fn routes_name_each_value_by_what_it_belongs_to() {
        let minted = create(&topology());
        assert_eq!(
            minted.routes(),
            [
                "nodes[a].mesh_private_key",
                "nodes[b].mesh_private_key",
                "mesh.links[a|b].obfuscation",
                "roadwarriors[obfuscated].private_key",
                "roadwarriors[plain].private_key",
            ]
        );
    }

    #[test]
    fn a_pool_is_minted_for_its_key_or_its_obfuscation_alone() {
        let mut topology = topology();
        let key_only = create(&topology);
        let obfuscated = &key_only.pools[0];
        assert_eq!(obfuscated.name, "obfuscated");
        assert!(obfuscated.private_key.is_some() && obfuscated.obfuscation.is_none());

        topology.roadwarriors[0].private_key = Some(PrivateKey::generate());
        topology.roadwarriors[0].obfuscation = Obfuscation::default();
        let obfuscation_only = create(&topology);
        let obfuscated = &obfuscation_only.pools[0];
        assert_eq!(obfuscated.name, "obfuscated");
        assert!(obfuscated.private_key.is_none() && obfuscated.obfuscation.is_some());
    }

    #[test]
    fn nothing_is_minted_for_a_topology_that_lacks_nothing() {
        let mut topology = topology();
        assert!(!create(&topology).is_empty());

        for node in &mut topology.nodes {
            node.mesh_private_key = Some(PrivateKey::generate());
        }
        topology.mesh.links[0].obfuscation = crate::test_support::minted_obfuscation();
        for pool in &mut topology.roadwarriors {
            pool.private_key = Some(PrivateKey::generate());
        }
        let minted = create(&topology);
        assert!(minted.is_empty());
        assert!(minted.pools.is_empty());
    }
}
