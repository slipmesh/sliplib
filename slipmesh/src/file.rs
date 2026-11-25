//! The files on disk: those of `--in`, and the `--edit` one a write goes into.

use anyhow::{Context, Result};
use sliplib_config::v2::{DocumentId, File, META_KEY, REVISION, Sources, VERSION};
use sliplib_yaml::{MultiDoc, Value};
use std::path::{Path, PathBuf};

/// The files read for a write, and where it goes.
pub struct Edit {
    sources: Sources,
    pub file: File,
    /// Of `sources`.
    at: usize,
    network: DocumentId,
    path: PathBuf,
}

impl Edit {
    /// An `edit` file not there yet holds one `network` document, of the version and revision read.
    pub fn read(inputs: &[PathBuf], edit: &Path) -> Result<Self> {
        let mut sources = Sources::read(inputs, Some(edit))?;
        let at = sources.edit().context("no file to write into")?;
        if sources.files()[at].is_new() {
            let block = format!(
                "{META_KEY}:\n  kind: network\n  version: {VERSION}\n  revision: {REVISION}\n"
            );
            *sources.files_mut()[at].yaml_mut() = MultiDoc::parse(&block)?;
        }
        let file = File::read(&sources)?;
        let networks: Vec<DocumentId> = file
            .network_documents()
            .iter()
            .filter(|id| id.file == at)
            .copied()
            .collect();
        let [network] = networks[..] else {
            anyhow::bail!(
                "{} holds {} `kind: network` documents, and a write goes into exactly one - give \
                 `--edit` a file that holds one",
                edit.display(),
                networks.len()
            );
        };
        Ok(Self {
            sources,
            file,
            at,
            network,
            path: edit.to_owned(),
        })
    }

    /// As given.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Into the `--edit` file's network document, where no other file holds the key: written
    /// there, the value would lie under or over the other by the order of the documents.
    pub fn write(&mut self, path: &[Value], value: Value) -> Result<()> {
        let elsewhere = self
            .file
            .network_documents()
            .iter()
            .filter(|id| id.file != self.at)
            .find(|id| held(self.document(**id), path));
        if let Some(id) = elsewhere {
            let shown = self.sources.files()[id.file]
                .path()
                .map_or_else(String::new, |path| path.display().to_string());
            anyhow::bail!(
                "`{}` is set in {shown}, and this write goes into {} alone - give that file to \
                 `--edit`, or move the value into this one",
                dotted(path),
                self.path.display()
            );
        }
        let network = self.sources.files_mut()[self.at]
            .yaml_mut()
            .document_mut(self.network.document)
            .context("the network document to write into is gone")?;
        set(network, path, value)
    }

    fn document(&self, id: DocumentId) -> Option<&Value> {
        self.sources.files()[id.file]
            .yaml()
            .documents()
            .iter()
            .find(|document| document.index == id.document)
            .map(|document| &document.value)
    }

    /// With what is written so far.
    pub fn reread(&self) -> Result<File> {
        Ok(File::read(&self.sources)?)
    }

    /// Once its text reads back, with the other files, as a network.
    pub fn save(&mut self) -> Result<()> {
        let text = self.sources.files()[self.at].yaml().text()?;
        *self.sources.files_mut()[self.at].yaml_mut() = MultiDoc::parse(&text)?;
        File::read(&self.sources).context("the edited files do not read back - not written")?;
        if self.sources.files()[self.at].is_new() {
            create_private(&self.path)?;
            if let Err(failed) = sliplib_fs::replace_file(&self.path, &text) {
                // Left empty, the file would be refused as holding no network document.
                let _ = std::fs::remove_file(&self.path);
                return Err(failed.into());
            }
        } else {
            sliplib_fs::replace_file(&self.path, &text)?;
        }
        Ok(())
    }
}

/// Its owner's alone, as it may hold private keys; refused if it appeared since it was read.
fn create_private(path: &Path) -> Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options
        .open(path)
        .with_context(|| format!("creating {}", path.display()))?;
    Ok(())
}

/// A key there, `null` or not.
fn held(document: Option<&Value>, path: &[Value]) -> bool {
    path.iter()
        .fold(document, |value, key| {
            value.and_then(|value| value.get(key))
        })
        .is_some()
}

fn dotted(path: &[Value]) -> String {
    let keys: Vec<String> = path
        .iter()
        .map(|key| match key {
            Value::String(key) => key.clone(),
            key => sliplib_yaml::to_string(key)
                .map(|written| written.trim_end().to_owned())
                .unwrap_or_default(),
        })
        .collect();
    keys.join(".")
}

/// The mappings on the way made where missing or not a mapping.
fn set(document: &mut Value, path: &[Value], value: Value) -> Result<()> {
    let (last, above) = path.split_last().context("an empty path to write at")?;
    let mut entry = document;
    for key in above {
        let mapping = entry
            .as_mapping_mut()
            .context("a value to write under is not a mapping")?;
        let next = mapping.entry(key.clone()).or_insert(Value::Null);
        if !next.is_mapping() {
            *next = Value::Mapping(Default::default());
        }
        entry = next;
    }
    entry
        .as_mapping_mut()
        .context("a value to write under is not a mapping")?
        .insert(last.clone(), value);
    Ok(())
}
