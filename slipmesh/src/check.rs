//! `check`: the whole file through the loader every tool reads it with, then every node's configs
//! computed and checked the way the tool that configures the node would. It writes nothing.

use anyhow::{Context, Result};
use sliplib_config::v2::platform::TalosSettings;
use sliplib_config::v2::{self, File};
use sliplib_mesh::model::{MetricsPorts, Network};
use sliplib_mesh::render::node_configs;
use sliplib_mesh::secrets::ResolvedSecrets;
use sliplib_yaml::MultiDoc;
use std::path::Path;
use std::process::ExitCode;

fn read(config_path: &Path) -> Result<File> {
    File::read(&MultiDoc::read(config_path)?)
        .with_context(|| format!("reading {}", config_path.display()))
}

/// What is wrong with the files as a whole and stops no node by itself, each on its own: one says
/// nothing about the next. A document of a platform whose keys do not read: the settings of
/// `talos` and of `routeros`, and the keys of each `template`. With them, the settings of `talos`
/// where they read, which the metrics ports of its nodes come from.
fn whole_file(config_path: &Path, files: &File) -> (Vec<anyhow::Error>, Option<TalosSettings>) {
    let in_the_file =
        |refusal: v2::Error| anyhow::Error::new(refusal).context(config_path.display().to_string());
    let mut refusals = Vec::new();
    let talos = files
        .talos_settings()
        .map_err(|refusal| refusals.push(in_the_file(refusal)))
        .ok();
    if let Err(refusal) = files.routeros_settings() {
        refusals.push(in_the_file(refusal));
    }
    for template in files.templates() {
        if let Err(refusal) = template.read() {
            refusals.push(in_the_file(refusal));
        }
    }
    (refusals, talos)
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

/// Every node of `network` in its order, each with what refuses its configs: one refusal says
/// nothing about the next node, so all of them are checked.
pub fn nodes<'n>(network: &'n Network, resolved: &ResolvedSecrets) -> Vec<(&'n str, Result<()>)> {
    network
        .nodes
        .iter()
        .map(|host| (host.name.as_str(), node(network, &host.name, resolved)))
        .collect()
}

/// The network of `files`, which every node's configs are computed from: what refuses it refuses
/// them all. The metrics ports are those of `talos`, its settings.
pub fn network(files: &File, talos: Option<&TalosSettings>) -> Result<Network> {
    let mut network = sliplib_mesh::v2::lower(files)?;
    if let Some(settings) = talos {
        network.metrics = MetricsPorts::from(settings);
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
            let (refusals, talos) = whole_file(config_path, &file);
            for refusal in refusals {
                refuse(refusal);
            }
            match network(&file, talos.as_ref()) {
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
