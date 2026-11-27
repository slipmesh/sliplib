//! The keys and obfuscation the files lack, generated as `Minted` for the caller to write. The whole
//! network at once: a link's ends share one obfuscation, and a node needs its peers' keys.

use crate::network::{self, Mesh};
use crate::secrets;
use sliplib_config::Obfuscation;
use sliplib_config::v2::network::LinkKind;
use sliplib_secrets::wireguard::PrivateKey;
use std::fmt;

mod obfuscation_gen;

#[derive(Default)]
pub struct Minted {
    /// By position in `nodes`.
    pub node_keys: Vec<(usize, PrivateKey)>,
    /// By position in `mesh.links`; only the fields generated.
    pub link_obfuscation: Vec<(usize, Obfuscation)>,
    names: Vec<String>,
}

impl Minted {
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    /// Where each value goes, as a message names it.
    pub fn routes(&self) -> &[String] {
        &self.names
    }
}

/// Its headers, and its type's where it sets none, leave fewer values than it lacks.
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

/// The fields it lacked, `None` for none. `entry` names it in a message.
fn mint_obfuscation(
    obfuscation: &mut Obfuscation,
    entry: &str,
) -> Result<Option<Obfuscation>, NoHeaderLeft> {
    let fresh =
        obfuscation_gen::generate(obfuscation).ok_or_else(|| NoHeaderLeft(entry.to_string()))?;
    Ok(secrets::fill(obfuscation, &fresh))
}

/// A `wireguard` link gets none: it would become an obfuscated one.
pub fn create(mesh: &Mesh<'_>) -> Result<Minted, NoHeaderLeft> {
    let mut minted = Minted::default();

    for (index, (name, node)) in mesh.network().nodes.iter().enumerate() {
        if node.private_key().is_some() || !mesh.needs_a_key(name.as_str()) {
            continue;
        }
        minted.node_keys.push((index, PrivateKey::generate()));
        minted.names.push(network::node_key(name.as_str()));
    }

    for (index, (name, link)) in mesh.network().mesh.links.iter().enumerate() {
        if matches!(link.kind, LinkKind::Wireguard(_)) {
            continue;
        }
        let mut obfuscation = link.kind.obfuscation();
        let entry = network::link_obfuscation(name.as_str());
        if let Some(generated) = mint_obfuscation(&mut obfuscation, &entry)? {
            minted.link_obfuscation.push((index, generated));
            minted.names.push(entry);
        }
    }

    Ok(minted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{NODE_D, example};

    /// The example before anything is minted.
    fn unminted(overlay: &str) -> Mesh<'static> {
        crate::test_support::mesh(crate::test_support::laid(&[
            crate::test_support::EXAMPLE_MESH,
            overlay,
        ]))
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
        let told = refusal.unwrap().to_string();
        assert!(
            told.contains("mesh.links.a-b.amneziawg.obfuscation"),
            "{told}"
        );
    }

    #[test]
    fn a_link_of_type_amneziawg_gets_every_minted_field_it_lacks() {
        let network = unminted("{}");
        let minted = super::create(&network).unwrap();
        // `a-b` and `a-c`; `a-r1` is of type `wireguard`.
        assert_eq!(minted.link_obfuscation.len(), 2);
        for (link, generated) in &minted.link_obfuscation {
            // Exactly the minted fields the link lacked: none it writes, none its type sets.
            let (_, written) = network.network().mesh.links.get_index(*link).unwrap();
            let mut lacked = secrets::unminted(&written.kind.obfuscation());
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

    #[test]
    fn mint_creates_a_key_for_a_node_on_a_link_alone() {
        let network = example(&format!(
            "{{nodes: {{node-b: {{wireguard: null}}, node-d: {NODE_D}}}}}"
        ));
        let minted = super::create(&network).unwrap();
        let keyed: Vec<&str> = minted
            .node_keys
            .iter()
            .map(|(index, _)| {
                network
                    .network()
                    .nodes
                    .get_index(*index)
                    .unwrap()
                    .0
                    .as_str()
            })
            .collect();
        assert_eq!(keyed, ["node-b"]);
    }
}
