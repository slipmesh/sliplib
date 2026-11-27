//! `manifest`: the cluster's `Mesh` object from the files, on stdout. Writes nothing.

use anyhow::Result;
use sliplib_config::mesh_object::{MeshSpec, manifest};
use sliplib_config::v2::File;
use std::path::PathBuf;

pub fn run(inputs: &[PathBuf]) -> Result<()> {
    let file = File::open(inputs)?;
    print!(
        "{}",
        sliplib_yaml::to_string(&manifest(&MeshSpec::of(file.network())))?
    );
    Ok(())
}
