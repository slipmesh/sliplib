use crate::common::{Config, Ran, edited, minted, slipmesh};
use sliplib_secrets::wireguard::PrivateKey;
use std::path::Path;

/// What a link that is not `plain` carries once its secrets are written.
const OBFUSCATION: &str = "{jc: 4, jmin: 10, jmax: 50, s1: 15, s2: 16, h1: 5, h2: 6, h3: 7, h4: 8}";

/// Four nodes, `b` and `c` on the one link, each with a key of its own unless `keyless` names it.
fn network(cluster: &str, keyless: &[&str]) -> String {
    let nodes: String = ["a", "b", "c", "d"]
        .iter()
        .zip(1..)
        .map(|(name, id)| {
            let key = if keyless.contains(name) {
                String::new()
            } else {
                format!(", mesh_private_key: \"{}\"", PrivateKey::generate().expose())
            };
            format!(
                "  - {{name: {name}, node_id: \"198.51.100.{id}\", endpoint: \"192.0.2.{id}\"{key}}}\n"
            )
        })
        .collect();
    format!(
        r#"slipmesh:
  kind: network
cluster:
  bgp_as: 64512
  loopback_networks: {{ipv4: "198.51.100.0/25", ipv6: "3fff::/32"}}
  interface_prefix: {{mesh: mesh-}}
  route_proto_id: {{egress: 152}}
{cluster}nodes:
{nodes}mesh:
  links:
    - {{pair: [b, c], port: 51820, obfuscation: {OBFUSCATION}}}
"#
    )
}

fn check_at(path: &Path) -> Ran {
    slipmesh()
        .args(["check", "--config"])
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

/// What `check` gives for a file holding `content`, and the file's path. Whatever it gives, it
/// has left the file as it was and nothing beside it.
fn check(content: &str) -> (Ran, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("network.yaml");
    std::fs::write(&path, content).unwrap();
    let checked = check_at(&path);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), content);
    assert_eq!(names_in(dir.path()), ["network.yaml"]);
    (checked, path.display().to_string())
}

/// The lines of `stderr`, each of which has to name every one of `named` in its place.
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
fn a_file_whose_every_node_renders_passes_naming_each_node() {
    let (checked, _) = check(&network("", &[]));
    assert_eq!(checked.code, Some(0), "{}", checked.stderr);
    assert_eq!(checked.stdout, "a: ok\nb: ok\nc: ok\nd: ok\n");
    assert_eq!(checked.stderr, "");
}

#[test]
fn a_link_that_loses_packets_is_warned_about_and_the_file_still_passes() {
    let losing = network("", &[]).replace("h1: 5,", "h1: 100-199, random_trailers: true,");
    let (checked, _) = check(&losing);
    assert_eq!(checked.code, Some(0), "{}", checked.stderr);
    assert_eq!(checked.stdout, "a: ok\nb: ok\nc: ok\nd: ok\n");
    let warnings = refusals(
        &checked.stderr,
        &[
            &["102 packets in 4294967296"],
            &["s1 stands 15 bytes above s4"],
        ],
    );
    for warning in warnings {
        assert!(warning.starts_with("warning: mesh link b-c: "), "{warning}");
    }
}

// What a link loses is counted from settings that pass the checks; of a file that is refused it
// would be told of settings nobody runs.
#[test]
fn a_file_with_a_refused_node_gets_no_warning() {
    let losing = network("", &["a"]).replace("h1: 5,", "h1: 100-199, random_trailers: true,");
    let (checked, _) = check(&losing);
    assert_eq!(checked.code, Some(1));
    assert!(checked.stdout.contains("b: ok"), "{}", checked.stdout);
    refusals(&checked.stderr, &[&["node \"a\"", "mesh_private_key"]]);
}

#[test]
fn every_refused_node_is_reported_and_the_others_still_pass() {
    let (checked, _) = check(&network("", &["a", "d"]));
    assert_eq!(checked.code, Some(1));
    assert_eq!(checked.stdout, "b: ok\nc: ok\n");
    refusals(
        &checked.stderr,
        &[
            &["for node \"a\"", "node \"a\": mesh_private_key"],
            &["for node \"d\"", "node \"d\": mesh_private_key"],
        ],
    );
}

#[test]
fn a_link_without_its_obfuscation_refuses_both_its_ends_naming_the_link() {
    let content = network("", &[]).replace(&format!(", obfuscation: {OBFUSCATION}"), "");
    let (checked, _) = check(&content);
    assert_eq!(checked.code, Some(1));
    assert_eq!(checked.stdout, "a: ok\nd: ok\n");
    refusals(
        &checked.stderr,
        &[
            &["for node \"b\"", "mesh link b-c: obfuscation"],
            &["for node \"c\"", "mesh link b-c: obfuscation"],
        ],
    );
}

#[test]
fn a_key_that_does_not_decode_is_refused_on_its_owners_line_and_its_peers() {
    let content = network("", &["b"]).replace(
        "endpoint: \"192.0.2.2\"}",
        "endpoint: \"192.0.2.2\", mesh_private_key: \"not a key\"}",
    );
    let (checked, _) = check(&content);
    assert_eq!(checked.code, Some(1));
    assert_eq!(checked.stdout, "a: ok\nd: ok\n");
    let not_a_key = "node \"b\": mesh_private_key: key is not valid base64";
    refusals(
        &checked.stderr,
        &[
            &["for node \"b\"", not_a_key],
            &["for node \"c\"", not_a_key],
        ],
    );
    assert!(
        !checked.stderr.contains("slipmesh mint"),
        "{}",
        checked.stderr
    );
}

#[test]
fn a_file_with_a_setting_the_loader_does_not_know_is_one_refusal_naming_the_file() {
    let (checked, path) = check(&network("  no_such_setting: 1\n", &[]));
    assert_eq!(checked.code, Some(1));
    assert_eq!(checked.stdout, "");
    refusals(&checked.stderr, &[&[&path, "no_such_setting"]]);
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

#[test]
fn the_file_is_slipmesh_yaml_of_the_working_directory_unless_given() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        Path::new(dir.path()).join("slipmesh.yaml"),
        network("", &[]),
    )
    .unwrap();
    let checked: Ran = slipmesh()
        .arg("check")
        .current_dir(dir.path())
        .output()
        .unwrap()
        .into();
    assert_eq!(checked.code, Some(0), "{}", checked.stderr);
    assert_eq!(checked.stdout, "a: ok\nb: ok\nc: ok\nd: ok\n");
}

#[test]
fn a_file_without_the_network_defaults_is_refused_naming_them_and_its_nodes_are_still_checked() {
    let content = network("", &[])
        .replace("  interface_prefix: {mesh: mesh-}\n", "")
        .replace("  route_proto_id: {egress: 152}\n", "");
    let (checked, path) = check(&content);
    assert_eq!(checked.code, Some(1));
    assert_eq!(checked.stdout, "a: ok\nb: ok\nc: ok\nd: ok\n");
    let refusals: Vec<&str> = checked.stderr.lines().collect();
    assert_eq!(refusals.len(), 1, "{}", checked.stderr);
    for named in [
        path.as_str(),
        "cluster.interface_prefix.mesh",
        "cluster.route_proto_id.egress",
        "slipmesh mint",
    ] {
        assert!(refusals[0].contains(named), "{named}: {}", refusals[0]);
    }
}

#[test]
fn a_file_without_one_network_default_is_refused_naming_only_that_one() {
    let content = network("", &[]).replace("  route_proto_id: {egress: 152}\n", "");
    let (checked, _) = check(&content);
    assert_eq!(checked.code, Some(1));
    assert!(checked.stderr.contains("cluster.route_proto_id.egress"));
    assert!(!checked.stderr.contains("cluster.interface_prefix.mesh"));
}

/// A pool on node `a`, with `settings` after its required ones.
fn pool(settings: &str) -> String {
    format!(
        "---\nslipmesh:\n  kind: roadwarriors\nname: north\nnode_hostnames: [a]\naddress: \"203.0.113.1/27\"\nlisten_port: 51900\n{settings}"
    )
}

#[test]
fn a_refusal_for_a_secret_the_file_lacks_names_it_and_the_command_that_creates_it() {
    let keyed = format!("private_key: \"{}\"\n", PrivateKey::generate().expose());
    let link = "mesh link b-c: obfuscation lacks";
    let cases: [(&str, String, &[&str]); 5] = [
        (
            "the node's own key",
            network("", &["a"]),
            &["node \"a\": mesh_private_key is not set"],
        ),
        (
            "a peer's key",
            network("", &["c"]),
            &[
                "node \"c\": mesh_private_key is not set",
                "node \"c\": mesh_private_key is not set",
            ],
        ),
        (
            "a link's obfuscation",
            network("", &[]).replace(&format!(", obfuscation: {OBFUSCATION}"), ""),
            &[link, link],
        ),
        (
            "a pool's key",
            network("", &[]) + &pool("plain: true\n"),
            &["roadwarriors pool \"north\": private_key is not set"],
        ),
        (
            "a pool's obfuscation",
            network("", &[]) + &pool(&keyed),
            &["roadwarriors pool \"north\": obfuscation lacks"],
        ),
    ];
    for (lacking, content, missing) in cases {
        let (checked, _) = check(&content);
        assert_eq!(checked.code, Some(1), "{lacking}");
        let named: Vec<[&str; 2]> = missing.iter().map(|m| [*m, "`slipmesh mint`"]).collect();
        let named: Vec<&[&str]> = named.iter().map(|n| &n[..]).collect();
        refusals(&checked.stderr, &named);
    }
}

#[test]
fn a_refusal_that_no_secret_cures_names_no_command() {
    // A port of 0 refuses the file, before any node.
    let (checked, _) = check(&network("  awg_metrics_port: 0\n", &[]));
    assert_eq!(checked.code, Some(1));
    assert_eq!(checked.stderr.lines().count(), 1, "{}", checked.stderr);
    assert!(!checked.stderr.contains("slipmesh"), "{}", checked.stderr);
}

/// `check` on `config`, which it has to leave as it was and nothing beside it.
fn checked(config: &Config) -> Ran {
    let before = config.text();
    let ran = config.run(&["check"]);
    assert_eq!(config.text(), before);
    assert_eq!(names_in(config.path.parent().unwrap()), ["network.yaml"]);
    ran
}

#[test]
fn files_of_version_2_with_a_refused_node_get_no_warning() {
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
fn a_link_of_version_2_without_its_obfuscation_refuses_both_its_ends_naming_it_and_mint() {
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
fn a_key_of_version_2_that_does_not_decode_is_refused_on_its_owners_line_and_its_peers() {
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
fn files_of_version_2_with_a_key_the_loader_does_not_know_are_one_refusal_naming_the_file() {
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

#[test]
fn the_file_of_version_2_is_slipmesh_yaml_of_the_working_directory_unless_given() {
    let config = minted();
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("slipmesh.yaml"), config.text()).unwrap();
    let checked: Ran = slipmesh()
        .arg("check")
        .current_dir(dir.path())
        .output()
        .unwrap()
        .into();
    assert_eq!(checked.code, Some(0), "{}", checked.stderr);
    assert_eq!(
        checked.stdout,
        "node-a: ok\nnode-b: ok\nnode-c: ok\nrouter1: ok\n"
    );
}
