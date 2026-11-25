//! `rekey`: a new key for one node. Every peer dials it by the public half.

use crate::edit;
use crate::file::Edit;
use anyhow::{Context, Result};
use sliplib_secrets::wireguard::PrivateKey;
use std::path::{Path, PathBuf};

pub fn run(node: &str, inputs: &[PathBuf], edit_path: &Path) -> Result<()> {
    let mut files = Edit::read(inputs, edit_path)?;
    let network = sliplib_mesh::Mesh::of(&files.file)?;
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
    edit::set_node_key(&mut files, node, &PrivateKey::generate())?;
    let edited = files
        .reread()
        .context("the files with the new key do not read back - not written")?;
    sliplib_mesh::Mesh::of(&edited).context("the files with the new key - not written")?;
    files.save()?;
    println!(
        "rekeyed {} in {}",
        sliplib_mesh::network::node_key(node),
        files.path().display()
    );
    Ok(())
}
