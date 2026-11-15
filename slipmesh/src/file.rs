//! `slipmesh.yaml` on disk: read for editing, and written back with its edits.

use anyhow::{Context, Result};
use sliplib_config::slipmesh_file::SlipmeshFile;
use sliplib_yaml::MultiDoc;
use std::path::Path;

pub fn read_slipmesh(config_path: &Path) -> Result<(MultiDoc, SlipmeshFile)> {
    let yaml = MultiDoc::read(config_path)?;
    let file =
        SlipmeshFile::read(&yaml).with_context(|| format!("reading {}", config_path.display()))?;
    Ok((yaml, file))
}

/// Writes `yaml` with its edits to `config_path` - once the result still reads as a valid
/// `slipmesh.yaml`, so a bad edit is refused instead of written.
pub fn write_edits(config_path: &Path, yaml: &MultiDoc) -> Result<()> {
    let updated = yaml.text()?;
    SlipmeshFile::parse(&updated)
        .context("the edited slipmesh.yaml does not read back - not written")?;
    sliplib_yaml::replace_file(config_path, &updated)
}
