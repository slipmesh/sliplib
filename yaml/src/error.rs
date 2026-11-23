use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Yaml(#[from] yaml_serde::Error),
    #[error("reading {}", path.display())]
    Reading {
        path: PathBuf,
        #[source]
        source: Box<Error>,
    },
    #[error("not valid YAML")]
    NotYaml(#[source] yaml_serde::Error),
    #[error("a document the parser did not keep")]
    NotKept,
    #[error("the document is no longer in the file")]
    Gone,
    #[error("writing the document starting at line {line}")]
    WritingDocument {
        line: usize,
        #[source]
        source: yaml_serde::Error,
    },
    #[error("`{key}` holds a YAML tag, `{tag}`, which is not read - write the value without it")]
    Tagged { key: String, tag: String },
    #[error("the document starting at line {line}")]
    InDocument {
        line: usize,
        #[source]
        source: Box<Error>,
    },
    #[error("the written YAML is not UTF-8")]
    NotUtf8(#[source] std::string::FromUtf8Error),
}

pub(crate) type Result<T, E = Error> = std::result::Result<T, E>;
