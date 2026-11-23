//! `check`: the file through the loader, then every node's configs as its tool would check them.

use anyhow::{Context, Result};
use sliplib_config::v2::File;
use sliplib_mesh::Mesh;
use sliplib_mesh::render::node_configs;
use sliplib_yaml::MultiDoc;
use std::path::Path;
use std::process::ExitCode;

fn read(config_path: &Path) -> Result<File> {
    File::read(&MultiDoc::read(config_path)?)
        .with_context(|| format!("reading {}", config_path.display()))
}

/// In order, each with its refusal: one says nothing of the next.
pub fn nodes<'a>(network: &Mesh<'a>) -> Vec<(&'a str, Result<()>)> {
    network
        .network()
        .nodes
        .keys()
        .map(|host| {
            let checked = node_configs(network, host.as_str()).map(drop);
            (host.as_str(), checked.map_err(anyhow::Error::new))
        })
        .collect()
}

/// `<node>: ok` or the refusal, each on a line. Warnings only where nothing was refused: they are
/// counted from settings that pass.
pub fn run(config_path: &Path) -> ExitCode {
    let mut passed = ExitCode::SUCCESS;
    let mut refuse = |refusal: anyhow::Error| {
        eprintln!("{refusal:#}");
        passed = ExitCode::FAILURE;
    };
    match read(config_path) {
        Err(refusal) => refuse(refusal),
        Ok(file) => match Mesh::of(&file) {
            Err(refusal) => refuse(refusal.into()),
            Ok(network) => {
                for (host, checked) in nodes(&network) {
                    match checked {
                        Ok(()) => println!("{host}: ok"),
                        Err(refusal) => refuse(refusal),
                    }
                }
                if passed == ExitCode::SUCCESS {
                    for warning in sliplib_mesh::warnings::warnings(&network) {
                        eprintln!("warning: {warning}");
                    }
                }
            }
        },
    }
    passed
}
