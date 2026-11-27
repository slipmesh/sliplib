//! AmneziaWG's device-level obfuscation: every field the kernel module (v3.0.20260805) takes, and
//! checks of what it refuses or mishandles.
//! - `jc`, `jmin`, `jmax`: junk packets before a handshake; `jmin` up to `jmax`.
//! - `s1`..`s4`: junk on initiation, response, cookie reply and data packets.
//! - `h1`..`h4`: a magic header per message type, a number or a range the module draws from. Both
//!   ends hold the same four, sharing no value; an unset one counts as its WireGuard type 1..4.
//! - `i1`..`i5`: decoy packets.
//! - `header_protection_key`: encrypts a header's type and reserved fields. It is the interface's,
//!   so every peer holds the same key and the same `s1`..`s4`, each at least 12 for the nonce.
//! - The six timing and padding values WireGuard fixes, each a number or a range with ends up to
//!   65535, per end. Zero or unset leaves the module's constant, or the padding a data packet gets
//!   otherwise. A timing range starts at 1 or above, `rekey_after_time` ends below 180, and
//!   `reject_after_time` stays above `keepalive_timeout` plus `rekey_timeout`.
//!
//! Not checked: the module stops using a key 180 seconds after its handshake whatever
//! `reject_after_time` says, so one far from 180 moves its timers off the key's real end.

use crate::Range;
use serde::{Deserialize, Serialize};
use sliplib_secrets::Secret;
use std::fmt;

#[derive(Deserialize, Serialize, Clone, Debug, Default, PartialEq)]
pub struct Obfuscation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jc: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jmin: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jmax: Option<u16>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub s1: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub s2: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub s3: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub s4: Option<u16>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub h1: Option<Range<u32>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub h2: Option<Range<u32>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub h3: Option<Range<u32>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub h4: Option<Range<u32>>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub i1: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub i2: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub i3: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub i4: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub i5: Option<String>,

    /// Base64 of 32 bytes, the interface's: the same for every peer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header_protection_key: Option<Secret>,

    /// In place of a data packet's padding, WireGuard's or the random trailer. Non-zero, it leaves
    /// `random_trailers` to handshake and cookie messages.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_padding_addition: Option<Range<u16>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rekey_after_time: Option<Range<u16>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rekey_timeout: Option<Range<u16>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reject_after_time: Option<Range<u16>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keepalive_timeout: Option<Range<u16>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_handshake_attempts: Option<Range<u16>>,

    /// AmneziaWG 3.1: a random-length trailer, sized to what the peer sends. An end without it
    /// drops a longer handshake or cookie message: both ends set it, or neither.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub random_trailers: Option<bool>,
    /// AmneziaWG 3.1: no cookie replies, a signature of their own, at the cost of their
    /// load-shedding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disable_cookies: Option<bool>,
}

/// The module refuses a header protection key, with a bare `EINVAL`, unless each of `s1`..`s4`
/// holds its nonce; an unset one counts as 0.
pub const HEADER_PROTECTION_NONCE_SIZE: u16 = 12;

/// An unset `h1`..`h4`: the WireGuard message types, a fresh device's own. 0 would be a magic
/// value.
pub const DEFAULT_MAGIC_HEADERS: [u32; 4] = [1, 2, 3, 4];

/// The module's, in seconds, for a timing value left zero; the first is also a key's age limit.
const REJECT_AFTER_TIME: u16 = 180;
const KEEPALIVE_TIMEOUT: u16 = 10;
const REKEY_TIMEOUT: u16 = 5;

/// The low end, or `constant` where unset or zero.
fn least(value: Option<Range<u16>>, constant: u16) -> u32 {
    match value {
        Some(range) if !range.is_zero() => range.lo().into(),
        _ => constant.into(),
    }
}

/// What the module refuses at configuration, or mishandles after.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ObfuscationError {
    #[error(
        "obfuscation.header_protection_key needs {} to be at least {HEADER_PROTECTION_NONCE_SIZE}, the size of its nonce",
        .too_small.join(", ")
    )]
    NoRoomForNonce { too_small: Vec<&'static str> },
    #[error(
        "obfuscation.h{} ({}) and obfuscation.h{} ({}) share a value; the four magic headers must not overlap, an unset h1..h4 counting as 1..4",
        .first.0, .first.1, .second.0, .second.1
    )]
    HeadersOverlap {
        /// Which of `h1`..`h4`, and its value.
        first: (usize, Range<u32>),
        second: (usize, Range<u32>),
    },
    #[error(
        "obfuscation.{name} is {range}; a range of it must start at 1 or above, and 0 alone selects the module's own value"
    )]
    RangeStartsAtZero {
        name: &'static str,
        range: Range<u16>,
    },
    #[error(
        "obfuscation.rekey_after_time ({0}) must end below {REJECT_AFTER_TIME}: the module stops using a key {REJECT_AFTER_TIME} seconds after its handshake, whatever reject_after_time says"
    )]
    RekeyAfterKeyIsGone(Range<u16>),
    #[error(
        "obfuscation.reject_after_time ({reject}) must be above keepalive_timeout ({keepalive}) plus rekey_timeout ({rekey}), each by the low end of its range; an unset or zero one counts as the module's own {REJECT_AFTER_TIME}, {KEEPALIVE_TIMEOUT} and {REKEY_TIMEOUT}"
    )]
    RejectTooEarly {
        reject: u32,
        keepalive: u32,
        rekey: u32,
    },
    #[error("obfuscation.jmin ({jmin}) must not exceed obfuscation.jmax ({jmax})")]
    JunkSizesReversed { jmin: u16, jmax: u16 },
}

/// With `random_trailers` on, the receiving module tells a message by the four bytes at `s1`, `s2`
/// and `s3`: a data packet whose bytes there fall in `h1`, `h2` or `h3` is dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum TrailerLoss {
    /// One of `h1`..`h3` is a range: at most `values` of 2^32 data packets, where the bytes read
    /// are random.
    Share { values: u64 },
    /// No `header_protection_key`, and `field` stands `above` 1..15 bytes over `data_junk`, the
    /// `s4` set: it reads the packet's clear header, so its key index and counter decide.
    ReadsHeader {
        field: &'static str,
        above: u16,
        data_junk: Option<u16>,
    },
}

impl fmt::Display for TrailerLoss {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Share { values } => {
                let percent = 100.0 * *values as f64 / MAGIC_HEADER_VALUES as f64;
                write!(
                    f,
                    "with random_trailers on, a data packet whose bytes fall inside h1, h2 or h3 is dropped as a handshake or cookie message: where those bytes are random, up to {values} packets in {MAGIC_HEADER_VALUES}, "
                )?;
                // Two decimals of a smaller share would read as none at all.
                if percent < 0.01 {
                    f.write_str("less than 0.01%")?;
                } else {
                    write!(f, "{percent:.2}%")?;
                }
                f.write_str(
                    " - keep h1 to h3 single values or narrow ranges, or put the range in h4",
                )
            }
            Self::ReadsHeader {
                field,
                above,
                data_junk,
            } => {
                let bytes = if *above == 1 { "byte" } else { "bytes" };
                // An unset `s4` is nowhere in the file to be looked for.
                let unset = match data_junk {
                    Some(_) => "",
                    None => " which is not set and counts as 0,",
                };
                write!(
                    f,
                    "with random_trailers on and no header_protection_key, {field} stands {above} {bytes} above s4,{unset} where a data packet carries its own key index and counter: data packets are dropped by those, not by chance - set a header_protection_key, or keep s1 to s3 from being 1 to 15 above s4"
                )
            }
        }
    }
}

/// The four bytes the module reads.
const MAGIC_HEADER_VALUES: u64 = 1 << 32;

/// A data packet's clear header after its junk, less the first byte: that is its type, never
/// another header's.
const CLEAR_HEADER_REACH: u16 = 15;

impl Obfuscation {
    /// Nothing with `random_trailers` off; the same at both ends.
    pub fn trailer_loss(&self) -> Vec<TrailerLoss> {
        if self.random_trailers != Some(true) {
            return Vec::new();
        }
        let mut loss = Vec::new();

        let [h1, h2, h3, _] = self.magic_headers();
        let read = [h1, h2, h3];
        if read.iter().any(|header| header.lo() != header.hi()) {
            let values = read
                .iter()
                .map(|header| u64::from(header.hi() - header.lo()) + 1)
                .sum();
            loss.push(TrailerLoss::Share { values });
        }

        if self.header_protection_key.is_none() {
            let data_junk = self.s4;
            for (field, offset) in [("s1", self.s1), ("s2", self.s2), ("s3", self.s3)] {
                let offset = offset.unwrap_or(0);
                if let Some(above) = Self::bytes_into_clear_header(offset, data_junk.unwrap_or(0)) {
                    loss.push(TrailerLoss::ReadsHeader {
                        field,
                        above,
                        data_junk,
                    });
                }
            }
        }
        loss
    }

    /// How far above `data_junk`, a data packet's `s4`, `offset` stands, where that reads into its
    /// clear header.
    pub fn bytes_into_clear_header(offset: u16, data_junk: u16) -> Option<u16> {
        let above = offset.saturating_sub(data_junk);
        (1..=CLEAR_HEADER_REACH).contains(&above).then_some(above)
    }

    /// Unset ones at their defaults.
    pub fn magic_headers(&self) -> [Range<u32>; 4] {
        let [d1, d2, d3, d4] = DEFAULT_MAGIC_HEADERS;
        [
            self.h1.unwrap_or(d1.into()),
            self.h2.unwrap_or(d2.into()),
            self.h3.unwrap_or(d3.into()),
            self.h4.unwrap_or(d4.into()),
        ]
    }

    /// What the module refuses at `SetDevice` or mishandles after.
    pub fn validate(&self) -> Result<(), ObfuscationError> {
        if self.header_protection_key.is_some() {
            let too_small: Vec<_> = [
                ("s1", self.s1),
                ("s2", self.s2),
                ("s3", self.s3),
                ("s4", self.s4),
            ]
            .into_iter()
            .filter(|(_, size)| size.unwrap_or(0) < HEADER_PROTECTION_NONCE_SIZE)
            .map(|(name, _)| name)
            .collect();
            if !too_small.is_empty() {
                return Err(ObfuscationError::NoRoomForNonce { too_small });
            }
        }

        // The module refuses overlapping magic headers with a bare EINVAL.
        let headers = self.magic_headers();
        for i in 0..4 {
            for j in i + 1..4 {
                if headers[i].overlaps(headers[j]) {
                    return Err(ObfuscationError::HeadersOverlap {
                        first: (i + 1, headers[i]),
                        second: (j + 1, headers[j]),
                    });
                }
            }
        }

        // The module draws as written, only an all-zero range being unset: a draw of 0 fires at
        // once, or gives up a handshake after one retry.
        for (name, value) in [
            ("rekey_after_time", self.rekey_after_time),
            ("rekey_timeout", self.rekey_timeout),
            ("reject_after_time", self.reject_after_time),
            ("keepalive_timeout", self.keepalive_timeout),
            ("max_handshake_attempts", self.max_handshake_attempts),
        ] {
            if let Some(range) = value
                && range.starts_at_zero()
            {
                return Err(ObfuscationError::RangeStartsAtZero { name, range });
            }
        }

        // The key ends at the module's constant whatever reject_after_time says: a renewal drawn at
        // that age or later comes after it.
        if let Some(range) = self.rekey_after_time
            && range.hi() >= REJECT_AFTER_TIME
        {
            return Err(ObfuscationError::RekeyAfterKeyIsGone(range));
        }

        // Due at reject_after_time less the other two's low ends: zero or less renews after every
        // handshake.
        let (reject, keepalive, rekey) = (
            least(self.reject_after_time, REJECT_AFTER_TIME),
            least(self.keepalive_timeout, KEEPALIVE_TIMEOUT),
            least(self.rekey_timeout, REKEY_TIMEOUT),
        );
        if reject <= keepalive + rekey {
            return Err(ObfuscationError::RejectTooEarly {
                reject,
                keepalive,
                rekey,
            });
        }

        // With jc and jmax non-zero, each junk size is drawn from jmin..=jmax into a jmax buffer,
        // order unchecked.
        let (jc, jmin, jmax) = (
            self.jc.unwrap_or(0),
            self.jmin.unwrap_or(0),
            self.jmax.unwrap_or(0),
        );
        if jc != 0 && jmax != 0 && jmin > jmax {
            return Err(ObfuscationError::JunkSizesReversed { jmin, jmax });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header_protected(junk_sizes: [Option<u16>; 4]) -> Obfuscation {
        let [s1, s2, s3, s4] = junk_sizes;
        Obfuscation {
            header_protection_key: Some("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".into()),
            s1,
            s2,
            s3,
            s4,
            ..Obfuscation::default()
        }
    }

    #[test]
    fn a_header_protection_key_needs_every_junk_size_to_hold_its_nonce() {
        let no_room = |too_small: &[&'static str]| {
            Err(ObfuscationError::NoRoomForNonce {
                too_small: too_small.to_vec(),
            })
        };
        for (junk_sizes, expected, told) in [
            (
                [Some(12), Some(12), Some(12), Some(11)],
                no_room(&["s4"]),
                "needs s4 to be",
            ),
            (
                [None; 4],
                no_room(&["s1", "s2", "s3", "s4"]),
                "needs s1, s2, s3, s4 to be",
            ),
            ([Some(12); 4], Ok(()), ""),
        ] {
            let checked = header_protected(junk_sizes).validate();
            assert_eq!(checked, expected, "{junk_sizes:?}");
            let message = checked.map_or_else(|refusal| refusal.to_string(), |()| String::new());
            assert!(message.contains(told), "{junk_sizes:?}: {message}");
        }
    }

    // An unset magic header is sent as its WireGuard message type, 1..4.
    #[test]
    fn rejects_magic_headers_that_share_a_value_an_unset_one_holding_its_default() {
        let overlap = |first, second| Err(ObfuscationError::HeadersOverlap { first, second });
        let range = |lo, hi| Range::new(lo, hi).unwrap();
        for (yaml, expected, told) in [
            (
                "{h1: 2}",
                overlap((1, Range::from(2)), (2, Range::from(2))),
                &["obfuscation.h1 (2)", "obfuscation.h2 (2)"][..],
            ),
            (
                "{h4: 3-9}",
                overlap((3, Range::from(3)), (4, range(3, 9))),
                &["obfuscation.h3 (3)", "obfuscation.h4 (3-9)"],
            ),
            (
                "{h1: 5-9, h2: 10, h3: 9-12, h4: 13}",
                overlap((1, range(5, 9)), (3, range(9, 12))),
                &["obfuscation.h1 (5-9)", "obfuscation.h3 (9-12)"],
            ),
            (
                "{h2: 7, h4: 7}",
                overlap((2, Range::from(7)), (4, Range::from(7))),
                &["obfuscation.h2 (7)", "obfuscation.h4 (7)"],
            ),
            ("{h1: 5-9, h2: 10, h3: 11-12, h4: 13}", Ok(()), &[]),
        ] {
            let checked = from_yaml(yaml).validate();
            assert_eq!(checked, expected, "{yaml}");
            let message = checked.map_or_else(|refusal| refusal.to_string(), |()| String::new());
            for text in told {
                assert!(message.contains(text), "{yaml}: {message}");
            }
        }
    }

    fn from_yaml(yaml: &str) -> Obfuscation {
        sliplib_yaml::from_str(yaml).unwrap()
    }

    fn refusal(yaml: &str) -> String {
        from_yaml(yaml).validate().unwrap_err().to_string()
    }

    // The module draws from the range as written: a draw of 0 is a timer that fires at once.
    #[test]
    fn rejects_a_timing_range_that_starts_at_zero_and_names_it() {
        for (key, least_accepted) in [
            ("rekey_after_time", 1),
            ("rekey_timeout", 1),
            // Its low end also has to stay above the other two timeouts.
            ("reject_after_time", 16),
            ("keepalive_timeout", 1),
            ("max_handshake_attempts", 1),
        ] {
            let err = refusal(&format!("{{{key}: 0-150}}"));
            assert!(err.contains(key) && err.contains("0-150"), "{err}");
            let accepted = from_yaml(&format!("{{{key}: {least_accepted}-150}}"));
            assert!(accepted.validate().is_ok(), "{key}");
        }
    }

    #[test]
    fn accepts_a_content_padding_addition_that_starts_at_zero() {
        assert!(
            from_yaml("{content_padding_addition: 0-128}")
                .validate()
                .is_ok()
        );
    }

    // The module subtracts the low ends of the other two from reject_after_time to decide when a
    // key is due for renewal; a result of zero or less renews it at once, every time. 180, 10 and 5
    // are what it uses for one that is unset or zero.
    #[test]
    fn rejects_a_reject_after_time_not_above_keepalive_timeout_plus_rekey_timeout() {
        let too_early = |reject, keepalive, rekey| {
            Err(ObfuscationError::RejectTooEarly {
                reject,
                keepalive,
                rekey,
            })
        };
        for (yaml, expected, told) in [
            (
                "{reject_after_time: 30-90, keepalive_timeout: 20-25, rekey_timeout: 10-15}",
                too_early(30, 20, 10),
                &[
                    "reject_after_time (30)",
                    "keepalive_timeout (20)",
                    "rekey_timeout (10)",
                ][..],
            ),
            (
                "{reject_after_time: 31-90, keepalive_timeout: 20-25, rekey_timeout: 10-15}",
                Ok(()),
                &[],
            ),
            (
                "{reject_after_time: 15}",
                too_early(15, 10, 5),
                &["reject_after_time (15)"],
            ),
            ("{reject_after_time: 16}", Ok(()), &[]),
            (
                "{keepalive_timeout: 175}",
                too_early(180, 175, 5),
                &["keepalive_timeout (175)"],
            ),
            ("{keepalive_timeout: 174}", Ok(()), &[]),
            (
                "{rekey_timeout: 170}",
                too_early(180, 10, 170),
                &["rekey_timeout (170)"],
            ),
            ("{rekey_timeout: 169}", Ok(()), &[]),
            (
                "{reject_after_time: 0, keepalive_timeout: 175}",
                too_early(180, 175, 5),
                &["reject_after_time (180)"],
            ),
            (
                "{reject_after_time: 16, keepalive_timeout: 0, rekey_timeout: 7}",
                too_early(16, 10, 7),
                &["keepalive_timeout (10)"],
            ),
            (
                "{reject_after_time: 16, keepalive_timeout: 12, rekey_timeout: 0}",
                too_early(16, 12, 5),
                &["rekey_timeout (5)"],
            ),
        ] {
            let checked = from_yaml(yaml).validate();
            assert_eq!(checked, expected, "{yaml}");
            let message = checked.map_or_else(|refusal| refusal.to_string(), |()| String::new());
            for text in told {
                assert!(message.contains(text), "{yaml}: {message}");
            }
        }
    }

    // The module stops using a key 180 seconds after its handshake, whatever reject_after_time
    // says: a renewal drawn at 180 or later comes after the key is gone.
    #[test]
    fn rejects_a_rekey_after_time_that_can_reach_180_and_names_it() {
        for written in ["180", "100-180", "65535"] {
            let err = refusal(&format!("{{rekey_after_time: {written}}}"));
            assert!(
                err.contains(&format!("obfuscation.rekey_after_time ({written})")),
                "{err}"
            );
            assert!(err.contains("below 180"), "{err}");
        }
        assert!(from_yaml("{rekey_after_time: 179}").validate().is_ok());
        assert!(from_yaml("{rekey_after_time: 100-179}").validate().is_ok());
    }

    #[test]
    fn a_larger_reject_after_time_does_not_allow_a_larger_rekey_after_time() {
        let err = refusal("{rekey_after_time: 200, reject_after_time: 300}");
        assert!(err.contains("obfuscation.rekey_after_time (200)"), "{err}");
    }

    #[test]
    fn rejects_jmin_above_jmax_where_junk_is_sent() {
        for (jc, jmin, jmax, expected) in [
            (
                Some(4),
                Some(90),
                Some(80),
                Err(ObfuscationError::JunkSizesReversed { jmin: 90, jmax: 80 }),
            ),
            // The module sends junk packets only when jc and jmax are both non-zero.
            (None, Some(40), None, Ok(())),
            (Some(4), Some(80), Some(80), Ok(())),
        ] {
            let o = Obfuscation {
                jc,
                jmin,
                jmax,
                ..Obfuscation::default()
            };
            let checked = o.validate();
            assert_eq!(checked, expected, "{jc:?} {jmin:?} {jmax:?}");
            if let Err(refusal) = checked {
                let message = refusal.to_string();
                assert!(
                    message.contains("jmin") && message.contains("jmax"),
                    "{message}"
                );
            }
        }
    }

    #[test]
    fn a_fully_unset_obfuscation_serializes_to_an_empty_mapping() {
        let yaml = sliplib_yaml::to_string(&Obfuscation::default()).unwrap();
        assert_eq!(yaml.trim(), "{}");
    }

    fn with_trailers(yaml: &str) -> Obfuscation {
        from_yaml(&format!("{{random_trailers: true, {yaml}}}"))
    }

    #[test]
    fn random_trailers_cost_a_range_in_h1_to_h3_and_an_offset_1_to_15_above_s4() {
        let (unset, off, on) = ("", "random_trailers: false, ", "random_trailers: true, ");
        let above = |field: &'static str, above: u16| TrailerLoss::ReadsHeader {
            field,
            above,
            data_junk: Some(20),
        };
        let protected = format!(
            "header_protection_key: {}=, s1: 21, s2: 22, s3: 23, s4: 20",
            "A".repeat(43)
        );
        for (trailers, rest, loss) in [
            (unset, "h1: 10-19, s1: 21, s4: 20", vec![]),
            (off, "h1: 10-19, s1: 21, s4: 20", vec![]),
            // 10 values, one value, and the one an unset header counts as.
            (
                on,
                "h1: 10-19, h2: 2202",
                vec![TrailerLoss::Share { values: 12 }],
            ),
            (
                on,
                "h1: 1101, h2: 2000-2099, h3: 3000-3999",
                vec![TrailerLoss::Share { values: 1101 }],
            ),
            (on, "h1: 1101, h2: 2202, h3: 3303, h4: 5505-5599", vec![]),
            (on, "jc: 4", vec![]),
            (on, "s1: 20, s4: 20", vec![]),
            (on, "s1: 21, s4: 20", vec![above("s1", 1)]),
            (on, "s2: 35, s4: 20", vec![above("s2", 15)]),
            (on, "s3: 36, s4: 20", vec![]),
            (
                on,
                "s3: 8",
                vec![TrailerLoss::ReadsHeader {
                    field: "s3",
                    above: 8,
                    data_junk: None,
                }],
            ),
            (on, "s1: 19, s4: 20", vec![]),
            (
                on,
                "s1: 24, s2: 28, s3: 60, s4: 20",
                vec![above("s1", 4), above("s2", 8)],
            ),
            (on, protected.as_str(), vec![]),
        ] {
            let o = from_yaml(&format!("{{{trailers}{rest}}}"));
            assert_eq!(o.trailer_loss(), loss, "{trailers}{rest}");
        }
    }

    #[test]
    fn the_share_is_told_in_packets_and_in_percent() {
        let half = TrailerLoss::Share { values: 1 << 31 }.to_string();
        assert!(
            half.contains("2147483648 packets in 4294967296, 50.00%"),
            "{half}"
        );
        let few = TrailerLoss::Share { values: 102 }.to_string();
        assert!(
            few.contains("102 packets in 4294967296, less than 0.01%"),
            "{few}"
        );
        let hundredth = TrailerLoss::Share { values: 429497 }.to_string();
        assert!(hundredth.contains(", 0.01%"), "{hundredth}");
    }

    #[test]
    fn an_offset_is_told_in_bytes_and_against_the_s4_in_force() {
        let reads = |above, data_junk| {
            TrailerLoss::ReadsHeader {
                field: "s1",
                above,
                data_junk,
            }
            .to_string()
        };
        let one = reads(1, Some(20));
        assert!(one.contains("s1 stands 1 byte above s4,"), "{one}");
        let several = reads(8, Some(20));
        assert!(several.contains("s1 stands 8 bytes above s4,"), "{several}");
        // Nothing to look for in the file: the line says what an unset `s4` counts as.
        let unset = reads(8, None);
        assert!(
            unset.contains("s1 stands 8 bytes above s4, which is not set and counts as 0,"),
            "{unset}"
        );
    }

    #[test]
    fn a_header_protection_key_does_not_keep_a_range_from_costing_its_share() {
        let mut o = header_protected([Some(21), Some(22), Some(23), Some(20)]);
        o.random_trailers = Some(true);
        o.h2 = Some("2200-2299".parse().unwrap());
        assert_eq!(o.trailer_loss(), [TrailerLoss::Share { values: 102 }]);
    }

    #[test]
    fn a_range_and_an_offset_into_the_header_are_both_told() {
        let o = with_trailers("h3: 7700-7799, s3: 28, s4: 20");
        assert_eq!(
            o.trailer_loss(),
            [
                TrailerLoss::Share { values: 102 },
                TrailerLoss::ReadsHeader {
                    field: "s3",
                    above: 8,
                    data_junk: Some(20),
                }
            ]
        );
    }
}
