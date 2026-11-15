//! The `slipmesh` binary run on a file of its own, as a caller would run it.

use sliplib_config::mesh_config::MeshConfig;
use sliplib_config::slipmesh_file::SlipmeshFile;
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

/// A topology with none of its secrets written, and a patch document beside it.
pub const SLIPMESH: &str = r#"slipmesh:
  kind: network
cluster:
  bgp_as: 64512
  loopback_networks: {ipv4: "198.51.100.0/25", ipv6: "3fff::/32"}
nodes:
  - {name: a, node_id: "198.51.100.1", endpoint: "192.0.2.10"}
  - {name: b, node_id: "198.51.100.2", endpoint: "192.0.2.11"}
  - {name: c, node_id: "198.51.100.3"}
  - {name: d, node_id: "198.51.100.4"}
mesh:
  links:
    - {pair: [a, b], port: 51820}
---
slipmesh:
  kind: patch
  include: [a]
apiVersion: v1alpha1
kind: UnattendedInstallConfig
installer:
    disk: /dev/vda
"#;

/// `slipmesh mint` on the file, which has to pass.
pub fn mint(config: &Config) {
    let ran = config.run(&["mint"]);
    assert_eq!(ran.code, Some(0), "{}", ran.stderr);
}

pub fn topology(config: &Config) -> MeshConfig {
    SlipmeshFile::parse(&config.text()).unwrap().into_topology()
}
