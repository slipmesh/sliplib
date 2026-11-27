use crate::common::Config;
use sliplib_secrets::wireguard::PrivateKey;
use sliplib_yaml::{MultiDoc, Value};

fn values(text: &str) -> Vec<Value> {
    MultiDoc::parse(text).unwrap().values()
}

/// What tells a file that was replaced from one that was left alone.
#[cfg(unix)]
fn inode(config: &Config) -> u64 {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(&config.path).unwrap().ino()
}

/// As a hand leaves them, with what a rewrite could get wrong: a hexadecimal integer, a string
/// read as a number, a block scalar, an alias, documents of only comments and of only `~`.
fn by_hand(key: &str) -> String {
    format!(
        r#"# The network, written by hand.
slipmesh: {{kind: network, version: 2, revision: 1}}
addressing:
  loopback: {{ipv4: "198.51.100.0/25", ipv6: "3fff::/64"}}
routing:
  bgp: {{as: 64496}}
mesh:
  interface_prefix: mesh-
  links:
    a-b: {{type: amneziawg, ends: {{a: {{}}, b: {{}}}}, amneziawg: {{port: 51820}}}}
nodes:
  a:
    {{platform: talos, node_id: "0.0.0.1", underlay: {{wan: {{endpoint: {{ipv4: "192.0.2.10"}}}}}},
     wireguard: {{private_key: "{key}"}}}}
  b:
    {{platform: talos, node_id: "0.0.0.2", underlay: {{wan: {{endpoint: {{ipv4: "192.0.2.11"}}}}}}}}
---
# Nothing but this comment.
---
~
---
slipmesh:
  kind: patch
  platform: talos
  include: [a]
apiVersion: v1alpha1
kind: UnattendedInstallConfig
installer:
    disk: "/dev/vda"
    mtu: 0x5DC
    release: "0123"
    script: |
      first line
        second line, indented
    first: &shared {{wipe: true}}
    second: *shared
"#
    )
}

fn network_of(text: &str) -> sliplib_config::v2::network::Network {
    sliplib_config::v2::File::parse(text)
        .unwrap()
        .network()
        .clone()
}

#[test]
fn fmt_rewrites_hand_written_files_in_the_serializers_form_and_keeps_every_value() {
    let key = PrivateKey::generate();
    let by_hand = by_hand(key.expose());
    let config = Config::holding(&by_hand);
    let ran = config.run(&["fmt"]);
    assert_eq!(ran.code, Some(0), "{}", ran.stderr);
    assert_eq!(
        ran.stdout,
        format!(
            "rewrote {} in the form slipmesh writes\n",
            config.path.display()
        )
    );
    assert_eq!(ran.stderr, "");
    assert!(!ran.stdout.contains(key.expose()));

    let rewritten = config.text();
    assert_eq!(
        rewritten,
        MultiDoc::parse(&by_hand).unwrap().text().unwrap()
    );
    assert_eq!(values(&rewritten), values(&by_hand));
    assert_eq!(network_of(&rewritten), network_of(&by_hand));
}

#[cfg(unix)]
#[test]
fn fmt_leaves_files_already_in_that_form_untouched_and_says_so() {
    let config = Config::holding(&by_hand(PrivateKey::generate().expose()));
    config.run(&["fmt"]);
    let (rewritten, replaced) = (config.text(), inode(&config));

    let second = config.run(&["fmt"]);
    assert_eq!(second.code, Some(0), "{}", second.stderr);
    assert_eq!(config.text(), rewritten);
    assert_eq!(inode(&config), replaced);
    assert_eq!(
        second.stdout,
        format!(
            "{}: already in the form slipmesh writes\n",
            config.path.display()
        )
    );
}

#[test]
fn a_block_the_loader_refuses_stops_every_write_for_its_reason_and_names_no_key() {
    let key = PrivateKey::generate();
    let content = by_hand(key.expose()).replace(
        "slipmesh: {kind: network, version: 2, revision: 1}",
        "slipmesh: {kind: network, version: 2, revision: 1, no_such_setting: 1}",
    );
    assert!(content.contains("no_such_setting"));
    for command in [&["mint"][..], &["rekey", "--node", "node-a"], &["fmt"]] {
        let config = Config::holding(&content);
        let ran = config.run(command);
        assert_eq!(ran.code, Some(1), "{command:?}");
        assert_eq!(ran.stdout, "", "{command:?}");
        assert!(
            ran.stderr.contains("no_such_setting"),
            "{command:?}: {}",
            ran.stderr
        );
        assert!(
            ran.stderr.contains(&config.path.display().to_string()),
            "{command:?}: {}",
            ran.stderr
        );
        assert!(
            !ran.stderr.contains("would change a value"),
            "{command:?}: {}",
            ran.stderr
        );
        assert!(!ran.stderr.contains(key.expose()), "{command:?}");
        assert_eq!(config.text(), content, "{command:?}");
    }
}
