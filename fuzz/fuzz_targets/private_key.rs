#![no_main]

use libfuzzer_sys::fuzz_target;
use sliplib_secrets::wireguard::PrivateKey;

fuzz_target!(|texts: (&str, &str)| {
    let (a, b) = (PrivateKey::from(texts.0), PrivateKey::from(texts.1));
    assert_eq!(format!("{a:?}"), "<redacted>");
    let _ = a.public_key();
    assert!(a.is_same_key(&a));
    assert_eq!(a.is_same_key(&b), b.is_same_key(&a));
});
