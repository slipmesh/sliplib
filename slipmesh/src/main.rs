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
mod fmt;
mod mint;
mod rekey;
mod roadwarrior;
mod rw;

#[derive(Parser)]
#[command(
    name = env!("CARGO_BIN_NAME"),
    bin_name = env!("CARGO_BIN_NAME"),
    version,
    about = "Checks slipmesh.yaml, writes what it lacks and edits its road-warrior clients"
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
    /// Write the secrets slipmesh.yaml lacks into it, and the defaults (version 1) or reference
    /// values (version 2) of the keys it leaves out.
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
    /// Add, remove or show a client of a roadwarriors pool.
    Rw {
        #[command(subcommand)]
        command: rw::Command,
    },
}

fn main() -> Result<ExitCode> {
    match Cli::parse().command {
        Command::Check { config } => return Ok(check::run(&config)),
        Command::Mint { config } => mint::run(&config)?,
        Command::Fmt { config } => fmt::run(&config)?,
        Command::Rekey { node, config } => rekey::run(&node, &config)?,
        Command::Rw { command } => rw::run(command)?,
    }
    Ok(ExitCode::SUCCESS)
}
