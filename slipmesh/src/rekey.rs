//! `rekey`: a new mesh key for one node, whether or not it had one. Every peer of the node dials
//! it by the public half, so their configs change with it.

use crate::edit;
use crate::file::{read_slipmesh, write_edits};
use anyhow::Result;
use sliplib_config::slipmesh_file::SlipmeshFile;
use sliplib_mesh::model::Format;
use sliplib_secrets::wireguard::PrivateKey;
use std::path::Path;

pub fn run(node: &str, config_path: &Path) -> Result<()> {
    let (mut yaml, file) = read_slipmesh(config_path)?;
    let (hosts, format): (Vec<&str>, Format) = match &file {
        SlipmeshFile::V1(file) => (file.hosts(), Format::V1),
        SlipmeshFile::V2(files) => {
            let nodes = files.network().nodes.keys();
            (nodes.map(|name| name.as_str()).collect(), Format::V2)
        }
    };
    anyhow::ensure!(
        hosts.contains(&node),
        "unknown node {node:?} - known nodes: {hosts:?}"
    );
    edit::set_node_key(&mut yaml, &file, node, &PrivateKey::generate())?;
    write_edits(config_path, &yaml)?;
    println!(
        "rekeyed {} in {}",
        format.node_key(node),
        config_path.display()
    );
    Ok(())
}
