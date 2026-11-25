use crate::common::{MESH, Ran, TALOS, slipmesh};
use sliplib_secrets::wireguard::PrivateKey;
use sliplib_yaml::{MultiDoc, Value};

/// Files by name in a directory of their own, the tool run there.
struct Dir(tempfile::TempDir);

impl Dir {
    fn holding(files: &[(&str, &str)]) -> Self {
        let dir = Self(tempfile::tempdir().unwrap());
        for (name, text) in files {
            std::fs::write(dir.path(name), text).unwrap();
        }
        dir
    }

    fn path(&self, name: &str) -> std::path::PathBuf {
        self.0.path().join(name)
    }

    fn text(&self, name: &str) -> String {
        std::fs::read_to_string(self.path(name)).unwrap()
    }

    fn documents(&self, name: &str) -> Vec<Value> {
        MultiDoc::parse(&self.text(name)).unwrap().values()
    }

    fn run(&self, args: &[&str]) -> Ran {
        slipmesh()
            .args(args)
            .current_dir(self.0.path())
            .output()
            .unwrap()
            .into()
    }
}

#[test]
fn mint_writes_into_the_edit_file_alone_and_creates_it_with_one_network_document() {
    let dir = Dir::holding(&[("mesh.yaml", MESH), ("talos.yaml", TALOS)]);
    let mint = [
        "mint",
        "--in",
        "mesh.yaml",
        "talos.yaml",
        "--edit",
        "secrets.yaml",
    ];
    let ran = dir.run(&mint);
    assert_eq!(ran.code, Some(0), "{}", ran.stderr);
    assert!(ran.stdout.contains("into secrets.yaml"), "{}", ran.stdout);
    assert_eq!(dir.text("mesh.yaml"), MESH);
    let [secrets] = &dir.documents("secrets.yaml")[..] else {
        panic!("{}", dir.text("secrets.yaml"));
    };
    let block: Value = sliplib_yaml::from_str("{kind: network, version: 2, revision: 1}").unwrap();
    assert_eq!(secrets["slipmesh"], block);
    assert!(secrets["nodes"]["node-a"]["wireguard"]["private_key"].is_string());
    assert!(secrets["mesh"]["links"]["a-b"]["amneziawg"]["obfuscation"]["jc"].is_number());

    let ran = dir.run(&[
        "check",
        "--in",
        "mesh.yaml",
        "--in",
        "secrets.yaml",
        "talos.yaml",
    ]);
    assert_eq!(ran.code, Some(0), "{}", ran.stderr);
    assert_eq!(
        ran.stdout,
        "node-a: ok\nnode-b: ok\nnode-c: ok\nrouter1: ok\n"
    );
    let written = dir.text("secrets.yaml");
    assert_eq!(dir.run(&mint).code, Some(0));
    assert_eq!(dir.text("secrets.yaml"), written);
}

#[cfg(unix)]
#[test]
fn an_edit_file_mint_creates_is_its_owners_alone() {
    use std::os::unix::fs::PermissionsExt;
    let dir = Dir::holding(&[("mesh.yaml", MESH)]);
    let ran = dir.run(&["mint", "--in", "mesh.yaml", "--edit", "secrets.yaml"]);
    assert_eq!(ran.code, Some(0), "{}", ran.stderr);
    let mode = std::fs::metadata(dir.path("secrets.yaml"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[test]
fn a_write_into_a_file_without_exactly_one_network_document_is_refused_and_writes_nothing() {
    let laid = "slipmesh: {kind: network, version: 2, revision: 1}\nmesh: {mtu: 1300}\n";
    let two = format!("{MESH}---\n{laid}");
    let dir = Dir::holding(&[
        ("mesh.yaml", MESH),
        ("talos.yaml", TALOS),
        ("two.yaml", &two),
    ]);
    for args in [
        &["mint", "--in", "mesh.yaml", "--edit", "talos.yaml"][..],
        &["rekey", "--node", "node-a", "--edit", "two.yaml"],
    ] {
        let edit = args[args.len() - 1];
        let before = dir.text(edit);
        let ran = dir.run(args);
        assert_eq!(ran.code, Some(1), "{args:?}: {}", ran.stdout);
        assert!(ran.stderr.contains(edit), "{args:?}: {}", ran.stderr);
        assert_eq!(dir.text(edit), before, "{args:?}");
    }
    assert_eq!(dir.text("mesh.yaml"), MESH);
}

#[test]
fn a_change_to_a_value_set_in_another_file_is_refused_naming_that_file() {
    let dir = Dir::holding(&[("mesh.yaml", MESH)]);
    let ran = dir.run(&["mint", "--in", "mesh.yaml", "--edit", "secrets.yaml"]);
    assert_eq!(ran.code, Some(0), "{}", ran.stderr);
    let (mesh, secrets) = (dir.text("mesh.yaml"), dir.text("secrets.yaml"));
    let ran = dir.run(&[
        "rekey",
        "--node",
        "node-a",
        "--in",
        "secrets.yaml",
        "--edit",
        "mesh.yaml",
    ]);
    assert_eq!(ran.code, Some(1), "{}", ran.stdout);
    assert!(
        ran.stderr.contains("secrets.yaml")
            && ran.stderr.contains("nodes.node-a.wireguard.private_key"),
        "{}",
        ran.stderr
    );
    assert_eq!(
        (dir.text("mesh.yaml"), dir.text("secrets.yaml")),
        (mesh.clone(), secrets.clone())
    );

    let ran = dir.run(&[
        "rekey",
        "--node",
        "node-a",
        "--in",
        "mesh.yaml",
        "--edit",
        "secrets.yaml",
    ]);
    assert_eq!(ran.code, Some(0), "{}", ran.stderr);
    assert_eq!(dir.text("mesh.yaml"), mesh);
    assert_ne!(dir.text("secrets.yaml"), secrets);
}

#[test]
fn fmt_rewrites_each_file_alone_and_none_where_one_is_refused() {
    let key = PrivateKey::generate();
    let secrets = format!(
        "# by hand\nslipmesh: {{kind: network, version: 2, revision: 1}}\nnodes: {{node-a: \
         {{wireguard: {{private_key: \"{}\"}}}}}}\n",
        key.expose()
    );
    let groups = "# by hand\nslipmesh: {kind: groups}\ncore: [node-a]\n";
    let version_1 = "slipmesh: {kind: network}\nnodes: []\n";
    let dir = Dir::holding(&[
        ("secrets.yaml", &secrets),
        ("groups.yaml", groups),
        ("old.yaml", version_1),
    ]);
    let ran = dir.run(&["fmt", "secrets.yaml", "old.yaml", "groups.yaml"]);
    assert_eq!(ran.code, Some(1), "{}", ran.stdout);
    assert!(ran.stderr.contains("old.yaml"), "{}", ran.stderr);
    assert_eq!(
        (dir.text("secrets.yaml"), dir.text("groups.yaml")),
        (secrets.clone(), groups.to_owned())
    );

    let ran = dir.run(&["fmt", "secrets.yaml", "./secrets.yaml", "groups.yaml"]);
    assert_eq!(ran.code, Some(0), "{}", ran.stderr);
    assert_eq!(ran.stdout.lines().count(), 2, "{}", ran.stdout);
    for (name, before) in [("secrets.yaml", secrets.as_str()), ("groups.yaml", groups)] {
        assert_ne!(dir.text(name), before, "{name}");
        assert_eq!(
            dir.documents(name),
            MultiDoc::parse(before).unwrap().values(),
            "{name}"
        );
    }
}

#[test]
fn a_write_another_file_takes_away_is_refused_naming_that_file() {
    let dir = Dir::holding(&[("mesh.yaml", MESH)]);
    assert_eq!(
        dir.run(&["mint", "--in", "mesh.yaml", "--edit", "secrets.yaml"])
            .code,
        Some(0)
    );
    let mask =
        "slipmesh: {kind: network, version: 2, revision: 1}\nnodes: {node-a: {wireguard: null}}\n";
    std::fs::write(dir.path("z-mask.yaml"), mask).unwrap();
    let secrets = dir.text("secrets.yaml");
    let ran = dir.run(&[
        "rekey",
        "--node",
        "node-a",
        "--in",
        "mesh.yaml",
        "z-mask.yaml",
        "--edit",
        "secrets.yaml",
    ]);
    assert_eq!(ran.code, Some(1), "{}", ran.stdout);
    assert!(
        ran.stderr.contains("z-mask.yaml")
            && ran.stderr.contains("nodes.node-a.wireguard.private_key"),
        "{}",
        ran.stderr
    );
    assert_eq!(dir.text("secrets.yaml"), secrets);
}

#[test]
fn a_null_inside_a_links_block_is_written_over_from_a_file_laid_after_it_alone() {
    let nulled = MESH.replace(
        "      amneziawg:\n        port: 51830\n",
        "      amneziawg:\n        port: 51830\n        obfuscation: {jc: null}\n",
    );
    assert_ne!(nulled, MESH);
    let dir = Dir::holding(&[("mesh.yaml", &nulled)]);
    let ran = dir.run(&["mint", "--in", "mesh.yaml", "--edit", "z-secrets.yaml"]);
    assert_eq!(ran.code, Some(0), "{}", ran.stderr);
    let [secrets] = &dir.documents("z-secrets.yaml")[..] else {
        panic!("{}", dir.text("z-secrets.yaml"));
    };
    assert!(secrets["mesh"]["links"]["a-b"]["amneziawg"]["obfuscation"]["jc"].is_number());

    let ran = dir.run(&["mint", "--in", "mesh.yaml", "--edit", "a-secrets.yaml"]);
    assert_eq!(ran.code, Some(1), "{}", ran.stdout);
    assert!(
        ran.stderr.contains("mesh.yaml")
            && ran
                .stderr
                .contains("mesh.links.a-b.amneziawg.obfuscation.jc"),
        "{}",
        ran.stderr
    );
    assert!(!dir.path("a-secrets.yaml").exists());
}

#[test]
fn a_write_that_would_move_the_nodes_is_refused() {
    let dir = Dir::holding(&[("mesh.yaml", MESH)]);
    let ran = dir.run(&[
        "rekey",
        "--node",
        "node-b",
        "--in",
        "mesh.yaml",
        "--edit",
        "keys.yaml",
    ]);
    assert_eq!(ran.code, Some(1), "{}", ran.stdout);
    assert!(
        ran.stderr.contains("keys.yaml") && ran.stderr.contains("change their order"),
        "{}",
        ran.stderr
    );
    assert!(!dir.path("keys.yaml").exists());
}
