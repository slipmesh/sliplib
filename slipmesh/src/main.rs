//! The one tool that writes `slipmesh.yaml`. Plain text to it: encryption is the caller's.

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;

mod check;
mod edit;
mod file;
mod fmt;
mod mint;
mod rekey;

#[derive(Parser)]
#[command(
    name = env!("CARGO_BIN_NAME"),
    bin_name = env!("CARGO_BIN_NAME"),
    version,
    about = "Checks slipmesh.yaml and writes what it lacks"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Check the whole file and every node's configs, writing nothing.
    Check {
        #[arg(long, default_value = "slipmesh.yaml")]
        config: PathBuf,
    },
    /// Write the secrets slipmesh.yaml lacks into it, and mesh.interface_prefix's reference value
    /// where it leaves it out.
    Mint {
        #[arg(long, default_value = "slipmesh.yaml")]
        config: PathBuf,
    },
    /// Rewrite slipmesh.yaml in the form this tool writes it, changing no value.
    Fmt {
        #[arg(long, default_value = "slipmesh.yaml")]
        config: PathBuf,
    },
    /// Replace one node's mesh key in slipmesh.yaml with a new one.
    Rekey {
        /// The node, by its name in the network document.
        #[arg(long)]
        node: String,
        #[arg(long, default_value = "slipmesh.yaml")]
        config: PathBuf,
    },
}

fn main() -> Result<ExitCode> {
    match Cli::parse().command {
        Command::Check { config } => return Ok(check::run(&config)),
        Command::Mint { config } => mint::run(&config)?,
        Command::Fmt { config } => fmt::run(&config)?,
        Command::Rekey { node, config } => rekey::run(&node, &config)?,
    }
    Ok(ExitCode::SUCCESS)
}
