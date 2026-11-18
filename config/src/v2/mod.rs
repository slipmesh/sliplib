//! `slipmesh.yaml` in format version 2, as `docs/slipmesh-yaml.md` describes it.

/// Implements `Deserialize` for `$type`, whose `#[serde(remote = "Self")]` derive gives the reading
/// of its keys, so that it is read from a mapping alone. serde reads a struct, and an internally
/// tagged enum, from a list as well, by position, and the format writes neither as one.
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
