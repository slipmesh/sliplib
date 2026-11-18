//! `check`: the whole file through the loader every tool reads it with, then every node's configs
//! computed and checked the way the tool that configures the node would. It writes nothing.

use anyhow::{Context, Result};
use sliplib_config::slipmesh_file::SlipmeshFile;
use sliplib_config::v1::mesh_config::missing_defaults;
use sliplib_mesh::model::{MetricsPorts, Network};
use sliplib_mesh::render::node_configs;
use sliplib_mesh::secrets::ResolvedSecrets;
use sliplib_yaml::MultiDoc;
use std::path::Path;
use std::process::ExitCode;

fn read(config_path: &Path) -> Result<SlipmeshFile> {
    SlipmeshFile::read(&MultiDoc::read(config_path)?)
        .with_context(|| format!("reading {}", config_path.display()))
}

/// What is wrong with the file as a whole and stops no node by itself. In version 1, the network
/// settings left to a default that `mint` writes. In version 2, a document of a platform whose
/// keys do not read: the settings of `talos` and of `routeros`, and the keys of a `template`.
fn whole_file(config_path: &Path, file: &SlipmeshFile) -> Result<()> {
    match file {
        SlipmeshFile::V1(file) => {
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
        }
        SlipmeshFile::V2(files) => {
            files.talos_settings()?;
            files.routeros_settings()?;
            for template in files.templates() {
                template.read()?;
            }
        }
    }
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
pub fn nodes<'n>(network: &'n Network, resolved: &ResolvedSecrets) -> Vec<(&'n str, Result<()>)> {
    network
        .nodes
        .iter()
        .map(|host| (host.name.as_str(), node(network, &host.name, resolved)))
        .collect()
}

/// The network of `file`, which every node's configs are computed from: what refuses it refuses
/// them all.
pub fn network(file: &SlipmeshFile) -> Result<Network> {
    let mut network = sliplib_mesh::model::lower(file)?;
    // The listeners are settings of `talos`. Settings that do not read are refused with the
    // rest of the file, and leave the nodes to be checked without them.
    if let SlipmeshFile::V2(files) = file
        && let Ok(settings) = files.talos_settings()
    {
        network.metrics = MetricsPorts::from(&settings);
    }
    Ok(network)
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
        Ok(file) => {
            if let Err(refusal) = whole_file(config_path, &file) {
                refuse(refusal);
            }
            match network(&file) {
                Err(refusal) => refuse(refusal),
                Ok(network) => {
                    let resolved = sliplib_mesh::secrets::read(&network);
                    for (host, checked) in nodes(&network, &resolved) {
                        match checked {
                            Ok(()) => println!("{host}: ok"),
                            Err(refusal) => refuse(refusal),
                        }
                    }
                    if passed == ExitCode::SUCCESS {
                        for warning in sliplib_mesh::warnings::warnings(&network, &resolved) {
                            eprintln!("warning: {warning}");
                        }
                    }
                }
            }
        }
    }
    passed
}
