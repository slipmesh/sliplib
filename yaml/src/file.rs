use std::io::Write;
use std::path::Path;

use serde::de::DeserializeOwned;

use crate::error::{Error, Result};

/// The one document. An error names the file, and the line of a value that does not fit.
pub fn read<T: DeserializeOwned>(path: &Path) -> Result<T> {
    reading(path, |text| {
        crate::tag::refuse(&yaml_serde::from_str(text)?)?;
        Ok(yaml_serde::from_str(text)?)
    })
}

/// The file's text as `parse` takes it; either failure names the file.
pub(crate) fn reading<T>(path: &Path, parse: impl FnOnce(&str) -> Result<T>) -> Result<T> {
    std::fs::read_to_string(path)
        .map_err(Error::from)
        .and_then(|text| parse(&text))
        .map_err(|source| Error::Reading {
            path: path.to_owned(),
            source: Box::new(source),
        })
}

/// Empty where there is no file; any other failure is an error.
pub fn read_or_empty(path: &Path) -> Result<String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(Error::Reading {
            path: path.to_owned(),
            source: Box::new(e.into()),
        }),
    }
}

/// Unless the file already holds it, creating the directory. Whether it wrote.
pub fn write_if_changed(path: &Path, content: &str) -> Result<bool> {
    if read_or_empty(path)? == content && path.exists() {
        return Ok(false);
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|source| Error::Creating {
            path: dir.to_owned(),
            source,
        })?;
    }
    write_atomically(path, content)?;
    Ok(true)
}

/// Atomically, keeping its permissions and following a symlink.
pub fn replace_file(path: &Path, content: &str) -> Result<()> {
    let target = std::fs::canonicalize(path).map_err(|source| Error::Resolving {
        path: path.to_owned(),
        source,
    })?;
    write_atomically(&target, content)
}

fn write_atomically(path: &Path, content: &str) -> Result<()> {
    let mut file =
        atomic_write_file::AtomicWriteFile::open(path).map_err(|source| Error::Opening {
            path: path.to_owned(),
            source,
        })?;
    file.write_all(content.as_bytes())
        .and_then(|()| file.commit())
        .map_err(|source| Error::Writing {
            path: path.to_owned(),
            source,
        })
}
