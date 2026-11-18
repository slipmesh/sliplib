//! `slipmesh.yaml` in format version 2, as `docs/slipmesh-yaml.md` describes it.

/// Implements `Deserialize` for `$type`, whose `#[serde(remote = "Self")]` derive gives the reading
/// of its keys, so that it is read from a mapping alone. serde reads an internally tagged enum
/// through a buffer that takes a list as well, by position, and the format writes none as one.
macro_rules! from_a_mapping {
    ($type:ty) => {
        impl<'de> serde::Deserialize<'de> for $type {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                struct Keys;

                impl<'de> serde::de::Visitor<'de> for Keys {
                    type Value = $type;

                    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        f.write_str("a mapping")
                    }

                    fn visit_map<A: serde::de::MapAccess<'de>>(
                        self,
                        keys: A,
                    ) -> Result<$type, A::Error> {
                        <$type>::deserialize(serde::de::value::MapAccessDeserializer::new(keys))
                    }
                }

                deserializer.deserialize_map(Keys)
            }
        }
    };
}

mod block;
pub mod groups;
mod name;

pub use block::{ALL, Block, Exclude, Include, NONE, Order, Platform, Rank};
pub use name::{ClassName, Name, NameError};

use serde::de::{self, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
#[cfg(test)]
use sliplib_yaml::Value;
use std::borrow::Borrow;
use std::fmt;
use std::hash::Hash;
use std::marker::PhantomData;

/// What is named, by its name, in the order written: the order reaches the nodes' configs.
pub type Map<K, V> = indexmap::IndexMap<K, V>;

/// Reads a mapping into a [`Map`], refusing a key that stands twice. YAML reads `1.0` and `1`, or
/// `True` and `true`, as one value: a mapping can hold one name under two spellings, and the
/// later entry would replace the earlier.
pub(crate) fn unique_keys<'de, D, K, V>(deserializer: D) -> Result<Map<K, V>, D::Error>
where
    D: Deserializer<'de>,
    K: Deserialize<'de> + Hash + Eq + Borrow<str>,
    V: Deserialize<'de>,
{
    struct Entries<K, V>(PhantomData<(K, V)>);

    impl<'de, K, V> Visitor<'de> for Entries<K, V>
    where
        K: Deserialize<'de> + Hash + Eq + Borrow<str>,
        V: Deserialize<'de>,
    {
        type Value = Map<K, V>;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a mapping")
        }

        fn visit_map<A: MapAccess<'de>>(self, mut entries: A) -> Result<Map<K, V>, A::Error> {
            let mut map = Map::new();
            while let Some((key, value)) = entries.next_entry::<K, V>()? {
                if map.contains_key(&key) {
                    let name: &str = key.borrow();
                    return Err(de::Error::custom(NameError::Twice(name.to_owned())));
                }
                map.insert(key, value);
            }
            Ok(map)
        }
    }

    deserializer.deserialize_map(Entries(PhantomData))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(yaml: &str) -> Result<Vec<String>, String> {
        let read = sliplib_yaml::from_str::<Value>(yaml).map_err(|e| e.to_string())?;
        let map: Map<Name, Value> = unique_keys(read).map_err(|e| e.to_string())?;
        Ok(map.keys().map(Name::to_string).collect())
    }

    #[derive(Deserialize, Debug)]
    #[serde(transparent)]
    struct Named(#[serde(deserialize_with = "unique_keys")] Map<Name, Value>);

    // Read from the text, where the reader leaves no duplicate behind it, and from a value.
    #[test]
    fn two_keys_that_read_as_one_name_are_refused_naming_it() {
        for (yaml, name) in [
            ("{True: a, true: b}", "true"),
            ("{1.0: a, 1: b}", "1"),
            ("{1e3: a, 1000: b}", "1000"),
        ] {
            let refused = sliplib_yaml::from_str::<Named>(yaml)
                .unwrap_err()
                .to_string();
            assert!(
                refused.starts_with(&NameError::Twice(name.to_owned()).to_string()),
                "{yaml}: {refused}"
            );
        }
        let from_a_value = names("{1.0: a, 1: b}").unwrap_err();
        assert!(
            from_a_value.starts_with(&NameError::Twice("1".to_owned()).to_string()),
            "{from_a_value}"
        );
    }

    #[test]
    fn keys_of_their_own_keep_the_order_written() {
        let yaml = "{b: 1, 07: 2, a: 3, 7: 4}";
        let from_text = sliplib_yaml::from_str::<Named>(yaml).unwrap();
        let from_text: Vec<&str> = from_text.0.keys().map(Name::as_str).collect();
        assert_eq!(from_text, ["b", "07", "a", "7"]);
        assert_eq!(names(yaml).unwrap(), ["b", "07", "a", "7"]);
    }

    #[test]
    fn two_class_names_that_read_as_one_are_refused_naming_it() {
        let read = sliplib_yaml::from_str::<Value>("{1.0: a, 1: b}").unwrap();
        let refused = unique_keys::<_, ClassName, Value>(read)
            .unwrap_err()
            .to_string();
        assert!(
            refused.starts_with(&NameError::Twice("1".to_owned()).to_string()),
            "{refused}"
        );
    }
}
