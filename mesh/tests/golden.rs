//! Format version 1, pinned: what every node of `golden/v1/slipmesh.yaml` gets, compared byte for
//! byte with the files beside it. A change that moves one of them changes what the nodes of a
//! working network are configured with. `UPDATE_EXPECT=1` rewrites the files; the difference is
//! then reviewed like any other.

use expect_test::expect_file;
use sliplib_config::mesh_config::MeshConfig;
use sliplib_config::slipmesh_file::SlipmeshFile;
use sliplib_mesh::render::node_configs;

const FIXTURE: &str = include_str!("golden/v1/slipmesh.yaml");

/// Every public key derived from a private key of `topology`, with whose it is. The fixture's
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

#[test]
fn every_node_of_a_version_1_file_gets_the_configs_it_got_before() {
    let file = SlipmeshFile::parse(FIXTURE).unwrap();
    let resolved = sliplib_mesh::secrets::read(file.topology());
    let public_keys = derived_public_keys(file.topology());
    let named = |text: String| {
        public_keys
            .iter()
            .fold(text, |text, (key, whose)| text.replace(key, whose))
    };

    for host in file.hosts() {
        let model = file.effective_for(host).unwrap();
        let configs = node_configs(&model, host, &resolved).unwrap();
        expect_file![format!("golden/v1/{host}.awg.yaml")]
            .assert_eq(&named(sliplib_yaml::to_string(&configs.awg).unwrap()));
        expect_file![format!("golden/v1/{host}.router.yaml")]
            .assert_eq(&named(sliplib_yaml::to_string(&configs.router).unwrap()));
    }
}
