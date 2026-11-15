//! The `slipmesh` binary run on a file of its own, as a caller would run it.

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

/// A `slipmesh.yaml` in a directory of its own, gone with the value.
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

    /// `slipmesh <command>` on the file, its path given as the last argument.
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
