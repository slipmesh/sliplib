//! YAML files read, edited and written; what the documents mean is the caller's.

mod error;
mod file;
mod merge;
mod multidoc;

pub use error::Error;
pub use file::{read, read_or_empty, replace_file, write_if_changed};
pub use merge::merge;
pub use multidoc::{Document, MultiDoc, join};
pub use yaml_serde::{Mapping, Value, from_str, from_value, to_string, to_value};
