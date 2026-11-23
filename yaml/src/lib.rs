//! YAML files read, edited and written; what the documents mean is the caller's.

mod error;
mod file;
mod merge;
mod multidoc;
mod tag;

pub use error::Error;
pub use file::read;
pub use merge::merge;
pub use multidoc::{Document, MultiDoc, Without, join};
pub use yaml_serde::{Mapping, Value, from_str, from_value, to_string, to_value};
