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
