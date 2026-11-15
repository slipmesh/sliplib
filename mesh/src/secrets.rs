//! The topology's secrets as `slipmesh.yaml` sets them: every node's mesh private key, every
//! link's and pool's obfuscation, every pool's private key. They are read, never generated: a key
//! the file lacks is absent here, every link and pool gets an obfuscation entry - empty when
//! nothing is set or it is `plain` - and rendering a config that needs what is missing is refused.
//!
//! They are read once for the whole topology, not per node: a link's two ends need the same
//! obfuscation, and rendering node A's config needs node B's private key to derive the public key
//! A dials it with.

use sliplib_config::Obfuscation;
use sliplib_config::mesh_config::MeshConfig;
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
pub fn link_key(pair: &[String; 2]) -> String {
    let mut sorted = pair.clone();
    sorted.sort();
    format!("{}|{}", sorted[0], sorted[1])
}

pub(crate) type FieldIsSet = fn(&Obfuscation) -> bool;

/// The obfuscation fields a link or pool that is not `plain` carries once its secrets are minted.
/// One that lacks any of them has not been minted yet: a config rendered from it would differ from
/// the one rendered after minting.
pub(crate) const MINTED_OBFUSCATION: [(&str, FieldIsSet); 9] = [
    ("jc", |o| o.jc.is_some()),
    ("jmin", |o| o.jmin.is_some()),
    ("jmax", |o| o.jmax.is_some()),
    ("s1", |o| o.s1.is_some()),
    ("s2", |o| o.s2.is_some()),
    ("h1", |o| o.h1.is_some()),
    ("h2", |o| o.h2.is_some()),
    ("h3", |o| o.h3.is_some()),
    ("h4", |o| o.h4.is_some()),
];

/// The fields `obfuscation` lacks of those a minted one carries - `jc`, `jmin`, `jmax`, `s1`, `s2`
/// and `h1` to `h4` - in that order.
pub fn unminted(obfuscation: &Obfuscation) -> Vec<&'static str> {
    MINTED_OBFUSCATION
        .iter()
        .filter(|(_, set)| !set(obfuscation))
        .map(|(name, _)| *name)
        .collect()
}

pub fn read(topology: &MeshConfig) -> ResolvedSecrets {
    let global = &topology.obfuscation;
    ResolvedSecrets {
        mesh_private_keys: topology
            .nodes
            .iter()
            .filter_map(|node| Some((node.name.clone(), node.mesh_private_key.clone()?)))
            .collect(),
        mesh_link_obfuscation: topology
            .mesh
            .links
            .iter()
            .map(|link| {
                let obfuscation = unless_plain(link.plain, &link.obfuscation, global);
                (link_key(&link.pair), obfuscation)
            })
            .collect(),
        roadwarrior_private_keys: topology
            .roadwarriors
            .iter()
            .filter_map(|pool| Some((pool.name.clone(), pool.private_key.clone()?)))
            .collect(),
        roadwarrior_obfuscation: topology
            .roadwarriors
            .iter()
            .map(|pool| {
                let obfuscation = unless_plain(pool.plain, &pool.obfuscation, global);
                (pool.name.clone(), obfuscation)
            })
            .collect(),
    }
}

/// `plain` links and pools (e.g. a RouterOS peer, or a stock WireGuard app, that can't speak
/// AmneziaWG's extensions at all) carry no obfuscation, whatever the topology's default says.
fn unless_plain(plain: bool, specific: &Obfuscation, global: &Obfuscation) -> Obfuscation {
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
            h3: Some(7),
            s4: Some(12),
            ..Obfuscation::default()
        };
        assert_eq!(
            unminted(&obfuscation),
            ["jc", "jmax", "s1", "h1", "h2", "h4"]
        );
        assert!(unminted(&crate::test_support::minted_obfuscation()).is_empty());
    }
}
