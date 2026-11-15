//! `check`: the whole file through the loader every tool reads it with, then every node's configs
//! computed and checked the way the tool that configures the node would. It writes nothing.

use anyhow::{Context, Result};
use sliplib_config::slipmesh_file::SlipmeshFile;
use sliplib_mesh::render::node_configs;
use sliplib_yaml::MultiDoc;
use std::path::Path;
use std::process::ExitCode;

fn nodes(config_path: &Path) -> Result<Vec<(String, Result<()>)>> {
    let file = SlipmeshFile::read(&MultiDoc::read(config_path)?)
        .with_context(|| format!("reading {}", config_path.display()))?;
    let resolved = sliplib_mesh::secrets::read(file.topology());
    Ok(file
        .hosts()
        .into_iter()
        .map(|host| {
            let checked = file
                .effective_for(host)
                .and_then(|model| node_configs(&model, host, &resolved))
                .map(drop);
            (host.to_owned(), checked)
        })
        .collect())
}

/// Prints `<node>: ok` for each node that passes and every refusal on a line of its own: one node
/// refused says nothing about the next, so all of them are checked.
pub fn run(config_path: &Path) -> ExitCode {
    let nodes = match nodes(config_path) {
        Ok(nodes) => nodes,
        Err(refusal) => {
            eprintln!("{refusal:#}");
            return ExitCode::FAILURE;
        }
    };
    let mut passed = ExitCode::SUCCESS;
    for (node, checked) in nodes {
        match checked {
            Ok(()) => println!("{node}: ok"),
            Err(refusal) => {
                eprintln!("{refusal:#}");
                passed = ExitCode::FAILURE;
            }
        }
    }
    passed
}
