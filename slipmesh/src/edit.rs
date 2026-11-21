//! Every edit made to `slipmesh.yaml`: minted secrets and a node's new key written into the
//! fields they belong to, the values `mint` writes where the file leaves a key out (version 1's
//! defaults, version 2's reference values), and roadwarrior clients added and removed.
//!
//! Written, a minted value is the operator's like any other: the next run mints nothing, and
//! deleting it mints a new identity for that node, link or pool, and for every peer that refers to
//! it. Only what the run generated is written - a field set on the entry or in the global
//! `obfuscation` is never copied down, and neither are the switches that are never generated.
//!
//! An edit changes one document's values. The file is written back from the values, so its
//! comments are not kept.

use anyhow::{Context, Result};
use sliplib_config::Obfuscation;
use sliplib_config::slipmesh_file::SlipmeshFile;
use sliplib_config::v1::mesh_config::RoadwarriorClient;
use sliplib_config::v1::mesh_config::missing_defaults;
use sliplib_config::v2::Name;
use sliplib_mesh::mint::Minted;
use sliplib_mesh::model::Network;
use sliplib_secrets::wireguard::PrivateKey;
use sliplib_yaml::{Mapping, MultiDoc, Value};

/// The position of the `network` document of `file`, where a value of the network is written.
fn network_document(file: &SlipmeshFile) -> usize {
    match file {
        SlipmeshFile::V1(file) => file.network_document(),
        SlipmeshFile::V2(files) => files.network_document(),
    }
}

/// Writes every value of `minted` into its field of `yaml`, the `slipmesh.yaml` that `file` and
/// then `network` were read from: `minted` names nodes and links by their positions in `network`.
pub fn record(
    yaml: &mut MultiDoc,
    file: &SlipmeshFile,
    network: &Network,
    minted: &Minted,
) -> Result<()> {
    for (node, key) in &minted.node_keys {
        set_node_key(yaml, file, &network.nodes[*node].name, key)?;
    }
    let mesh = document(yaml, network_document(file))?.get_mut("mesh");
    // The fields of the type's own obfuscation, which a link that writes `null` there goes
    // without; version 1 lays nothing under a link's own.
    let of_the_type: Vec<Value> = match file {
        SlipmeshFile::V1(_) => Vec::new(),
        SlipmeshFile::V2(_) => mesh
            .as_deref()
            .and_then(|mesh| mesh.get("amneziawg"))
            .and_then(|block| block.get("obfuscation"))
            .and_then(Value::as_mapping)
            .into_iter()
            .flatten()
            .filter(|(_, value)| !value.is_null())
            .map(|(key, _)| key.clone())
            .collect(),
    };
    let mut links = mesh.and_then(|mesh| mesh.get_mut("links"));
    for (link, generated) in &minted.link_obfuscation {
        let name = &network.links[*link].name;
        // A list by position in version 1; in version 2 a mapping by name, each link with the
        // block of its type, and only a link of type `amneziawg` is minted for.
        let entry = match file {
            SlipmeshFile::V1(_) => links.as_deref_mut().and_then(|links| links.get_mut(*link)),
            SlipmeshFile::V2(_) => {
                named(links.as_deref_mut(), name).and_then(|link| link.get_mut("amneziawg"))
            }
        }
        .with_context(|| format!("the link {name} is not in the network document"))?;
        write_obfuscation(entry, generated, &of_the_type)?;
    }

    let SlipmeshFile::V1(file) = file else {
        return Ok(());
    };
    for pool_minted in &minted.pools {
        let name = &pool_minted.name;
        let index = file
            .pool_document(name)
            .with_context(|| format!("pool {name:?} is in no document"))?;
        let pool = document(yaml, index)?;
        if let Some(key) = &pool_minted.private_key {
            set(pool, "private_key", key.expose().into())?;
        }
        if let Some(generated) = &pool_minted.obfuscation {
            write_obfuscation(pool, generated, &[])?;
        }
    }
    Ok(())
}

/// Sets the key of `node` in `yaml`, the `slipmesh.yaml` `file` was read from, where the version
/// of the file holds it.
pub fn set_node_key(
    yaml: &mut MultiDoc,
    file: &SlipmeshFile,
    node: &str,
    key: &PrivateKey,
) -> Result<()> {
    let nodes = document(yaml, network_document(file))?.get_mut("nodes");
    let missing = || format!("the node {node} is not in the network document");
    match file {
        SlipmeshFile::V1(file) => {
            let position = file.hosts().iter().position(|host| *host == node);
            let entry = nodes
                .zip(position)
                .and_then(|(nodes, position)| nodes.get_mut(position))
                .with_context(missing)?;
            set(entry, "mesh_private_key", key.expose().into())
        }
        SlipmeshFile::V2(_) => {
            let entry = named(nodes, node).with_context(missing)?;
            if entry.get("wireguard").is_none_or(Value::is_null) {
                set(entry, "wireguard", Value::Mapping(Mapping::new()))?;
            }
            set(&mut entry["wireguard"], "private_key", key.expose().into())
        }
    }
}

/// The keys of the network that `file` leaves out and `mint` writes, each with its value: the
/// defaults of version 1, the reference values of version 2.
pub fn unset(file: &SlipmeshFile) -> Vec<(&'static str, Value)> {
    match file {
        SlipmeshFile::V1(file) => missing_defaults(file.topology()),
        SlipmeshFile::V2(files) => files.missing_reference_values(),
    }
}

/// Writes each of `unset`, a key and its value, in the network document of `yaml`, the
/// `slipmesh.yaml` `file` was read from. Indexing inserts each missing step, and panics on none:
/// every step that is there is a mapping, or the file would not have parsed.
pub fn write_unset(
    yaml: &mut MultiDoc,
    file: &SlipmeshFile,
    unset: &[(&str, Value)],
) -> Result<()> {
    let network = document(yaml, network_document(file))?;
    for (key, value) in unset {
        *key.split('.')
            .fold(&mut *network, |value, step| &mut value[step]) = value.clone();
    }
    Ok(())
}

/// Adds `client` to the end of the `clients` of the pool at `document` in `yaml`, starting the
/// list when the pool has none.
pub fn add_client(yaml: &mut MultiDoc, document: usize, client: &RoadwarriorClient) -> Result<()> {
    let pool = self::document(yaml, document)?;
    let entry = client_entry(client);
    match pool.get_mut("clients") {
        Some(clients) => clients
            .as_sequence_mut()
            .context("the pool's `clients` is not a list")?
            .push(entry),
        None => set(pool, "clients", Value::Sequence(vec![entry]))?,
    }
    Ok(())
}

/// Removes `clients[client_index]` from the pool at `document` in `yaml`.
pub fn remove_client(yaml: &mut MultiDoc, document: usize, client_index: usize) -> Result<()> {
    let clients = self::document(yaml, document)?
        .get_mut("clients")
        .and_then(Value::as_sequence_mut)
        .context("pool has no clients list")?;
    anyhow::ensure!(
        client_index < clients.len(),
        "pool has no client at position {client_index}"
    );
    clients.remove(client_index);
    Ok(())
}

/// One client entry as `rw add` writes it: by its private key when that was kept, else by its
/// public key - never both.
fn client_entry(client: &RoadwarriorClient) -> Value {
    let mut entry = Mapping::new();
    entry.insert("name".into(), client.name.clone().into());
    match &client.private_key {
        Some(key) => entry.insert("private_key".into(), key.expose().into()),
        None => entry.insert("public_key".into(), client.public_key.clone().into()),
    };
    entry.insert(
        "allowed_ips".into(),
        Value::Sequence(
            client
                .allowed_ips
                .iter()
                .map(|ip| ip.clone().into())
                .collect(),
        ),
    );
    Value::Mapping(entry)
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
    use sliplib_config::v1::mesh_config::MeshConfig;
    use sliplib_mesh::mint;

    const NINE: [&str; 9] = ["jc", "jmin", "jmax", "s1", "s2", "h1", "h2", "h3", "h4"];

    const NETWORK: &str = "\
slipmesh:
    kind: network
cluster:
    bgp_as: 64512
    loopback_networks:
        ipv4: 198.51.100.0/25
        ipv6: 3fff::/32
nodes:
    - name: node-a
      node_id: 198.51.100.1
    - name: node-b
      node_id: 198.51.100.2
    - name: node-c
      node_id: 198.51.100.3
mesh:
    links:
        - pair:
            - node-b
            - node-a
          port: 51820
        - pair:
            - node-a
            - node-c
          port: 51821
          plain: true
";

    const POOLS: &str = "\
---
slipmesh:
    kind: roadwarriors
name: obfuscated
node_hostnames:
    - node-a
address: 203.0.113.1/27
listen_port: 51900
obfuscation:
    jc: 4
    jmin: 10
    jmax: 50
    s1: 1
    s2: 2
    h1: 5
    h2: 6
    h3: 7
    h4: 8
---
slipmesh:
    kind: roadwarriors
name: plain
node_hostnames:
    - node-b
address: 203.0.113.33/27
listen_port: 51901
plain: true
";

    fn slipmesh() -> String {
        format!("{NETWORK}{POOLS}")
    }

    /// One run: resolve, write down what was minted.
    fn run(raw: &str) -> String {
        let mut yaml = MultiDoc::parse(raw).unwrap();
        let file = SlipmeshFile::read(&yaml).unwrap();
        let network = sliplib_mesh::model::lower(&file).unwrap();
        let minted = mint::create(&network).unwrap();
        record(&mut yaml, &file, &network, &minted).unwrap();
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

    /// The example of version 2 with a node and a link whose names YAML reads as numbers: they
    /// stand under keys that are numbers.
    fn numbered() -> String {
        format!(
            "{}  7: {{platform: talos, node_id: 0.0.0.7, underlay: {{wan: {{}}}}}}\n",
            include_str!("../../docs/example/mesh.yaml").replace(
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
        let file = SlipmeshFile::read(&yaml).unwrap();
        set_node_key(&mut yaml, &file, "7", &PrivateKey::generate()).unwrap();
        let node = &yaml.documents()[0].value["nodes"][&Value::from(7)];
        assert!(node["wireguard"]["private_key"].is_string());
    }

    fn topology(raw: &str) -> MeshConfig {
        sliplib_config::v1::slipmesh_file::SlipmeshFile::parse(raw)
            .unwrap()
            .into_topology()
    }

    fn keys_of(obfuscation: &Obfuscation) -> Vec<String> {
        let value = sliplib_yaml::to_value(obfuscation).unwrap();
        value
            .as_mapping()
            .unwrap()
            .keys()
            .map(|k| k.as_str().unwrap().to_owned())
            .collect()
    }

    #[test]
    fn every_node_gets_its_key_written_into_its_entry() {
        for node in topology(&run(&slipmesh())).nodes {
            assert!(node.mesh_private_key.is_some(), "{}", node.name);
        }
    }

    #[test]
    fn a_link_gets_the_nine_generated_fields_nested_under_it() {
        let links = topology(&run(&slipmesh())).mesh.links;
        assert_eq!(keys_of(&links[0].obfuscation), NINE);
    }

    #[test]
    fn plain_links_and_pools_get_no_obfuscation() {
        let topology = topology(&run(&slipmesh()));
        assert_eq!(topology.mesh.links[1].obfuscation, Obfuscation::default());
        let plain = &topology.roadwarriors[1];
        assert!(plain.private_key.is_some());
        assert_eq!(plain.obfuscation, Obfuscation::default());
    }

    #[test]
    fn a_pool_with_all_nine_fields_written_gets_only_its_key() {
        let recorded = run(&slipmesh());
        let pool = &topology(&recorded).roadwarriors[0];
        assert!(pool.private_key.is_some());
        assert_eq!(pool.obfuscation.jc, Some(4));
        assert_eq!(recorded.matches("jc:").count(), 2, "{recorded}");
    }

    #[test]
    fn switches_that_are_never_generated_are_never_written() {
        let raw = slipmesh().replace(
            "cluster:\n",
            "obfuscation:\n    random_trailers: true\n    disable_cookies: true\ncluster:\n",
        );
        let links = topology(&run(&raw)).mesh.links;
        let written = keys_of(&links[0].obfuscation);
        assert_eq!(written, NINE, "the link's own fields only");
    }

    #[test]
    fn a_field_set_globally_is_not_written_down() {
        let raw = slipmesh().replace("cluster:\n", "obfuscation:\n    jc: 5\ncluster:\n");
        let recorded = run(&raw);
        let links = topology(&recorded).mesh.links;
        assert!(!keys_of(&links[0].obfuscation).contains(&"jc".to_owned()));
        assert_eq!(keys_of(&links[0].obfuscation).len(), 8);
    }

    #[test]
    fn a_field_set_on_the_link_stays_and_the_rest_join_it() {
        let raw = slipmesh().replace(
            "          port: 51820\n",
            "          port: 51820\n          obfuscation:\n            jc: 9\n",
        );
        let recorded = run(&raw);
        let obfuscation = &topology(&recorded).mesh.links[0].obfuscation;
        assert_eq!(obfuscation.jc, Some(9));
        assert_eq!(keys_of(obfuscation).len(), 9);
    }

    #[test]
    fn every_value_written_before_stays() {
        let raw = slipmesh();
        let (before, after) = (values(&raw), values(&run(&raw)));
        assert_eq!(before.len(), after.len());
        for (before, after) in before.iter().zip(&after) {
            assert!(within(before, after), "{before:?}\n{after:?}");
        }
    }

    /// A pool as its own `slipmesh.yaml` document.
    const PLAIN_POOL: &str = r#"slipmesh:
  kind: roadwarriors
name: plain
node_hostnames: ["a", "b"]
address: "203.0.113.1/27"
listen_port: 51820
plain: true
clients:
  - {name: alice, public_key: "AAA=", allowed_ips: ["203.0.113.21/32"]}
  - {name: bob, public_key: "BBB=", allowed_ips: ["203.0.113.12/32"]}
"#;

    fn client(name: &str) -> RoadwarriorClient {
        RoadwarriorClient {
            name: name.to_owned(),
            public_key: format!("{name}="),
            private_key: None,
            allowed_ips: vec!["203.0.113.29/32".to_owned()],
        }
    }

    /// `source` after `edit`, as the text it comes back as.
    fn edited(source: &str, edit: impl FnOnce(&mut MultiDoc)) -> String {
        let mut yaml = MultiDoc::parse(source).unwrap();
        edit(&mut yaml);
        yaml.text().unwrap()
    }

    /// The pool in `source` without its clients.
    fn without_clients(source: &str) -> Value {
        let mut pool = values(source).remove(0);
        pool.as_mapping_mut().unwrap().shift_remove("clients");
        pool
    }

    fn client_names(source: &str) -> Vec<String> {
        let value: sliplib_yaml::Value = sliplib_yaml::from_str(source).unwrap();
        value["clients"]
            .as_sequence()
            .unwrap()
            .iter()
            .map(|c| c["name"].as_str().unwrap().to_owned())
            .collect()
    }

    #[test]
    fn add_client_appends_and_leaves_the_rest_untouched() {
        let out = edited(PLAIN_POOL, |yaml| {
            add_client(yaml, 0, &client("dave")).unwrap()
        });
        assert_eq!(without_clients(&out), without_clients(PLAIN_POOL));
        let (before, after) = (&values(PLAIN_POOL)[0], &values(&out)[0]);
        for kept in 0..2 {
            assert_eq!(after["clients"][kept], before["clients"][kept]);
        }
        assert_eq!(client_names(&out), ["alice", "bob", "dave"]);
    }

    #[test]
    fn add_client_starts_the_list_a_pool_lacks() {
        let pool = PLAIN_POOL.split("clients:").next().unwrap();
        let out = edited(pool, |yaml| add_client(yaml, 0, &client("dave")).unwrap());
        assert_eq!(without_clients(&out), without_clients(pool));
        assert_eq!(client_names(&out), ["dave"]);
    }

    #[test]
    fn remove_client_deletes_exactly_the_target() {
        let out = edited(PLAIN_POOL, |yaml| {
            remove_client(yaml, 0, 0).unwrap() // alice
        });
        assert_eq!(without_clients(&out), without_clients(PLAIN_POOL));
        assert_eq!(
            values(&out)[0]["clients"][0],
            values(PLAIN_POOL)[0]["clients"][1]
        );
        assert_eq!(client_names(&out), ["bob"]);
    }
}
