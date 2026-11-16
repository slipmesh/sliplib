//! AmneziaWG's device-level obfuscation parameters, covering the full protocol surface of the
//! current (v3.0.20260805) kernel module - confirmed field-by-field against
//! amneziawg-linux-kernel-module's `src/netlink.c` `device_policy` table and
//! amneziawg-tools' `src/config.c` (the authoritative list of what's actually user-configurable;
//! the kernel module's own README only documents jc/jmin/jmax/s1/s2/h1-h4).
//!
//! Grouped below by what each knob actually does, matching the struct's own field order, with the
//! constraints `Obfuscation::validate` enforces because the kernel module refuses or mishandles
//! anything else:
//! - jc/jmin/jmax: how many junk packets to send before the handshake, and their size range;
//!   jmin must not exceed jmax.
//! - s1-s4: junk size for each handshake-adjacent packet type - s1/s2 for init/response, s3/s4
//!   (AmneziaWG 3.0) extending the same idea to cookie-reply and transport (data) packets.
//! - h1-h4: magic header replacement values, one per packet type (init/response/cookie/transport),
//!   each a number or a range `lo-hi` out of which the module draws the header of every packet it
//!   sends. Both ends of a tunnel carry the same four; the four must not share a value, an unset
//!   one counting as its WireGuard message type 1..4.
//! - i1-i5: decoy/cover packets with a configurable header spec, sent alongside real traffic.
//! - header_protection_key: encrypts the WireGuard packet header's own type/reserved fields with a
//!   separate key - a distinct obfuscation layer from the junk-packet tricks above. Requires
//!   `AdvancedSecurity` enabled on the peer(s) that should use it (see
//!   [`crate::awg::PeerEntry`]; setting only this field does nothing on its own), and every s1..s4 at
//!   least 12, room for its nonce.
//! - content_padding_addition, rekey_after_time, rekey_timeout, reject_after_time,
//!   keepalive_timeout, max_handshake_attempts: protocol timing/padding constants vanilla
//!   WireGuard hardcodes - configurable here specifically so they stop being a fixed,
//!   fingerprintable signature. Each is a number or a range `lo-hi` with ends up to 65535, out of
//!   which the module draws; each end of a tunnel has its own. Zero or unset leaves a timing
//!   value to the constants the module has for it, and content_padding_addition to WireGuard's
//!   own padding. A range of a timing value starts at 1 or above, rekey_after_time ends below
//!   180, and reject_after_time stays above keepalive_timeout plus rekey_timeout.
//!
//! Not checked: the module stops using a key 180 seconds after its handshake whatever
//! reject_after_time says, so a reject_after_time far from 180 moves the timers that depend on it
//! away from the moment the key really ends.

use crate::Range;
use serde::{Deserialize, Serialize};
use sliplib_secrets::Secret;

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

    /// Base64 of 32 bytes like a WireGuard key, but one both peers share.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header_protection_key: Option<Secret>,

    /// Bytes added to a data packet in place of the padding it would otherwise get: WireGuard's
    /// own, or the random trailer when `random_trailers` is on. With this set, `random_trailers`
    /// still lengthens handshake and cookie messages and no longer touches data packets.
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

    /// AmneziaWG 3.1. Appends a random-length trailer to outgoing packets, sized against what
    /// the peer has been seen to send, so the wire lengths stop being a fingerprint. Receive-side
    /// length checks become "at least" rather than "exactly", so both ends may set it
    /// independently.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub random_trailers: Option<bool>,
    /// AmneziaWG 3.1. Suppresses cookie replies, which are a message type of their own and
    /// therefore a signature. The cost is the load-shedding those replies exist for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disable_cookies: Option<bool>,
}

/// The AmneziaWG module refuses a header protection key unless every junk size s1..s4 can hold
/// its nonce, and says why only in a debug log line that is off by default: userspace gets a bare
/// `EINVAL` from `SetDevice`. `netlink::awg` sends an unset size as 0.
pub const HEADER_PROTECTION_NONCE_SIZE: u16 = 12;

/// What `SetDevice` sends for an unset h1..h4: the real WireGuard message types (`enum
/// message_type` in AmneziaWG's `messages.h`), which a fresh device already defaults to
/// (`device.c`'s `wg_init`). Not 0, which would be a distinct, still obfuscated magic value.
pub const DEFAULT_MAGIC_HEADERS: [u32; 4] = [1, 2, 3, 4];

/// The module's own constants, in seconds. It uses each for the timing value of its name when
/// that is zero, and the first also as the age at which a key is no longer used.
const REJECT_AFTER_TIME: u16 = 180;
const KEEPALIVE_TIMEOUT: u16 = 10;
const REKEY_TIMEOUT: u16 = 5;

/// The least a timing value can come out as: the low end of its range, or the module's constant
/// when it is unset or zero.
fn least(value: Option<Range<u16>>, constant: u16) -> u32 {
    match value {
        Some(range) if !range.is_zero() => range.lo().into(),
        _ => constant.into(),
    }
}

impl Obfuscation {
    /// The magic headers as the kernel module ends up with them, unset ones at their defaults.
    pub fn magic_headers(&self) -> [Range<u32>; 4] {
        let [d1, d2, d3, d4] = DEFAULT_MAGIC_HEADERS;
        [
            self.h1.unwrap_or(d1.into()),
            self.h2.unwrap_or(d2.into()),
            self.h3.unwrap_or(d3.into()),
            self.h4.unwrap_or(d4.into()),
        ]
    }

    /// Rejects combinations the kernel module refuses at `SetDevice` or mishandles afterwards.
    pub fn validate(&self) -> anyhow::Result<()> {
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
            anyhow::ensure!(
                too_small.is_empty(),
                "obfuscation.header_protection_key needs {} to be at least {HEADER_PROTECTION_NONCE_SIZE}, the size of its nonce",
                too_small.join(", ")
            );
        }

        // The module refuses overlapping magic headers with a bare EINVAL.
        let headers = self.magic_headers();
        for i in 0..4 {
            for j in i + 1..4 {
                anyhow::ensure!(
                    !headers[i].overlaps(headers[j]),
                    "obfuscation.h{} ({}) and obfuscation.h{} ({}) share a value; the four magic headers must not overlap, an unset h1..h4 counting as 1..4",
                    i + 1,
                    headers[i],
                    j + 1,
                    headers[j]
                );
            }
        }

        // The module draws from a range as written and treats only an all-zero one as unset: a
        // draw of 0 is a timer that fires at once, or a handshake given up after one retry.
        for (name, value) in [
            ("rekey_after_time", self.rekey_after_time),
            ("rekey_timeout", self.rekey_timeout),
            ("reject_after_time", self.reject_after_time),
            ("keepalive_timeout", self.keepalive_timeout),
            ("max_handshake_attempts", self.max_handshake_attempts),
        ] {
            if let Some(range) = value {
                anyhow::ensure!(
                    range.is_zero() || range.lo() >= 1,
                    "obfuscation.{name} is {range}; a range of it must start at 1 or above, and 0 alone selects the module's own value"
                );
            }
        }

        // The module stops using a key at its own constant, whatever reject_after_time says; a
        // renewal drawn at that age or later comes after the key is gone, and traffic waits for
        // a handshake at every expiry.
        if let Some(range) = self.rekey_after_time {
            anyhow::ensure!(
                range.hi() < REJECT_AFTER_TIME,
                "obfuscation.rekey_after_time ({range}) must end below {REJECT_AFTER_TIME}: the module stops using a key {REJECT_AFTER_TIME} seconds after its handshake, whatever reject_after_time says"
            );
        }

        // The module takes a key for due once it is older than reject_after_time less the low
        // ends of the other two, and a result of zero or less renews it right after every
        // handshake.
        let (reject, keepalive, rekey) = (
            least(self.reject_after_time, REJECT_AFTER_TIME),
            least(self.keepalive_timeout, KEEPALIVE_TIMEOUT),
            least(self.rekey_timeout, REKEY_TIMEOUT),
        );
        anyhow::ensure!(
            reject > keepalive + rekey,
            "obfuscation.reject_after_time ({reject}) must be above keepalive_timeout ({keepalive}) plus rekey_timeout ({rekey}), each by the low end of its range; an unset or zero one counts as the module's own {REJECT_AFTER_TIME}, {KEEPALIVE_TIMEOUT} and {REKEY_TIMEOUT}"
        );

        // When jc and jmax are non-zero, the module draws each junk packet's size from
        // jmin..=jmax, into a buffer of jmax bytes, without checking the order.
        let (jc, jmin, jmax) = (
            self.jc.unwrap_or(0),
            self.jmin.unwrap_or(0),
            self.jmax.unwrap_or(0),
        );
        anyhow::ensure!(
            jc == 0 || jmax == 0 || jmin <= jmax,
            "obfuscation.jmin ({jmin}) must not exceed obfuscation.jmax ({jmax})"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializing_omits_unset_fields_instead_of_writing_null() {
        let cfg = Obfuscation {
            jc: Some(4),
            ..Obfuscation::default()
        };
        let yaml = sliplib_yaml::to_string(&cfg).unwrap();
        assert!(yaml.contains("jc: 4"), "yaml was:\n{yaml}");
        assert!(!yaml.contains("null"), "yaml was:\n{yaml}");
    }

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
    fn rejects_header_protection_key_with_a_junk_size_below_the_nonce() {
        let err = header_protected([Some(12), Some(12), Some(12), Some(11)])
            .validate()
            .unwrap_err()
            .to_string();
        assert!(err.contains("s4"), "{err}");
        assert!(!err.contains("s1"), "{err}");
    }

    #[test]
    fn rejects_header_protection_key_with_junk_sizes_left_unset() {
        let err = header_protected([None; 4])
            .validate()
            .unwrap_err()
            .to_string();
        assert!(err.contains("s1, s2, s3, s4"), "{err}");
    }

    #[test]
    fn accepts_header_protection_key_with_every_junk_size_holding_the_nonce() {
        assert!(header_protected([Some(12); 4]).validate().is_ok());
    }

    #[test]
    fn accepts_small_junk_sizes_without_header_protection() {
        let o = Obfuscation {
            s1: Some(0),
            ..Obfuscation::default()
        };
        assert!(o.validate().is_ok());
    }

    #[test]
    fn accepts_a_fully_unset_obfuscation() {
        assert!(Obfuscation::default().validate().is_ok());
    }

    #[test]
    fn rejects_two_equal_magic_headers() {
        let o = Obfuscation {
            h2: Some(7.into()),
            h4: Some(7.into()),
            ..Obfuscation::default()
        };
        let err = o.validate().unwrap_err().to_string();
        assert!(err.contains("obfuscation.h2 (7)"), "{err}");
        assert!(err.contains("obfuscation.h4 (7)"), "{err}");
    }

    // An unset magic header is sent as its WireGuard message type, 1..4.
    #[test]
    fn rejects_a_magic_header_equal_to_an_unset_ones_default() {
        let o = Obfuscation {
            h1: Some(2.into()),
            ..Obfuscation::default()
        };
        let err = o.validate().unwrap_err().to_string();
        assert!(err.contains("obfuscation.h1 (2)"), "{err}");
        assert!(err.contains("obfuscation.h2 (2)"), "{err}");
    }

    fn from_yaml(yaml: &str) -> Obfuscation {
        sliplib_yaml::from_str(yaml).unwrap()
    }

    fn refusal(yaml: &str) -> String {
        from_yaml(yaml).validate().unwrap_err().to_string()
    }

    #[test]
    fn a_header_or_a_tuning_value_is_read_as_a_number_or_as_a_range() {
        let o = from_yaml("{h1: 5-9, h2: 10, rekey_after_time: 100-140, rekey_timeout: 6}");
        assert_eq!(o.h1, Some(Range::new(5, 9).unwrap()));
        assert_eq!(o.h2, Some(10.into()));
        assert_eq!(o.rekey_after_time, Some(Range::new(100, 140).unwrap()));
        assert_eq!(o.rekey_timeout, Some(6.into()));
        o.validate().unwrap();
    }

    // The module holds each end of a tuning value in 16 bits.
    #[test]
    fn a_tuning_value_above_65535_is_not_read() {
        for key in [
            "content_padding_addition",
            "rekey_after_time",
            "rekey_timeout",
            "reject_after_time",
            "keepalive_timeout",
            "max_handshake_attempts",
        ] {
            let read = |value: u32| {
                sliplib_yaml::from_str::<Obfuscation>(&format!("{{{key}: {value}}}")).is_ok()
            };
            assert!(read(65535) && !read(65536), "{key}");
        }
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

    // Zero alone is the module's "not set".
    #[test]
    fn accepts_zero_for_a_timing_value() {
        let o = from_yaml(
            "{rekey_after_time: 0, rekey_timeout: 0, reject_after_time: 0, keepalive_timeout: 0, \
             max_handshake_attempts: 0}",
        );
        assert!(o.validate().is_ok());
    }

    #[test]
    fn accepts_a_content_padding_addition_that_starts_at_zero() {
        assert!(
            from_yaml("{content_padding_addition: 0-128}")
                .validate()
                .is_ok()
        );
    }

    #[test]
    fn rejects_magic_header_ranges_that_share_a_value_and_names_both() {
        let err = refusal("{h1: 5-9, h2: 10, h3: 9-12, h4: 13}");
        assert!(err.contains("obfuscation.h1 (5-9)"), "{err}");
        assert!(err.contains("obfuscation.h3 (9-12)"), "{err}");
        assert!(
            from_yaml("{h1: 5-9, h2: 10, h3: 11-12, h4: 13}")
                .validate()
                .is_ok()
        );
    }

    #[test]
    fn rejects_a_magic_header_range_that_covers_an_unset_ones_default() {
        let err = refusal("{h4: 3-9}");
        assert!(err.contains("obfuscation.h3 (3)"), "{err}");
        assert!(err.contains("obfuscation.h4 (3-9)"), "{err}");
    }

    // The module subtracts the low ends of the other two from reject_after_time to decide when a
    // key is due for renewal; a result of zero or less renews it at once, every time.
    #[test]
    fn rejects_a_reject_after_time_not_above_keepalive_timeout_plus_rekey_timeout() {
        let err =
            refusal("{reject_after_time: 30-90, keepalive_timeout: 20-25, rekey_timeout: 10-15}");
        assert!(err.contains("reject_after_time (30)"), "{err}");
        assert!(err.contains("keepalive_timeout (20)"), "{err}");
        assert!(err.contains("rekey_timeout (10)"), "{err}");
        let just_above =
            "{reject_after_time: 31-90, keepalive_timeout: 20-25, rekey_timeout: 10-15}";
        assert!(from_yaml(just_above).validate().is_ok());
    }

    // 180, 10 and 5 are what the module uses for one that is unset or zero.
    #[test]
    fn an_unset_or_zero_timing_value_counts_as_the_modules_own_in_that_check() {
        let names = |yaml: &str, value: &str| {
            let err = refusal(yaml);
            assert!(err.contains(value), "{yaml}: {err}");
        };
        names("{reject_after_time: 15}", "reject_after_time (15)");
        assert!(from_yaml("{reject_after_time: 16}").validate().is_ok());
        names("{keepalive_timeout: 175}", "keepalive_timeout (175)");
        assert!(from_yaml("{keepalive_timeout: 174}").validate().is_ok());
        names("{rekey_timeout: 170}", "rekey_timeout (170)");
        assert!(from_yaml("{rekey_timeout: 169}").validate().is_ok());
        names(
            "{reject_after_time: 0, keepalive_timeout: 175}",
            "reject_after_time (180)",
        );
        names(
            "{reject_after_time: 16, keepalive_timeout: 0, rekey_timeout: 7}",
            "keepalive_timeout (10)",
        );
        names(
            "{reject_after_time: 16, keepalive_timeout: 12, rekey_timeout: 0}",
            "rekey_timeout (5)",
        );
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
    fn rejects_jmin_above_jmax() {
        let o = Obfuscation {
            jc: Some(4),
            jmin: Some(90),
            jmax: Some(80),
            ..Obfuscation::default()
        };
        let err = o.validate().unwrap_err().to_string();
        assert!(err.contains("jmin") && err.contains("jmax"), "{err}");
    }

    // The module sends junk packets only when jc and jmax are both non-zero.
    #[test]
    fn accepts_jmin_alone_when_no_junk_is_sent() {
        let o = Obfuscation {
            jmin: Some(40),
            ..Obfuscation::default()
        };
        assert!(o.validate().is_ok());
    }

    #[test]
    fn accepts_jmin_equal_to_jmax() {
        let o = Obfuscation {
            jc: Some(4),
            jmin: Some(80),
            jmax: Some(80),
            ..Obfuscation::default()
        };
        assert!(o.validate().is_ok());
    }

    #[test]
    fn a_fully_unset_obfuscation_serializes_to_an_empty_mapping() {
        let yaml = sliplib_yaml::to_string(&Obfuscation::default()).unwrap();
        assert_eq!(yaml.trim(), "{}");
    }
}
