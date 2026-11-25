//! `mint`: the secrets the files lack, and `mesh.interface_prefix`'s reference value, written once
//! every node passes with them.

use crate::file::{read_slipmesh, write_edits};
use crate::{check, edit};
use anyhow::{Context, Result};
use sliplib_config::v2::File;
use std::path::Path;

pub fn run(config_path: &Path) -> Result<()> {
    let (mut sources, mut file) = read_slipmesh(config_path)?;
    // First: the files give no mesh without it.
    let unset = file.network().mesh.interface_prefix.is_none();
    if unset {
        edit::write_interface_prefix(&mut sources, &file)?;
        file = File::read(&sources)
            .context("slipmesh.yaml with what it lacked does not read back - not written")?;
    }
    let minted = sliplib_mesh::mint::create(&sliplib_mesh::Mesh::of(&file)?)?;
    edit::record(&mut sources, &file, &minted)?;
    if minted.is_empty() && !unset {
        return Ok(());
    }
    let path = config_path.display();

    // A written value is never minted again, so one a node is refused with must not be written.
    let edited = File::read(&sources)
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
    write_edits(config_path, &sources)?;

    if !minted.is_empty() {
        println!("minted {} into {path}", minted.routes().join(", "));
    }
    if unset {
        println!("wrote the reference value of mesh.interface_prefix into {path}");
    }
    Ok(())
}
