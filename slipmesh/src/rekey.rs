//! `rekey`: a new key for one node. Every peer dials it by the public half.

use crate::edit;
use crate::file::{read_slipmesh, write_edits};
use anyhow::{Context, Result};
use sliplib_config::v2::File;
use sliplib_secrets::wireguard::PrivateKey;
use std::path::Path;

pub fn run(node: &str, config_path: &Path) -> Result<()> {
    let (mut sources, file) = read_slipmesh(config_path)?;
    let network = sliplib_mesh::Mesh::of(&file)?;
    if network.node(node).is_err() {
        let known: Vec<&str> = network
            .network()
            .nodes
            .keys()
            .map(|name| name.as_str())
            .collect();
        anyhow::bail!("unknown node {node:?} - known nodes: {known:?}");
    }
    anyhow::ensure!(
        network.needs_a_key(node),
        "node {node:?} is on no link, and a node holds a key only on a link - put it on one first"
    );
    edit::set_node_key(&mut sources, &file, node, &PrivateKey::generate())?;
    let edited = File::read(&sources)
        .context("slipmesh.yaml with the new key does not read back - not written")?;
    sliplib_mesh::Mesh::of(&edited).context("slipmesh.yaml with the new key - not written")?;
    write_edits(config_path, &sources)?;
    println!(
        "rekeyed {} in {}",
        sliplib_mesh::network::node_key(node),
        config_path.display()
    );
    Ok(())
}
