//! Every edit a command makes. Only what a run generated is written, never a field copied down from
//! the link's type. Written back from values, the file keeps no comments.

use crate::file::Edit;
use anyhow::{Context, Result};
use sliplib_config::v2::Name;
use sliplib_config::v2::network::InterfacePrefix;
use sliplib_mesh::mint::Minted;
use sliplib_secrets::wireguard::PrivateKey;
use sliplib_yaml::{Mapping, Value};

/// `minted` names nodes and links by position.
pub fn record(edit: &mut Edit, minted: &Minted) -> Result<()> {
    let network = edit.file.network();
    let nodes: Vec<(String, &PrivateKey)> = minted
        .node_keys
        .iter()
        .map(|(node, key)| {
            let (name, _) = network
                .nodes
                .get_index(*node)
                .context("no node at that position")?;
            Ok((name.to_string(), key))
        })
        .collect::<Result<_>>()?;
    let links: Vec<(String, Value)> = minted
        .link_obfuscation
        .iter()
        .map(|(link, generated)| {
            let (name, _) = network
                .mesh
                .links
                .get_index(*link)
                .context("no link at that position")?;
            let fields =
                sliplib_yaml::to_value(generated).context("serializing generated obfuscation")?;
            Ok((name.to_string(), fields))
        })
        .collect::<Result<_>>()?;
    for (name, key) in nodes {
        set_node_key(edit, &name, key)?;
    }
    let laid = edit.file.laid().clone();
    let mesh = laid.get("mesh");
    // Those a link writing `null` there goes without.
    let of_the_type: Vec<Value> = mesh
        .and_then(|mesh| mesh.get("amneziawg"))
        .and_then(|block| block.get("obfuscation"))
        .and_then(Value::as_mapping)
        .into_iter()
        .flatten()
        .filter(|(_, value)| !value.is_null())
        .map(|(key, _)| key.clone())
        .collect();
    let all_links = mesh.and_then(|mesh| mesh.get("links"));
    for (name, fields) in links {
        let link = key_of(all_links, &name)
            .with_context(|| format!("the link {name} is not in the network"))?;
        // Only `amneziawg` is minted for, under its block.
        let block = &all_links
            .and_then(|links| links.get(&link))
            .and_then(|link| link.get("amneziawg"));
        let path = [
            "mesh".into(),
            "links".into(),
            link,
            "amneziawg".into(),
            "obfuscation".into(),
        ];
        write_obfuscation(
            edit,
            &path,
            block.and_then(|block| block.get("obfuscation")),
            &fields,
            &of_the_type,
        )?;
    }
    Ok(())
}

pub fn set_node_key(edit: &mut Edit, node: &str, key: &PrivateKey) -> Result<()> {
    let nodes = edit.file.laid().get("nodes");
    let name =
        key_of(nodes, node).with_context(|| format!("the node {node} is not in the network"))?;
    let path = [
        "nodes".into(),
        name,
        "wireguard".into(),
        "private_key".into(),
    ];
    edit.write(&path, key.expose().into())
}

pub fn write_interface_prefix(edit: &mut Edit) -> Result<()> {
    edit.write(
        &["mesh".into(), "interface_prefix".into()],
        InterfacePrefix::REFERENCE.into(),
    )
}

/// Where `obfuscation` is laid `null`, each of `laid_under` not minted is written `null` too, so
/// minting brings none back.
fn write_obfuscation(
    edit: &mut Edit,
    path: &[Value],
    laid: Option<&Value>,
    generated: &Value,
    laid_under: &[Value],
) -> Result<()> {
    let fields: Mapping = generated
        .as_mapping()
        .into_iter()
        .flatten()
        .filter(|(_, value)| !value.is_null())
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    if laid.is_some_and(Value::is_null) {
        let mut dropped = fields;
        for key in laid_under {
            dropped.entry(key.clone()).or_insert(Value::Null);
        }
        return edit.write(path, Value::Mapping(dropped));
    }
    for (key, value) in fields {
        edit.write(&[path, &[key]].concat(), value)?;
    }
    Ok(())
}

/// As the loader reads keys: a name YAML reads as a number stands under a key that is one.
fn key_of(mapping: Option<&Value>, name: &str) -> Option<Value> {
    mapping?.as_mapping()?.keys().find_map(|key| {
        let read: Name = sliplib_yaml::from_value(key.clone()).ok()?;
        (read.as_str() == name).then(|| key.clone())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sliplib_mesh::mint;

    const NINE: [&str; 9] = ["jc", "jmin", "jmax", "s1", "s2", "h1", "h2", "h3", "h4"];

    /// `edit` run on `raw` as the one file, its text after.
    fn edited(raw: &str, edit: impl FnOnce(&mut Edit)) -> String {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("network.yaml");
        std::fs::write(&path, raw).unwrap();
        let mut files = Edit::read(&[], &path).unwrap();
        edit(&mut files);
        files.save().unwrap();
        std::fs::read_to_string(&path).unwrap()
    }

    /// Read, mint, write.
    fn run(raw: &str) -> String {
        edited(raw, |edit| {
            let minted = mint::create(&sliplib_mesh::Mesh::of(&edit.file).unwrap()).unwrap();
            record(edit, &minted).unwrap();
        })
    }

    fn values(raw: &str) -> Vec<Value> {
        sliplib_yaml::MultiDoc::parse(raw).unwrap().values()
    }

    /// Its keys with their values, its items in their places.
    fn within(part: &Value, whole: &Value) -> bool {
        match (part, whole) {
            (Value::Mapping(part), Value::Mapping(whole)) => part
                .iter()
                .all(|(key, value)| whole.get(key).is_some_and(|w| within(value, w))),
            (Value::Sequence(part), Value::Sequence(whole)) => {
                part.len() == whole.len() && part.iter().zip(whole).all(|(p, w)| within(p, w))
            }
            _ => part == whole,
        }
    }

    /// Nothing minted.
    const EXAMPLE: &str = include_str!("../../docs/example/mesh.yaml");

    /// With a node and a link whose names YAML reads as numbers.
    fn numbered() -> String {
        format!(
            "{}  7: {{platform: talos, node_id: 0.0.0.7, underlay: {{wan: {{}}}}}}\n",
            EXAMPLE.replace(
                "  links:\n",
                "  links:\n    9: {type: amneziawg, ends: {node-a: {}, 7: {}}, \
                 amneziawg: {port: 51840}}\n",
            )
        )
    }

    #[test]
    fn a_node_and_a_link_named_like_numbers_are_minted_under_their_own_keys() {
        let written: Value = sliplib_yaml::from_str(&run(&numbered())).unwrap();
        let node = &written["nodes"][&Value::from(7)];
        assert!(node["wireguard"]["private_key"].is_string(), "{written:?}");
        let link = &written["mesh"]["links"][&Value::from(9)];
        assert!(
            link["amneziawg"]["obfuscation"]["jc"].is_number(),
            "{written:?}"
        );
    }

    /// Once minted.
    fn written_under(raw: &str, link: &str) -> Vec<String> {
        let written: Value = sliplib_yaml::from_str(&run(raw)).unwrap();
        written["mesh"]["links"][link]["amneziawg"]["obfuscation"]
            .as_mapping()
            .unwrap()
            .keys()
            .map(|key| key.as_str().unwrap().to_owned())
            .collect()
    }

    // The type of the example sets `s3`, `s4`, the `i` fields and both switches: none of them is
    // written under a link.
    #[test]
    fn a_link_gets_the_minted_fields_it_lacks_after_those_it_writes_and_none_its_type_sets() {
        for (link, written) in [
            ("a-b", &NINE[..]),
            (
                "a-c",
                &[
                    "h4",
                    "rekey_after_time",
                    "keepalive_timeout",
                    "jc",
                    "jmin",
                    "jmax",
                    "s1",
                    "s2",
                    "h1",
                    "h2",
                    "h3",
                ],
            ),
        ] {
            assert_eq!(written_under(EXAMPLE, link), written, "{link}");
        }
        let written: Value = sliplib_yaml::from_str(&run(EXAMPLE)).unwrap();
        assert_eq!(
            written["mesh"]["links"]["a-c"]["amneziawg"]["obfuscation"]["h4"],
            Value::from("5505-5599")
        );
    }

    #[test]
    fn every_value_written_before_stays() {
        let (before, after) = (values(EXAMPLE), values(&run(EXAMPLE)));
        assert_eq!(before.len(), after.len());
        for (before, after) in before.iter().zip(&after) {
            assert!(within(before, after), "{before:?}\n{after:?}");
        }
    }
}
