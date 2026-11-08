//! WireGuard keys in the form configs and RouterOS carry them: the base64 text of 32 bytes. The
//! same form holds every X25519 key - private, public, and AmneziaWG's header protection key.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use curve25519_dalek::scalar::clamp_integer;
use serde::{Deserialize, Serialize};
use std::fmt;
use x25519_dalek::{PublicKey, StaticSecret};

#[derive(Debug, PartialEq, Eq)]
pub enum KeyError {
    Base64(base64::DecodeError),
    /// How many bytes the text decoded to instead of 32.
    Length(usize),
}

impl fmt::Display for KeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Base64(_) => f.write_str("key is not valid base64"),
            Self::Length(n) => write!(f, "key is {n} bytes, not 32"),
        }
    }
}

impl std::error::Error for KeyError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Base64(e) => Some(e),
            Self::Length(_) => None,
        }
    }
}

/// Surrounding whitespace is ignored: a key read from a file usually ends in a newline.
pub fn decode_key(b64: &str) -> Result<[u8; 32], KeyError> {
    let bytes = STANDARD.decode(b64.trim()).map_err(KeyError::Base64)?;
    let len = bytes.len();
    bytes.try_into().map_err(|_| KeyError::Length(len))
}

pub fn encode_key(bytes: &[u8; 32]) -> String {
    STANDARD.encode(bytes)
}

/// A private key, kept as the text it was given so that it is written back unchanged.
///
/// It stays out of printed output: `Debug` prints `<redacted>` and there is no `Display`. It leaves
/// through [`Self::expose`] and through `Serialize`, which writes it into a config, so a structure
/// holding one is serialized only where the key is meant to go. Its memory is not wiped on drop:
/// every config written with the key holds a copy of it anyway.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PrivateKey(String);

impl PrivateKey {
    pub fn generate() -> Self {
        Self(encode_key(&StaticSecret::random().to_bytes()))
    }

    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn public_key(&self) -> Result<String, KeyError> {
        let secret = StaticSecret::from(decode_key(&self.0)?);
        Ok(encode_key(PublicKey::from(&secret).as_bytes()))
    }

    /// Whether both are the same key, whatever form each is in. WireGuard clamps a private key
    /// when it loads one and RouterOS hands back the clamped form, while a generated key is stored
    /// unclamped: as text the two never match, although X25519 makes them the same key. Text that
    /// is not a key is compared as written: a spurious difference is safer than mangling a value
    /// this does not understand.
    pub fn is_same_key(&self, other: &Self) -> bool {
        match (clamped(&self.0), clamped(&other.0)) {
            (Some(a), Some(b)) => a == b,
            _ => self.0 == other.0,
        }
    }
}

fn clamped(b64: &str) -> Option<[u8; 32]> {
    decode_key(b64).ok().map(clamp_integer)
}

impl fmt::Debug for PrivateKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}

impl From<String> for PrivateKey {
    fn from(b64: String) -> Self {
        Self(b64)
    }
}

impl From<&str> for PrivateKey {
    fn from(b64: &str) -> Self {
        Self(b64.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex_key(hex: &str) -> [u8; 32] {
        let mut bytes = [0; 32];
        for (byte, pair) in bytes.iter_mut().zip(hex.as_bytes().chunks(2)) {
            *byte = u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap();
        }
        bytes
    }

    #[test]
    fn decodes_what_it_encodes() {
        let bytes: [u8; 32] = std::array::from_fn(|i| i as u8);
        assert_eq!(decode_key(&encode_key(&bytes)), Ok(bytes));
    }

    #[test]
    fn decodes_a_key_with_surrounding_whitespace() {
        let bytes = [7; 32];
        assert_eq!(decode_key(&format!(" {}\n", encode_key(&bytes))), Ok(bytes));
    }

    #[test]
    fn rejects_text_that_is_not_base64() {
        assert!(matches!(
            decode_key("not base64!"),
            Err(KeyError::Base64(_))
        ));
    }

    #[test]
    fn keeps_the_base64_error_as_the_source() {
        use std::error::Error as _;
        assert!(decode_key("not base64!").unwrap_err().source().is_some());
        assert!(decode_key("AAAA").unwrap_err().source().is_none());
    }

    #[test]
    fn rejects_a_key_of_the_wrong_length() {
        assert_eq!(decode_key("AAAA"), Err(KeyError::Length(3)));
    }

    /// RFC 7748, section 6.1: Alice's key pair.
    #[test]
    fn derives_the_public_key_of_the_rfc_7748_vector() {
        let private = PrivateKey::from(encode_key(&hex_key(
            "77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2a",
        )));
        let public = hex_key("8520f0098930a754748b7ddcb43ef75a0dbf3a0d26381af4eba4a98eaa9b4e6a");
        assert_eq!(private.public_key(), Ok(encode_key(&public)));
    }

    #[test]
    fn a_public_key_needs_a_private_key_that_decodes() {
        assert_eq!(
            PrivateKey::from("AAAA").public_key(),
            Err(KeyError::Length(3))
        );
    }

    #[test]
    fn generates_a_fresh_key_each_time() {
        let (a, b) = (PrivateKey::generate(), PrivateKey::generate());
        assert_ne!(a, b);
        for key in [a, b] {
            decode_key(key.expose()).unwrap();
            decode_key(&key.public_key().unwrap()).unwrap();
        }
    }

    #[test]
    fn debug_output_never_carries_the_key() {
        #[derive(Debug)]
        #[allow(dead_code)]
        struct Interface {
            private_key: PrivateKey,
        }
        let key = PrivateKey::generate();
        let interface = Interface {
            private_key: key.clone(),
        };
        for printed in [
            format!("{key:?}"),
            format!("{interface:?}"),
            format!("{interface:#?}"),
        ] {
            assert!(!printed.contains(key.expose()), "{printed}");
            assert!(printed.contains("<redacted>"), "{printed}");
        }
    }

    /// Every bit set: clamping clears the low three bits of byte 0 and the top bit of byte 31.
    fn all_ones() -> (PrivateKey, PrivateKey) {
        let mut clamped = [0xFF; 32];
        clamped[0] = 0xF8;
        clamped[31] = 0x7F;
        (
            PrivateKey::from(encode_key(&[0xFF; 32])),
            PrivateKey::from(encode_key(&clamped)),
        )
    }

    /// No bit set: clamping sets bit 6 of byte 31.
    fn all_zeros() -> (PrivateKey, PrivateKey) {
        let mut clamped = [0x00; 32];
        clamped[31] = 0x40;
        (
            PrivateKey::from(encode_key(&[0x00; 32])),
            PrivateKey::from(encode_key(&clamped)),
        )
    }

    #[test]
    fn a_key_is_the_same_key_as_its_clamped_form() {
        for (generated, clamped) in [all_ones(), all_zeros()] {
            assert_ne!(generated, clamped, "the two differ as text");
            assert!(generated.is_same_key(&clamped));
            assert!(clamped.is_same_key(&generated));
            assert!(clamped.is_same_key(&clamped));
        }
    }

    #[test]
    fn different_keys_are_not_the_same_key() {
        let (ones, _) = all_ones();
        let (zeros, _) = all_zeros();
        assert!(!ones.is_same_key(&zeros));
    }

    #[test]
    fn text_that_is_not_a_key_is_compared_as_written() {
        let (ones, _) = all_ones();
        assert!(PrivateKey::from("AAAA").is_same_key(&PrivateKey::from("AAAA")));
        assert!(!PrivateKey::from("AAAA").is_same_key(&PrivateKey::from("AAAB")));
        assert!(!PrivateKey::from("not base64!").is_same_key(&ones));
    }
}
