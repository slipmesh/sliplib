//! Named sets of nodes to aim documents at, and the one rule turning names into nodes.

use super::Map;
use super::{ALL, Exclude, Include, Name};
use serde::de::value::{MapAccessDeserializer, SeqAccessDeserializer};
use serde::de::{MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use std::fmt;

/// A list of names, short for `include`, or `include` and `exclude`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Group {
    /// Empty, no node.
    Short(Vec<Name>),
    Full(Selection),
}

// Not `untagged`: serde reads a struct from a list too, and an empty list would pass as a
// `Selection` that sets nothing.
impl<'de> Deserialize<'de> for Group {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ListOrKeys;

        impl<'de> Visitor<'de> for ListOrKeys {
            type Value = Group;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a list of names, or a mapping with `include` and `exclude`")
            }

            fn visit_seq<A: SeqAccess<'de>>(self, names: A) -> Result<Group, A::Error> {
                Vec::<Name>::deserialize(SeqAccessDeserializer::new(names)).map(Group::Short)
            }

            fn visit_map<A: MapAccess<'de>>(self, keys: A) -> Result<Group, A::Error> {
                Selection::deserialize(MapAccessDeserializer::new(keys)).map(Group::Full)
            }
        }

        deserializer.deserialize_any(ListOrKeys)
    }
}

#[derive(Deserialize, Debug, Clone, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    #[serde(default)]
    pub include: Include,
    #[serde(default)]
    pub exclude: Exclude,
}

/// By name.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq, Default)]
#[serde(transparent)]
pub struct Groups(#[serde(deserialize_with = "super::unique_keys")] pub Map<Name, Group>);

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum GroupsError {
    #[error("the group {0:?} is named like a node; nodes and groups share their names")]
    NamedLikeANode(Name),
    #[error("no group is named `{0}`: the word is reserved")]
    Reserved(Name),
    #[error("{whose} names {name:?}, which is neither a node nor a group")]
    UnknownName { whose: String, name: Name },
    #[error("the group {0:?} reaches itself through the groups it names: {1}")]
    Cycle(Name, String),
    #[error("{0} is left with no node")]
    Empty(String),
}

type Members = Result<Vec<Name>, GroupsError>;

/// Only [`Groups::resolve`] makes one: without the nodes a selection would reach none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    /// The network's, which the groups were resolved against.
    nodes: Vec<Name>,
    groups: Map<Name, Vec<Name>>,
}

impl Groups {
    /// In the order of `nodes`, every node of the network. A node named `all` or `none` is the
    /// network's checks to refuse; here the name reads as the word.
    pub fn resolve(&self, nodes: &[Name]) -> Result<Resolved, GroupsError> {
        for name in self.0.keys() {
            if name.is_reserved() {
                return Err(GroupsError::Reserved(name.clone()));
            }
            if nodes.contains(name) {
                return Err(GroupsError::NamedLikeANode(name.clone()));
            }
        }
        let mut resolved = Resolved {
            nodes: nodes.to_vec(),
            groups: Map::new(),
        };
        for name in self.0.keys() {
            self.expand(name, nodes, &mut resolved, &mut Vec::new())?;
        }
        Ok(resolved)
    }

    /// Kept in `resolved` once known. Meeting one of `on_the_way` again is a group that reaches
    /// itself.
    fn expand(
        &self,
        name: &Name,
        nodes: &[Name],
        resolved: &mut Resolved,
        on_the_way: &mut Vec<Name>,
    ) -> Result<Vec<Name>, GroupsError> {
        if let Some(known) = resolved.groups.get(name) {
            return Ok(known.clone());
        }
        if let Some(start) = on_the_way.iter().position(|group| group == name) {
            let way: Vec<&str> = on_the_way[start..]
                .iter()
                .chain([name])
                .map(Name::as_str)
                .collect();
            return Err(GroupsError::Cycle(name.clone(), way.join(", ")));
        }
        let whose = format!("the group {:?}", name.as_str());
        let selection = match &self.0[name] {
            Group::Short(names) if names.is_empty() => return Err(GroupsError::Empty(whose)),
            Group::Short(names) => Selection {
                include: Include::listing(names.clone()),
                exclude: Exclude::default(),
            },
            Group::Full(selection) => selection.clone(),
        };
        on_the_way.push(name.clone());
        let members = select(&selection, nodes, nodes, &whose, &mut |group| {
            self.0
                .contains_key(group)
                .then(|| self.expand(group, nodes, resolved, on_the_way))
        })?;
        on_the_way.pop();
        resolved.groups.insert(name.clone(), members.clone());
        Ok(members)
    }
}

/// In the order of `nodes`; `group` turns a group into its nodes.
fn select(
    selection: &Selection,
    nodes: &[Name],
    all: &[Name],
    whose: &str,
    group: &mut dyn FnMut(&Name) -> Option<Members>,
) -> Members {
    let mut listed = |names: &[Name]| -> Members {
        let mut reached = Vec::new();
        for name in names {
            if name.as_str() == ALL {
                reached.extend_from_slice(all);
            } else if nodes.contains(name) {
                reached.push(name.clone());
            } else if let Some(members) = group(name) {
                reached.extend(members?);
            } else {
                return Err(GroupsError::UnknownName {
                    whose: whose.to_owned(),
                    name: name.clone(),
                });
            }
        }
        Ok(reached)
    };
    let included = listed(selection.include.names())?;
    let excluded = listed(selection.exclude.names())?;
    let reached: Vec<Name> = nodes
        .iter()
        .filter(|node| included.contains(node) && !excluded.contains(node))
        .cloned()
        .collect();
    if reached.is_empty() {
        return Err(GroupsError::Empty(whose.to_owned()));
    }
    Ok(reached)
}

impl Resolved {
    /// In the network's order. `all` is what the word stands for where the selection is written;
    /// `whose` names it in a refusal. A node of another platform is the caller's to refuse.
    pub fn reach(
        &self,
        selection: &Selection,
        all: &[Name],
        whose: &str,
    ) -> Result<Vec<Name>, GroupsError> {
        select(selection, &self.nodes, all, whose, &mut |group| {
            self.groups.get(group).cloned().map(Ok)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(names: &[&str]) -> Vec<Name> {
        names
            .iter()
            .map(|name| Name::try_from((*name).to_owned()).unwrap())
            .collect()
    }

    fn nodes() -> Vec<Name> {
        names(&["node-a", "node-b", "node-c", "router1"])
    }

    fn resolved(yaml: &str) -> Result<Resolved, GroupsError> {
        sliplib_yaml::from_str::<Groups>(yaml)
            .unwrap()
            .resolve(&nodes())
    }

    fn group(yaml: &str, name: &str) -> Vec<String> {
        resolved(yaml).unwrap().groups[name]
            .iter()
            .map(Name::to_string)
            .collect()
    }

    const EXAMPLE: &str =
        "{core: [node-a, node-b, node-c], workers: {include: [core], exclude: [node-a]}}";

    #[test]
    fn a_group_is_the_nodes_it_reaches_in_the_order_of_the_network() {
        let nested = "{inner: [node-a, node-b], outer: [inner, node-c], \
                      rest: {exclude: [outer]}, others: {include: all, exclude: [inner]}, \
                      again: [outer, inner, node-a]}";
        for (yaml, name, members) in [
            (
                "{back: [router1, node-c, node-a]}",
                "back",
                &["node-a", "node-c", "router1"][..],
            ),
            (nested, "outer", &["node-a", "node-b", "node-c"]),
            (nested, "rest", &["router1"]),
            (nested, "others", &["node-c", "router1"]),
            (nested, "again", &["node-a", "node-b", "node-c"]),
            ("{outer: [inner], inner: [node-b]}", "outer", &["node-b"]),
        ] {
            assert_eq!(group(yaml, name), members, "{name} of {yaml}");
        }
    }

    #[test]
    fn a_name_that_is_neither_a_node_nor_a_group_is_refused_with_the_group_that_names_it() {
        for yaml in ["{core: [node-a, node-x]}", "{core: {exclude: [node-x]}}"] {
            assert_eq!(
                resolved(yaml),
                Err(GroupsError::UnknownName {
                    whose: "the group \"core\"".to_owned(),
                    name: names(&["node-x"]).remove(0)
                }),
                "{yaml}"
            );
        }
    }

    #[test]
    fn a_group_named_like_a_node_or_by_a_reserved_word_is_refused() {
        assert_eq!(
            resolved("{node-a: [node-b]}"),
            Err(GroupsError::NamedLikeANode(names(&["node-a"]).remove(0)))
        );
        for word in ["all", "none"] {
            assert_eq!(
                resolved(&format!("{{{word}: [node-b]}}")),
                Err(GroupsError::Reserved(names(&[word]).remove(0)))
            );
        }
    }

    #[test]
    fn a_group_that_reaches_itself_is_refused_naming_the_way_round() {
        for (yaml, way) in [
            ("{a: [a]}", "a, a"),
            ("{a: [b], b: [node-a, a]}", "a, b, a"),
            (
                "{a: [node-a], b: {include: all, exclude: [c]}, c: [b]}",
                "b, c, b",
            ),
            // `x` leads into the cycle and is not on it.
            ("{x: [a], a: [b], b: [a]}", "a, b, a"),
        ] {
            match resolved(yaml) {
                Err(GroupsError::Cycle(group, told)) => {
                    assert_eq!(told, way, "{yaml}");
                    assert_eq!(group.as_str(), &way[..1], "{yaml}");
                }
                other => panic!("{yaml}: {other:?}"),
            }
        }
    }

    #[test]
    fn a_group_left_with_no_node_is_refused_naming_it() {
        for yaml in [
            "{core: {include: [node-a], exclude: [node-a]}}",
            "{core: []}",
        ] {
            assert_eq!(
                resolved(yaml),
                Err(GroupsError::Empty("the group \"core\"".to_owned())),
                "{yaml}"
            );
        }
    }

    fn selection(yaml: &str) -> Selection {
        sliplib_yaml::from_str(yaml).unwrap()
    }

    #[test]
    fn a_selection_reaches_nodes_and_groups_and_all_is_what_its_writer_means_by_it() {
        let groups = resolved(EXAMPLE).unwrap();
        let reach = |yaml: &str, all: &[&str]| -> Vec<String> {
            groups
                .reach(&selection(yaml), &names(all), "the document")
                .unwrap()
                .iter()
                .map(Name::to_string)
                .collect()
        };
        let talos = ["node-a", "node-b", "node-c"];
        assert_eq!(reach("{}", &talos), talos);
        assert_eq!(reach("{exclude: [workers]}", &talos), ["node-a"]);
        assert_eq!(
            reach("{include: [workers, router1]}", &talos),
            ["node-b", "node-c", "router1"]
        );
        assert_eq!(
            reach("{include: [node-c, all]}", &["router1"]),
            ["node-c", "router1"]
        );
        // What `exclude` names and `include` did not bring in changes nothing.
        assert_eq!(
            reach("{include: [node-a], exclude: [router1]}", &talos),
            ["node-a"]
        );
    }
}
