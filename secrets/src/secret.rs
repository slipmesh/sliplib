use serde::{Deserialize, Serialize};
use std::fmt;
use zeroize::ZeroizeOnDrop;

/// A secret held as text, such as a password or AmneziaWG's header protection key.
///
/// It stays out of printed output: `Debug` prints `<redacted>` and there is no `Display`. It leaves
/// through [`Self::expose`] and through `Serialize`, which writes it into a config, so a structure
/// holding one is serialized only where the secret is meant to go. Only the stored text is wiped
/// when it is dropped; every copy made from it is not.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, ZeroizeOnDrop)]
#[serde(transparent)]
pub struct Secret(String);

impl Secret {
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}

impl From<String> for Secret {
    fn from(text: String) -> Self {
        Self(text)
    }
}

impl From<&str> for Secret {
    fn from(text: &str) -> Self {
        Self(text.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wipes_its_text_when_dropped() {
        fn wiped_on_drop<T: ZeroizeOnDrop>() {}
        wiped_on_drop::<Secret>();
    }

    #[test]
    fn debug_output_never_carries_the_secret() {
        #[derive(Debug)]
        #[expect(dead_code)]
        struct Device {
            password: Secret,
        }
        let password = Secret::from("hunter2");
        let device = Device {
            password: password.clone(),
        };
        for printed in [
            format!("{password:?}"),
            format!("{device:?}"),
            format!("{device:#?}"),
        ] {
            assert!(!printed.contains(password.expose()), "{printed}");
            assert!(printed.contains("<redacted>"), "{printed}");
        }
    }
}
