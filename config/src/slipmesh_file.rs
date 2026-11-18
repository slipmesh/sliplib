//! `slipmesh.yaml` of whichever version of the format it is written in: every tool reads it here,
//! and the `network` document says which reader takes it. Without `version` it is version 1.

use crate::{v1, v2};
use sliplib_yaml::{MultiDoc, Value};
use std::fmt;

/// The top-level block of every document: what the document is for and which hosts it reaches.
pub const META_KEY: &str = "slipmesh";

type Source = Box<dyn std::error::Error + Send + Sync>;

/// Where a document stands in the file: its kind and the line it starts at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    kind: &'static str,
    line: usize,
}

impl Place {
    pub(crate) fn new(kind: &'static str, line: usize) -> Self {
        Self { kind, line }
    }
}

impl fmt::Display for Place {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "the `kind: {}` document at line {}",
            self.kind, self.line
        )
    }
}

/// What is wrong with a document's `slipmesh:` block.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BlockError {
    #[error("no `slipmesh:` block saying what the document is for")]
    Missing,
    #[error("its `slipmesh:` block")]
    Malformed(#[source] Source),
}

/// `slipmesh.yaml` as the reader of its version gives it.
pub enum SlipmeshFile {
    V1(v1::slipmesh_file::SlipmeshFile),
    V2(v2::File),
}

/// Why `slipmesh.yaml` does not read, as the reader of its version says it.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error(transparent)]
    V1(#[from] v1::slipmesh_file::Error),
    #[error(transparent)]
    V2(#[from] v2::Error),
}

/// Whether a `network` document of `yaml` says a `version`: only version 1 has none.
fn says_its_version(yaml: &MultiDoc) -> bool {
    yaml.documents()
        .iter()
        .filter_map(|document| document.value.get(META_KEY))
        .any(|block| {
            block.get("kind").and_then(Value::as_str) == Some("network")
                && block.get("version").is_some()
        })
}

impl SlipmeshFile {
    pub fn parse(raw: &str) -> Result<Self, Error> {
        let yaml =
            MultiDoc::parse(raw).map_err(|source| v1::slipmesh_file::Error::Yaml(source.into()))?;
        Self::read(&yaml)
    }

    /// `slipmesh.yaml` as `yaml` holds it - for a caller that goes on to edit it.
    pub fn read(yaml: &MultiDoc) -> Result<Self, Error> {
        if says_its_version(yaml) {
            Ok(Self::V2(v2::File::read(yaml)?))
        } else {
            Ok(Self::V1(v1::slipmesh_file::SlipmeshFile::read(yaml)?))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VERSION_1: &str = "slipmesh: {kind: network}\ncluster:\n  bgp_as: 64500\n  \
        loopback_networks: {ipv4: 198.51.100.0/25, ipv6: '3fff:0:0:1::/64'}\nnodes:\n  - {name: \
        node-a, node_id: 0.0.0.1}\n";
    const VERSION_2: &str = include_str!("../../docs/example/mesh.yaml");

    fn refusal(raw: &str) -> String {
        let refusal = SlipmeshFile::parse(raw).err().expect("the file reads");
        format!("{:#}", anyhow::Error::new(refusal))
    }

    #[test]
    fn a_file_whose_network_says_no_version_is_read_as_version_1() {
        let SlipmeshFile::V1(file) = SlipmeshFile::parse(VERSION_1).unwrap() else {
            panic!("read as version 2");
        };
        assert_eq!(file.hosts(), ["node-a"]);
    }

    #[test]
    fn a_file_whose_network_says_its_version_is_read_as_version_2() {
        let SlipmeshFile::V2(file) = SlipmeshFile::parse(VERSION_2).unwrap() else {
            panic!("read as version 1");
        };
        assert_eq!(file.network().nodes.len(), 4);
    }

    #[test]
    fn a_version_on_another_document_does_not_make_a_file_version_2() {
        // The block of a version 1 `patch` takes no `version`, and its reader says so.
        let patch = "---\nslipmesh: {kind: patch, version: 2}\nkind: Example\n";
        let refusal = refusal(&format!("{VERSION_1}{patch}"));
        assert!(refusal.contains("unknown field `version`"), "{refusal}");
    }

    #[test]
    fn a_refusal_reads_as_the_reader_of_the_version_gives_it() {
        let of_version_1 = refusal("slipmesh: {kind: patch}\nkind: Example\n");
        assert_eq!(
            of_version_1,
            "slipmesh.yaml needs exactly one `kind: network` document, found 0"
        );
        let of_version_2 = refusal("slipmesh: {kind: network, version: 3, revision: 1}\n");
        assert!(of_version_2.starts_with("`version: 3`"), "{of_version_2}");
    }

    #[test]
    fn a_text_that_is_not_yaml_is_refused() {
        let refusal = refusal("slipmesh: [unclosed\n");
        assert!(refusal.contains("not valid YAML"), "{refusal}");
    }
}
