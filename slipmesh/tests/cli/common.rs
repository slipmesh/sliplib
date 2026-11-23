//! The `slipmesh` binary run on a file of its own, as a caller would run it.

use sliplib_yaml::{MultiDoc, Value};
use std::path::PathBuf;
use std::process::{Command, Output};

pub struct Ran {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl From<Output> for Ran {
    fn from(output: Output) -> Self {
        Self {
            code: output.status.code(),
            stdout: String::from_utf8(output.stdout).unwrap(),
            stderr: String::from_utf8(output.stderr).unwrap(),
        }
    }
}

pub fn slipmesh() -> Command {
    Command::new(env!("CARGO_BIN_EXE_slipmesh"))
}

/// In a directory of its own, gone with the value.
pub struct Config {
    _dir: tempfile::TempDir,
    pub path: PathBuf,
}

impl Config {
    pub fn holding(content: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("network.yaml");
        std::fs::write(&path, content).unwrap();
        Self { _dir: dir, path }
    }

    pub fn text(&self) -> String {
        std::fs::read_to_string(&self.path).unwrap()
    }

    pub fn documents(&self) -> Vec<Value> {
        MultiDoc::parse(&self.text())
            .unwrap()
            .documents()
            .iter()
            .map(|document| document.value.clone())
            .collect()
    }

    /// The path given last.
    pub fn run(&self, command: &[&str]) -> Ran {
        slipmesh()
            .args(command)
            .arg("--config")
            .arg(&self.path)
            .output()
            .unwrap()
            .into()
    }
}

/// Which has to pass.
pub fn mint(config: &Config) {
    let ran = config.run(&["mint"]);
    assert_eq!(ran.code, Some(0), "{}", ran.stderr);
}

/// No secret written.
pub const MESH: &str = include_str!("../../../docs/example/mesh.yaml");
/// And the groups.
pub const TALOS: &str = include_str!("../../../docs/example/talos.yaml");

/// The number whole, not the start of a longer one.
pub fn names_line(text: &str, line: usize) -> bool {
    let named = format!("line {line}");
    text.match_indices(&named)
        .any(|(at, _)| !text[at + named.len()..].starts_with(|c: char| c.is_ascii_digit()))
}

/// In one file, minted.
pub fn minted() -> Config {
    let config = Config::holding(&format!("{MESH}---\n{TALOS}"));
    mint(&config);
    config
}

/// To the network document.
pub fn edited(config: &Config, edit: impl FnOnce(&mut Value)) {
    let mut yaml = MultiDoc::parse(&config.text()).unwrap();
    edit(yaml.document_mut(0).unwrap());
    std::fs::write(&config.path, yaml.text().unwrap()).unwrap();
}
