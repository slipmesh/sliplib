//! `slipmesh.yaml` of format version 2, as `docs/slipmesh-yaml.md` describes it.

mod block;
mod check;
mod file;
pub mod groups;
mod name;
pub mod network;
#[cfg(test)]
mod testing;

pub use block::{
    ALL, Aimed, Block, Exclude, GroupsBlock, Include, NONE, NetworkBlock, Order, Platform,
    PlatformBlock, Rank,
};
pub use check::Invalid;
pub use file::{
    Aim, AimedDocument, BlockError, Error, File, META_KEY, Place, PlatformDocument, REVISION,
    VERSION,
};
pub(crate) use name::is_name_character;
pub use name::{ClassName, Name, NameError};

use serde::de::{self, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use std::borrow::Borrow;
use std::fmt;
use std::hash::Hash;
use std::marker::PhantomData;

/// In the order written, which reaches the nodes' configs.
pub type Map<K, V> = indexmap::IndexMap<K, V>;

/// An entry written `null` is left out, and a key twice refused: YAML reads `1.0` and `1`, or `True`
/// and `true`, as one, and the later would replace the earlier.
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
            let mut map: Map<K, Option<V>> = Map::new();
            while let Some((key, value)) = entries.next_entry::<K, Option<V>>()? {
                if map.contains_key(&key) {
                    let name: &str = key.borrow();
                    return Err(de::Error::custom(NameError::Twice(name.to_owned())));
                }
                map.insert(key, value);
            }
            Ok(map
                .into_iter()
                .filter_map(|(key, value)| Some((key, value?)))
                .collect())
        }
    }

    deserializer.deserialize_map(Entries(PhantomData))
}

/// `null` reads as the key left out.
pub(crate) fn absent_when_null<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

/// Through [`unique_keys`], for a field that also takes `null` as left out.
pub(crate) struct UniqueKeys<K, V>(pub Map<K, V>);

impl<'de, K, V> Deserialize<'de> for UniqueKeys<K, V>
where
    K: Deserialize<'de> + Hash + Eq + Borrow<str>,
    V: Deserialize<'de>,
{
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        unique_keys(deserializer).map(Self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sliplib_yaml::Value;

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
