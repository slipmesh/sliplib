//! Every edit made to `slipmesh.yaml`: minted secrets and a node's new key written into the
//! fields they belong to, and the reference values `mint` writes where the files leave a key out.
//!
//! Written, a minted value is the operator's like any other: the next run mints nothing, and
//! deleting it mints a new identity for that node or link, and for every peer that refers to it.
//! Only what the run generated is written - a field set on the link or on its type is never copied
//! down, and neither are the switches that are never generated.
//!
//! An edit changes one document's values. The file is written back from the values, so its
//! comments are not kept.

use anyhow::{Context, Result};
use sliplib_config::Obfuscation;
use sliplib_config::v2::{File, Name};
use sliplib_mesh::mint::Minted;
use sliplib_secrets::wireguard::PrivateKey;
use sliplib_yaml::{Mapping, MultiDoc, Value};

/// Writes every value of `minted` into its field of `yaml`, the `slipmesh.yaml` `files` were read
/// from: `minted` names nodes and links by their positions in the network.
pub fn record(yaml: &mut MultiDoc, files: &File, minted: &Minted) -> Result<()> {
    let network = files.network();
    for (node, key) in &minted.node_keys {
        let (name, _) = network
            .nodes
            .get_index(*node)
            .context("no node at that position")?;
        set_node_key(yaml, files, name.as_str(), key)?;
    }
    let mesh = document(yaml, files.network_document())?.get_mut("mesh");
    // The fields of the type's own obfuscation, which a link that writes `null` there goes
    // without.
    let of_the_type: Vec<Value> = mesh
        .as_deref()
        .and_then(|mesh| mesh.get("amneziawg"))
        .and_then(|block| block.get("obfuscation"))
        .and_then(Value::as_mapping)
        .into_iter()
        .flatten()
        .filter(|(_, value)| !value.is_null())
        .map(|(key, _)| key.clone())
        .collect();
    let mut links = mesh.and_then(|mesh| mesh.get_mut("links"));
    for (link, generated) in &minted.link_obfuscation {
        let (name, _) = network
            .mesh
            .links
            .get_index(*link)
            .context("no link at that position")?;
        let name = name.as_str();
        // Only a link of type `amneziawg` is minted for, under the block of its type.
        let entry = named(links.as_deref_mut(), name)
            .and_then(|link| link.get_mut("amneziawg"))
            .with_context(|| format!("the link {name} is not in the network document"))?;
        write_obfuscation(entry, generated, &of_the_type)?;
    }
    Ok(())
}

/// Sets the key of `node` in `yaml`, the `slipmesh.yaml` `files` was read from.
pub fn set_node_key(yaml: &mut MultiDoc, files: &File, node: &str, key: &PrivateKey) -> Result<()> {
    let nodes = document(yaml, files.network_document())?.get_mut("nodes");
    let entry = named(nodes, node)
        .with_context(|| format!("the node {node} is not in the network document"))?;
    if entry.get("wireguard").is_none_or(Value::is_null) {
        set(entry, "wireguard", Value::Mapping(Mapping::new()))?;
    }
    set(&mut entry["wireguard"], "private_key", key.expose().into())
}

/// Writes each of `unset`, a key and its value, in the network document of `yaml`, the
/// `slipmesh.yaml` `files` was read from. Indexing inserts each missing step, and panics on none:
/// every step that is there is a mapping, or the file would not have parsed.
pub fn write_unset(yaml: &mut MultiDoc, files: &File, unset: &[(&str, Value)]) -> Result<()> {
    let network = document(yaml, files.network_document())?;
    for (key, value) in unset {
        *key.split('.')
            .fold(&mut *network, |value, step| &mut value[step]) = value.clone();
    }
    Ok(())
}

/// Writes the minted fields into `owner`'s `obfuscation`. Where it is written `null`, which takes
/// away every field laid under it, each of `laid_under` not minted is written `null` too, so that
/// minting brings back none of them.
fn write_obfuscation(
    owner: &mut Value,
    generated: &Obfuscation,
    laid_under: &[Value],
) -> Result<()> {
    let fields = sliplib_yaml::to_value(generated).context("serializing generated obfuscation")?;
    let fields: Mapping = fields
        .as_mapping()
        .into_iter()
        .flatten()
        .filter(|(_, value)| !value.is_null())
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    match owner.get_mut("obfuscation") {
        Some(Value::Mapping(obfuscation)) => obfuscation.extend(fields),
        Some(Value::Null) => {
            let mut dropped = fields;
            for key in laid_under {
                dropped.entry(key.clone()).or_insert(Value::Null);
            }
            set(owner, "obfuscation", Value::Mapping(dropped))?;
        }
        _ => set(owner, "obfuscation", Value::Mapping(fields))?,
    }
    Ok(())
}

/// The entry of `mapping` whose key reads as `name`, as the loader reads it: a name YAML reads as
/// a number, `7`, stands under a key that is one.
fn named<'a>(mapping: Option<&'a mut Value>, name: &str) -> Option<&'a mut Value> {
    mapping?
        .as_mapping_mut()?
        .iter_mut()
        .find_map(|(key, value)| {
            let key: Name = sliplib_yaml::from_value(key.clone()).ok()?;
            (key.as_str() == name).then_some(value)
        })
}

fn set(entry: &mut Value, key: &str, value: Value) -> Result<()> {
    entry
        .as_mapping_mut()
        .with_context(|| format!("an entry to set `{key}` on is not a mapping"))?
        .insert(key.into(), value);
    Ok(())
}

fn document(yaml: &mut MultiDoc, index: usize) -> Result<&mut Value> {
    yaml.document_mut(index)
        .with_context(|| format!("no document at position {index}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sliplib_mesh::mint;

    const NINE: [&str; 9] = ["jc", "jmin", "jmax", "s1", "s2", "h1", "h2", "h3", "h4"];

    /// One run: read, mint, write down what was minted.
    fn run(raw: &str) -> String {
        let mut yaml = MultiDoc::parse(raw).unwrap();
        let files = File::read(&yaml).unwrap();
        let minted = mint::create(&sliplib_mesh::Mesh::of(&files).unwrap()).unwrap();
        record(&mut yaml, &files, &minted).unwrap();
        yaml.text().unwrap()
    }

    fn values(raw: &str) -> Vec<Value> {
        MultiDoc::parse(raw)
            .unwrap()
            .documents()
            .iter()
            .map(|d| d.value.clone())
            .collect()
    }

    /// Whether everything `part` holds is in `whole` as well: its keys with their values, its
    /// items in their places.
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

    /// The example of the specification with nothing minted.
    const EXAMPLE: &str = include_str!("../../docs/example/mesh.yaml");

    /// The example with a node and a link whose names YAML reads as numbers: they
    /// stand under keys that are numbers.
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

    #[test]
    fn a_node_named_like_a_number_is_rekeyed_under_its_own_key() {
        let mut yaml = MultiDoc::parse(&numbered()).unwrap();
        let files = File::read(&yaml).unwrap();
        set_node_key(&mut yaml, &files, "7", &PrivateKey::generate()).unwrap();
        let node = &yaml.documents()[0].value["nodes"][&Value::from(7)];
        assert!(node["wireguard"]["private_key"].is_string());
    }

    /// The keys of the obfuscation the link `link` of `raw` writes, once minted.
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
    fn a_link_gets_the_nine_minted_fields_and_none_its_type_sets() {
        assert_eq!(written_under(EXAMPLE, "a-b"), NINE);
    }

    #[test]
    fn a_minted_field_its_type_sets_is_not_written_under_a_link() {
        let of_the_type = EXAMPLE.replace(
            "    obfuscation:\n      s3: 40\n",
            "    obfuscation:\n      jc: 5\n      s3: 40\n",
        );
        assert_ne!(of_the_type, EXAMPLE);
        assert_eq!(written_under(&of_the_type, "a-b"), &NINE[1..]);
    }

    #[test]
    fn a_field_the_link_writes_stays_and_the_minted_ones_join_it() {
        let written = written_under(EXAMPLE, "a-c");
        assert_eq!(
            written,
            [
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
            ]
        );
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
