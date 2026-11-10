use std::fmt;
use std::marker::PhantomData;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use serde::de::value::{MapAccessDeserializer, StrDeserializer};
use serde::de::{
    DeserializeOwned, DeserializeSeed, Deserializer as _, IgnoredAny, IntoDeserializer, MapAccess,
    Visitor,
};
use serde::{Deserialize, Serialize};
use yaml_serde::{Deserializer, Serializer, Value};

/// A multi-document YAML file, read into values to be edited and written back. Writing it leaves
/// out its comments, and every document of only comments.
pub struct MultiDoc {
    documents: Vec<Document>,
}

impl MultiDoc {
    pub fn parse(text: &str) -> Result<Self> {
        let source: Arc<str> = text.into();
        let mut documents = Vec::new();
        for (index, document) in Deserializer::from_str(text).enumerate() {
            let value = Value::deserialize(document).context("not valid YAML")?;
            if value.is_null() {
                continue;
            }
            documents.push(Document {
                index,
                line: line_of(&source, index)?,
                value,
                source: Some(Arc::clone(&source)),
                taken: Vec::new(),
            });
        }
        Ok(Self { documents })
    }

    pub fn read(path: &Path) -> Result<Self> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        Self::parse(&text).with_context(|| format!("reading {}", path.display()))
    }

    /// Every document that holds anything but comments, in file order, edits included.
    pub fn documents(&self) -> &[Document] {
        &self.documents
    }

    /// The value of the document at `index`, to edit; none for a document of only comments.
    pub fn document_mut(&mut self, index: usize) -> Option<&mut Value> {
        let document = self.documents.iter_mut().find(|d| d.index == index)?;
        document.source = None;
        Some(&mut document.value)
    }

    /// The file's text with every edit in it.
    pub fn text(&self) -> Result<String> {
        let mut text = Vec::new();
        let mut serializer = Serializer::new(&mut text);
        for document in &self.documents {
            document.value.serialize(&mut serializer).with_context(|| {
                format!("writing the document starting at line {}", document.line)
            })?;
        }
        drop(serializer);
        String::from_utf8(text).context("the written YAML is not UTF-8")
    }
}

/// One document of a [`MultiDoc`].
#[derive(Clone)]
pub struct Document {
    /// Its position among the documents of the file, those of only comments included.
    pub index: usize,
    /// The line of the file its content starts on.
    pub line: usize,
    pub value: Value,
    /// The file it was read from, while it holds the document as read.
    source: Option<Arc<str>>,
    taken: Vec<String>,
}

impl Document {
    /// Takes the top-level `key` out of the document: out of `value`, and out of what
    /// [`Document::deserialize`] reads.
    pub fn take(&mut self, key: &str) -> Option<Value> {
        let value = self.value.as_mapping_mut()?.shift_remove(key)?;
        self.taken.push(key.to_owned());
        Some(value)
    }

    /// The document as `T`. As read from the file, an error names the field and the line of the
    /// file it is on.
    pub fn deserialize<T: DeserializeOwned>(&self) -> Result<T> {
        let Some(source) = &self.source else {
            return Ok(yaml_serde::from_value(self.value.clone())?);
        };
        let document = Deserializer::from_str(source)
            .nth(self.index)
            .context("the document is no longer in the file")?;
        Ok(document.deserialize_map(Without {
            keys: &self.taken,
            target: PhantomData,
        })?)
    }
}

/// The line the content of document `index` of `source` starts on: where the parser stands when
/// it is asked for something no YAML value is.
fn line_of(source: &str, index: usize) -> Result<usize> {
    struct Nothing;
    impl Visitor<'_> for Nothing {
        type Value = ();
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("nothing")
        }
    }
    let document = Deserializer::from_str(source)
        .nth(index)
        .context("a document the parser did not keep")?;
    let stop = document
        .deserialize_any(Nothing)
        .expect_err("no YAML value is nothing");
    Ok(stop.location().map_or(1, |location| location.line()))
}

/// Deserializes a mapping as `T` without the entries of `keys`.
struct Without<'k, T> {
    keys: &'k [String],
    target: PhantomData<T>,
}

impl<'de, T: Deserialize<'de>> Visitor<'de> for Without<'_, T> {
    type Value = T;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a mapping")
    }

    fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<T, A::Error> {
        T::deserialize(MapAccessDeserializer::new(Skipping {
            map,
            keys: self.keys,
        }))
    }
}

struct Skipping<'k, A> {
    map: A,
    keys: &'k [String],
}

impl<'de, A: MapAccess<'de>> MapAccess<'de> for Skipping<'_, A> {
    type Error = A::Error;

    fn next_key_seed<K: DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, A::Error> {
        let mut seed = Some(seed);
        loop {
            let key = KeySeed {
                seed: &mut seed,
                skipped: self.keys,
            };
            match self.map.next_key_seed(key)? {
                None => return Ok(None),
                Some(Key::Skipped) => {
                    self.map.next_value::<IgnoredAny>()?;
                }
                Some(Key::Read(key)) => return Ok(Some(key)),
            }
        }
    }

    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value, A::Error> {
        self.map.next_value_seed(seed)
    }
}

enum Key<K> {
    Skipped,
    Read(K),
}

/// A key read by the parser itself, so that an error about it - an unknown field - carries its
/// line.
struct KeySeed<'s, 'k, K> {
    seed: &'s mut Option<K>,
    skipped: &'k [String],
}

impl<'de, K: DeserializeSeed<'de>> DeserializeSeed<'de> for KeySeed<'_, '_, K> {
    type Value = Key<K::Value>;

    fn deserialize<D: serde::Deserializer<'de>>(self, key: D) -> Result<Self::Value, D::Error> {
        key.deserialize_str(self)
    }
}

impl<'de, K: DeserializeSeed<'de>> Visitor<'de> for KeySeed<'_, '_, K> {
    type Value = Key<K::Value>;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a string key")
    }

    fn visit_str<E: serde::de::Error>(self, key: &str) -> Result<Self::Value, E> {
        if self.skipped.iter().any(|skipped| skipped == key) {
            return Ok(Key::Skipped);
        }
        let seed = self.seed.take().expect("a key is read once");
        let key: StrDeserializer<E> = key.into_deserializer();
        seed.deserialize(key).map(Key::Read)
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
