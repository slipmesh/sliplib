//! `slipmesh.yaml` on disk.

use anyhow::{Context, Result};
use sliplib_config::v2::{File, Sources};
use std::path::Path;

pub fn read_text(config_path: &Path) -> Result<String> {
    std::fs::read_to_string(config_path)
        .with_context(|| format!("reading {}", config_path.display()))
}

/// For editing, and read.
pub fn parse_slipmesh(config_path: &Path, text: &str) -> Result<(Sources, File)> {
    let reading = || format!("reading {}", config_path.display());
    let sources = Sources::parse(text).with_context(reading)?;
    let file = File::read(&sources).with_context(reading)?;
    Ok((sources, file))
}

pub fn read_slipmesh(config_path: &Path) -> Result<(Sources, File)> {
    parse_slipmesh(config_path, &read_text(config_path)?)
}

/// Once the result still reads: a bad edit is refused, not written.
/// Of the one text [`parse_slipmesh`] reads.
pub fn write_edits(config_path: &Path, sources: &Sources) -> Result<()> {
    write_text(config_path, &sources.files()[0].yaml().text()?)
}

/// Once it reads.
pub fn write_text(config_path: &Path, updated: &str) -> Result<()> {
    File::parse(updated).context("the edited slipmesh.yaml does not read back - not written")?;
    Ok(sliplib_fs::replace_file(config_path, updated)?)
}
