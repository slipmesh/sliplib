use yaml_serde::Value;

use crate::error::{Error, Result};

/// No tag is read: a tagged value would otherwise read as the plain one, or as nothing.
pub(crate) fn refuse(value: &Value) -> Result<()> {
    match first(value, "") {
        Some((key, tag)) => Err(Error::Tagged { key, tag }),
        None => Ok(()),
    }
}

/// Its key path, `.` for the document itself, and its tag.
fn first(value: &Value, path: &str) -> Option<(String, String)> {
    match value {
        Value::Tagged(tagged) => Some((
            if path.is_empty() { "." } else { path }.to_owned(),
            tagged.tag.to_string(),
        )),
        Value::Sequence(items) => items
            .iter()
            .enumerate()
            .find_map(|(index, item)| first(item, &format!("{path}[{index}]"))),
        Value::Mapping(entries) => entries.iter().find_map(|(key, value)| {
            let name = match key.as_str() {
                Some(name) => name.to_owned(),
                None => yaml_serde::to_string(key)
                    .map(|written| written.trim_end().to_owned())
                    .unwrap_or_default(),
            };
            let path = if path.is_empty() {
                name
            } else {
                format!("{path}.{name}")
            };
            first(key, &path).or_else(|| first(value, &path))
        }),
        _ => None,
    }
}
