use std::fmt;
use std::marker::PhantomData;
use std::path::Path;
use std::sync::Arc;

use serde::de::value::{MapAccessDeserializer, StrDeserializer};
use serde::de::{
    DeserializeOwned, DeserializeSeed, Deserializer as _, IgnoredAny, IntoDeserializer, MapAccess,
    Visitor,
};
use serde::{Deserialize, Serialize};
use yaml_serde::{Deserializer, Serializer, Value};

use crate::error::{Error, Result};

/// Written back without its comments and documents of only comments.
pub struct MultiDoc {
    documents: Vec<Document>,
}

impl MultiDoc {
    pub fn parse(text: &str) -> Result<Self> {
        let source: Arc<str> = text.into();
        let mut documents = Vec::new();
        for (index, document) in Deserializer::from_str(text).enumerate() {
            let value = Value::deserialize(document).map_err(Error::NotYaml)?;
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
        crate::file::reading(path, Self::parse)
    }

    /// Those that hold more than comments, in file order.
    pub fn documents(&self) -> &[Document] {
        &self.documents
    }

    /// None for a document of only comments.
    pub fn document_mut(&mut self, index: usize) -> Option<&mut Value> {
        let document = self.documents.iter_mut().find(|d| d.index == index)?;
        document.source = None;
        Some(&mut document.value)
    }

    pub fn text(&self) -> Result<String> {
        let mut text = Vec::new();
        let mut serializer = Serializer::new(&mut text);
        for document in &self.documents {
            document
                .value
                .serialize(&mut serializer)
                .map_err(|source| Error::WritingDocument {
                    line: document.line,
                    source,
                })?;
        }
        drop(serializer);
        String::from_utf8(text).map_err(Error::NotUtf8)
    }
}

#[derive(Clone)]
pub struct Document {
    /// Among the file's documents, those of only comments included.
    pub index: usize,
    /// Where its content starts.
    pub line: usize,
    pub value: Value,
    /// While the document is as read.
    source: Option<Arc<str>>,
    taken: Vec<String>,
}

impl Document {
    /// A top-level key, out of `value` and of what [`Document::deserialize`] reads.
    pub fn take(&mut self, key: &str) -> Option<Value> {
        let value = self.value.as_mapping_mut()?.shift_remove(key)?;
        self.taken.push(key.to_owned());
        Some(value)
    }

    /// As read, an error names the field and its line.
    pub fn deserialize<T: DeserializeOwned>(&self) -> Result<T> {
        let Some(source) = &self.source else {
            return Ok(yaml_serde::from_value(self.value.clone())?);
        };
        let document = Deserializer::from_str(source)
            .nth(self.index)
            .ok_or(Error::Gone)?;
        Ok(document.deserialize_map(Without {
            keys: &self.taken,
            target: PhantomData,
        })?)
    }
}

/// Where the parser stands when asked for something no YAML value is.
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
        .ok_or(Error::NotKept)?;
    let stop = document
        .deserialize_any(Nothing)
        .expect_err("no YAML value is nothing");
    Ok(stop.location().map_or(1, |location| location.line()))
}

/// A mapping as `T`, less `keys`.
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

/// Read by the parser itself, so an unknown field's error carries its line.
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

pub fn join(documents: &[String]) -> String {
    documents.join("---\n")
}
