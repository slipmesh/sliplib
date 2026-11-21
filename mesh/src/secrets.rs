//! The topology's secrets as `slipmesh.yaml` sets them: every node's mesh private key, every
//! link's and pool's obfuscation, every pool's private key. They are read, never generated: a key
//! the file lacks is absent here, every link and pool gets an obfuscation entry - empty when
//! nothing is set or it is `plain` - and rendering a config that needs what is missing is refused.
//!
//! They are read once for the whole topology, not per node: a link's two ends need the same
//! obfuscation, and rendering node A's config needs node B's private key to derive the public key
//! A dials it with.

use crate::model::Network;
use sliplib_config::Obfuscation;
use sliplib_config::v1::mesh_config::{MeshConfig, RoadwarriorPool};
use sliplib_secrets::wireguard::PrivateKey;
use std::collections::HashMap;

pub struct ResolvedSecrets {
    pub mesh_private_keys: HashMap<String, PrivateKey>,
    pub mesh_link_obfuscation: HashMap<String, Obfuscation>,
    pub roadwarrior_private_keys: HashMap<String, PrivateKey>,
    pub roadwarrior_obfuscation: HashMap<String, Obfuscation>,
}

/// A key or an obfuscation the rendered node needs and the file does not set. A refusal that
/// carries one is cured by creating the secret, which a caller can tell its user how to do.
#[derive(Debug)]
pub struct MissingSecret(pub(crate) String);

impl std::fmt::Display for MissingSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for MissingSecret {}

/// Canonical, order-independent key for a mesh link's maps (`pair`'s two names, sorted).
pub(crate) fn link_key(pair: &[String; 2]) -> String {
    let mut sorted = pair.clone();
    sorted.sort();
    format!("{}|{}", sorted[0], sorted[1])
}

/// The obfuscation fields a link or pool that is not `plain` carries once its secrets are minted,
/// each with the type of its value. One that lacks any of them has not been minted yet: a config
/// rendered from it would differ from the one rendered after minting. What is looked for and what
/// minting fills are generated from this one list, so they cannot differ.
macro_rules! minted_fields {
    ($($field:ident: $value:ty),* $(,)?) => {
        /// The fields `obfuscation` lacks of those a minted one carries, in the list's order.
        pub(crate) fn unminted(obfuscation: &Obfuscation) -> Vec<&'static str> {
            let mut lacking = Vec::new();
            $(if obfuscation.$field.is_none() {
                lacking.push(stringify!($field));
            })*
            lacking
        }

        /// A new value for every minted field.
        #[cfg(feature = "mint")]
        #[derive(Debug)]
        pub(crate) struct Fresh {
            $(pub(crate) $field: $value,)*
        }

        /// Sets each minted field `obfuscation` lacks to its value in `fresh`, and returns the
        /// fields it set - `None` when `obfuscation` lacked none.
        #[cfg(feature = "mint")]
        pub(crate) fn fill(obfuscation: &mut Obfuscation, fresh: &Fresh) -> Option<Obfuscation> {
            let mut filled = Obfuscation::default();
            $(if obfuscation.$field.is_none() {
                obfuscation.$field = Some(fresh.$field.into());
                filled.$field = Some(fresh.$field.into());
            })*
            (filled != Obfuscation::default()).then_some(filled)
        }
    };
}

minted_fields!(jc: u16, jmin: u16, jmax: u16, s1: u16, s2: u16, h1: u32, h2: u32, h3: u32, h4: u32);

/// Whether pool `name` has its key written and, unless it is `plain`, every minted field of its
/// obfuscation: what a client's config is built with. Nothing else of the topology is looked at.
pub fn pool_is_written(topology: &MeshConfig, name: &str) -> bool {
    let written = read_pools(topology);
    topology
        .roadwarriors
        .iter()
        .find(|pool| pool.name == name)
        .is_some_and(|pool| {
            written.roadwarrior_private_keys.contains_key(name)
                && (pool.plain
                    || written
                        .roadwarrior_obfuscation
                        .get(name)
                        .is_some_and(|obfuscation| unminted(obfuscation).is_empty()))
        })
}

/// The secrets of `pools`, each pool's obfuscation laid over `global`, with none of the mesh.
fn of_pools(pools: &[RoadwarriorPool], global: &Obfuscation) -> ResolvedSecrets {
    ResolvedSecrets {
        mesh_private_keys: HashMap::new(),
        mesh_link_obfuscation: HashMap::new(),
        roadwarrior_private_keys: pools
            .iter()
            .filter_map(|pool| Some((pool.name.clone(), pool.private_key.clone()?)))
            .collect(),
        roadwarrior_obfuscation: pools
            .iter()
            .map(|pool| {
                let obfuscation = unless_plain(pool.plain, &pool.obfuscation, global);
                (pool.name.clone(), obfuscation)
            })
            .collect(),
    }
}

/// The secrets of the pools of `topology`, and none of the mesh: what a client's config is built
/// with, read without the network the nodes' configs are computed from.
pub fn read_pools(topology: &MeshConfig) -> ResolvedSecrets {
    of_pools(&topology.roadwarriors, &topology.obfuscation)
}

pub fn read(network: &Network) -> ResolvedSecrets {
    let pools = match network.format.v1() {
        Some(v1) => of_pools(&v1.pools, &v1.obfuscation),
        None => of_pools(&[], &Obfuscation::default()),
    };
    ResolvedSecrets {
        mesh_private_keys: network
            .nodes
            .iter()
            .filter_map(|node| Some((node.name.clone(), node.private_key.clone()?)))
            .collect(),
        mesh_link_obfuscation: network
            .links
            .iter()
            .map(|link| (link_key(&link.ends), link.obfuscation.clone()))
            .collect(),
        ..pools
    }
}

/// `plain` links and pools (e.g. a RouterOS peer, or a stock WireGuard app, that can't speak
/// AmneziaWG's extensions at all) carry no obfuscation, whatever the topology's default says.
pub(crate) fn unless_plain(
    plain: bool,
    specific: &Obfuscation,
    global: &Obfuscation,
) -> Obfuscation {
    if plain {
        Obfuscation::default()
    } else {
        layer(specific, global)
    }
}

/// `specific` (a link's or pool's own `obfuscation`) over `global` (the topology's top-level
/// default), field by field.
pub(crate) fn layer(specific: &Obfuscation, global: &Obfuscation) -> Obfuscation {
    macro_rules! field {
        ($f:ident) => {
            specific.$f.clone().or_else(|| global.$f.clone())
        };
    }
    Obfuscation {
        jc: field!(jc),
        jmin: field!(jmin),
        jmax: field!(jmax),
        s1: field!(s1),
        s2: field!(s2),
        s3: field!(s3),
        s4: field!(s4),
        h1: field!(h1),
        h2: field!(h2),
        h3: field!(h3),
        h4: field!(h4),
        i1: field!(i1),
        i2: field!(i2),
        i3: field!(i3),
        i4: field!(i4),
        i5: field!(i5),
        header_protection_key: field!(header_protection_key),
        content_padding_addition: field!(content_padding_addition),
        rekey_after_time: field!(rekey_after_time),
        rekey_timeout: field!(rekey_timeout),
        reject_after_time: field!(reject_after_time),
        keepalive_timeout: field!(keepalive_timeout),
        max_handshake_attempts: field!(max_handshake_attempts),
        random_trailers: field!(random_trailers),
        disable_cookies: field!(disable_cookies),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The fixtures are topologies of format version 1, read through its lowering.
    fn read(mesh: &MeshConfig) -> ResolvedSecrets {
        super::read(&crate::v1::lower(mesh).unwrap())
    }

    /// The pools of `topology()`, with `north` given every secret a client's config is built
    /// with.
    fn with_north_written() -> MeshConfig {
        let mut topology = topology();
        let north = &mut topology.roadwarriors[0];
        north.private_key = Some(PrivateKey::generate());
        north.obfuscation = sliplib_yaml::from_str(
            "{jc: 4, jmin: 10, jmax: 50, s1: 15, s2: 16, h1: 5, h2: 6, h3: 7, h4: 8}",
        )
        .unwrap();
        topology
    }

    #[test]
    fn a_pool_with_its_key_and_every_minted_field_is_written() {
        assert!(pool_is_written(&with_north_written(), "north"));
    }

    #[test]
    fn a_pool_without_its_key_or_one_minted_field_is_not_written() {
        let mut keyless = with_north_written();
        keyless.roadwarriors[0].private_key = None;
        assert!(!pool_is_written(&keyless, "north"));

        let mut lacking = with_north_written();
        lacking.roadwarriors[0].obfuscation.h4 = None;
        assert!(!pool_is_written(&lacking, "north"));
    }

    #[test]
    fn a_plain_pool_needs_its_key_and_no_obfuscation() {
        let mut plain = with_north_written();
        plain.roadwarriors[0].plain = true;
        plain.roadwarriors[0].obfuscation = Obfuscation::default();
        assert!(pool_is_written(&plain, "north"));
        plain.roadwarriors[0].private_key = None;
        assert!(!pool_is_written(&plain, "north"));
    }

    #[test]
    fn a_pool_is_judged_by_itself_and_one_the_file_lacks_is_not_written() {
        let topology = with_north_written();
        for other in topology.roadwarriors.iter().skip(1) {
            assert!(!pool_is_written(&topology, &other.name), "{}", other.name);
        }
        assert!(!pool_is_written(&topology, "nowhere"));
    }

    #[test]
    fn link_key_does_not_depend_on_the_order_of_the_pair() {
        let ab = link_key(&["a".to_owned(), "b".to_owned()]);
        let ba = link_key(&["b".to_owned(), "a".to_owned()]);
        assert_eq!(ab, ba);
    }

    fn topology() -> MeshConfig {
        sliplib_yaml::from_str(
            r#"
cluster:
  bgp_as: 64512
  loopback_networks: {ipv4: "198.51.100.0/25", ipv6: "3fff::/32"}
obfuscation: {jc: 1, jmin: 10, random_trailers: true}
nodes:
  - {name: a, node_id: "198.51.100.1"}
  - {name: b, node_id: "198.51.100.2"}
  - {name: c, node_id: "198.51.100.3"}
mesh:
  links:
    - {pair: [a, b], port: 51820, obfuscation: {jc: 9}}
    - {pair: [c, b], port: 51821, plain: true}
roadwarriors:
  - name: north
    node_hostnames: [a]
    address: "203.0.113.1/27"
    listen_port: 51900
    obfuscation: {jc: 7}
    clients: []
  - name: south
    node_hostnames: [a]
    address: "203.0.113.33/27"
    listen_port: 51901
    plain: true
    clients: []
"#,
        )
        .unwrap()
    }

    #[test]
    fn the_keys_the_file_sets_are_read_and_the_others_are_absent() {
        let mut topology = topology();
        let node_key = PrivateKey::generate();
        let pool_key = PrivateKey::generate();
        topology.nodes[1].mesh_private_key = Some(node_key.clone());
        topology.roadwarriors[0].private_key = Some(pool_key.clone());
        let read = read(&topology);
        assert_eq!(read.mesh_private_keys.len(), 1);
        assert_eq!(read.mesh_private_keys["b"], node_key);
        assert_eq!(read.roadwarrior_private_keys.len(), 1);
        assert_eq!(read.roadwarrior_private_keys["north"], pool_key);
    }

    #[test]
    fn a_link_or_pool_takes_its_own_obfuscation_over_the_topologys_field_by_field() {
        let read = read(&topology());
        for obfuscation in [
            &read.mesh_link_obfuscation["a|b"],
            &read.roadwarrior_obfuscation["north"],
        ] {
            assert_ne!(obfuscation.jc, Some(1));
            assert_eq!(obfuscation.jmin, Some(10));
            assert_eq!(obfuscation.random_trailers, Some(true));
            assert_eq!(obfuscation.jmax, None);
        }
        assert_eq!(read.mesh_link_obfuscation["a|b"].jc, Some(9));
        assert_eq!(read.roadwarrior_obfuscation["north"].jc, Some(7));
    }

    #[test]
    fn a_plain_link_or_pool_carries_no_obfuscation_whatever_the_topology_says() {
        let read = read(&topology());
        assert_eq!(read.mesh_link_obfuscation["b|c"], Obfuscation::default());
        assert_eq!(
            read.roadwarrior_obfuscation["south"],
            Obfuscation::default()
        );
    }

    #[test]
    fn unminted_names_the_minted_fields_an_obfuscation_lacks_in_order() {
        let obfuscation = Obfuscation {
            jmin: Some(10),
            s2: Some(16),
            h3: Some(7.into()),
            s4: Some(12),
            ..Obfuscation::default()
        };
        assert_eq!(
            unminted(&obfuscation),
            ["jc", "jmax", "s1", "h1", "h2", "h4"]
        );
        assert!(unminted(&crate::test_support::minted_obfuscation()).is_empty());
    }

    /// The secrets of [`crate::test_support::example`].
    fn example(overlay: &str) -> ResolvedSecrets {
        super::read(&crate::test_support::example(overlay))
    }

    #[test]
    fn the_keys_the_files_write_are_read_and_the_others_are_absent() {
        let read = example("{nodes: {node-c: {wireguard: null}}}");
        let mut keyed: Vec<&str> = read.mesh_private_keys.keys().map(String::as_str).collect();
        keyed.sort_unstable();
        assert_eq!(keyed, ["node-a", "node-b", "router1"]);
        assert_eq!(
            read.mesh_private_keys["node-b"].expose(),
            "EEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEE="
        );
    }

    #[test]
    fn every_link_carries_the_obfuscation_in_force_on_it_and_one_of_wireguard_none() {
        let read = example("{}");
        let a_b = &read.mesh_link_obfuscation["node-a|node-b"];
        // Its own, and the type's beneath it.
        assert_eq!(a_b.jc, Some(4));
        assert_eq!(a_b.s3, Some(40));
        assert_eq!(
            read.mesh_link_obfuscation["node-a|router1"],
            Obfuscation::default()
        );
        assert_eq!(read.mesh_link_obfuscation.len(), 3);
    }
}
