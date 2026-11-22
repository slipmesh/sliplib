//! `check`: the whole file through the loader every tool reads it with, then every node's configs
//! computed and checked the way the tool that configures the node would. It writes nothing.

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

fn node(network: &Mesh<'_>, host: &str) -> Result<()> {
    let Err(refusal) = node_configs(network, host) else {
        return Ok(());
    };
    let cured_by_minting = refusal.missing_secret().is_some();
    let refusal = anyhow::Error::new(refusal);
    Err(if cured_by_minting {
        anyhow::anyhow!("{refusal:#} - `slipmesh mint` creates it")
    } else {
        refusal
    })
}

/// Every node of `network` in its order, each with what refuses its configs: one refusal says
/// nothing about the next node, so all of them are checked.
pub fn nodes<'a>(network: &Mesh<'a>) -> Vec<(&'a str, Result<()>)> {
    network
        .network()
        .nodes
        .keys()
        .map(|host| (host.as_str(), node(network, host.as_str())))
        .collect()
}

/// Prints `<node>: ok` for each node that passes and every refusal on a line of its own. Where
/// nothing was refused it then prints every warning, which fails nothing: a warning is counted
/// from settings that pass the checks.
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
