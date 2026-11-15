use crate::common::Config;
use sliplib_config::slipmesh_file::SlipmeshFile;
use sliplib_yaml::{MultiDoc, Value};

/// A file as a hand leaves it: flow collections, quotes, comments, a document of only comments.
const BY_HAND: &str = r#"# The network, written by hand.
slipmesh: {kind: network}
cluster:
  bgp_as: 64512   # a private one
  loopback_networks: {ipv4: "198.51.100.0/25", ipv6: "3fff::/32"}
nodes:
  - {name: a, node_id: "198.51.100.1", endpoint: "192.0.2.10"}
  - {name: b, node_id: "198.51.100.2", endpoint: "192.0.2.11"}
mesh:
  links:
    - {pair: [a, b], port: 51820}
---
# Nothing but this comment.
---
slipmesh:
  kind: patch
  include: [a]
apiVersion: v1alpha1
kind: UnattendedInstallConfig
installer:
    disk: "/dev/vda"
"#;

fn values(text: &str) -> Vec<Value> {
    MultiDoc::parse(text)
        .unwrap()
        .documents()
        .iter()
        .map(|document| document.value.clone())
        .collect()
}

/// What tells a file that was replaced from one that was left alone.
#[cfg(unix)]
fn inode(config: &Config) -> u64 {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(&config.path).unwrap().ino()
}

#[test]
fn fmt_rewrites_a_hand_written_file_and_keeps_every_value() {
    let config = Config::holding(BY_HAND);
    let ran = config.run(&["fmt"]);
    assert_eq!(ran.code, Some(0), "{}", ran.stderr);
    assert_eq!(ran.stdout.lines().count(), 1, "{}", ran.stdout);

    let rewritten = config.text();
    assert_ne!(rewritten, BY_HAND);
    assert_eq!(values(&rewritten), values(BY_HAND));
    assert_eq!(
        SlipmeshFile::parse(&rewritten).unwrap().into_topology(),
        SlipmeshFile::parse(BY_HAND).unwrap().into_topology()
    );
}

#[cfg(unix)]
#[test]
fn fmt_leaves_a_file_already_in_that_form_untouched_and_says_so() {
    let config = Config::holding(BY_HAND);
    let first = config.run(&["fmt"]);
    let (rewritten, replaced) = (config.text(), inode(&config));

    let second = config.run(&["fmt"]);
    assert_eq!(second.code, Some(0), "{}", second.stderr);
    assert_eq!(config.text(), rewritten);
    assert_eq!(inode(&config), replaced);
    assert_eq!(second.stdout.lines().count(), 1, "{}", second.stdout);
    assert_ne!(second.stdout, first.stdout);
}

#[test]
fn fmt_refuses_a_file_that_does_not_read_and_leaves_it_as_it_is() {
    let unknown_setting =
        BY_HAND.replace("  bgp_as: 64512", "  no_such_setting: 1\n  bgp_as: 64512");
    for content in [unknown_setting.as_str(), "nodes: [unclosed\n"] {
        let config = Config::holding(content);
        let ran = config.run(&["fmt"]);
        assert_eq!(ran.code, Some(1), "{content}");
        assert_eq!(ran.stdout, "", "{content}");
        assert!(
            ran.stderr.contains(&config.path.display().to_string()),
            "{}",
            ran.stderr
        );
        assert_eq!(config.text(), content);
    }
}
