use crate::common::Config;
use sliplib_config::Range;
use sliplib_config::v2::network::LinkKind;
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
  bgp: {{as: 64512}}   # a private one
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

#[test]
fn fmt_keeps_a_number_a_string_and_a_block_scalar_what_they_were() {
    let config = Config::holding(&by_hand(PrivateKey::generate().expose()));
    assert_eq!(config.run(&["fmt"]).code, Some(0));
    let rewritten = config.text();
    let installer = &values(&rewritten)[1]["installer"];
    assert_eq!(installer["mtu"], Value::from(1500));
    assert_eq!(installer["release"], Value::from("0123"));
    assert_eq!(
        installer["script"],
        Value::from("first line\n  second line, indented\n")
    );
    // An alias is written out as a copy, and a document that holds nothing is dropped.
    assert!(
        !rewritten.contains("&shared") && !rewritten.contains("*shared"),
        "{rewritten}"
    );
    assert_eq!(installer["first"], installer["second"]);
    assert_eq!(installer["second"]["wipe"], Value::from(true));
    assert_eq!(rewritten.matches("\n---\n").count(), 1, "{rewritten}");
}

#[test]
fn fmt_keeps_a_range_a_range_and_a_number_a_number() {
    let by_hand = by_hand(PrivateKey::generate().expose()).replace(
        "amneziawg: {port: 51820}",
        "amneziawg: {port: 51820, obfuscation: {h1: 5-9, h2: 0xA, rekey_after_time: 100-140}}",
    );
    let config = Config::holding(&by_hand);
    assert_eq!(config.run(&["fmt"]).code, Some(0));

    let obfuscation =
        &values(&config.text())[0]["mesh"]["links"]["a-b"]["amneziawg"]["obfuscation"];
    assert_eq!(obfuscation["h1"], Value::from("5-9"));
    assert_eq!(obfuscation["h2"], Value::from(10));
    assert_eq!(obfuscation["rekey_after_time"], Value::from("100-140"));

    let network = network_of(&config.text());
    let LinkKind::Amneziawg(block) = &network.mesh.links["a-b"].kind else {
        panic!("a-b is not of type amneziawg");
    };
    let read = &block.obfuscation.as_ref().unwrap().0;
    assert_eq!(read.h1, Some(Range::new(5, 9).unwrap()));
    assert_eq!(read.h2, Some(10.into()));
    assert_eq!(read.rekey_after_time, Some(Range::new(100, 140).unwrap()));
    // Read and written back through the types, they are what the file holds.
    assert_eq!(&sliplib_yaml::to_value(read).unwrap(), obfuscation);
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
fn fmt_refuses_files_the_loader_refuses_for_its_reason_and_names_no_key() {
    let key = PrivateKey::generate();
    let content = by_hand(key.expose()).replace(
        "  interface_prefix: mesh-\n",
        "  interface_prefix: mesh-\n  no_such_setting: 1\n",
    );
    let config = Config::holding(&content);
    let ran = config.run(&["fmt"]);
    assert_eq!(ran.code, Some(1));
    assert_eq!(ran.stdout, "");
    assert!(ran.stderr.contains("no_such_setting"), "{}", ran.stderr);
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
    assert!(!ran.stderr.contains(key.expose()));
    assert_eq!(config.text(), content);
}
