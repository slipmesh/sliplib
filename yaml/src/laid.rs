use std::fmt;

use serde::de::{
    DeserializeOwned, DeserializeSeed, Deserializer, IgnoredAny, MapAccess, SeqAccess, Visitor,
};
use serde_path_to_error::Segment;
use yaml_serde::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    Key(String),
    Index(usize),
}

#[derive(Debug, thiserror::Error)]
#[error("`{path}`")]
pub struct Refused {
    path: serde_path_to_error::Path,
    #[source]
    source: yaml_serde::Error,
}

impl Refused {
    /// Up to the first step that is neither a key nor an item.
    pub fn path(&self) -> Vec<Step> {
        self.path
            .iter()
            .map_while(|segment| match segment {
                Segment::Map { key } => Some(Step::Key(key.clone())),
                Segment::Seq { index } => Some(Step::Index(*index)),
                Segment::Enum { .. } | Segment::Unknown => None,
            })
            .collect()
    }
}

/// For a value laid from several texts, which has no line of its own.
pub fn from_value_at<T: DeserializeOwned>(value: Value) -> Result<T, Refused> {
    serde_path_to_error::deserialize(value).map_err(|refused| Refused {
        path: refused.path().clone(),
        source: refused.into_inner(),
    })
}

/// The last layer holding, not `null`, the deepest part of `path` any layer holds, and how many
/// steps that part is.
pub fn written_by(layers: &[&Value], path: &[Step]) -> Option<(usize, usize)> {
    (0..=path.len()).rev().find_map(|depth| {
        layers
            .iter()
            .rposition(|layer| at(layer, &path[..depth]).is_some_and(|value| !value.is_null()))
            .map(|layer| (layer, depth))
    })
}

fn at<'a>(value: &'a Value, path: &[Step]) -> Option<&'a Value> {
    path.iter().try_fold(value, |value, step| match step {
        Step::Key(name) => value
            .as_mapping()?
            .iter()
            .find_map(|(key, value)| is_named(key, name).then_some(value)),
        Step::Index(index) => value.as_sequence()?.get(*index),
    })
}

/// A key YAML reads as a number or a boolean is named by its text.
fn is_named(key: &Value, name: &str) -> bool {
    match key {
        Value::String(key) => key == name,
        Value::Number(key) => key.to_string() == name,
        Value::Bool(key) => key.to_string() == name,
        _ => false,
    }
}

pub(crate) fn line_at(source: &str, index: usize, path: &[Step]) -> Option<usize> {
    let document = yaml_serde::Deserializer::from_str(source).nth(index)?;
    let stop = Walk(path).deserialize(document).err()?;
    stop.location().map(|location| location.line())
}

/// Down `path`, refusing at its last step for the parser to tell where that stands.
struct Walk<'p>(&'p [Step]);

impl<'de> DeserializeSeed<'de> for Walk<'_> {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, value: D) -> Result<(), D::Error> {
        match self.0.first() {
            Some(Step::Key(_)) => value.deserialize_map(self),
            Some(Step::Index(_)) => value.deserialize_seq(self),
            None => value.deserialize_any(crate::multidoc::Nothing).map(|_| ()),
        }
    }
}

impl<'de> Visitor<'de> for Walk<'_> {
    type Value = ();

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the path")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut entries: A) -> Result<(), A::Error> {
        let Some((Step::Key(name), rest)) = self.0.split_first() else {
            return Ok(());
        };
        while let Some(found) = entries.next_key_seed(Key {
            name,
            last: rest.is_empty(),
        })? {
            if found {
                entries.next_value_seed(Walk(rest))?;
            } else {
                entries.next_value::<IgnoredAny>()?;
            }
        }
        Ok(())
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut items: A) -> Result<(), A::Error> {
        let Some((Step::Index(index), rest)) = self.0.split_first() else {
            return Ok(());
        };
        // Read to the end, as the parser has to be for the document to end.
        for position in 0.. {
            let item = if position == *index {
                items.next_element_seed(Walk(rest))?
            } else {
                items.next_element::<IgnoredAny>()?.map(|_| ())
            };
            if item.is_none() {
                break;
            }
        }
        Ok(())
    }
}

/// Whether a key is `name`; refused where it is the last step, so the error carries its line.
struct Key<'n> {
    name: &'n str,
    last: bool,
}

impl Key<'_> {
    fn found<E: serde::de::Error>(&self, key: Value) -> Result<bool, E> {
        let found = is_named(&key, self.name);
        if found && self.last {
            return Err(E::custom("the key looked for"));
        }
        Ok(found)
    }
}

impl<'de> DeserializeSeed<'de> for Key<'_> {
    type Value = bool;

    fn deserialize<D: Deserializer<'de>>(self, key: D) -> Result<bool, D::Error> {
        key.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Key<'_> {
    type Value = bool;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a key")
    }

    fn visit_str<E: serde::de::Error>(self, key: &str) -> Result<bool, E> {
        self.found(key.into())
    }

    fn visit_u64<E: serde::de::Error>(self, key: u64) -> Result<bool, E> {
        self.found(key.into())
    }

    fn visit_i64<E: serde::de::Error>(self, key: i64) -> Result<bool, E> {
        self.found(key.into())
    }

    fn visit_bool<E: serde::de::Error>(self, key: bool) -> Result<bool, E> {
        self.found(key.into())
    }
}
