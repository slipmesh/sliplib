//! `mint`: writes whatever secrets the network lacks into the fields of `slipmesh.yaml` they
//! belong to, and the reference values of the keys it leaves out - once every node's config passes
//! with them. A run with nothing to write leaves the file as it is.

use crate::file::{read_slipmesh, write_edits};
use crate::{check, edit};
use anyhow::{Context, Result};
use sliplib_config::v2::File;
use std::path::Path;

pub fn run(config_path: &Path) -> Result<()> {
    let (mut yaml, mut file) = read_slipmesh(config_path)?;
    // Before anything is minted: the files give no network without it.
    let unset = file.network().mesh.interface_prefix.is_none();
    if unset {
        edit::write_interface_prefix(&mut yaml, &file)?;
        file = File::read(&yaml)
            .context("slipmesh.yaml with what it lacked does not read back - not written")?;
    }
    let minted = sliplib_mesh::mint::create(&sliplib_mesh::Mesh::of(&file)?)?;
    edit::record(&mut yaml, &file, &minted)?;
    if minted.is_empty() && !unset {
        return Ok(());
    }
    let path = config_path.display();

    // What is written is the operator's from then on and is never minted again, so a value a
    // node's config is refused with must not reach the file.
    let edited = File::read(&yaml)
        .context("slipmesh.yaml with what was minted does not read back - not written")?;
    let network = sliplib_mesh::Mesh::of(&edited)?;
    let refusals: Vec<anyhow::Error> = check::nodes(&network)
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
    if unset {
        println!("wrote the reference value of mesh.interface_prefix into {path}");
    }
    Ok(())
}
