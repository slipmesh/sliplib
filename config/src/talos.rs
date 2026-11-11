//! Talos machine-config documents, as far as the slipmesh components read and write them.

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use sliplib_yaml::{Mapping, Value};

const API_VERSION: &str = "v1alpha1";
const EXTENSION_SERVICE_CONFIG: &str = "ExtensionServiceConfig";
const KUBE_NODE_CONFIG: &str = "KubeNodeConfig";

/// The directive that turns a document into a deletion: a message to Talos, not values to merge.
const PATCH_KEY: &str = "$patch";

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
            api_version: API_VERSION.into(),
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

/// `overlay` merged over `base`, two documents of one identity: JSON Merge Patch, as
/// [`sliplib_yaml::merge`] does it. A `$patch: delete` document on either side is not merged - over
/// the base it would turn a deletion into an edit, under the overlay an edit into a deletion - and
/// the overlay replaces the result outright.
///
/// Sequences are replaced rather than appended, unlike Talos' own strategic merge: Talos appends to
/// a keyed list by its key because it knows the types, and schemaless YAML could neither remove an
/// element nor tell that two `configFiles` entries share a `mountPath`.
pub fn merge(base: Value, overlay: &Value) -> Value {
    if is_deletion(overlay) || is_deletion(&base) {
        return overlay.clone();
    }
    sliplib_yaml::merge(base, overlay)
}

fn is_deletion(document: &Value) -> bool {
    document.get(PATCH_KEY).and_then(Value::as_str) == Some("delete")
}

/// A `KubeNodeConfig` that sets nothing: the document of the node's labels, annotations and
/// taints, one to a machine config.
pub fn kube_node_config() -> Value {
    Value::Mapping(Mapping::from_iter([
        ("apiVersion".into(), API_VERSION.into()),
        ("kind".into(), KUBE_NODE_CONFIG.into()),
    ]))
}

pub fn is_kube_node_config(document: &Value) -> bool {
    Identity::of(document).kind.as_deref() == Some(KUBE_NODE_CONFIG)
}

/// Adds `annotations`, all in `namespace`, to the `KubeNodeConfig` `document`. Refuses a document
/// that sets an annotation in `namespace` itself, which `annotations` would overwrite silently, and
/// one that deletes the `KubeNodeConfig`, which would drop them.
pub fn annotate(document: &mut Value, namespace: &str, annotations: Mapping) -> Result<()> {
    ensure!(
        !is_deletion(document),
        "a document deletes the {KUBE_NODE_CONFIG}, which carries the annotations in {namespace}"
    );
    let given = document
        .as_mapping_mut()
        .with_context(|| format!("a {KUBE_NODE_CONFIG} document is not a mapping"))?
        .entry("annotations".into())
        .or_insert(Value::Null);
    if given.is_null() {
        *given = Value::Mapping(Mapping::new());
    }
    let given = given
        .as_mapping_mut()
        .with_context(|| format!("the annotations of a {KUBE_NODE_CONFIG} are not a mapping"))?;
    if let Some(set) = given
        .keys()
        .filter_map(Value::as_str)
        .find(|key| key.starts_with(namespace))
    {
        bail!("a document sets the annotation {set}, and those in {namespace} are generated");
    }
    given.extend(annotations);
    Ok(())
}
