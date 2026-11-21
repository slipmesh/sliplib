//! The secrets `slipmesh.yaml` leaves unset, generated: a node's mesh private key and a link's
//! obfuscation. What was generated comes back as `Minted`, for the caller to write into the field
//! it belongs to - the next run finds it there and generates nothing.
//!
//! The whole network at once, not per node: a link's two ends share one obfuscation, and a node's
//! config needs its peers' keys.

use crate::model::{self, Network};
use crate::secrets::{self, link_key};
use sliplib_config::Obfuscation;
use sliplib_secrets::wireguard::PrivateKey;
use std::fmt;

mod obfuscation_gen;

/// What a run generated, by the entry it belongs to.
#[derive(Default)]
pub struct Minted {
    /// By the node's position in `nodes`.
    pub node_keys: Vec<(usize, PrivateKey)>,
    /// By the link's position in `mesh.links`; only the fields generated.
    pub link_obfuscation: Vec<(usize, Obfuscation)>,
    /// How a message names each value.
    names: Vec<String>,
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

/// The entry whose magic headers, its own and its type's it takes where it sets none, leave
/// fewer values than the headers it lacks.
#[derive(Debug, PartialEq, Eq)]
pub struct NoHeaderLeft(String);

impl fmt::Display for NoHeaderLeft {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: the magic headers it holds leave fewer values than the headers it lacks; they are its own h1..h4 and the ones its type's obfuscation gives it - narrow them where they are written, or write all four",
            self.0
        )
    }
}

impl std::error::Error for NoHeaderLeft {}

/// Generates the minted fields `obfuscation` lacks and returns them, `None` when it lacked none.
/// `entry` is how a message names what the obfuscation belongs to.
fn mint_obfuscation(
    obfuscation: &mut Obfuscation,
    entry: &str,
) -> Result<Option<Obfuscation>, NoHeaderLeft> {
    let fresh =
        obfuscation_gen::generate(obfuscation).ok_or_else(|| NoHeaderLeft(entry.to_string()))?;
    Ok(secrets::fill(obfuscation, &fresh))
}

/// Whatever secrets `network` lacks, generated, each by the entry it belongs to. A `plain` link
/// gets no obfuscation generated: it would turn "no obfuscation" back into an obfuscated one.
pub fn create(network: &Network) -> Result<Minted, NoHeaderLeft> {
    let mut written = secrets::read(network);
    let mut minted = Minted::default();

    for (index, node) in network.nodes.iter().enumerate() {
        if written.mesh_private_keys.contains_key(&node.name) || !network.needs_a_key(&node.name) {
            continue;
        }
        minted.node_keys.push((index, PrivateKey::generate()));
        minted.names.push(model::node_key(&node.name));
    }

    for (index, link) in network.links.iter().enumerate() {
        if link.plain {
            continue;
        }
        let pair = link_key(&link.ends);
        let obfuscation = written
            .mesh_link_obfuscation
            .entry(pair.clone())
            .or_default();
        let entry = model::link_obfuscation(link);
        if let Some(generated) = mint_obfuscation(obfuscation, &entry)? {
            minted.link_obfuscation.push((index, generated));
            minted.names.push(entry);
        }
    }

    Ok(minted)
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
        let generated = mint_obfuscation(&mut resolved, "an entry")
            .unwrap()
            .unwrap();
        assert_eq!(resolved.random_trailers, Some(true));
        assert_eq!(resolved.disable_cookies, None);
        assert_eq!(generated.random_trailers, None);
    }

    #[test]
    fn mint_obfuscation_generates_when_nothing_else_is_set() {
        let mut resolved = Obfuscation::default();
        let generated = mint_obfuscation(&mut resolved, "an entry")
            .unwrap()
            .unwrap();
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
        let generated = mint_obfuscation(&mut resolved, "an entry")
            .unwrap()
            .unwrap();
        assert_eq!((generated.jc, generated.h1), (None, None));
        assert_eq!(generated.jmin, resolved.jmin);
        assert!(generated.jmin.is_some());
    }

    /// The example of the specification as it is before anything is minted, with `overlay` laid
    /// over it.
    fn unminted(overlay: &str) -> Network {
        let files = crate::test_support::laid(&[crate::test_support::EXAMPLE_MESH, overlay]);
        crate::v2::lower(&files).unwrap()
    }

    #[test]
    fn what_is_minted_is_named_as_the_files_write_it() {
        let minted = super::create(&unminted("{}")).unwrap();
        assert_eq!(
            minted.routes(),
            [
                "nodes.node-a.wireguard.private_key",
                "nodes.node-b.wireguard.private_key",
                "nodes.node-c.wireguard.private_key",
                "nodes.router1.wireguard.private_key",
                "mesh.links.a-b.amneziawg.obfuscation",
                "mesh.links.a-c.amneziawg.obfuscation",
            ]
        );
        // By their positions in the network: the link of type `wireguard` is the third.
        let links: Vec<usize> = minted
            .link_obfuscation
            .iter()
            .map(|(link, _)| *link)
            .collect();
        assert_eq!(links, [0, 1]);
    }

    #[test]
    fn a_link_with_no_header_left_is_named_as_the_files_write_it() {
        let full = "{mesh: {links: {a-b: {amneziawg: {obfuscation: {h1: 0-4294967295}}}}}}";
        let refusal = super::create(&unminted(full)).err();
        assert_eq!(
            refusal,
            Some(NoHeaderLeft(
                "mesh.links.a-b.amneziawg.obfuscation".to_owned()
            ))
        );
    }

    #[test]
    fn a_node_with_its_key_written_gets_none() {
        let network = unminted(
            "{nodes: {node-b: {wireguard: \
             {private_key: EEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEE=}}}}",
        );
        let minted = super::create(&network).unwrap();
        let keyed: Vec<&str> = minted
            .node_keys
            .iter()
            .map(|(node, _)| network.nodes[*node].name.as_str())
            .collect();
        assert_eq!(keyed, ["node-a", "node-c", "router1"]);
    }

    #[test]
    fn a_link_of_type_amneziawg_gets_every_minted_field_it_lacks() {
        let network = unminted("{}");
        let minted = super::create(&network).unwrap();
        // `a-b` and `a-c`; `a-r1` is of type `wireguard`.
        assert_eq!(minted.link_obfuscation.len(), 2);
        for (link, generated) in &minted.link_obfuscation {
            // Exactly the minted fields the link lacked: none it writes, none its type sets.
            let mut lacked = secrets::unminted(&network.links[*link].obfuscation);
            let mut generated = sliplib_yaml::to_value(generated)
                .unwrap()
                .as_mapping()
                .unwrap()
                .iter()
                .filter(|(_, value)| !value.is_null())
                .map(|(field, _)| field.as_str().unwrap().to_owned())
                .collect::<Vec<_>>();
            lacked.sort_unstable();
            generated.sort_unstable();
            assert!(!lacked.is_empty(), "{link}");
            assert_eq!(generated, lacked, "{link}");
        }
    }

    #[test]
    fn nothing_is_minted_for_files_that_lack_nothing() {
        let minted = super::create(&crate::test_support::example("{}")).unwrap();
        assert!(minted.is_empty());
    }

    // The `s4` in force on a link is its type's where it sets none of its own.
    #[test]
    fn offsets_go_around_the_s4_the_type_gives_a_link() {
        let network = unminted("{mesh: {amneziawg: {obfuscation: {s4: 100}}}}");
        let off_the_header = |offset: Option<u16>| !(101..=115).contains(&offset.unwrap());
        for _ in 0..200 {
            let minted = super::create(&network).unwrap();
            assert_eq!(minted.link_obfuscation.len(), 2);
            for (link, generated) in &minted.link_obfuscation {
                assert!(
                    off_the_header(generated.s1) && off_the_header(generated.s2),
                    "{link}: {generated:?}"
                );
            }
        }
    }
}
