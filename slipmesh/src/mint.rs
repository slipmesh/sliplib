//! `mint`: writes whatever secrets the topology lacks into the fields of `slipmesh.yaml` they
//! belong to, and the defaults of the network settings it leaves out - once every node's config
//! passes with them. A run with nothing to write leaves the file as it is.

use crate::file::{read_slipmesh, write_edits};
use crate::{check, edit};
use anyhow::{Context, Result};
use sliplib_config::mesh_config::missing_defaults;
use sliplib_config::slipmesh_file::SlipmeshFile;
use std::path::Path;

pub fn run(config_path: &Path) -> Result<()> {
    let (mut yaml, file) = read_slipmesh(config_path)?;
    let minted = sliplib_mesh::mint::create(file.topology())?;
    edit::record(&mut yaml, &file, &minted)?;
    let unset = missing_defaults(file.topology());
    edit::write_defaults(&mut yaml, &file, &unset)?;
    if minted.is_empty() && unset.is_empty() {
        return Ok(());
    }
    let path = config_path.display();

    // What is written is the operator's from then on and is never minted again, so a value a
    // node's config is refused with must not reach the file.
    let edited = SlipmeshFile::read(&yaml)
        .context("slipmesh.yaml with what was minted does not read back - not written")?;
    let refusals: Vec<anyhow::Error> = check::nodes(&edited)
        .into_iter()
        .filter_map(|(_, checked)| checked.err())
        .collect();
    for refusal in &refusals {
        eprintln!("{refusal:#}");
    }
    anyhow::ensure!(
        refusals.is_empty(),
        "{path}: not written - with what was minted, {} of its nodes would be refused",
        refusals.len()
    );
    write_edits(config_path, &yaml)?;

    if !minted.is_empty() {
        println!("minted {} into {path}", minted.routes().join(", "));
    }
    if !unset.is_empty() {
        let keys: Vec<&str> = unset.iter().map(|(key, _)| *key).collect();
        println!("wrote the defaults of {} into {path}", keys.join(", "));
    }
    Ok(())
}
