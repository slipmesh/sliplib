//! Format version 1, pinned: what every node of the fixtures under `golden/` gets, compared byte
//! for byte with the files beside each fixture, with derived public keys named by their owner. A
//! change that moves one of them changes what the nodes of a working network are configured
//! with. `UPDATE_EXPECT=1` rewrites the files; the difference is then reviewed like any other.

use expect_test::expect_file;
use sha2::{Digest, Sha256};
use sliplib_config::mesh_config::MeshConfig;
use sliplib_config::slipmesh_file::SlipmeshFile;
use sliplib_config::v2::Platform;
use sliplib_mesh::model::MetricsPorts;
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

/// A fixture of version 1 beside the same network written in version 2 by hand.
struct Twin {
    v1: &'static Fixture,
    text: &'static str,
    /// An endpoint a peer dialed in version 1 because the node wrote a port into its own, and
    /// the one it dials now: the node's address and the link's port.
    redialed: &'static [(&'static str, &'static str)],
}

const FULL_TWIN: Twin = Twin {
    v1: &FULL,
    text: include_str!("golden/v2/slipmesh.yaml"),
    redialed: &[("192.0.2.4:51999", "192.0.2.4:51832")],
};

const ALTERNATE_TWIN: Twin = Twin {
    v1: &ALTERNATE,
    text: include_str!("golden/v2-alternate/slipmesh.yaml"),
    redialed: &[],
};

/// Every node of the twin gets what the file of version 1 gave it, but for what
/// `docs/slipmesh-yaml-v1-to-v2.md` says a conversion changes: no pool interface, no bypass list,
/// no label on what is announced, the link's port where a node's endpoint held another, and no
/// metrics listener on a node that is not of `talos`.
fn the_twin_gives_every_node_what_version_1_gave_it(twin: &Twin) {
    let file = SlipmeshFile::parse(twin.v1.text).unwrap();
    let before = sliplib_mesh::v1::lower(file.topology()).unwrap();
    let before_secrets = sliplib_mesh::secrets::read(&before);

    let files = sliplib_config::v2::File::parse(twin.text).unwrap();
    let mut after = sliplib_mesh::v2::lower(&files).unwrap();
    let metrics = files.talos_settings().unwrap().metrics.unwrap_or_default();
    after.metrics = MetricsPorts {
        awg: metrics.mesh.map(|listener| listener.port.get()),
        router: metrics.router.map(|listener| listener.port.get()),
    };
    let after_secrets = sliplib_mesh::secrets::read(&after);
    let names: Vec<&str> = after.nodes.iter().map(|node| node.name.as_str()).collect();
    assert_eq!(names, twin.v1.nodes);

    for host in twin.v1.nodes {
        let mut expected = node_configs(&before, host, &before_secrets).unwrap();
        expected
            .awg
            .interfaces
            .retain(|interface| !interface.name.starts_with("rw-"));
        expected.router.bypass = None;
        for announced in &mut expected.router.announce {
            announced.label = None;
        }
        let endpoints = expected
            .awg
            .interfaces
            .iter_mut()
            .flat_map(|interface| &mut interface.peers)
            .filter_map(|peer| peer.endpoint.as_mut());
        for endpoint in endpoints {
            if let Some((_, now)) = twin.redialed.iter().find(|(was, _)| endpoint == was) {
                *endpoint = (*now).to_owned();
            }
        }
        if after.node(host).unwrap().platform != Some(Platform::Talos) {
            expected.awg.metrics = None;
            expected.router.metrics = None;
        }

        let got = node_configs(&after, host, &after_secrets).unwrap();
        assert_eq!(
            sliplib_yaml::to_string(&got.awg).unwrap(),
            sliplib_yaml::to_string(&expected.awg).unwrap(),
            "the awg config of {host}"
        );
        assert_eq!(
            sliplib_yaml::to_string(&got.router).unwrap(),
            sliplib_yaml::to_string(&expected.router).unwrap(),
            "the router config of {host}"
        );
    }
}

#[test]
fn the_version_2_twin_of_the_full_file_gives_every_node_what_version_1_gave_it() {
    the_twin_gives_every_node_what_version_1_gave_it(&FULL_TWIN);
}

#[test]
fn the_version_2_twin_of_the_alternate_file_gives_every_node_what_version_1_gave_it() {
    the_twin_gives_every_node_what_version_1_gave_it(&ALTERNATE_TWIN);
}
