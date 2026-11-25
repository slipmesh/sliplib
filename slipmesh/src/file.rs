//! The files on disk: those of `--in`, and the `--edit` one a write goes into.

use anyhow::{Context, Result};
use sliplib_config::v2::{DocumentId, File, META_KEY, REVISION, Sources, VERSION};
use sliplib_yaml::{MultiDoc, Value};
use std::io::Write;
use std::path::{Path, PathBuf};

/// The files read for a write, and where it goes.
pub struct Edit {
    sources: Sources,
    pub file: File,
    /// Of `sources`.
    at: usize,
    network: DocumentId,
    path: PathBuf,
    /// Each path written, with its value.
    written: Vec<(Vec<Value>, Value)>,
    /// The nodes and the links as read, in order.
    order: (Vec<String>, Vec<String>),
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
        let order = order(&file);
        Ok(Self {
            sources,
            file,
            at,
            network,
            path: edit.to_owned(),
            written: Vec::new(),
            order,
        })
    }

    /// As given.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Into the `--edit` file's network document, where no other file sets the key: written
    /// there, the value would lie under or over the other by the order of the documents.
    pub fn write(&mut self, path: &[Value], value: Value) -> Result<()> {
        let elsewhere = self
            .file
            .network_documents()
            .iter()
            .filter(|id| id.file != self.at)
            .find(|id| at(self.document(**id), path).is_some_and(|value| !value.is_null()));
        if let Some(id) = elsewhere {
            anyhow::bail!(
                "`{}` is set in {}, and this write goes into {} alone - give that file to \
                 `--edit`, or move the value into this one",
                dotted(path),
                self.shown(id.file),
                self.path.display()
            );
        }
        let network = self.sources.files_mut()[self.at]
            .yaml_mut()
            .document_mut(self.network.document)
            .context("the network document to write into is gone")?;
        set(network, path, value.clone())?;
        self.written.push((path.to_vec(), value));
        Ok(())
    }

    fn shown(&self, file: usize) -> String {
        self.sources.files()[file]
            .path()
            .map_or_else(String::new, |path| path.display().to_string())
    }

    /// Of the layers laid over the `--edit` file's, the last holding the deepest part of `path`.
    fn laid_over(&self, path: &[Value]) -> Option<usize> {
        let layers = self.file.network_documents();
        let after = layers.iter().position(|id| *id == self.network)? + 1;
        (1..=path.len()).rev().find_map(|depth| {
            layers[after..]
                .iter()
                .rev()
                .find(|id| at(self.document(**id), &path[..depth]).is_some())
                .map(|id| id.file)
        })
    }

    fn document(&self, id: DocumentId) -> Option<&Value> {
        self.sources.files()[id.file]
            .yaml()
            .documents()
            .iter()
            .find(|document| document.index == id.document)
            .map(|document| &document.value)
    }

    /// With what is written so far, which the files laid together have to keep as written, and
    /// the nodes and links in their order.
    pub fn reread(&self) -> Result<File> {
        let file = File::read(&self.sources)?;
        for (path, value) in &self.written {
            if at(Some(file.laid()), path) != Some(value) {
                let over = self
                    .laid_over(path)
                    .map_or_else(|| "a file laid over it".to_owned(), |file| self.shown(file));
                anyhow::bail!(
                    "`{}` written into {} is taken away by {over} - write it there, or give \
                     `--edit` a file laid after it, by its name or by `order`",
                    dotted(path),
                    self.path.display()
                );
            }
        }
        anyhow::ensure!(
            order(&file) == self.order,
            "written into {}, laid before the files that name them, the nodes or the links would \
             change their order, and with it every node's peers and interfaces - give it a name \
             that sorts after those files, or a later `order`",
            self.path.display()
        );
        Ok(file)
    }

    /// Once its text reads back, with the other files, as a network.
    pub fn save(&mut self) -> Result<()> {
        let text = self.sources.files()[self.at].yaml().text()?;
        *self.sources.files_mut()[self.at].yaml_mut() = MultiDoc::parse(&text)?;
        File::read(&self.sources).context("the edited files do not read back - not written")?;
        if self.sources.files()[self.at].is_new() {
            create_private(&self.path, &text)?;
        } else {
            sliplib_fs::replace_file(&self.path, &text)?;
        }
        Ok(())
    }
}

/// Its owner's alone, as it may hold private keys; refused if it appeared since it was read.
/// Written through the handle that created it, so nothing put at the path since is followed.
fn create_private(path: &Path, text: &str) -> Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options
        .open(path)
        .with_context(|| format!("creating {}", path.display()))?;
    let written = file
        .write_all(text.as_bytes())
        .and_then(|()| file.sync_all());
    if written.is_err() {
        // Cut short, it would hold part of the keys.
        let _ = std::fs::remove_file(path);
    }
    written.with_context(|| format!("writing {}", path.display()))
}

fn at<'a>(document: Option<&'a Value>, path: &[Value]) -> Option<&'a Value> {
    path.iter().fold(document, |value, key| {
        value.and_then(|value| value.get(key))
    })
}

fn order(file: &File) -> (Vec<String>, Vec<String>) {
    let network = file.network();
    (
        network.nodes.keys().map(ToString::to_string).collect(),
        network.mesh.links.keys().map(ToString::to_string).collect(),
    )
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
