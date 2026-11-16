//! Random values for the minted fields of an AmneziaWG obfuscation: `jc`, `jmin`, `jmax`, `s1`,
//! `s2` and `h1` to `h4`, the only parameters the kernel module's README gives a recommended range
//! for. `s3`, `s4`, `i1` to `i5`, `header_protection_key` and the timing knobs have no established
//! safe default, so none of them is ever generated.

use crate::secrets::Fresh;
use rand::{Rng, RngExt};
use sliplib_config::{Obfuscation, Range};

/// From 5 up, the floor the kernel module's README recommends: 1..4 are WireGuard's own message
/// types, `sliplib_config::DEFAULT_MAGIC_HEADERS`, which amneziawg-tools' generator avoids too.
const FIRST_HEADER: u32 = 5;
const LAST_HEADER: u32 = 2_147_483_647;

/// A value a header is drawn from that is in none of `taken`, each as likely as any other;
/// `None` when they leave none.
fn draw_outside(rng: &mut impl Rng, taken: &[Range<u32>]) -> Option<u32> {
    let mut taken: Vec<(u32, u32)> = taken
        .iter()
        .filter(|range| range.lo() <= LAST_HEADER && range.hi() >= FIRST_HEADER)
        .map(|range| (range.lo().max(FIRST_HEADER), range.hi().min(LAST_HEADER)))
        .collect();
    taken.sort_unstable();
    let mut disjoint: Vec<(u32, u32)> = Vec::new();
    for (lo, hi) in taken {
        match disjoint.last_mut() {
            Some((_, end)) if lo <= *end => *end = hi.max(*end),
            _ => disjoint.push((lo, hi)),
        }
    }

    let width = |(lo, hi): (u32, u32)| hi - lo + 1;
    let free =
        width((FIRST_HEADER, LAST_HEADER)) - disjoint.iter().copied().map(width).sum::<u32>();
    if free == 0 {
        return None;
    }
    // The free values counted from the first, each taken range in the way stepped over.
    let mut value = FIRST_HEADER + rng.random_range(0..free);
    for range in disjoint {
        if value < range.0 {
            break;
        }
        value += width(range);
    }
    Some(value)
}

/// Every range is the recommended one from the kernel module's README. The s1/s2 floor of 15
/// also leaves room for the 12-byte header protection nonce, should `slipmesh.yaml` set a key.
/// `held` is the entry the values are for, with what it already carries; `None` when its headers
/// leave no value for a new one.
pub fn generate(held: &Obfuscation) -> Option<Fresh> {
    let mut rng = rand::rng();
    // A header for each one the entry lacks, sharing no value with one it holds or with another
    // drawn here, which the module would refuse. For one it holds, its own low end, which is
    // never written: only a field the entry lacks is filled.
    let mut taken: Vec<Range<u32>> = [held.h1, held.h2, held.h3, held.h4]
        .into_iter()
        .flatten()
        .collect();
    let mut headers = [0; 4];
    for (header, written) in headers.iter_mut().zip([held.h1, held.h2, held.h3, held.h4]) {
        *header = match written {
            Some(range) => range.lo(),
            None => {
                let drawn = draw_outside(&mut rng, &taken)?;
                taken.push(drawn.into());
                drawn
            }
        };
    }
    let [h1, h2, h3, h4] = headers;
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
    Some(Fresh {
        jc: rng.random_range(4..=12),
        jmin,
        jmax,
        s1,
        s2,
        h1,
        h2,
        h3,
        h4,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets;

    fn generate() -> Fresh {
        super::generate(&Obfuscation::default()).unwrap()
    }

    fn holding(h1: &str) -> Obfuscation {
        Obfuscation {
            h1: Some(h1.parse().unwrap()),
            ..Obfuscation::default()
        }
    }

    // A header inside one the entry holds is refused when its config is checked, and with a wide
    // range written nearly every draw would be such a header.
    #[test]
    fn a_created_header_falls_outside_a_range_the_entry_holds() {
        for _ in 0..200 {
            let o = super::generate(&holding("5-2000000000")).unwrap();
            let created = [o.h2, o.h3, o.h4];
            assert!(created.iter().all(|h| *h > 2_000_000_000), "{o:?}");
            assert!(
                created[0] != created[1] && created[1] != created[2],
                "{o:?}"
            );
            assert_ne!(created[0], created[2], "{o:?}");
        }
    }

    fn taking(ranges: &[&str]) -> Vec<Range<u32>> {
        ranges.iter().map(|range| range.parse().unwrap()).collect()
    }

    #[test]
    fn the_one_value_left_is_the_one_drawn_wherever_it_lies() {
        let mut rng = rand::rng();
        for (taken, left) in [
            (taking(&["6-2147483647"]), 5),
            (taking(&["5-2147483646"]), 2_147_483_647),
            (taking(&["5-10", "12-2147483647"]), 11),
            // In any order, overlapping, and reaching past the values a header is drawn from.
            (taking(&["12-4294967295", "14-20", "1-10"]), 11),
            // Ending on the first value, starting on the last, and two that share an end.
            (taking(&["1-5", "7-2147483647"]), 6),
            (taking(&["5-2147483645", "2147483647"]), 2_147_483_646),
            (taking(&["5-10", "10-20", "22-2147483647"]), 21),
        ] {
            assert_eq!(draw_outside(&mut rng, &taken), Some(left), "{taken:?}");
        }
    }

    #[test]
    fn ranges_that_share_an_end_and_cover_every_value_leave_none() {
        let mut rng = rand::rng();
        let taken = taking(&["5-10", "10-2147483647"]);
        assert_eq!(draw_outside(&mut rng, &taken), None);
    }

    #[test]
    fn ranges_beside_the_values_a_header_is_drawn_from_take_nothing() {
        let mut rng = rand::rng();
        let taken = taking(&["1-4", "2147483648-4294967295", "5-2147483646"]);
        assert_eq!(draw_outside(&mut rng, &taken), Some(2_147_483_647));
    }

    #[test]
    fn no_header_is_created_when_those_the_entry_holds_leave_none() {
        assert!(super::generate(&holding("5-2147483647")).is_none());
        assert!(super::generate(&holding("1-4294967295")).is_none());
        // One value left, for the three it lacks.
        assert!(super::generate(&holding("5-2147483646")).is_none());
    }

    // Ranges that cover every value are no obstacle when no header is to be created.
    #[test]
    fn an_entry_that_holds_all_four_headers_keeps_them_and_needs_no_room_for_another() {
        let held = Obfuscation {
            h1: Some("5-1000".parse().unwrap()),
            h2: Some("1001-2000".parse().unwrap()),
            h3: Some("2001-3000".parse().unwrap()),
            h4: Some("3001-2147483647".parse().unwrap()),
            ..Obfuscation::default()
        };
        let mut filled = held.clone();
        let created = secrets::fill(&mut filled, &super::generate(&held).unwrap()).unwrap();
        assert_eq!(
            [filled.h1, filled.h2, filled.h3, filled.h4],
            [held.h1, held.h2, held.h3, held.h4]
        );
        assert_eq!([created.h1, created.h2, created.h3, created.h4], [None; 4]);
        assert!(created.jc.is_some());
    }

    #[test]
    fn exactly_the_values_left_are_enough_for_the_headers_it_lacks() {
        let held = Obfuscation {
            h1: Some("5-2147483645".parse().unwrap()),
            h2: Some(4_000_000_000.into()),
            ..Obfuscation::default()
        };
        let o = super::generate(&held).unwrap();
        let mut created = [o.h3, o.h4];
        created.sort_unstable();
        assert_eq!(created, [2_147_483_646, 2_147_483_647]);
    }

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
                assert!((FIRST_HEADER..=LAST_HEADER).contains(&h), "{o:?}");
            }
            let mut filled = Obfuscation::default();
            secrets::fill(&mut filled, &o);
            filled.validate().unwrap();
        }
    }
}
