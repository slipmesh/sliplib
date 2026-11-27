use serde::{Deserialize, Serialize};
use std::fmt;
use zeroize::ZeroizeOnDrop;

/// As text, such as AmneziaWG's header protection key. `Debug` prints `<redacted>`; no `Display`.
/// It leaves through [`Self::expose`] and `Serialize`. Only the stored text is wiped on drop.
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
