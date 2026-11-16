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

/// A pool on node `a` named `name`, with `settings` after its required ones and one client.
fn pool(name: &str, port: u16, settings: &str) -> String {
    format!(
        "---\nslipmesh:\n  kind: roadwarriors\nname: {name}\nnode_hostnames: [a]\naddress: \"203.0.113.65/27\"\nlisten_port: {port}\n{settings}clients:\n  - {{name: carol, public_key: \"CCC=\", allowed_ips: [\"203.0.113.71/32\"]}}\n"
    )
}

fn keyed() -> String {
    format!("private_key: \"{}\"\n", PrivateKey::generate().expose())
}

/// A file whose pool `lone` lacks one of its secrets, by what it lacks.
fn lacking_a_pool_secret() -> [(&'static str, Config); 2] {
    [
        ("its key", "plain: true\n".to_owned()),
        ("its obfuscation", keyed()),
    ]
    .map(|(lacking, settings)| {
        let lone = pool("lone", 51902, &settings);
        (lacking, Config::holding(&format!("{SLIPMESH}{lone}")))
    })
}

/// Asserts that `ran` refused pool `lone` for its secrets, naming the command that writes them,
/// and printed nothing else.
fn refused_for_the_pools_secrets(ran: &crate::common::Ran, lacking: &str) {
    assert_eq!(ran.code, Some(1), "{lacking}");
    assert!(
        ran.stderr.contains("\"lone\"") && ran.stderr.contains("`slipmesh mint`"),
        "{lacking}: {}",
        ran.stderr
    );
    assert_eq!(ran.stdout, "", "{lacking}");
}

#[test]
fn rw_add_to_a_pool_without_its_key_or_obfuscation_is_refused_and_writes_nothing() {
    for (lacking, config) in lacking_a_pool_secret() {
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
            refused_for_the_pools_secrets(&ran, lacking);
            assert_eq!(config.text(), before, "{lacking}");
        }
    }
}

#[test]
fn rw_inspect_of_a_pool_without_its_key_or_obfuscation_is_refused() {
    for (lacking, config) in lacking_a_pool_secret() {
        let before = config.text();
        let ran = config.run(&["rw", "inspect", "--if", "lone", "--name", "carol"]);
        refused_for_the_pools_secrets(&ran, lacking);
        assert_eq!(config.text(), before, "{lacking}");
    }
}

#[test]
fn rw_add_and_rw_inspect_need_no_secret_but_their_own_pools() {
    // No node has a key, and pool `lone` has neither its key nor its obfuscation.
    let ready = pool("ready", 51903, &format!("plain: true\n{}", keyed()));
    let lone = pool("lone", 51902, "");
    let config = Config::holding(&format!("{SLIPMESH}{ready}{lone}"));

    let ran = config.run(&["rw", "inspect", "--if", "ready", "--name", "carol"]);
    assert_eq!(ran.code, Some(0), "{}", ran.stderr);

    let ran = config.run(&[
        "rw",
        "add",
        "--if",
        "ready",
        "--name",
        "dave",
        "--allowed-ips",
        "203.0.113.70",
        "--export",
    ]);
    assert_eq!(ran.code, Some(0), "{}", ran.stderr);
    let (documents, ready) = documents_and_pool(&config, "ready");
    assert_eq!(client_names(&documents[ready]), ["carol", "dave"]);
}

/// The file with a link `mint` cannot complete: its one written header leaves no value for the
/// three it lacks.
fn with_a_link_that_has_no_room_for_a_header(rest: &str) -> Config {
    let network = SLIPMESH.replace(
        "{pair: [a, b], port: 51820}",
        "{pair: [a, b], port: 51820, obfuscation: {h1: 5-2147483647}}",
    );
    assert_ne!(network, SLIPMESH);
    Config::holding(&format!("{network}{rest}"))
}

#[test]
fn rw_add_and_rw_inspect_are_not_held_up_by_an_entry_mint_cannot_complete() {
    let ready = pool("ready", 51903, &format!("plain: true\n{}", keyed()));
    let config = with_a_link_that_has_no_room_for_a_header(&ready);

    let ran = config.run(&["rw", "inspect", "--if", "ready", "--name", "carol"]);
    assert_eq!(ran.code, Some(0), "{}", ran.stderr);

    let ran = config.run(&[
        "rw",
        "add",
        "--if",
        "ready",
        "--name",
        "dave",
        "--allowed-ips",
        "203.0.113.70",
        "--export",
    ]);
    assert_eq!(ran.code, Some(0), "{}", ran.stderr);
}

/// `pool` without its `clients`.
fn without_clients(pool: &Value) -> Value {
    let mut pool = pool.clone();
    pool.as_mapping_mut().unwrap().shift_remove("clients");
    pool
}

#[test]
fn rw_add_writes_the_clients_public_key_and_nothing_else() {
    let config = pools();
    mint(&config);
    let (before, first) = documents_and_pool(&config, "first");

    let ran = add_dave(&config, &["--export"]);
    assert_eq!(ran.code, Some(0), "{}", ran.stderr);

    let (after, _) = documents_and_pool(&config, "first");
    only_edited(&before, &after, first);
    assert_eq!(
        without_clients(&after[first]),
        without_clients(&before[first])
    );
    let clients = after[first]["clients"].as_sequence().unwrap();
    assert_eq!(clients.len(), 1);
    assert_eq!(clients[0]["name"], "dave");
    assert!(clients[0]["public_key"].is_string(), "{:?}", clients[0]);
    assert!(clients[0].get("private_key").is_none(), "{:?}", clients[0]);
}

/// The value of the `PrivateKey` line of a client config.
fn private_key_in(config_text: &str) -> &str {
    config_text
        .lines()
        .find_map(|line| line.strip_prefix("PrivateKey = "))
        .unwrap()
}

#[test]
fn rw_inspect_fills_in_the_clients_own_private_key_and_refuses_another() {
    let config = pools();
    mint(&config);
    let added = add_dave(&config, &["--export"]);
    assert_eq!(added.code, Some(0), "{}", added.stderr);
    let own = private_key_in(&added.stdout).to_owned();
    let inspect = |key: &str| {
        config.run(&[
            "rw",
            "inspect",
            "--if",
            "first",
            "--name",
            "dave",
            "--private-key",
            key,
        ])
    };

    let ran = inspect(&own);
    assert_eq!(ran.code, Some(0), "{}", ran.stderr);
    assert_eq!(private_key_in(&ran.stdout), own);

    let ran = inspect(PrivateKey::generate().expose());
    assert_eq!(ran.code, Some(1));
    assert_eq!(ran.stdout, "");
    assert!(ran.stderr.contains("--private-key"), "{}", ran.stderr);
}

#[test]
fn rw_add_keeping_the_private_key_lets_rw_inspect_render_the_config_in_full() {
    let config = pools();
    mint(&config);
    let ran = add_dave(&config, &["--keep-private"]);
    assert_eq!(ran.code, Some(0), "{}", ran.stderr);

    let (documents, first) = documents_and_pool(&config, "first");
    let entry = &documents[first]["clients"][0];
    assert!(entry["private_key"].is_string(), "{entry:?}");
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
