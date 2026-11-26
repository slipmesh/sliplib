//! Names of nodes, groups, links, underlay entries, route classes and rules. A name becomes part of
//! an interface name, a routing configuration and an annotation key, which take no more.

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer};
use std::fmt;

/// ASCII letters, digits, `-`, `_` and `.`, a letter or digit first and last. One YAML reads as a
/// number or a boolean is the name it prints as: `1.0` is `1`, `True` is `true`; quoted, it keeps
/// its spelling.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Name(String);

impl<'de> Deserialize<'de> for Name {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Text;

        impl Visitor<'_> for Text {
            type Value = Name;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a name")
            }

            fn visit_str<E: de::Error>(self, text: &str) -> Result<Name, E> {
                Name::try_from(text.to_owned()).map_err(E::custom)
            }

            fn visit_u64<E: de::Error>(self, number: u64) -> Result<Name, E> {
                self.visit_str(&number.to_string())
            }

            fn visit_i64<E: de::Error>(self, number: i64) -> Result<Name, E> {
                self.visit_str(&number.to_string())
            }

            fn visit_f64<E: de::Error>(self, number: f64) -> Result<Name, E> {
                self.visit_str(&number.to_string())
            }

            fn visit_bool<E: de::Error>(self, word: bool) -> Result<Name, E> {
                self.visit_str(&word.to_string())
            }

            fn visit_unit<E: de::Error>(self) -> Result<Name, E> {
                Err(E::custom(NameError::Empty))
            }
        }

        deserializer.deserialize_any(Text)
    }
}

// A refusal quotes it as a text.
impl fmt::Debug for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

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
    #[error("the name {name:?} of a route class is longer than {limit} characters")]
    TooLong { name: String, limit: usize },
    #[error(
        "the name {0:?} stands twice among the keys of one mapping, written in two ways YAML \
         reads as one value - give each its own name, or quote both as written"
    )]
    Twice(String),
}

impl Name {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn all() -> Self {
        Self(super::ALL.to_owned())
    }

    /// A word of a selection, which no node or group is named.
    pub fn is_reserved(&self) -> bool {
        [super::ALL, super::NONE].contains(&self.as_str())
    }
}

pub(crate) fn is_name_character(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')
}

impl TryFrom<String> for Name {
    type Error = NameError;

    fn try_from(name: String) -> Result<Self, NameError> {
        if name.is_empty() {
            return Err(NameError::Empty);
        }
        if let Some(character) = name.chars().find(|&c| !is_name_character(c)) {
            return Err(NameError::Character { name, character });
        }
        let at_an_edge = |c: Option<char>| c.is_some_and(|c| !c.is_ascii_alphanumeric());
        if at_an_edge(name.chars().next()) || at_an_edge(name.chars().next_back()) {
            return Err(NameError::Edge(name));
        }
        Ok(Self(name))
    }
}

// Looked up by text: it hashes and compares as its text.
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

/// It ends an annotation key, which takes 63 characters: 48.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Deserialize)]
#[serde(try_from = "Name")]
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
        Self::try_from(Name::try_from(name)?)
    }
}

impl TryFrom<Name> for ClassName {
    type Error = NameError;

    fn try_from(name: Name) -> Result<Self, NameError> {
        if name.as_str().len() > Self::LIMIT {
            return Err(NameError::TooLong {
                name: name.0,
                limit: Self::LIMIT,
            });
        }
        Ok(Self(name))
    }
}

impl std::borrow::Borrow<str> for ClassName {
    fn borrow(&self) -> &str {
        self.0.as_str()
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
