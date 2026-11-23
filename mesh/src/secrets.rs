//! The refusal of a missing secret, and the fields a minted obfuscation carries.

use sliplib_config::Obfuscation;

/// A key or an obfuscation the node needs and the files lack.
#[derive(Debug)]
pub struct MissingSecret(pub(crate) String);

impl std::fmt::Display for MissingSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} - `slipmesh mint` creates it", self.0)
    }
}

impl std::error::Error for MissingSecret {}

/// The fields a minted `amneziawg` obfuscation carries, with their types. What is looked for and
/// what minting fills come from this one list.
macro_rules! minted_fields {
    ($($field:ident: $value:ty),* $(,)?) => {
        /// In the list's order.
        pub(crate) fn unminted(obfuscation: &Obfuscation) -> Vec<&'static str> {
            let mut lacking = Vec::new();
            $(if obfuscation.$field.is_none() {
                lacking.push(stringify!($field));
            })*
            lacking
        }

        #[cfg(feature = "mint")]
        #[derive(Debug)]
        pub(crate) struct Fresh {
            $(pub(crate) $field: $value,)*
        }

        /// The fields set, `None` for none.
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
