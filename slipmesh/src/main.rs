//! The one tool that writes `slipmesh.yaml`. Plain text to it: encryption is the caller's.

use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;

mod check;
mod edit;
mod file;
mod fmt;
mod manifest;
mod mint;
mod rekey;

#[derive(Parser)]
#[command(
    name = env!("CARGO_BIN_NAME"),
    bin_name = env!("CARGO_BIN_NAME"),
    version,
    about = "Checks the files of slipmesh.yaml and writes what they lack"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

/// What a write reads, and the one file it writes into.
#[derive(Args)]
struct Edit {
    /// Read; may be repeated.
    #[arg(long = "in", num_args = 1..)]
    inputs: Vec<PathBuf>,
    /// Read too, and the one file written; created with one network document if not there.
    #[arg(long)]
    edit: PathBuf,
}

#[derive(Subcommand)]
enum Command {
    /// Check the files as a whole and every node's configs, writing nothing.
    Check {
        /// Read; may be repeated.
        #[arg(long = "in", num_args = 1.., required = true)]
        inputs: Vec<PathBuf>,
    },
    /// Write the secrets the files lack, and mesh.interface_prefix's reference value where they
    /// leave it out.
    Mint {
        #[command(flatten)]
        files: Edit,
    },
    /// Print the cluster's `Mesh` object, every node's endpoint, writing nothing.
    Manifest {
        /// Read; may be repeated.
        #[arg(long = "in", num_args = 1.., required = true)]
        inputs: Vec<PathBuf>,
    },
    /// Rewrite each file in the form this tool writes it, changing no value.
    Fmt {
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },
    /// Replace one node's mesh key with a new one.
    Rekey {
        /// The node, by its name in the network.
        #[arg(long)]
        node: String,
        #[command(flatten)]
        files: Edit,
    },
}

fn main() -> Result<ExitCode> {
    match Cli::parse().command {
        Command::Check { inputs } => return Ok(check::run(&inputs)),
        Command::Mint { files } => mint::run(&files.inputs, &files.edit)?,
        Command::Manifest { inputs } => manifest::run(&inputs)?,
        Command::Fmt { files } => fmt::run(&files)?,
        Command::Rekey { node, files } => rekey::run(&node, &files.inputs, &files.edit)?,
    }
    Ok(ExitCode::SUCCESS)
}
