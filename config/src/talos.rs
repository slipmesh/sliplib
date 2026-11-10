//! Talos machine-config documents, as far as the slipmesh components read and write them.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sliplib_yaml::Value;

const EXTENSION_SERVICE_CONFIG: &str = "ExtensionServiceConfig";

/// What Talos keys a document by: two documents of one identity are one document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub api_version: Option<String>,
    pub kind: Option<String>,
    pub name: Option<String>,
}

impl Identity {
    pub fn of(document: &Value) -> Self {
        let field = |key: &str| document.get(key).and_then(Value::as_str).map(str::to_owned);
        Self {
            api_version: field("apiVersion"),
            kind: field("kind"),
            name: field("name"),
        }
    }

    fn is_extension_service(&self, name: &str) -> bool {
        self.kind.as_deref() == Some(EXTENSION_SERVICE_CONFIG) && self.name.as_deref() == Some(name)
    }
}

impl std::fmt::Display for Identity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.kind.as_deref().unwrap_or_default())?;
        if let Some(name) = &self.name {
            write!(f, "/{name}")?;
        }
        Ok(())
    }
}

/// The files Talos mounts into the extension service `name`.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionServiceConfig {
    pub api_version: String,
    pub kind: String,
    pub name: String,
    #[serde(default)]
    pub config_files: Vec<ConfigFile>,
}

impl ExtensionServiceConfig {
    pub fn new(name: &str, config_files: Vec<ConfigFile>) -> Self {
        Self {
            api_version: "v1alpha1".into(),
            kind: EXTENSION_SERVICE_CONFIG.into(),
            name: name.into(),
            config_files,
        }
    }

    /// The `ExtensionServiceConfig` named `name` among `documents`, if there is one.
    pub fn find(documents: &[Value], name: &str) -> Result<Option<Self>> {
        let Some(document) = documents
            .iter()
            .find(|document| Identity::of(document).is_extension_service(name))
        else {
            return Ok(None);
        };
        sliplib_yaml::from_value(document.clone())
            .with_context(|| format!("malformed {EXTENSION_SERVICE_CONFIG} {name:?}"))
            .map(Some)
    }

    pub fn file(&self, mount_path: &str) -> Option<&ConfigFile> {
        self.config_files
            .iter()
            .find(|f| f.mount_path == mount_path)
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConfigFile {
    pub mount_path: String,
    pub content: String,
}

/// Whether `document` is the `ExtensionServiceConfig` of `service` and sets its file `mount_path`.
pub fn sets_file(document: &Value, service: &str, mount_path: &str) -> bool {
    Identity::of(document).is_extension_service(service)
        && document
            .get("configFiles")
            .and_then(Value::as_sequence)
            .into_iter()
            .flatten()
            .any(|f| f.get("mountPath").and_then(Value::as_str) == Some(mount_path))
}

/// Replaces each `configFiles[].content` of `document` written as a mapping or a list with that
/// YAML's text: Talos takes only a string there.
pub fn contents_as_text(document: &mut Value) -> Result<()> {
    let Some(files) = document
        .get_mut("configFiles")
        .and_then(Value::as_sequence_mut)
    else {
        return Ok(());
    };
    for content in files.iter_mut().filter_map(|file| file.get_mut("content")) {
        if content.is_mapping() || content.is_sequence() {
            let text =
                sliplib_yaml::to_string(&*content).context("serializing a file's contents")?;
            *content = Value::String(text);
        }
    }
    Ok(())
}
