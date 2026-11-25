//! YAML files read, edited and written; what the documents mean is the caller's.

mod error;
mod file;
mod laid;
mod merge;
mod multidoc;
mod tag;

pub use error::Error;
pub use file::read;
pub use laid::{Refused, Step, from_value_at, written_by};
pub use merge::{merge, merge_keeping};
pub use multidoc::{Document, MultiDoc, Without, join, strict};
pub use yaml_serde::{Mapping, Value, from_str, from_value, to_string, to_value};
