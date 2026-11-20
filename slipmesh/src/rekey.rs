//! `rekey`: a new mesh key for one node, whether or not it had one. Every peer of the node dials
//! it by the public half, so their configs change with it.

use crate::file::{read_slipmesh, write_edits};
use crate::{check, edit};
use anyhow::{Context, Result};
use sliplib_config::slipmesh_file::SlipmeshFile;
use sliplib_secrets::wireguard::PrivateKey;
use std::path::Path;

pub fn run(node: &str, config_path: &Path) -> Result<()> {
    let (mut yaml, file) = read_slipmesh(config_path)?;
    let network = check::network(&file, None)?;
    if network.node(node).is_err() {
        let known: Vec<&str> = network
            .nodes
            .iter()
            .map(|node| node.name.as_str())
            .collect();
        anyhow::bail!("unknown node {node:?} - known nodes: {known:?}");
    }
    anyhow::ensure!(
        network.needs_a_key(node),
        "node {node:?} is on no link, and a node of format version 2 holds a key only on a link - put it on one first"
    );
    edit::set_node_key(&mut yaml, &file, node, &PrivateKey::generate())?;
    let edited = SlipmeshFile::read(&yaml)
        .context("slipmesh.yaml with the new key does not read back - not written")?;
    check::network(&edited, None).context("slipmesh.yaml with the new key - not written")?;
    write_edits(config_path, &yaml)?;
    println!(
        "rekeyed {} in {}",
        network.format.node_key(node),
        config_path.display()
    );
    Ok(())
}
