//! `slipmesh`: the tool for `slipmesh.yaml` itself, whatever platform its nodes run. The file is
//! read as plain text; decrypting it is the caller's.

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;

mod check;

#[derive(Parser)]
#[command(
    name = env!("CARGO_BIN_NAME"),
    bin_name = env!("CARGO_BIN_NAME"),
    version,
    about = "Checks slipmesh.yaml"
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
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Check { config } => check::run(&config),
    }
}
