//! What a node's configs need of the network's secrets, which the model holds as the files set
//! them: the refusal of one the files lack, and the fields an obfuscation of a link of type
//! `amneziawg` carries once minted. Nothing here generates a secret.

use sliplib_config::Obfuscation;

/// A key or an obfuscation the rendered node needs and the file does not set. A refusal that
/// carries one is cured by creating the secret, which a caller can tell its user how to do.
#[derive(Debug)]
pub struct MissingSecret(pub(crate) String);

impl std::fmt::Display for MissingSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for MissingSecret {}

/// The obfuscation fields a link of type `amneziawg` carries once its secrets are minted, each
/// with the type of its value. One that lacks any of them has not been minted yet: a config
/// rendered from it would differ from the one rendered after minting. What is looked for and what
/// minting fills are generated from this one list, so they cannot differ.
macro_rules! minted_fields {
    ($($field:ident: $value:ty),* $(,)?) => {
        /// The fields `obfuscation` lacks of those a minted one carries, in the list's order.
        pub(crate) fn unminted(obfuscation: &Obfuscation) -> Vec<&'static str> {
            let mut lacking = Vec::new();
            $(if obfuscation.$field.is_none() {
                lacking.push(stringify!($field));
            })*
            lacking
        }

        /// A new value for every minted field.
        #[cfg(feature = "mint")]
        #[derive(Debug)]
        pub(crate) struct Fresh {
            $(pub(crate) $field: $value,)*
        }

        /// Sets each minted field `obfuscation` lacks to its value in `fresh`, and returns the
        /// fields it set - `None` when `obfuscation` lacked none.
        #[cfg(feature = "mint")]
        pub(crate) fn fill(obfuscation: &mut Obfuscation, fresh: &Fresh) -> Option<Obfuscation> {
            let mut filled = Obfuscation::default();
            $(if obfuscation.$field.is_none() {
                obfuscation.$field = Some(fresh.$field.into());
                filled.$field = Some(fresh.$field.into());
            })*
            (filled != Obfuscation::default()).then_some(filled)
        }
    };
}

minted_fields!(jc: u16, jmin: u16, jmax: u16, s1: u16, s2: u16, h1: u32, h2: u32, h3: u32, h4: u32);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unminted_names_the_minted_fields_an_obfuscation_lacks_in_order() {
        let obfuscation = Obfuscation {
            jmin: Some(10),
            s2: Some(16),
            h3: Some(7.into()),
            s4: Some(12),
            ..Obfuscation::default()
        };
        assert_eq!(
            unminted(&obfuscation),
            ["jc", "jmax", "s1", "h1", "h2", "h4"]
        );
        assert!(unminted(&crate::test_support::minted_obfuscation()).is_empty());
    }
}
