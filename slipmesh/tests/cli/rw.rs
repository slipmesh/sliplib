use crate::common::{Config, SLIPMESH, mint, topology};
use sliplib_secrets::wireguard::PrivateKey;
use sliplib_yaml::{MultiDoc, Value};

const POOLS: &str = r#"---
slipmesh:
  kind: roadwarriors
name: first
node_hostnames: [a]
address: "203.0.113.1/27"
listen_port: 51900
clients: []
---
slipmesh:
  kind: roadwarriors
name: second
node_hostnames: [b]
address: "203.0.113.33/27"
listen_port: 51901
clients:
  - {name: carol, public_key: "CCC=", allowed_ips: ["203.0.113.42/32"]}
"#;

fn pools() -> Config {
    Config::holding(&format!("{SLIPMESH}{POOLS}"))
}

/// The values of the file's documents, and the position among them of pool `name`.
fn documents_and_pool(config: &Config, name: &str) -> (Vec<Value>, usize) {
    let documents: Vec<_> = MultiDoc::parse(&config.text())
        .unwrap()
        .documents()
        .iter()
        .map(|d| d.value.clone())
        .collect();
    let pool = documents
        .iter()
        .position(|d| d["slipmesh"]["kind"] == "roadwarriors" && d["name"] == name)
        .unwrap();
    (documents, pool)
}

/// Asserts that every document but `edited` holds the same values in `after` as in `before`.
fn only_edited(before: &[Value], after: &[Value], edited: usize) {
    assert_eq!(after.len(), before.len());
    for (index, (before, after)) in before.iter().zip(after).enumerate() {
        if index != edited {
            assert_eq!(after, before, "document {index}");
        }
    }
}

fn client_names(pool: &Value) -> Vec<&str> {
    pool["clients"]
        .as_sequence()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect()
}

const ADD_DAVE: [&str; 8] = [
    "rw",
    "add",
    "--if",
    "first",
    "--name",
    "dave",
    "--allowed-ips",
    "203.0.113.29",
];

fn add_dave(config: &Config, more: &[&str]) -> crate::common::Ran {
    config.run(&[&ADD_DAVE[..], more].concat())
}

#[test]
fn rw_add_edits_only_the_pools_own_document() {
    let config = pools();
    // Minted first, so that the only change the client makes is its own.
    mint(&config);
    let (before, first) = documents_and_pool(&config, "first");

    let ran = add_dave(&config, &["--public-key", "DDD="]);
    assert_eq!(ran.code, Some(0), "{}", ran.stderr);

    let (after, _) = documents_and_pool(&config, "first");
    only_edited(&before, &after, first);
    assert_eq!(client_names(&after[first]), ["dave"]);
}

#[test]
fn rw_del_edits_only_the_pools_own_document() {
    let config = pools();
    let (before, second) = documents_and_pool(&config, "second");

    let ran = config.run(&["rw", "del", "--if", "second", "--name", "carol"]);
    assert_eq!(ran.code, Some(0), "{}", ran.stderr);

    let (after, _) = documents_and_pool(&config, "second");
    only_edited(&before, &after, second);
    assert!(client_names(&after[second]).is_empty());
}

#[test]
fn rw_add_to_an_unknown_pool_names_the_pools_there_are() {
    let config = pools();
    let ran = config.run(&[
        "rw",
        "add",
        "--if",
        "third",
        "--name",
        "dave",
        "--allowed-ips",
        "203.0.113.29",
        "--public-key",
        "DDD=",
    ]);
    assert_eq!(ran.code, Some(1));
    assert!(
        ran.stderr.contains("first") && ran.stderr.contains("second"),
        "{}",
        ran.stderr
    );
}

/// A pool on node `a` named `lone`, with `settings` after its required ones.
fn lone_pool(settings: &str) -> String {
    format!(
        "---\nslipmesh:\n  kind: roadwarriors\nname: lone\nnode_hostnames: [a]\naddress: \"203.0.113.65/27\"\nlisten_port: 51902\n{settings}"
    )
}

#[test]
fn rw_add_to_a_pool_without_its_key_or_obfuscation_is_refused_and_writes_nothing() {
    let keyed = format!("private_key: \"{}\"\n", PrivateKey::generate().expose());
    for (lacking, settings) in [("its key", "plain: true\n"), ("its obfuscation", &keyed)] {
        let config = Config::holding(&format!("{SLIPMESH}{}", lone_pool(settings)));
        let before = config.text();
        for how in [&["--export"][..], &["--public-key", "DDD="][..]] {
            let ran = config.run(
                &[
                    &["rw", "add", "--if", "lone", "--name", "dave"][..],
                    &["--allowed-ips", "203.0.113.70"][..],
                    how,
                ]
                .concat(),
            );
            assert_eq!(ran.code, Some(1), "{lacking}");
            assert!(
                ran.stderr.contains("\"lone\"") && ran.stderr.contains("`slipmesh mint`"),
                "{lacking}: {}",
                ran.stderr
            );
            assert_eq!(ran.stdout, "", "{lacking}");
            assert_eq!(config.text(), before, "{lacking}");
        }
    }
}

#[test]
fn rw_add_creates_nothing_but_the_clients_own_key() {
    let config = pools();
    mint(&config);
    let mut unkeyed = MultiDoc::parse(&config.text()).unwrap();
    unkeyed.document_mut(0).unwrap()["nodes"]
        .as_sequence_mut()
        .unwrap()
        .push(sliplib_yaml::from_str("{name: e, node_id: \"198.51.100.5\"}").unwrap());
    std::fs::write(&config.path, unkeyed.text().unwrap()).unwrap();
    let (before, first) = documents_and_pool(&config, "first");

    let ran = add_dave(&config, &["--export"]);
    assert_eq!(ran.code, Some(0), "{}", ran.stderr);

    let (after, _) = documents_and_pool(&config, "first");
    only_edited(&before, &after, first);
    assert_eq!(client_names(&after[first]), ["dave"]);
    assert!(!ran.stdout.contains("minted"), "{}", ran.stdout);
}

#[test]
fn rw_add_keeping_the_private_key_lets_rw_inspect_render_the_config_in_full() {
    let config = pools();
    mint(&config);
    let ran = add_dave(&config, &["--keep-private"]);
    assert_eq!(ran.code, Some(0), "{}", ran.stderr);

    let (documents, first) = documents_and_pool(&config, "first");
    let entry = &documents[first]["clients"][0];
    assert!(entry.get("public_key").is_none(), "{entry:?}");
    let topology = topology(&config);
    let dave = &topology.roadwarriors[0].clients[0];
    let private = dave.private_key.as_ref().unwrap();
    assert_eq!(private.public_key().unwrap(), dave.public_key);

    let ran = config.run(&["rw", "inspect", "--if", "first", "--name", "dave"]);
    assert_eq!(ran.code, Some(0), "{}", ran.stderr);
    assert!(
        ran.stdout
            .contains(&format!("PrivateKey = {}", private.expose())),
        "{}",
        ran.stdout
    );
}

#[test]
fn rw_inspect_needs_the_pool_key_to_be_written_down_already() {
    let config = pools();
    let inspect = || config.run(&["rw", "inspect", "--if", "second", "--name", "carol"]);
    let before = config.text();
    let ran = inspect();
    assert_eq!(ran.code, Some(1));
    assert!(ran.stderr.contains("`slipmesh mint`"), "{}", ran.stderr);
    assert_eq!(config.text(), before);

    mint(&config);
    assert_eq!(inspect().code, Some(0));
}

#[test]
fn rw_inspect_needs_no_secret_but_its_pools() {
    let config = pools();
    mint(&config);
    let mut unkeyed = MultiDoc::parse(&config.text()).unwrap();
    unkeyed.document_mut(0).unwrap()["nodes"]
        .as_sequence_mut()
        .unwrap()
        .push(sliplib_yaml::from_str("{name: e, node_id: \"198.51.100.5\"}").unwrap());
    std::fs::write(&config.path, unkeyed.text().unwrap()).unwrap();
    let ran = config.run(&["rw", "inspect", "--if", "second", "--name", "carol"]);
    assert_eq!(ran.code, Some(0), "{}", ran.stderr);
}
