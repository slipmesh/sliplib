//! A value AmneziaWG draws from a range: one number, or `lo-hi`, both ends included.

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::marker::PhantomData;
use std::str::FromStr;

mod sealed {
    pub trait Sealed {}
    impl Sealed for u16 {}
    impl Sealed for u32 {}
}

/// The module packs two `u16` into a `u32` and two `u32` into a `u64`.
pub trait End: sealed::Sealed + Copy + Ord + fmt::Display + TryFrom<u64> + Into<u64> {}

impl End for u16 {}
impl End for u32 {}

/// `lo..=hi`, never reversed: the module checks no order, draws anything from a reversed range and
/// matches nothing against it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Range<T> {
    lo: T,
    hi: T,
}

#[derive(Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RangeError {
    /// Neither a number nor two around one dash.
    Malformed(String),
    /// The end, and the width it has to fit.
    TooLarge {
        end: String,
        bits: usize,
    },
    Reversed {
        lo: u64,
        hi: u64,
    },
}

impl fmt::Display for RangeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed(text) => write!(
                f,
                "{text:?} is neither a number nor a range written lo-hi in decimal digits"
            ),
            Self::TooLarge { end, bits } => write!(f, "{end} does not fit in {bits} bits"),
            Self::Reversed { lo, hi } => {
                write!(
                    f,
                    "the range {lo}-{hi} starts above its end; write it {hi}-{lo}"
                )
            }
        }
    }
}

impl std::error::Error for RangeError {}

impl<T: End> Range<T> {
    pub fn new(lo: T, hi: T) -> Result<Self, RangeError> {
        if lo > hi {
            return Err(RangeError::Reversed {
                lo: lo.into(),
                hi: hi.into(),
            });
        }
        Ok(Self { lo, hi })
    }

    pub fn lo(self) -> T {
        self.lo
    }

    pub fn hi(self) -> T {
        self.hi
    }

    pub fn overlaps(self, other: Self) -> bool {
        self.lo <= other.hi && other.lo <= self.hi
    }

    /// The module's "not set".
    pub fn is_zero(self) -> bool {
        self.hi.into() == 0
    }

    /// Whether the module could draw 0, which alone means "not set".
    pub fn starts_at_zero(self) -> bool {
        self.lo.into() == 0 && !self.is_zero()
    }
}

impl Range<u16> {
    /// A reversed one does not compile.
    pub(crate) const fn constant(lo: u16, hi: u16) -> Self {
        assert!(lo <= hi);
        Self { lo, hi }
    }
}

impl<T: End> From<T> for Range<T> {
    fn from(value: T) -> Self {
        Self {
            lo: value,
            hi: value,
        }
    }
}

fn too_large<T: End>(end: &str) -> RangeError {
    RangeError::TooLarge {
        end: end.to_string(),
        bits: 8 * size_of::<T>(),
    }
}

fn end<T: End>(digits: &str, whole: &str) -> Result<T, RangeError> {
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(RangeError::Malformed(whole.to_string()));
    }
    digits
        .parse::<u64>()
        .ok()
        .and_then(|value| T::try_from(value).ok())
        .ok_or_else(|| too_large::<T>(digits))
}

impl<T: End> FromStr for Range<T> {
    type Err = RangeError;

    fn from_str(text: &str) -> Result<Self, RangeError> {
        match text.split_once('-') {
            Some((lo, hi)) => Self::new(end(lo, text)?, end(hi, text)?),
            None => end::<T>(text, text).map(Self::from),
        }
    }
}

impl<T: End> fmt::Display for Range<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.lo == self.hi {
            write!(f, "{}", self.lo)
        } else {
            write!(f, "{}-{}", self.lo, self.hi)
        }
    }
}

// A number where the ends are equal: a file from before ranges reads back unchanged, and a reader
// of numbers alone takes it.
impl<T: End> Serialize for Range<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if self.lo == self.hi {
            serializer.serialize_u64(self.lo.into())
        } else {
            serializer.collect_str(self)
        }
    }
}

struct RangeVisitor<T>(PhantomData<T>);

impl<T: End> Visitor<'_> for RangeVisitor<T> {
    type Value = Range<T>;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a number or a range written lo-hi")
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
        T::try_from(value)
            .map(Range::<T>::from)
            .map_err(|_| E::custom(too_large::<T>(&value.to_string())))
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E> {
        let value = u64::try_from(value)
            .map_err(|_| E::invalid_value(de::Unexpected::Signed(value), &self))?;
        self.visit_u64(value)
    }

    fn visit_str<E: de::Error>(self, text: &str) -> Result<Self::Value, E> {
        text.parse().map_err(E::custom)
    }
}

impl<'de, T: End> Deserialize<'de> for Range<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(RangeVisitor(PhantomData))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed<T: End>(text: &str) -> Result<Range<T>, RangeError> {
        text.parse()
    }

    fn refused_as_too_large<T: End>(end: &str, bits: usize) -> Result<Range<T>, RangeError> {
        Err(RangeError::TooLarge {
            end: end.to_string(),
            bits,
        })
    }

    fn from_yaml<T: End>(yaml: &str) -> Result<Range<T>, String> {
        sliplib_yaml::from_str(yaml).map_err(|e| e.to_string())
    }

    #[test]
    fn a_number_is_the_range_of_that_one_value() {
        let range: Range<u16> = parsed("120").unwrap();
        assert_eq!((range.lo(), range.hi()), (120, 120));
        assert_eq!(range, Range::from(120));
    }

    #[test]
    fn two_numbers_around_a_dash_are_the_ends() {
        let range: Range<u16> = parsed("100-140").unwrap();
        assert_eq!((range.lo(), range.hi()), (100, 140));
        assert_eq!(range, Range::new(100, 140).unwrap());
    }

    #[test]
    fn a_range_whose_ends_are_out_of_order_is_refused_and_named() {
        let reversed = Err(RangeError::Reversed { lo: 140, hi: 100 });
        assert_eq!(parsed::<u16>("140-100"), reversed);
        assert_eq!(Range::<u16>::new(140, 100), reversed);
        assert!(Range::<u16>::new(140, 140).is_ok());
    }

    #[test]
    fn an_end_too_large_for_its_width_is_refused_and_named() {
        assert_eq!(parsed::<u16>("65535").unwrap().hi(), 65535);
        assert_eq!(parsed::<u16>("65536"), refused_as_too_large("65536", 16));
        let shown = parsed::<u16>("65536").unwrap_err().to_string();
        assert!(shown.contains("65536") && shown.contains("16"), "{shown}");
        assert_eq!(parsed::<u16>("1-65536"), refused_as_too_large("65536", 16));
        assert_eq!(parsed::<u32>("65536").unwrap().lo(), 65536);
        assert_eq!(
            parsed::<u32>("4294967296"),
            refused_as_too_large("4294967296", 32)
        );
        // Too large for any width a number is read in.
        assert_eq!(
            parsed::<u32>("99999999999999999999"),
            refused_as_too_large("99999999999999999999", 32)
        );
    }

    #[test]
    fn anything_but_decimal_digits_and_one_dash_is_refused_and_named() {
        for text in [
            "", "-", "5-", "-5", "1-2-3", "0x10", "+5", " 5", "5 ", "1 - 2", "five",
        ] {
            let err = parsed::<u32>(text).unwrap_err().to_string();
            assert!(err.contains(&format!("{text:?}")), "{text:?}: {err}");
        }
    }

    #[test]
    fn it_is_shown_as_a_number_when_its_ends_are_equal_and_as_a_range_otherwise() {
        assert_eq!(Range::<u32>::from(7).to_string(), "7");
        assert_eq!(Range::<u32>::new(5, 9).unwrap().to_string(), "5-9");
    }

    #[test]
    fn yaml_gives_it_as_an_integer_or_as_a_string() {
        assert_eq!(from_yaml::<u16>("120"), Ok(Range::from(120)));
        assert_eq!(
            from_yaml::<u16>("100-140"),
            Range::new(100, 140).map_err(|_| String::new())
        );
        assert_eq!(from_yaml::<u16>("\"120\""), Ok(Range::from(120)));
    }

    #[test]
    fn yaml_values_that_are_no_range_are_refused() {
        assert!(from_yaml::<u16>("70000").is_err());
        assert!(from_yaml::<u16>("-1").is_err());
        assert!(from_yaml::<u16>("1.5").is_err());
        assert!(from_yaml::<u16>("true").is_err());
        assert!(from_yaml::<u16>("[1, 2]").is_err());
        assert!(from_yaml::<u16>("140-100").is_err());
    }

    // A number where the ends are equal: a reader that knows only numbers still takes it.
    #[test]
    fn it_is_written_as_a_number_when_its_ends_are_equal_and_as_a_string_otherwise() {
        let single = sliplib_yaml::to_value(Range::<u16>::from(120)).unwrap();
        assert_eq!(single, sliplib_yaml::Value::from(120u64));
        let range = sliplib_yaml::to_value(Range::<u16>::new(100, 140).unwrap()).unwrap();
        assert_eq!(range, sliplib_yaml::Value::from("100-140"));
    }

    #[test]
    fn ranges_overlap_when_they_share_a_value() {
        let range = |lo, hi| Range::<u32>::new(lo, hi).unwrap();
        assert!(range(5, 9).overlaps(range(9, 12)));
        assert!(range(9, 12).overlaps(range(5, 9)));
        assert!(range(5, 20).overlaps(range(9, 12)));
        assert!(range(7, 7).overlaps(range(7, 7)));
        assert!(!range(5, 8).overlaps(range(9, 12)));
        assert!(!range(9, 12).overlaps(range(5, 8)));
    }

    #[test]
    fn only_the_range_from_zero_to_zero_is_zero() {
        assert!(Range::<u16>::from(0).is_zero());
        assert!(!Range::<u16>::new(0, 5).unwrap().is_zero());
        assert!(!Range::<u16>::from(5).is_zero());
    }

    #[test]
    fn a_range_starts_at_zero_when_it_holds_zero_and_more() {
        assert!(Range::<u16>::new(0, 5).unwrap().starts_at_zero());
        assert!(!Range::<u16>::from(0).starts_at_zero());
        assert!(!Range::<u16>::new(1, 5).unwrap().starts_at_zero());
        assert!(Range::<u32>::new(0, 5).unwrap().starts_at_zero());
        assert!(!Range::<u32>::from(0).starts_at_zero());
        assert!(!Range::<u32>::new(1, 5).unwrap().starts_at_zero());
    }
}
