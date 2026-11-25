use crate::common::{Config, Ran, edited, minted, slipmesh};
use std::path::Path;

fn check_at(path: &Path) -> Ran {
    slipmesh()
        .args(["check", "--in"])
        .arg(path)
        .output()
        .unwrap()
        .into()
}

fn names_in(dir: &Path) -> Vec<std::ffi::OsString> {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect()
}

/// And the file's path; the file left as it was, with nothing beside it.
fn check(content: &str) -> (Ran, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("network.yaml");
    std::fs::write(&path, content).unwrap();
    let checked = check_at(&path);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), content);
    assert_eq!(names_in(dir.path()), ["network.yaml"]);
    (checked, path.display().to_string())
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

#[test]
fn a_file_that_is_not_yaml_is_one_refusal_naming_the_file() {
    let (checked, path) = check("nodes: [unclosed\n");
    assert_eq!(checked.code, Some(1));
    assert_eq!(checked.stdout, "");
    refusals(&checked.stderr, &[&[&path]]);
}

#[test]
fn a_file_that_is_not_there_is_one_refusal_naming_the_path_and_is_not_created() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("absent.yaml");
    let checked = check_at(&path);
    assert_eq!(checked.code, Some(1));
    assert_eq!(checked.stdout, "");
    refusals(&checked.stderr, &[&[&path.display().to_string()]]);
    assert!(names_in(dir.path()).is_empty());
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
fn a_link_without_its_obfuscation_refuses_both_its_ends_naming_it_and_mint() {
    let config = minted();
    edited(&config, |network| {
        network["mesh"]["links"]["a-b"]["amneziawg"]["obfuscation"] = sliplib_yaml::Value::Null;
    });
    let checked = checked(&config);
    assert_eq!(checked.code, Some(1));
    assert_eq!(checked.stdout, "node-c: ok\nrouter1: ok\n");
    refusals(
        &checked.stderr,
        &[
            &[
                "for node \"node-a\"",
                "mesh link a-b: obfuscation lacks",
                "`slipmesh mint`",
            ],
            &[
                "for node \"node-b\"",
                "mesh link a-b: obfuscation lacks",
                "`slipmesh mint`",
            ],
        ],
    );
}

#[test]
fn a_key_that_does_not_decode_is_refused_on_its_owners_line_and_its_peers() {
    let config = minted();
    edited(&config, |network| {
        network["nodes"]["node-b"]["wireguard"]["private_key"] = "not a key".into();
    });
    let checked = checked(&config);
    assert_eq!(checked.code, Some(1));
    assert_eq!(checked.stdout, "node-c: ok\nrouter1: ok\n");
    let not_a_key = sliplib_secrets::wireguard::decode_key("not a key")
        .unwrap_err()
        .to_string();
    refusals(
        &checked.stderr,
        &[
            &[
                "for node \"node-a\"",
                "node-b",
                "wireguard.private_key",
                &not_a_key,
            ],
            &[
                "for node \"node-b\"",
                "node-b",
                "wireguard.private_key",
                &not_a_key,
            ],
        ],
    );
    assert!(
        !checked.stderr.contains("slipmesh mint"),
        "{}",
        checked.stderr
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
