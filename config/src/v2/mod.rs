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
mod check;
pub mod groups;
mod name;
pub mod network;
pub mod platform;
#[cfg(test)]
mod testing;

pub use block::{ALL, Block, Exclude, Include, NONE, Order, Platform, Rank};
pub use check::Invalid;
pub use name::{ClassName, Name, NameError};

/// What is named, by its name, in the order written: the order reaches the nodes' configs.
pub type Map<K, V> = indexmap::IndexMap<K, V>;

/// Reads a mapping into a [`Map`], refusing a key that stands twice. YAML reads `1.0` and `1`, or
/// `True` and `true`, as one value: a mapping can hold one name under two spellings, and the
/// later entry would replace the earlier.
pub(crate) fn unique_keys<'de, D, K, V>(deserializer: D) -> Result<Map<K, V>, D::Error>
where
    D: serde::Deserializer<'de>,
    K: serde::Deserialize<'de> + std::hash::Hash + Eq + std::fmt::Debug,
    V: serde::Deserialize<'de>,
{
    struct Entries<K, V>(std::marker::PhantomData<(K, V)>);

    impl<'de, K, V> serde::de::Visitor<'de> for Entries<K, V>
    where
        K: serde::Deserialize<'de> + std::hash::Hash + Eq + std::fmt::Debug,
        V: serde::Deserialize<'de>,
    {
        type Value = Map<K, V>;

        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a mapping")
        }

        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut entries: A,
        ) -> Result<Map<K, V>, A::Error> {
            let mut map = Map::with_capacity(entries.size_hint().unwrap_or(0));
            while let Some((key, value)) = entries.next_entry::<K, V>()? {
                if map.contains_key(&key) {
                    return Err(serde::de::Error::custom(format!(
                        "the name {key:?} stands twice among the keys of one mapping, written in two ways YAML reads as one value - give each its own name, or quote both as written"
                    )));
                }
                map.insert(key, value);
            }
            Ok(map)
        }
    }

    deserializer.deserialize_map(Entries(std::marker::PhantomData))
}

/// A [`Map`] read by [`unique_keys`], for a field that also takes `null` as the map left out.
pub(crate) struct UniqueKeys<K, V>(pub Map<K, V>);

impl<'de, K, V> serde::Deserialize<'de> for UniqueKeys<K, V>
where
    K: serde::Deserialize<'de> + std::hash::Hash + Eq + std::fmt::Debug,
    V: serde::Deserialize<'de>,
{
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        unique_keys(deserializer).map(Self)
    }
}
