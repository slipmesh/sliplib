//! Format version 1, pinned: what every node of the fixtures under `golden/` gets, compared byte
//! for byte with the files beside each fixture, with derived public keys named by their owner. A
//! change that moves one of them changes what the nodes of a working network are configured
//! with. `UPDATE_EXPECT=1` rewrites the files; the difference is then reviewed like any other.

use expect_test::expect_file;
use sha2::{Digest, Sha256};
use sliplib_config::mesh_config::MeshConfig;
use sliplib_config::slipmesh_file::SlipmeshFile;
use sliplib_mesh::render::node_configs;

/// A fixture, the nodes it holds and the SHA-256 of its text below the comment that opens it. The
/// Talos generator pins its patch files on a copy of the same fixture, and holds the same hash:
/// an edit of one copy fails until the other has it too.
struct Fixture {
    directory: &'static str,
    text: &'static str,
    nodes: &'static [&'static str],
    sha256: &'static str,
}

const FULL: Fixture = Fixture {
    directory: "golden/v1",
    text: include_str!("golden/v1/slipmesh.yaml"),
    nodes: &[
        "node-a", "node-b", "node-c", "node-d", "node-e", "node-f", "router1",
    ],
    sha256: "1c9f8be29d87786776aa969d09e1c2009e1897d43d8edbce8aa4eafd3fbeadc6",
};

const ALTERNATE: Fixture = Fixture {
    directory: "golden/v1-alternate",
    text: include_str!("golden/v1-alternate/slipmesh.yaml"),
    nodes: &["east", "west", "south"],
    sha256: "72c4b4a2206dafb2fb8119bf5e92fde3d97407b5bcee0a059189be78bc4b11a3",
};

/// `text` from its first line that is neither a comment nor empty.
fn below_the_opening_comment(text: &str) -> &str {
    let opening: usize = text
        .split_inclusive('\n')
        .take_while(|line| line.starts_with('#') || line.trim().is_empty())
        .map(str::len)
        .sum();
    &text[opening..]
}

fn sha256(text: &str) -> String {
    Sha256::digest(text)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Every public key derived from a private key of `topology`, with whose it is. The fixtures'
/// private keys are one letter repeated; their public halves read as random text, which the
/// golden files name instead of holding.
fn derived_public_keys(topology: &MeshConfig) -> Vec<(String, String)> {
    let nodes = topology.nodes.iter().filter_map(|node| {
        let key = node.mesh_private_key.as_ref()?.public_key().unwrap();
        Some((key, format!("<public key of {}>", node.name)))
    });
    let clients = topology
        .roadwarriors
        .iter()
        .flat_map(|pool| &pool.clients)
        .filter(|client| client.private_key.is_some())
        .map(|client| {
            (
                client.public_key.clone(),
                format!("<public key of client {}>", client.name),
            )
        });
    nodes.chain(clients).collect()
}

fn every_node_gets_the_configs_it_got_before(fixture: &Fixture) {
    // Set there, `UPDATE_EXPECT` would rewrite the golden files and pass whatever the code gives.
    assert!(
        std::env::var_os("CI").is_none() || std::env::var_os("UPDATE_EXPECT").is_none(),
        "UPDATE_EXPECT is set in CI"
    );
    assert_eq!(
        sha256(below_the_opening_comment(fixture.text)),
        fixture.sha256,
        "{}/slipmesh.yaml changed: change its copy in the Talos generator's tests alike, and the \
         hash in both tests",
        fixture.directory
    );

    let file = SlipmeshFile::parse(fixture.text).unwrap();
    // Named here, not taken from the code under test: a node that code lost would leave its
    // golden files unread.
    assert_eq!(file.hosts(), fixture.nodes);
    let network = sliplib_mesh::v1::lower(file.topology()).unwrap();
    let resolved = sliplib_mesh::secrets::read(&network);
    let public_keys = derived_public_keys(file.topology());
    let named = |text: String| {
        public_keys
            .iter()
            .fold(text, |text, (key, whose)| text.replace(key, whose))
    };

    for host in fixture.nodes {
        let configs = node_configs(&network, host, &resolved).unwrap();
        let directory = fixture.directory;
        expect_file![format!("{directory}/{host}.awg.yaml")]
            .assert_eq(&named(sliplib_yaml::to_string(&configs.awg).unwrap()));
        expect_file![format!("{directory}/{host}.router.yaml")]
            .assert_eq(&named(sliplib_yaml::to_string(&configs.router).unwrap()));
    }
}

#[test]
fn every_node_of_the_full_version_1_file_gets_the_configs_it_got_before() {
    every_node_gets_the_configs_it_got_before(&FULL);
}

#[test]
fn every_node_of_the_alternate_version_1_file_gets_the_configs_it_got_before() {
    every_node_gets_the_configs_it_got_before(&ALTERNATE);
}
