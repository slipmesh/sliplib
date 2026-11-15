use sliplib_secrets::wireguard::PrivateKey;
use std::path::Path;
use std::process::{Command, Output};

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
{cluster}nodes:
{nodes}mesh:
  links:
    - {{pair: [b, c], port: 51820, obfuscation: {OBFUSCATION}}}
"#
    )
}

struct Checked {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

impl From<Output> for Checked {
    fn from(output: Output) -> Self {
        Self {
            code: output.status.code(),
            stdout: String::from_utf8(output.stdout).unwrap(),
            stderr: String::from_utf8(output.stderr).unwrap(),
        }
    }
}

fn slipmesh() -> Command {
    Command::new(env!("CARGO_BIN_EXE_slipmesh"))
}

/// `slipmesh check` of a file holding `content`, the path given as the last argument.
fn check(content: &str) -> (Checked, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("network.yaml");
    std::fs::write(&path, content).unwrap();
    let output = slipmesh()
        .args(["check", "--config"])
        .arg(&path)
        .output()
        .unwrap();
    (output.into(), path.display().to_string())
}

#[test]
fn a_file_whose_every_node_renders_passes_naming_each_node() {
    let (checked, _) = check(&network("", &[]));
    assert_eq!(checked.code, Some(0), "{}", checked.stderr);
    assert_eq!(checked.stdout, "a: ok\nb: ok\nc: ok\nd: ok\n");
    assert_eq!(checked.stderr, "");
}

#[test]
fn every_refused_node_is_reported_and_the_others_still_pass() {
    let (checked, _) = check(&network("", &["a", "d"]));
    assert_eq!(checked.code, Some(1));
    assert_eq!(checked.stdout, "b: ok\nc: ok\n");
    let refusals: Vec<&str> = checked.stderr.lines().collect();
    assert_eq!(refusals.len(), 2, "{}", checked.stderr);
    for (refusal, node) in refusals.iter().zip(["\"a\"", "\"d\""]) {
        assert!(refusal.contains(node), "{refusal}");
        assert!(refusal.contains("mesh_private_key"), "{refusal}");
    }
}

#[test]
fn a_file_that_does_not_parse_is_one_refusal_naming_the_file() {
    let (checked, path) = check(&network("  no_such_setting: 1\n", &[]));
    assert_eq!(checked.code, Some(1));
    assert_eq!(checked.stdout, "");
    let refusals: Vec<&str> = checked.stderr.lines().collect();
    assert_eq!(refusals.len(), 1, "{}", checked.stderr);
    assert!(refusals[0].contains(&path), "{}", refusals[0]);
    assert!(refusals[0].contains("no_such_setting"), "{}", refusals[0]);
}

#[test]
fn the_file_is_slipmesh_yaml_of_the_working_directory_unless_given() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        Path::new(dir.path()).join("slipmesh.yaml"),
        network("", &[]),
    )
    .unwrap();
    let checked: Checked = slipmesh()
        .arg("check")
        .current_dir(dir.path())
        .output()
        .unwrap()
        .into();
    assert_eq!(checked.code, Some(0), "{}", checked.stderr);
    assert_eq!(checked.stdout, "a: ok\nb: ok\nc: ok\nd: ok\n");
}
