//! `mint`: writes whatever secrets the topology lacks into the fields of `slipmesh.yaml` they
//! belong to, and the defaults of the network settings it leaves out. A run with nothing to write
//! leaves the file as it is.

use crate::edit;
use crate::file::{read_slipmesh, write_edits};
use anyhow::Result;
use sliplib_config::mesh_config::missing_defaults;
use std::path::Path;

pub fn run(config_path: &Path) -> Result<()> {
    let (mut yaml, file) = read_slipmesh(config_path)?;
    let (_, minted) = sliplib_mesh::mint::resolve(file.topology());
    edit::record(&mut yaml, &file, &minted)?;
    let unset = missing_defaults(file.topology());
    edit::write_defaults(&mut yaml, &file, &unset)?;
    if minted.is_empty() && unset.is_empty() {
        return Ok(());
    }
    write_edits(config_path, &yaml)?;

    let path = config_path.display();
    if !minted.is_empty() {
        println!("minted {} into {path}", minted.routes().join(", "));
    }
    if !unset.is_empty() {
        let keys: Vec<&str> = unset.iter().map(|(key, _)| *key).collect();
        println!("wrote the defaults of {} into {path}", keys.join(", "));
    }
    Ok(())
}
