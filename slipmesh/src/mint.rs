//! `mint`: the secrets the files lack, and `mesh.interface_prefix`'s reference value, written once
//! every node passes with them.

use crate::file::Edit;
use crate::{check, edit};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub fn run(inputs: &[PathBuf], edit_path: &Path) -> Result<()> {
    let mut files = Edit::read(inputs, edit_path)?;
    // First: the files give no mesh without it.
    let unset = files.file.network().mesh.interface_prefix.is_none();
    if unset {
        edit::write_interface_prefix(&mut files)?;
        files.file = files
            .reread()
            .context("the files with what they lacked do not read back - not written")?;
    }
    let minted = sliplib_mesh::mint::create(&sliplib_mesh::Mesh::of(&files.file)?)?;
    edit::record(&mut files, &minted)?;
    if minted.is_empty() && !unset {
        return Ok(());
    }
    let path = files.path().display().to_string();

    // A written value is never minted again, so one a node is refused with must not be written.
    let edited = files
        .reread()
        .context("the files with what was minted do not read back - not written")?;
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
        "{path}: not written - with what was minted, {} of the nodes would be refused",
        refusals.len()
    );
    files.save()?;

    if !minted.is_empty() {
        println!("minted {} into {path}", minted.routes().join(", "));
    }
    if unset {
        println!("wrote the reference value of mesh.interface_prefix into {path}");
    }
    Ok(())
}
