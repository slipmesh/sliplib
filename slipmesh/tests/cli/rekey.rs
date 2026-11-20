use crate::common::{Config, SLIPMESH, mint};

#[test]
fn rekey_gives_the_node_a_new_key_and_changes_no_other_value() {
    let config = Config::holding(SLIPMESH);
    mint(&config);
    let before = config.documents();

    let ran = config.run(&["rekey", "--node", "b"]);
    assert_eq!(ran.code, Some(0), "{}", ran.stderr);

    let mut after = config.documents();
    let key = after[0]["nodes"][1]["mesh_private_key"].clone();
    let old = &before[0]["nodes"][1]["mesh_private_key"];
    assert_ne!(&key, old);
    after[0]["nodes"][1]["mesh_private_key"] = old.clone();
    assert_eq!(after, before);

    assert!(
        ran.stdout.contains("nodes[b].mesh_private_key"),
        "{}",
        ran.stdout
    );
    assert!(
        !ran.stdout.contains(key.as_str().unwrap()),
        "{}",
        ran.stdout
    );
}

#[test]
fn rekey_gives_a_key_to_a_node_that_had_none_and_to_no_other() {
    let config = Config::holding(SLIPMESH);
    let ran = config.run(&["rekey", "--node", "c"]);
    assert_eq!(ran.code, Some(0), "{}", ran.stderr);
    let keyed: Vec<bool> = config.documents()[0]["nodes"]
        .as_sequence()
        .unwrap()
        .iter()
        .map(|node| node.get("mesh_private_key").is_some())
        .collect();
    assert_eq!(keyed, [false, false, true, false]);
}

#[test]
fn rekey_of_an_unknown_node_is_refused_naming_the_nodes_and_writes_nothing() {
    let config = Config::holding(SLIPMESH);
    mint(&config);
    let before = config.text();

    let ran = config.run(&["rekey", "--node", "e"]);
    assert_eq!(ran.code, Some(1));
    for node in ["\"e\"", "\"a\"", "\"b\"", "\"c\"", "\"d\""] {
        assert!(ran.stderr.contains(node), "{node}: {}", ran.stderr);
    }
    assert_eq!(config.text(), before);
}
