//! `rekey`: a new mesh key for one node, whether or not it had one. Every peer of the node dials
//! it by the public half, so their configs change with it.

use crate::edit;
use crate::file::{read_slipmesh, write_edits};
use anyhow::{Context, Result};
use sliplib_secrets::wireguard::PrivateKey;
use std::path::Path;

pub fn run(node: &str, config_path: &Path) -> Result<()> {
    let (mut yaml, file) = read_slipmesh(config_path)?;
    let hosts = file.hosts();
    let index = hosts
        .iter()
        .position(|host| *host == node)
        .with_context(|| format!("unknown node {node:?} - known nodes: {hosts:?}"))?;
    edit::set_node_key(&mut yaml, &file, index, &PrivateKey::generate())?;
    write_edits(config_path, &yaml)?;
    println!(
        "rekeyed nodes[{node}].mesh_private_key in {}",
        config_path.display()
    );
    Ok(())
}
