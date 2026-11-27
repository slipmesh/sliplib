use crate::common::{Config, Ran, edited, minted};
use std::path::Path;

fn names_in(dir: &Path) -> Vec<std::ffi::OsString> {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect()
}

/// Each line naming every one of `named` in its place.
fn refusals<'a>(stderr: &'a str, named: &[&[&str]]) -> Vec<&'a str> {
    let refusals: Vec<&str> = stderr.lines().collect();
    assert_eq!(refusals.len(), named.len(), "{stderr}");
    for (refusal, parts) in refusals.iter().zip(named) {
        for part in *parts {
            assert!(refusal.contains(part), "{part:?} missing from: {refusal}");
        }
    }
    refusals
}

/// Which it leaves as it was, with nothing beside it.
fn checked(config: &Config) -> Ran {
    let before = config.text();
    let ran = config.run(&["check"]);
    assert_eq!(config.text(), before);
    assert_eq!(names_in(config.path.parent().unwrap()), ["network.yaml"]);
    ran
}

#[test]
fn files_with_a_refused_node_get_no_warning() {
    // A lossy link between two nodes that pass: `check` warns about it while nothing is refused.
    let config = minted();
    edited(&config, |network| {
        network["mesh"]["links"]["b-c"] = sliplib_yaml::from_str(
            "{type: amneziawg, ends: {node-b: {}, node-c: {}}, amneziawg: {port: 51833, \
             obfuscation: {jc: 4, jmin: 40, jmax: 400, s1: 64, s2: 48, h1: 100-199, h2: 2202, \
             h3: 3303, h4: 4404}}}",
        )
        .unwrap();
    });
    let passing = checked(&config);
    assert_eq!(passing.code, Some(0), "{}", passing.stderr);
    assert!(
        passing.stderr.contains("warning: mesh link b-c: "),
        "{}",
        passing.stderr
    );

    edited(&config, |network| {
        network["nodes"]["router1"]["wireguard"] = sliplib_yaml::Value::Null;
    });
    let refused = checked(&config);
    assert_eq!(refused.code, Some(1));
    assert_eq!(refused.stdout, "node-b: ok\nnode-c: ok\n");
    refusals(
        &refused.stderr,
        &[
            &["for node \"node-a\"", "router1", "wireguard.private_key"],
            &["for node \"router1\"", "router1", "wireguard.private_key"],
        ],
    );
}

#[test]
fn files_with_a_key_the_loader_does_not_know_are_one_refusal_naming_the_file() {
    let config = minted();
    edited(&config, |network| {
        network["mesh"]["no_such_setting"] = 1.into();
    });
    let checked = checked(&config);
    assert_eq!(checked.code, Some(1));
    assert_eq!(checked.stdout, "");
    refusals(
        &checked.stderr,
        &[&[&config.path.display().to_string(), "no_such_setting"]],
    );
    assert!(
        !checked.stderr.contains("slipmesh mint"),
        "{}",
        checked.stderr
    );
}
