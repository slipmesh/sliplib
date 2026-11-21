//! The network's secrets as `slipmesh.yaml` sets them: every node's mesh private key and every
//! link's obfuscation. They are read, never generated: a key the files lack is absent here, every
//! link gets an obfuscation entry - empty when nothing is set or it is `plain` - and rendering a
//! config that needs what is missing is refused.
//!
//! They are read once for the whole network, not per node: a link's two ends need the same
//! obfuscation, and rendering node A's config needs node B's private key to derive the public key
//! A dials it with.

use crate::model::Network;
use sliplib_config::Obfuscation;
use sliplib_secrets::wireguard::PrivateKey;
use std::collections::HashMap;

pub struct ResolvedSecrets {
    pub mesh_private_keys: HashMap<String, PrivateKey>,
    pub mesh_link_obfuscation: HashMap<String, Obfuscation>,
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

pub fn read(network: &Network) -> ResolvedSecrets {
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
