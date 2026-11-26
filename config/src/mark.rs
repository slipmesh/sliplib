//! A firewall mark: `0x` and hex digits, with `/` and a mask in the same form.

use serde::de::{self, Unexpected, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::num::NonZeroU32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mark {
    pub value: NonZeroU32,
    /// Every bit where none is written, as the kernel compares.
    pub mask: NonZeroU32,
}

fn hex(text: &str) -> Option<u32> {
    let digits = text.strip_prefix("0x")?;
    if !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(digits, 16).ok()
}

struct MarkText;

impl Visitor<'_> for MarkText {
    type Value = Mark;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(
            "a quoted string of `0x` and hexadecimal digits, at most 32 bits, with `/` and a mask \
             in the same form if it has one",
        )
    }

    fn visit_str<E: de::Error>(self, text: &str) -> Result<Mark, E> {
        let (value, mask) = match text.split_once('/') {
            Some((value, mask)) => (hex(value), hex(mask)),
            None => (hex(text), Some(u32::MAX)),
        };
        let (Some(value), Some(mask)) = (value, mask) else {
            return Err(E::invalid_value(Unexpected::Str(text), &self));
        };
        match (NonZeroU32::new(value), NonZeroU32::new(mask)) {
            (Some(value), Some(mask)) => Ok(Mark { value, mask }),
            _ => Err(E::custom(format_args!(
                "the mark {text:?} has a mark or a mask of 0, which Talos takes as none written - \
                 leave the key out for a rule on every packet, or write the bits to compare"
            ))),
        }
    }
}

/// As it is read: without the mask where every bit is compared.
impl fmt::Display for Mark {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#x}", self.value)?;
        if self.mask.get() != u32::MAX {
            write!(f, "/{:#x}", self.mask)?;
        }
        Ok(())
    }
}

impl Serialize for Mark {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Mark {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_str(MarkText)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Through a value, as the loader reads.
    fn read(text: &str) -> Result<Mark, String> {
        let value: sliplib_yaml::Value = sliplib_yaml::from_str(text).unwrap();
        sliplib_yaml::from_value(value).map_err(|error| error.to_string())
    }

    fn mark(value: u32, mask: u32) -> Mark {
        Mark {
            value: NonZeroU32::new(value).unwrap(),
            mask: NonZeroU32::new(mask).unwrap(),
        }
    }

    #[test]
    fn a_mark_reads_with_its_mask_or_with_every_bit() {
        assert_eq!(read("0x100000/0x100000"), Ok(mark(0x10_0000, 0x10_0000)));
        assert_eq!(read("'0x00Ff'"), Ok(mark(0xff, u32::MAX)));
    }

    #[test]
    fn what_is_not_a_mark_of_32_bits_or_is_zero_is_refused() {
        for text in [
            "0x100000",
            "'100000'",
            "'0x'",
            "'0x+1'",
            "'0xg'",
            "'0X10'",
            "'0x100000000'",
            "0x1/",
            "0x1/0x",
            "0x1/0x100000000",
            "0x1/0x1/0x1",
            "'0x0'",
            "0x0/0x1",
            "0x1/0x0",
        ] {
            assert!(read(text).is_err(), "{text}");
        }
    }
}
