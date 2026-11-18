//! The `slipmesh:` block that opens every document of format version 2: what the document is,
//! which nodes it reaches and where it stands among the others.

use super::Name;
use serde::de::{self, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use std::fmt;

/// Who configures a node, and whose terms a document is written in.
#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Talos,
    Routeros,
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Talos => "talos",
            Self::Routeros => "routeros",
        })
    }
}

/// Where a document stands among all the documents read: lower first, `1000` when left out.
#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Order(u16);

impl Default for Order {
    fn default() -> Self {
        Self(1000)
    }
}

/// The group nobody defines: every node, and for a document of one platform every node of it.
pub const ALL: &str = "all";

/// The word that stands for no node in `exclude`.
pub const NONE: &str = "none";

/// The names a document's `include` lists: nodes, groups and `all`. Left out, and written as the
/// word `all`, it is the list of `all` alone. Never empty: `include: []` would reach no node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Include(Vec<Name>);

/// The names a document's `exclude` lists. Left out, and written as the word `none`, it is empty.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Exclude(Vec<Name>);

impl Include {
    /// `names` as an `include`; never empty, or it would reach no node.
    pub(super) fn listing(names: Vec<Name>) -> Self {
        debug_assert!(!names.is_empty());
        Self(names)
    }

    pub fn names(&self) -> &[Name] {
        &self.0
    }

    /// Whether the document is for all: its `include` names nothing but `all`.
    pub fn is_all(&self) -> bool {
        self.0.iter().all(|name| name.as_str() == ALL)
    }
}

impl Default for Include {
    fn default() -> Self {
        Self(vec![Name::all()])
    }
}

impl Exclude {
    pub fn names(&self) -> &[Name] {
        &self.0
    }
}

/// A list of names, or the one word that may stand in its place.
struct Names {
    word: &'static str,
}

impl<'de> Visitor<'de> for Names {
    type Value = Option<Vec<Name>>;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "a list of names, or the word `{}`", self.word)
    }

    /// `None` for the word.
    fn visit_str<E: de::Error>(self, text: &str) -> Result<Self::Value, E> {
        if text == self.word {
            Ok(None)
        } else {
            Err(E::invalid_value(de::Unexpected::Str(text), &self))
        }
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut names: A) -> Result<Self::Value, A::Error> {
        let mut list = Vec::new();
        while let Some(name) = names.next_element()? {
            list.push(name);
        }
        Ok(Some(list))
    }
}

impl<'de> Deserialize<'de> for Include {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match deserializer.deserialize_any(Names { word: ALL })? {
            None => Ok(Self::default()),
            Some(names) if names.is_empty() => Err(de::Error::custom(
                "`include: []` reaches no node - leave `include` out to reach every node",
            )),
            Some(names) => Ok(Self(names)),
        }
    }
}

impl<'de> Deserialize<'de> for Exclude {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer
            .deserialize_any(Names { word: NONE })
            .map(|names| Self(names.unwrap_or_default()))
    }
}

/// A `slipmesh:` block, one variant for each kind with the keys that kind takes: a key of
/// another kind is an unknown one, so a misspelled `include` cannot aim a document at every
/// node.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(
    remote = "Self",
    tag = "kind",
    rename_all = "lowercase",
    deny_unknown_fields
)]
pub enum Block {
    /// The values of `version` and `revision` are the loader's to check against the ones its
    /// tool is built for.
    Network {
        version: u32,
        revision: u32,
        #[serde(default)]
        order: Order,
    },
    Groups {
        #[serde(default)]
        order: Order,
    },
    Platform {
        platform: Platform,
        #[serde(default)]
        order: Order,
    },
    Template {
        platform: Platform,
        #[serde(default)]
        include: Include,
        #[serde(default)]
        exclude: Exclude,
        #[serde(default)]
        order: Order,
    },
    Patch {
        platform: Platform,
        #[serde(default)]
        include: Include,
        #[serde(default)]
        exclude: Exclude,
        #[serde(default)]
        order: Order,
    },
}

from_a_mapping!(Block);

impl Block {
    pub fn order(&self) -> Order {
        match self {
            Self::Network { order, .. }
            | Self::Groups { order }
            | Self::Platform { order, .. }
            | Self::Template { order, .. }
            | Self::Patch { order, .. } => *order,
        }
    }
}

/// Which of two documents of one kind that reach one node is laid over the other, or chosen: the
/// greater over the lesser, and of two equal ones the later read. `order` decides first; among
/// documents of one `order`, one aimed at some nodes ranks above one for all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Rank {
    order: Order,
    narrower: bool,
}

impl Rank {
    pub fn of(order: Order, include: &Include) -> Self {
        Self {
            order,
            narrower: !include.is_all(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(yaml: &str) -> Result<Block, String> {
        sliplib_yaml::from_str(yaml).map_err(|e| e.to_string())
    }

    fn names(names: &[Name]) -> Vec<&str> {
        names.iter().map(Name::as_str).collect()
    }

    #[test]
    fn every_kind_reads_with_the_keys_it_takes() {
        assert_eq!(
            block("{kind: network, version: 2, revision: 1}").unwrap(),
            Block::Network {
                version: 2,
                revision: 1,
                order: Order(1000)
            }
        );
        assert_eq!(
            block("{kind: groups, order: 5}").unwrap(),
            Block::Groups { order: Order(5) }
        );
        assert_eq!(
            block("{kind: platform, platform: talos}").unwrap(),
            Block::Platform {
                platform: Platform::Talos,
                order: Order(1000)
            }
        );
        for kind in ["template", "patch"] {
            let read = block(&format!(
                "{{kind: {kind}, platform: talos, include: [workers], exclude: [node-c]}}"
            ))
            .unwrap();
            let (Block::Template {
                include, exclude, ..
            }
            | Block::Patch {
                include, exclude, ..
            }) = read
            else {
                panic!("{read:?}");
            };
            assert_eq!(names(include.names()), ["workers"], "{kind}");
            assert_eq!(names(exclude.names()), ["node-c"], "{kind}");
        }
    }

    #[test]
    fn a_key_its_kind_does_not_take_is_refused() {
        for (yaml, key) in [
            (
                "{kind: network, version: 2, revision: 1, include: [a]}",
                "include",
            ),
            (
                "{kind: network, version: 2, revision: 1, platform: talos}",
                "platform",
            ),
            ("{kind: groups, exclude: [a]}", "exclude"),
            ("{kind: groups, version: 2}", "version"),
            ("{kind: platform, platform: talos, include: [a]}", "include"),
            ("{kind: patch, platform: talos, revision: 1}", "revision"),
            ("{kind: template, platform: talos, inculde: [a]}", "inculde"),
            ("{kind: groups, platform: talos}", "platform"),
            (
                "{kind: network, version: 2, revision: 1, exclude: [a]}",
                "exclude",
            ),
            ("{kind: platform, platform: talos, exclude: [a]}", "exclude"),
        ] {
            let refused = block(yaml).unwrap_err();
            assert!(
                refused.contains(&format!("unknown field `{key}`")),
                "{yaml}: {refused}"
            );
        }
    }

    #[test]
    fn a_block_is_a_mapping_and_one_written_as_a_list_is_refused() {
        for list in [
            "[network, 2, 1]",
            "[groups]",
            "[platform, talos]",
            "[template, talos]",
            "[patch, talos]",
        ] {
            let refused = block(list).unwrap_err();
            assert!(refused.contains("a mapping"), "{list}: {refused}");
        }
    }

    #[test]
    fn a_block_without_a_kind_is_refused() {
        let refused = block("{platform: talos}").unwrap_err();
        assert!(refused.contains("missing field `kind`"), "{refused}");
    }

    #[test]
    fn a_key_its_kind_requires_is_missed() {
        for (yaml, key) in [
            ("{kind: network, revision: 1}", "version"),
            ("{kind: network, version: 2}", "revision"),
            ("{kind: platform}", "platform"),
            ("{kind: template}", "platform"),
            ("{kind: patch, include: [a]}", "platform"),
        ] {
            let refused = block(yaml).unwrap_err();
            assert!(
                refused.contains(&format!("missing field `{key}`")),
                "{yaml}: {refused}"
            );
        }
    }

    #[test]
    fn a_kind_or_a_platform_the_format_does_not_have_is_refused() {
        assert!(
            block("{kind: roadwarriors}")
                .unwrap_err()
                .contains("roadwarriors")
        );
        assert!(
            block("{kind: patch, platform: openwrt}")
                .unwrap_err()
                .contains("openwrt")
        );
    }

    #[test]
    fn a_platform_reads_as_the_files_write_it() {
        assert_eq!(Platform::Talos.to_string(), "talos");
        assert_eq!(Platform::Routeros.to_string(), "routeros");
    }

    #[test]
    fn an_order_is_a_whole_number_up_to_65535() {
        assert_eq!(block("{kind: groups, order: 0}").unwrap().order(), Order(0));
        assert_eq!(
            block("{kind: groups, order: 65535}").unwrap().order(),
            Order(65535)
        );
        for order in ["65536", "-1", "1.5", "first"] {
            let refused = block(&format!("{{kind: groups, order: {order}}}")).unwrap_err();
            assert!(refused.contains("u16"), "{order}: {refused}");
        }
    }

    #[test]
    fn an_order_left_out_is_1000_on_every_kind_that_is_aimed() {
        for kind in ["template", "patch"] {
            let read = block(&format!("{{kind: {kind}, platform: talos}}")).unwrap();
            assert_eq!(read.order(), Order(1000), "{kind}");
        }
    }

    /// The block as the loader takes it: out of its document as a value.
    fn block_from_value(yaml: &str) -> Block {
        let value: sliplib_yaml::Value = sliplib_yaml::from_str(yaml).unwrap();
        sliplib_yaml::from_value(value).unwrap()
    }

    #[test]
    fn a_name_yaml_reads_as_a_number_or_a_boolean_is_the_name_written() {
        let yaml = "{kind: patch, platform: talos, include: [7, 1.5], exclude: [true]}";
        for read in [block(yaml).unwrap(), block_from_value(yaml)] {
            let Block::Patch {
                include, exclude, ..
            } = read
            else {
                panic!("{read:?}");
            };
            assert_eq!(names(include.names()), ["7", "1.5"]);
            assert_eq!(names(exclude.names()), ["true"]);
        }
    }

    #[test]
    fn a_negative_number_is_refused_by_the_name_rule() {
        let refused = block("{kind: patch, platform: talos, include: [-7]}").unwrap_err();
        assert!(
            refused.contains("has to start and end with a letter or a digit"),
            "{refused}"
        );
    }

    #[test]
    fn a_name_left_empty_is_refused() {
        let refused = block("{kind: patch, platform: talos, include: [node-a, null]}").unwrap_err();
        assert!(refused.contains("a name"), "{refused}");
    }

    fn aim(keys: &str) -> (Include, Exclude) {
        match block(&format!("{{kind: patch, platform: talos{keys}}}")).unwrap() {
            Block::Patch {
                include, exclude, ..
            } => (include, exclude),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn include_left_out_or_written_as_the_word_is_all() {
        for keys in ["", ", include: all", ", include: [all]"] {
            let (include, _) = aim(keys);
            assert_eq!(names(include.names()), ["all"], "{keys}");
            assert!(include.is_all(), "{keys}");
        }
        let (include, _) = aim(", include: [all, node-a]");
        assert!(!include.is_all());
        let (include, _) = aim(", include: [node-a]");
        assert!(!include.is_all());
    }

    #[test]
    fn exclude_left_out_or_written_as_the_word_is_none() {
        for keys in ["", ", exclude: none", ", exclude: []"] {
            let (_, exclude) = aim(keys);
            assert!(exclude.names().is_empty(), "{keys}");
        }
        let (_, exclude) = aim(", exclude: [node-a, workers]");
        assert_eq!(names(exclude.names()), ["node-a", "workers"]);
    }

    #[test]
    fn an_empty_include_and_the_wrong_word_are_refused() {
        for (keys, said) in [
            (", include: []", "reaches no node"),
            (", include: none", "`all`"),
            (", exclude: all", "`none`"),
            (", include: 5", "`all`"),
            (", include: [node a]", "node a"),
        ] {
            let refused = block(&format!("{{kind: patch, platform: talos{keys}}}")).unwrap_err();
            assert!(refused.contains(said), "{keys}: {refused}");
        }
    }

    #[test]
    fn order_ranks_before_how_narrowly_a_document_is_aimed() {
        let (all, _) = aim("");
        let (some, _) = aim(", include: [node-a]");
        let rank = |order, include| Rank::of(Order(order), include);
        assert!(rank(1000, &some) > rank(1000, &all));
        assert!(rank(1000, &all) > rank(999, &some));
        assert_eq!(rank(7, &all), rank(7, &aim(", include: [all]").0));
    }
}
