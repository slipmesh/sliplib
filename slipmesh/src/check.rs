//! `check`: the whole file through the loader every tool reads it with, then every node's configs
//! computed and checked the way the tool that configures the node would. It writes nothing.

use anyhow::{Context, Result};
use sliplib_config::mesh_config::missing_defaults;
use sliplib_config::slipmesh_file::SlipmeshFile;
use sliplib_mesh::model::Network;
use sliplib_mesh::render::node_configs;
use sliplib_mesh::secrets::ResolvedSecrets;
use sliplib_yaml::MultiDoc;
use std::path::Path;
use std::process::ExitCode;

fn read(config_path: &Path) -> Result<SlipmeshFile> {
    SlipmeshFile::read(&MultiDoc::read(config_path)?)
        .with_context(|| format!("reading {}", config_path.display()))
}

/// What refuses the file whichever node is configured from it: a network setting left to its
/// default, which a tool that publishes it to the nodes takes only as written.
fn whole_file(config_path: &Path, file: &SlipmeshFile) -> Result<()> {
    let unset: Vec<&str> = missing_defaults(file.topology())
        .into_iter()
        .map(|(key, _)| key)
        .collect();
    anyhow::ensure!(
        unset.is_empty(),
        "{} lacks {} - `slipmesh mint` writes their defaults",
        config_path.display(),
        unset.join(", ")
    );
    Ok(())
}

fn node(network: &Network, host: &str, resolved: &ResolvedSecrets) -> Result<()> {
    let Err(refusal) = node_configs(network, host, resolved) else {
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

/// Every node of `file` in its order, each with what refuses its configs: one refusal says nothing
/// about the next node, so all of them are checked.
pub fn nodes<'f>(file: &'f SlipmeshFile, network: &Network) -> Vec<(&'f str, Result<()>)> {
    let resolved = sliplib_mesh::secrets::read(network);
    file.hosts()
        .into_iter()
        .map(|host| (host, node(network, host, &resolved)))
        .collect()
}

/// The network of `file`, which every node's configs are computed from: what refuses it refuses
/// them all.
pub fn network(file: &SlipmeshFile) -> Result<Network> {
    Ok(sliplib_mesh::v1::lower(file.topology())?)
}

/// Prints `<node>: ok` for each node that passes, every refusal on a line of its own, and then
/// every warning, which fails nothing.
pub fn run(config_path: &Path) -> ExitCode {
    let mut passed = ExitCode::SUCCESS;
    let mut refuse = |refusal: anyhow::Error| {
        eprintln!("{refusal:#}");
        passed = ExitCode::FAILURE;
    };
    match read(config_path) {
        Err(refusal) => refuse(refusal),
        Ok(file) => {
            if let Err(refusal) = whole_file(config_path, &file) {
                refuse(refusal);
            }
            match network(&file) {
                Err(refusal) => refuse(refusal),
                Ok(network) => {
                    for (host, checked) in nodes(&file, &network) {
                        match checked {
                            Ok(()) => println!("{host}: ok"),
                            Err(refusal) => refuse(refusal),
                        }
                    }
                    let resolved = sliplib_mesh::secrets::read(&network);
                    for warning in sliplib_mesh::warnings::warnings(&network, &resolved) {
                        eprintln!("warning: {warning}");
                    }
                }
            }
        }
    }
    passed
}
