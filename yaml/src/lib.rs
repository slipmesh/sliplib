//! Every read, edit and write of a YAML file the slipmesh components do. It knows YAML and files,
//! not what the documents mean: a caller brings its own types.

mod file;
mod merge;
mod multidoc;

pub use file::{read_or_empty, replace_file, write_if_changed};
pub use merge::merge;
pub use multidoc::{Document, MultiDoc, Operation, append, join};
pub use yaml_serde::{Mapping, Value, from_str, from_value, to_string, to_value};
