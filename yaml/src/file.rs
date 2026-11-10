use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};
use serde::de::DeserializeOwned;

/// The one document in `path`, as `T`. An error names the file, and the line for a value that
/// does not fit `T`.
pub fn read<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    yaml_rt::from_str(&text).with_context(|| format!("reading {}", path.display()))
}

/// The text of `path`, empty when there is no such file. Any other failure is an error: a file that
/// cannot be read must not pass for one that is not there.
pub fn read_or_empty(path: &Path) -> Result<String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
    }
}

/// Writes `content` to `path`, creating its directory, unless the file already holds exactly that.
/// Whether it wrote.
pub fn write_if_changed(path: &Path, content: &str) -> Result<bool> {
    if read_or_empty(path)? == content && path.exists() {
        return Ok(false);
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    write_atomically(path, content)?;
    Ok(true)
}

/// Replaces `path` with `content` atomically, so an interrupted write cannot leave it half-written.
/// Its permissions carry over, and a symlink is followed rather than replaced.
pub fn replace_file(path: &Path, content: &str) -> Result<()> {
    let target =
        std::fs::canonicalize(path).with_context(|| format!("resolving {}", path.display()))?;
    write_atomically(&target, content)
}

fn write_atomically(path: &Path, content: &str) -> Result<()> {
    let mut file = atomic_write_file::AtomicWriteFile::open(path)
        .with_context(|| format!("opening {} for writing", path.display()))?;
    file.write_all(content.as_bytes())
        .and_then(|()| file.commit())
        .with_context(|| format!("writing {}", path.display()))
}
