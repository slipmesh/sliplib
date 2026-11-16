use crate::common::{Config, topology};
use sliplib_config::Range;
use sliplib_config::slipmesh_file::SlipmeshFile;
use sliplib_secrets::wireguard::PrivateKey;
use sliplib_yaml::{MultiDoc, Value};

/// A file as a hand leaves it, with node `a`'s key: flow collections, quotes, comments, and the
/// shapes a rewrite could get wrong - a hexadecimal integer, a string that reads as a number, a
/// block scalar, an alias, a document of only comments and one of only `~`.
fn by_hand(key: &str) -> String {
    format!(
        r#"# The network, written by hand.
slipmesh: {{kind: network}}
cluster:
  bgp_as: 64512   # a private one
  loopback_networks: {{ipv4: "198.51.100.0/25", ipv6: "3fff::/32"}}
nodes:
  - {{name: a, node_id: "198.51.100.1", endpoint: "192.0.2.10", mesh_private_key: "{key}"}}
  - {{name: b, node_id: "198.51.100.2", endpoint: "192.0.2.11"}}
mesh:
  links:
    - {{pair: [a, b], port: 51820}}
---
# Nothing but this comment.
---
~
---
slipmesh:
  kind: patch
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
fn fmt_rewrites_a_hand_written_file_in_the_serializers_form_and_keeps_every_value() {
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

    let rewritten = config.text();
    assert_eq!(
        rewritten,
        MultiDoc::parse(&by_hand).unwrap().text().unwrap()
    );
    assert_eq!(values(&rewritten), values(&by_hand));
    assert_eq!(
        SlipmeshFile::parse(&rewritten).unwrap().into_topology(),
        SlipmeshFile::parse(&by_hand).unwrap().into_topology()
    );
}

#[test]
fn fmt_keeps_a_number_a_string_and_a_block_scalar_what_they_were() {
    let config = Config::holding(&by_hand(PrivateKey::generate().expose()));
    assert_eq!(config.run(&["fmt"]).code, Some(0));
    let installer = &values(&config.text())[1]["installer"];
    assert_eq!(installer["mtu"], Value::from(1500));
    assert_eq!(installer["release"], Value::from("0123"));
    assert_eq!(
        installer["script"],
        Value::from("first line\n  second line, indented\n")
    );
}

#[test]
fn fmt_keeps_a_range_a_range_and_a_number_a_number() {
    let by_hand = by_hand(PrivateKey::generate().expose()).replace(
        "{pair: [a, b], port: 51820}",
        "{pair: [a, b], port: 51820, obfuscation: {h1: 5-9, h2: 0xA, rekey_after_time: 100-140}}",
    );
    let config = Config::holding(&by_hand);
    assert_eq!(config.run(&["fmt"]).code, Some(0));

    let obfuscation = &values(&config.text())[0]["mesh"]["links"][0]["obfuscation"];
    assert_eq!(obfuscation["h1"], Value::from("5-9"));
    assert_eq!(obfuscation["h2"], Value::from(10));
    assert_eq!(obfuscation["rekey_after_time"], Value::from("100-140"));

    let read = topology(&config).mesh.links[0].obfuscation.clone();
    assert_eq!(read.h1, Some(Range::new(5, 9).unwrap()));
    assert_eq!(read.h2, Some(10.into()));
    assert_eq!(read.rekey_after_time, Some(Range::new(100, 140).unwrap()));
}

#[test]
fn fmt_writes_an_alias_out_as_a_copy_and_drops_a_document_that_holds_nothing() {
    let by_hand = by_hand(PrivateKey::generate().expose());
    let config = Config::holding(&by_hand);
    assert_eq!(config.run(&["fmt"]).code, Some(0));
    let rewritten = config.text();

    assert!(
        !rewritten.contains("&shared") && !rewritten.contains("*shared"),
        "{rewritten}"
    );
    let installer = &values(&rewritten)[1]["installer"];
    assert_eq!(installer["first"], installer["second"]);
    assert_eq!(installer["second"]["wipe"], Value::from(true));

    assert_eq!(by_hand.matches("\n---\n").count(), 3);
    assert_eq!(rewritten.matches("\n---\n").count(), 1, "{rewritten}");
}

#[cfg(unix)]
#[test]
fn fmt_leaves_a_file_already_in_that_form_untouched_and_says_so() {
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
fn fmt_refuses_a_file_that_does_not_read_for_its_own_reason_and_leaves_it_as_it_is() {
    let key = PrivateKey::generate();
    let unknown_setting =
        by_hand(key.expose()).replace("  bgp_as: 64512", "  no_such_setting: 1\n  bgp_as: 64512");
    let not_yaml = format!("nodes: [unclosed, \"{}\"\n", key.expose());
    for (content, reason) in [
        (unknown_setting, "no_such_setting"),
        (not_yaml, "not valid YAML"),
    ] {
        let config = Config::holding(&content);
        let ran = config.run(&["fmt"]);
        assert_eq!(ran.code, Some(1), "{reason}");
        assert_eq!(ran.stdout, "", "{reason}");
        assert!(ran.stderr.contains(reason), "{reason}: {}", ran.stderr);
        assert!(
            ran.stderr.contains(&config.path.display().to_string()),
            "{}",
            ran.stderr
        );
        assert!(
            !ran.stderr.contains("would change a value"),
            "{}",
            ran.stderr
        );
        assert!(!ran.stderr.contains(key.expose()), "{reason}");
        assert_eq!(config.text(), content);
    }
}

#[test]
fn fmt_prints_no_key_of_the_file_it_rewrites() {
    let key = PrivateKey::generate();
    let config = Config::holding(&by_hand(key.expose()));
    let ran = config.run(&["fmt"]);
    assert_eq!(ran.code, Some(0), "{}", ran.stderr);
    assert!(config.text().contains(key.expose()));
    assert!(!ran.stdout.contains(key.expose()) && !ran.stderr.contains(key.expose()));
}
