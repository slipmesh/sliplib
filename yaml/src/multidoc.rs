use std::ops::Range;
use std::path::Path;

use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use yaml_rt::{NodeId, YamlDoc, YamlPatch};
use yaml_serde::Value;

/// A multi-document YAML file, as its text and parsed for editing.
pub struct MultiDoc {
    yaml: YamlDoc,
}

impl MultiDoc {
    pub fn parse(text: &str) -> Result<Self> {
        Ok(Self {
            yaml: YamlDoc::parse(text).context("not valid YAML")?,
        })
    }

    pub fn read(path: &Path) -> Result<Self> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        Self::parse(&text).with_context(|| format!("reading {}", path.display()))
    }

    /// Every document that holds anything but comments, in file order. A document of only
    /// comments still takes a position: `index` counts it, as the edits do.
    pub fn documents(&self) -> Result<Vec<Document>> {
        let raw = self.text();
        // Each document runs to where the next one starts. Its own span cannot say where it ends:
        // `yaml-rt` ends it at the header of a block scalar closing the document, before the text
        // under it (`hdevalke/yaml-rt#5`).
        let mut starts: Vec<usize> = self
            .yaml
            .documents()
            .map(|document| span_of(&self.yaml, document).map(|span| span.start))
            .collect::<Result<_>>()?;
        if let Some(first) = starts.first_mut() {
            *first = 0;
        }
        let mut documents = Vec::new();
        for (index, &start) in starts.iter().enumerate() {
            let end = starts.get(index + 1).copied().unwrap_or(raw.len());
            let Some(root) = self.yaml.document_root(index)? else {
                continue;
            };
            let line = raw[..start].matches('\n').count() + 1;
            let text = raw[start..end].to_owned();
            let read = || -> Result<Document> {
                let value: Value = yaml_serde::from_str(&text).context("not valid YAML")?;
                let entries = self.entries(root, &value, start)?;
                Ok(Document {
                    index,
                    line,
                    value,
                    text,
                    entries,
                })
            };
            documents
                .push(read().with_context(|| format!("the document starting at line {line}"))?);
        }
        Ok(documents)
    }

    /// The top-level keys of `value` with where each entry sits in its document's text: from the
    /// key to the end of its value, which for a block scalar lies past the entry's own span.
    fn entries(
        &self,
        root: NodeId,
        value: &Value,
        document_start: usize,
    ) -> Result<Vec<(String, Range<usize>)>> {
        let Some(mapping) = value.as_mapping() else {
            return Ok(Vec::new());
        };
        let mut entries = Vec::new();
        for key in mapping.keys().filter_map(Value::as_str) {
            let Some(entry) = self.yaml.get_mapping_entry(root, key)? else {
                continue;
            };
            let mut span = span_of(&self.yaml, entry)?;
            if let Some(value) = self.yaml.get_mapping_value(root, key)? {
                span.end = span.end.max(span_of(&self.yaml, value)?.end);
            }
            entries.push((
                key.to_owned(),
                span.start - document_start..span.end - document_start,
            ));
        }
        Ok(entries)
    }

    /// Applies `operations` to document `index` as RFC 6902 (JSON Patch) does, all of them or none:
    /// a failing one leaves the file as it was. Each is the smallest edit - everything it does not
    /// touch, comments and layout included, stays as written - and an added value is written in
    /// block style.
    pub fn patch(&mut self, index: usize, operations: &[Operation]) -> Result<()> {
        let patch: Vec<Value> = operations.iter().map(Operation::to_patch_entry).collect();
        let patch = yaml_serde::to_string(&patch).context("serializing the patch")?;
        let patch = YamlPatch::parse(&patch).context("the patch does not parse")?;
        let mut patched = self.yaml.clone();
        patched.apply_patch(index, &patch)?;
        patched.commit_edits()?;
        self.yaml = YamlDoc::parse(&patched.to_string())?;
        Ok(())
    }

    /// The file's text with every edit in it.
    pub fn text(&self) -> String {
        self.yaml.to_string()
    }
}

/// The byte range of `node` in the source.
fn span_of(yaml: &YamlDoc, node: NodeId) -> Result<Range<usize>> {
    let span = yaml
        .node(node)
        .context("a node the parser did not keep")?
        .span();
    Ok(span.start as usize..span.end as usize)
}

/// One document of a [`MultiDoc`].
pub struct Document {
    /// Its position among the documents of the file, those of only comments included.
    pub index: usize,
    /// The line of the file it starts on.
    pub line: usize,
    pub value: Value,
    text: String,
    entries: Vec<(String, Range<usize>)>,
}

impl Document {
    /// The document as written, from its separator on, with any taken key blanked out.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Takes the top-level `key` out of the document: out of `value`, and out of the text, where
    /// its lines are left empty so that every line after it keeps its number.
    pub fn take(&mut self, key: &str) -> Option<Value> {
        let value = self.value.as_mapping_mut()?.shift_remove(key)?;
        if let Some(position) = self.entries.iter().position(|(k, _)| k == key) {
            let (_, range) = self.entries.remove(position);
            let blank = "\n".repeat(self.text[range.clone()].matches('\n').count());
            let removed = range.len() - blank.len();
            self.text.replace_range(range.clone(), &blank);
            for (_, other) in &mut self.entries {
                if other.start >= range.end {
                    *other = other.start - removed..other.end - removed;
                }
            }
        }
        Some(value)
    }

    /// The document as `T`. An error names the field and the line of the file it is on.
    pub fn deserialize<T: DeserializeOwned>(&self) -> Result<T> {
        let positioned = "\n".repeat(self.line - 1) + &self.text;
        Ok(yaml_serde::from_str(&positioned)?)
    }
}

/// Documents joined into one multi-document text.
pub fn join(documents: &[String]) -> String {
    documents.join("---\n")
}

/// `text` with `documents` added after its last line.
pub fn append(text: &str, documents: &[String]) -> String {
    let mut out = text.to_owned();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    for document in documents {
        out.push_str("---\n");
        out.push_str(document);
    }
    out
}

/// One edit of a document, at `path`: a key at each step, an item index as its number, or `-` for
/// the end of a sequence.
pub enum Operation {
    /// Sets the value at `path`, adding the key or inserting the item. Its parent has to exist.
    Add { path: Vec<String>, value: Value },
    /// Removes the key or the item at `path`, which has to exist.
    Remove { path: Vec<String> },
}

impl Operation {
    fn to_patch_entry(&self) -> Value {
        let mut entry = yaml_serde::Mapping::new();
        let (op, path, value) = match self {
            Operation::Add { path, value } => ("add", path, Some(value)),
            Operation::Remove { path } => ("remove", path, None),
        };
        entry.insert("op".into(), op.into());
        entry.insert("path".into(), json_pointer(path).into());
        if let Some(value) = value {
            entry.insert("value".into(), value.clone());
        }
        Value::Mapping(entry)
    }
}

/// `path` as a JSON Pointer (RFC 6901): `~` and `/` inside a step are escaped, so each step names
/// exactly one key.
fn json_pointer(path: &[String]) -> String {
    path.iter()
        .map(|step| format!("/{}", step.replace('~', "~0").replace('/', "~1")))
        .collect()
}
