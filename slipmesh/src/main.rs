//! `slipmesh`: the tool for `slipmesh.yaml` itself, whatever platform its nodes run, and the only
//! one that writes into it. The file is plain text to it; decrypting it, and encrypting what a
//! command wrote, is the caller's.

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;

mod check;
mod edit;
mod file;
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
    /// Write the secrets and the defaults slipmesh.yaml lacks into it.
    Mint {
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
        Command::Rekey { node, config } => rekey::run(&node, &config)?,
    }
    Ok(ExitCode::SUCCESS)
}
