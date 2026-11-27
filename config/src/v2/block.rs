//! The `slipmesh:` block of every document: its kind, aim and order.

use super::{META_KEY, Name};
use serde::de::{self, DeserializeOwned, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use sliplib_yaml::{Document, Value, Without};
use std::fmt;
use std::marker::PhantomData;

/// Whose tool configures a node, and whose terms a document is in.
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

/// Lower first; `1000` left out.
#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Order(u16);

impl Default for Order {
    fn default() -> Self {
        Self(1000)
    }
}

/// Every node, or for a document of one platform every node of it.
pub const ALL: &str = "all";

/// No node, in `exclude`.
pub const NONE: &str = "none";

/// Nodes, groups and `all`; left out or written `all`, `all` alone. Never empty, or it would reach
/// no node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Include(Vec<Name>);

/// Left out or written `none`, empty.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Exclude(Vec<Name>);

impl Include {
    /// Never empty.
    pub(super) fn listing(names: Vec<Name>) -> Self {
        debug_assert!(!names.is_empty());
        Self(names)
    }

    pub fn names(&self) -> &[Name] {
        &self.0
    }

    /// Nothing but `all`.
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

/// Or the one word that may stand in their place.
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

/// One variant per kind, with its keys: a misspelled `include` is unknown, not a document aimed at
/// every node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Network(NetworkBlock),
    Groups(GroupsBlock),
    Platform(PlatformBlock),
    Template(Aimed),
    Patch(Aimed),
    Vars(VarsBlock),
}

/// The loader checks `version` and `revision` against its own.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct NetworkBlock {
    pub version: u32,
    pub revision: u32,
    #[serde(default)]
    pub order: Order,
}

#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GroupsBlock {
    #[serde(default)]
    pub order: Order,
}

#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PlatformBlock {
    pub platform: Platform,
    #[serde(default)]
    pub order: Order,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum Kind {
    Network,
    Groups,
    Platform,
    Template,
    Patch,
    Vars,
}

impl Block {
    /// `kind` first, then that kind's keys: a key the kind does not take is refused as that,
    /// whatever it holds.
    pub(super) fn read(document: &Document) -> Result<Self, sliplib_yaml::Error> {
        let Header(KindOf(kind)) = document.deserialize()?;
        Ok(match kind {
            Kind::Network => Self::Network(read_as(document)?),
            Kind::Groups => Self::Groups(read_as(document)?),
            Kind::Platform => Self::Platform(read_as(document)?),
            Kind::Template => Self::Template(read_as(document)?),
            Kind::Patch => Self::Patch(read_as(document)?),
            Kind::Vars => Self::Vars(read_as(document)?),
        })
    }
}

/// The block as its kind's own type.
fn read_as<T: DeserializeOwned>(document: &Document) -> Result<T, sliplib_yaml::Error> {
    document
        .deserialize()
        .map(|Header(WithoutKind(block))| block)
}

/// The block, every other key passed over whatever its type.
struct Header<T>(T);

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Header<T> {
    fn deserialize<D: Deserializer<'de>>(document: D) -> Result<Self, D::Error> {
        document.deserialize_map(HeaderKeys(PhantomData))
    }
}

struct HeaderKeys<T>(PhantomData<T>);

impl<'de, T: Deserialize<'de>> Visitor<'de> for HeaderKeys<T> {
    type Value = Header<T>;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a mapping")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut keys: A) -> Result<Header<T>, A::Error> {
        let mut block = None;
        while let Some(key) = keys.next_key::<Value>()? {
            if key.as_str() == Some(META_KEY) {
                block = Some(keys.next_value()?);
            } else {
                keys.next_value::<de::IgnoredAny>()?;
            }
        }
        block
            .map(Header)
            .ok_or_else(|| de::Error::missing_field(META_KEY))
    }
}

/// From a mapping alone: serde's derive reads a struct from a list too.
struct KindOf(Kind);

impl<'de> Deserialize<'de> for KindOf {
    fn deserialize<D: Deserializer<'de>>(block: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct JustKind {
            kind: Kind,
        }
        block
            .deserialize_map(Without::<JustKind>::new(&[]))
            .map(|just| Self(just.kind))
    }
}

/// `kind` passed over: no kind's type holds it.
struct WithoutKind<T>(T);

impl<'de, T: Deserialize<'de>> Deserialize<'de> for WithoutKind<T> {
    fn deserialize<D: Deserializer<'de>>(block: D) -> Result<Self, D::Error> {
        block.deserialize_map(Without::new(&["kind"])).map(Self)
    }
}

/// Of a `template` or a `patch`.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Aimed {
    pub platform: Platform,
    #[serde(default)]
    pub include: Include,
    #[serde(default)]
    pub exclude: Exclude,
    #[serde(default)]
    pub order: Order,
}

/// Without `platform`, for the nodes of every platform.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct VarsBlock {
    #[serde(default)]
    pub platform: Option<Platform>,
    #[serde(default)]
    pub include: Include,
    #[serde(default)]
    pub exclude: Exclude,
    #[serde(default)]
    pub order: Order,
}

impl Block {
    pub fn order(&self) -> Order {
        match self {
            Self::Network(NetworkBlock { order, .. })
            | Self::Groups(GroupsBlock { order })
            | Self::Platform(PlatformBlock { order, .. })
            | Self::Template(Aimed { order, .. })
            | Self::Patch(Aimed { order, .. })
            | Self::Vars(VarsBlock { order, .. }) => *order,
        }
    }
}

/// How far a document reaches, the widest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Scope {
    Network,
    Platform,
    Nodes,
}

/// The greater is laid over or chosen, ties to the later read: `order`, then the narrower scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Rank {
    order: Order,
    scope: Scope,
}

impl Rank {
    pub fn of(order: Order, platform: Option<Platform>, include: &Include) -> Self {
        let scope = match (platform, include.is_all()) {
            (_, false) => Scope::Nodes,
            (Some(_), true) => Scope::Platform,
            (None, true) => Scope::Network,
        };
        Self { order, scope }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sliplib_yaml::MultiDoc;

    fn block(yaml: &str) -> Result<Block, String> {
        let file = MultiDoc::parse(&format!("{META_KEY}: {yaml}\n")).unwrap();
        Block::read(&file.documents()[0]).map_err(|e| e.to_string())
    }

    fn names(names: &[Name]) -> Vec<&str> {
        names.iter().map(Name::as_str).collect()
    }

    #[test]
    fn every_kind_reads_with_the_keys_it_takes() {
        assert_eq!(
            block("{kind: network, version: 2, revision: 1}").unwrap(),
            Block::Network(NetworkBlock {
                version: 2,
                revision: 1,
                order: Order(1000)
            })
        );
        assert_eq!(
            block("{kind: groups, order: 5}").unwrap(),
            Block::Groups(GroupsBlock { order: Order(5) })
        );
        assert_eq!(
            block("{kind: platform, platform: talos}").unwrap(),
            Block::Platform(PlatformBlock {
                platform: Platform::Talos,
                order: Order(1000)
            })
        );
        assert_eq!(
            block("{kind: vars}").unwrap(),
            Block::Vars(VarsBlock {
                platform: None,
                include: Include::default(),
                exclude: Exclude::default(),
                order: Order(1000)
            })
        );
        let Block::Vars(VarsBlock {
            platform, include, ..
        }) = block("{kind: vars, platform: talos, include: [workers]}").unwrap()
        else {
            panic!("not vars");
        };
        assert_eq!(platform, Some(Platform::Talos));
        assert_eq!(names(include.names()), ["workers"]);
        for kind in ["template", "patch"] {
            let read = block(&format!(
                "{{kind: {kind}, platform: talos, include: [workers], exclude: [node-c]}}"
            ))
            .unwrap();
            let (Block::Template(Aimed {
                include, exclude, ..
            })
            | Block::Patch(Aimed {
                include, exclude, ..
            })) = read
            else {
                panic!("{read:?}");
            };
            assert_eq!(names(include.names()), ["workers"], "{kind}");
            assert_eq!(names(exclude.names()), ["node-c"], "{kind}");
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
    fn a_block_without_a_kind_is_refused_whatever_its_other_keys_hold() {
        for yaml in ["{platform: talos}", "{include: []}"] {
            let refused = block(yaml).unwrap_err();
            assert!(
                refused.contains("missing field `kind`"),
                "{yaml}: {refused}"
            );
        }
    }

    #[test]
    fn a_key_its_kind_does_not_take_is_refused_whatever_it_holds() {
        for (yaml, key) in [
            ("{kind: groups, include: []}", "include"),
            (
                "{kind: network, version: 2, revision: 1, platform: mikrotik}",
                "platform",
            ),
            ("{kind: platform, platform: talos, version: abc}", "version"),
            (
                "{kind: template, platform: talos, revision: -1}",
                "revision",
            ),
            ("{kind: patch, platform: talos, version: []}", "version"),
            ("{kind: vars, version: 2}", "version"),
            ("{kind: groups, 0: 5}", "0"),
        ] {
            let refused = block(yaml).unwrap_err();
            assert!(
                refused.contains(&format!("unknown field `{key}`")),
                "{yaml}: {refused}"
            );
        }
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
    fn a_platform_reads_as_the_files_write_it() {
        assert_eq!(Platform::Talos.to_string(), "talos");
        assert_eq!(Platform::Routeros.to_string(), "routeros");
    }

    fn aim(keys: &str) -> (Include, Exclude) {
        match block(&format!("{{kind: patch, platform: talos{keys}}}")).unwrap() {
            Block::Patch(Aimed {
                include, exclude, ..
            }) => (include, exclude),
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
}
