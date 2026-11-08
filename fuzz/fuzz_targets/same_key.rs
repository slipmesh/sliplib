#![no_main]

use libfuzzer_sys::fuzz_target;
use sliplib_secrets::wireguard::{PrivateKey, encode_key};
use x25519_dalek::{PublicKey, StaticSecret};

/// Bits of the 32-byte key that the fuzzer may flip: the first five are those X25519 clamping
/// overwrites, the rest change the key.
const FLIPPABLE: [(usize, u8); 8] = [
    (0, 0),
    (0, 1),
    (0, 2),
    (31, 6),
    (31, 7),
    (0, 3),
    (15, 0),
    (31, 5),
];

// X25519 uses a private key only after clamping it, and two different clamped keys have different
// public keys: "the same key" has to mean exactly "the same public key".
fuzz_target!(|input: ([u8; 32], u8)| {
    let (bytes, flips) = input;
    let mut flipped = bytes;
    for (i, (byte, bit)) in FLIPPABLE.iter().enumerate() {
        if flips & (1 << i) != 0 {
            flipped[*byte] ^= 1 << bit;
        }
    }
    let public = |b: [u8; 32]| PublicKey::from(&StaticSecret::from(b)).to_bytes();
    let (a, b) = (
        PrivateKey::from(encode_key(&bytes)),
        PrivateKey::from(encode_key(&flipped)),
    );
    assert_eq!(a.is_same_key(&b), public(bytes) == public(flipped));
});
