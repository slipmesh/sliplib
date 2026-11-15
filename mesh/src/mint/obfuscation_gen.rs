//! Random values for the minted fields of an AmneziaWG obfuscation: `jc`, `jmin`, `jmax`, `s1`,
//! `s2` and `h1` to `h4`, the only parameters the kernel module's README gives a recommended range
//! for. `s3`, `s4`, `i1` to `i5` and the timing knobs have no established safe default, and
//! `header_protection_key` is a secret both ends share by other means, so none of them is ever
//! generated.

use crate::secrets::Fresh;
use rand::{Rng, RngExt};

/// From 5 up, the floor the kernel module's README recommends: 1..4 are WireGuard's own message
/// types, `sliplib_config::DEFAULT_MAGIC_HEADERS`, which amneziawg-tools' generator avoids too.
fn random_magic_headers(rng: &mut impl Rng) -> [u32; 4] {
    let mut values = Vec::with_capacity(4);
    while values.len() < 4 {
        let candidate = rng.random_range(5..=2_147_483_647);
        if !values.contains(&candidate) {
            values.push(candidate);
        }
    }
    [values[0], values[1], values[2], values[3]]
}

/// Every range is the recommended one from the kernel module's README. The s1/s2 floor of 15
/// also leaves room for the 12-byte header protection nonce, should `slipmesh.yaml` set a key.
pub fn generate() -> Fresh {
    let mut rng = rand::rng();
    let jmin: u16 = rng.random_range(10..=100);
    let jmax: u16 = jmin + rng.random_range(1..=900);
    let s1: u16 = rng.random_range(15..=150);
    // S2 = S1 + 56 would give the handshake initiation and response the same length.
    let s2 = loop {
        let s2: u16 = rng.random_range(15..=150);
        if s2 != s1 + 56 {
            break s2;
        }
    };
    let [h1, h2, h3, h4] = random_magic_headers(&mut rng);
    Fresh {
        jc: rng.random_range(4..=12),
        jmin,
        jmax,
        s1,
        s2,
        h1,
        h2,
        h3,
        h4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets;
    use sliplib_config::Obfuscation;

    #[test]
    fn generates_jmin_less_than_jmax() {
        let o = generate();
        assert!(o.jmin < o.jmax);
    }

    #[test]
    fn generates_four_distinct_magic_headers_avoiding_reserved_message_types() {
        let o = generate();
        let headers = [o.h1, o.h2, o.h3, o.h4];
        for h in headers {
            assert!(!sliplib_config::DEFAULT_MAGIC_HEADERS.contains(&h));
        }
        let mut sorted = headers.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 4, "h1-h4 must all be distinct");
    }

    // The recommended ranges of the kernel module's README; s1/s2 also hold the header
    // protection nonce, and S1 + 56 = S2 would give initiation and response the same length.
    #[test]
    fn generates_values_in_the_documented_ranges() {
        for _ in 0..2000 {
            let o = generate();
            assert!((4..=12).contains(&o.jc), "{o:?}");
            assert!(o.jmin < o.jmax && o.jmax <= 1280, "{o:?}");
            assert!(
                (15..=150).contains(&o.s1) && (15..=150).contains(&o.s2),
                "{o:?}"
            );
            assert_ne!(o.s1 + 56, o.s2, "{o:?}");
            for h in [o.h1, o.h2, o.h3, o.h4] {
                assert!((5..=2_147_483_647).contains(&h), "{o:?}");
            }
            let mut filled = Obfuscation::default();
            secrets::fill(&mut filled, &o);
            filled.validate().unwrap();
        }
    }
}
