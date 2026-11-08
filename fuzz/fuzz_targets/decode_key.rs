#![no_main]

use libfuzzer_sys::fuzz_target;
use sliplib_secrets::wireguard::{decode_key, encode_key};

// A key has exactly one text, apart from surrounding whitespace: comparing keys as text, as
// `PrivateKey`'s `Eq` does, relies on the decoder rejecting every other spelling of the same bytes.
fuzz_target!(|text: &str| {
    if let Ok(key) = decode_key(text) {
        assert_eq!(encode_key(&key), text.trim());
    }
});
