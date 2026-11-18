//! The names of format version 2: of nodes, groups, links, a node's underlay entries and route
//! classes, each the key of the mapping that holds it. A name becomes part of an interface name,
//! of a name in a node's routing configuration or of an annotation key, which take no more than
//! this.

use serde::Deserialize;
use std::fmt;

/// A name: ASCII letters, digits, `-`, `_` and `.`, with a letter or a digit first and last.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Deserialize)]
#[serde(try_from = "String")]
pub struct Name(String);

// A refusal quotes a name with `{:?}`, as it quotes a text.
impl fmt::Debug for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

/// What keeps a text from being a name.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum NameError {
    #[error("a name must not be empty")]
    Empty,
    #[error(
        "the name {name:?} holds {character:?}; a name takes ASCII letters, digits, `-`, `_` and `.`"
    )]
    Character { name: String, character: char },
    #[error("the name {0:?} has to start and end with a letter or a digit")]
    Edge(String),
    #[error("the name {name:?} is longer than {limit} characters")]
    TooLong { name: String, limit: usize },
}

impl Name {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The name of the group nobody defines.
    pub(crate) fn all() -> Self {
        Self(super::ALL.to_owned())
    }
}

impl TryFrom<String> for Name {
    type Error = NameError;

    fn try_from(name: String) -> Result<Self, NameError> {
        if name.is_empty() {
            return Err(NameError::Empty);
        }
        if let Some(character) = name
            .chars()
            .find(|c| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')))
        {
            return Err(NameError::Character { name, character });
        }
        let at_an_edge = |c: Option<char>| c.is_some_and(|c| !c.is_ascii_alphanumeric());
        if at_an_edge(name.chars().next()) || at_an_edge(name.chars().next_back()) {
            return Err(NameError::Edge(name));
        }
        Ok(Self(name))
    }
}

// A map keyed by names is looked up by the text of one; a `Name` hashes and compares as its text.
impl std::borrow::Borrow<str> for Name {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The name of a route class. It ends the key of a node annotation, which takes 63 characters,
/// so it takes 48.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Deserialize)]
#[serde(try_from = "String")]
pub struct ClassName(Name);

impl fmt::Debug for ClassName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl ClassName {
    pub const LIMIT: usize = 48;

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl TryFrom<String> for ClassName {
    type Error = NameError;

    fn try_from(name: String) -> Result<Self, NameError> {
        let name = Name::try_from(name)?;
        if name.as_str().len() > Self::LIMIT {
            return Err(NameError::TooLong {
                name: name.0,
                limit: Self::LIMIT,
            });
        }
        Ok(Self(name))
    }
}

impl fmt::Display for ClassName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(text: &str) -> Result<Name, NameError> {
        Name::try_from(text.to_owned())
    }

    #[test]
    fn a_name_takes_letters_digits_and_three_marks_inside() {
        for text in ["a", "7", "node-a", "edge_1.site", "A-b.C_d9"] {
            assert_eq!(name(text).unwrap().as_str(), text);
        }
    }

    #[test]
    fn a_name_with_any_other_character_is_refused_naming_it() {
        for (text, character) in [
            ("node a", ' '),
            ("node/a", '/'),
            ("node*", '*'),
            ("nöde", 'ö'),
        ] {
            assert_eq!(
                name(text),
                Err(NameError::Character {
                    name: text.to_owned(),
                    character
                })
            );
        }
    }

    #[test]
    fn a_name_starts_and_ends_with_a_letter_or_a_digit() {
        for text in ["-a", "a-", ".a", "a.", "_a", "a_", "-"] {
            assert_eq!(name(text), Err(NameError::Edge(text.to_owned())), "{text}");
        }
    }

    #[test]
    fn a_refusal_quotes_a_name_as_it_quotes_a_text() {
        assert_eq!(format!("{:?}", name("node-a").unwrap()), "\"node-a\"");
        let class = ClassName::try_from("egress".to_owned()).unwrap();
        assert_eq!(format!("{class:?}"), "\"egress\"");
    }

    #[test]
    fn a_name_reads_as_its_text_wherever_a_message_names_it() {
        assert_eq!(name("node-a").unwrap().to_string(), "node-a");
        assert_eq!(
            ClassName::try_from("egress".to_owned())
                .unwrap()
                .to_string(),
            "egress"
        );
    }

    #[test]
    fn an_empty_name_is_refused() {
        assert_eq!(name(""), Err(NameError::Empty));
    }

    #[test]
    fn a_name_is_read_from_a_yaml_key() {
        let read: std::collections::BTreeMap<Name, u8> =
            sliplib_yaml::from_str("{node-a: 1}").unwrap();
        assert_eq!(read.keys().next().unwrap().as_str(), "node-a");
        let refused = sliplib_yaml::from_str::<std::collections::BTreeMap<Name, u8>>("{node a: 1}")
            .unwrap_err();
        assert!(refused.to_string().contains("\"node a\""), "{refused}");
    }

    #[test]
    fn a_class_name_follows_the_rule_and_takes_48_characters() {
        let longest = "c".repeat(48);
        assert_eq!(
            ClassName::try_from(longest.clone()).unwrap().as_str(),
            longest
        );
        let longer = "c".repeat(49);
        assert_eq!(
            ClassName::try_from(longer.clone()),
            Err(NameError::TooLong {
                name: longer,
                limit: 48
            })
        );
        assert_eq!(
            ClassName::try_from("-egress".to_owned()),
            Err(NameError::Edge("-egress".to_owned()))
        );
    }
}
