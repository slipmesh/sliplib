//! `rw add`/`rw del`/`rw inspect`: the clients of a roadwarriors pool, edited in the pool's own
//! document and rendered as the config a client imports.

use crate::file::{read_slipmesh, write_edits};
use crate::{edit, roadwarrior};
use anyhow::{Context, Result};
use clap::Subcommand;
use sliplib_config::mesh_config;
use sliplib_config::slipmesh_file::SlipmeshFile;
use sliplib_mesh::mint;
use sliplib_secrets::wireguard::PrivateKey;
use sliplib_yaml::MultiDoc;
use std::path::{Path, PathBuf};

#[derive(Subcommand)]
pub enum Command {
    /// Add a client to a roadwarriors pool in slipmesh.yaml.
    Add {
        /// Which roadwarriors pool (its document's `name`, e.g. `plain`).
        #[arg(long = "if")]
        if_: String,
        /// Client name (must not already exist in the pool).
        #[arg(long)]
        name: String,
        /// Comma-separated CIDRs (v4/v6 mixed OK). A bare address gets /32 (v4) or /128 (v6).
        #[arg(long = "allowed-ips")]
        allowed_ips: String,
        /// Client already has its own keypair - only its public half is ever given to us.
        #[arg(long = "public-key")]
        public_key: Option<String>,
        /// The client's Endpoint: one of the pool's node_hostnames (default: the first one), or
        /// any other host or address taken as written, the pool's listen_port added when it
        /// carries no port of its own.
        #[arg(long)]
        endpoint: Option<String>,
        /// Print a ready-to-import client config to stdout.
        #[arg(long)]
        export: bool,
        /// Print the client config as an in-terminal QR code too.
        #[arg(long)]
        qr: bool,
        /// Swap dark/light QR modules. A dark-themed terminal renders "dark" modules as the
        /// foreground text color and "light" ones as the background - visually the opposite of
        /// standard (dark-on-light) QR polarity. Confirmed: the official WireGuard app's own
        /// scanner rejects that, while AmneziaWG's and a plain camera don't care either way.
        #[arg(long)]
        invert: bool,
        /// Write the generated private key into the client's entry in place of its public key, so
        /// rw-inspect can re-render the full config later.
        #[arg(long = "keep-private")]
        keep_private: bool,
        #[arg(long, default_value = "slipmesh.yaml")]
        config: PathBuf,
    },
    /// Remove a client from a roadwarriors pool in slipmesh.yaml.
    Del {
        #[arg(long = "if")]
        if_: String,
        #[arg(long)]
        name: String,
        #[arg(long, default_value = "slipmesh.yaml")]
        config: PathBuf,
    },
    /// Re-render an existing client's config/QR without changing slipmesh.yaml.
    Inspect {
        #[arg(long = "if")]
        if_: String,
        #[arg(long)]
        name: String,
        /// This client's private key, if you have it and rw-add did not keep it - fills in the
        /// config in full instead of a placeholder.
        #[arg(long = "private-key")]
        private_key: Option<PrivateKey>,
        /// The client's Endpoint: one of the pool's node_hostnames (default: the first one), or
        /// any other host or address taken as written, the pool's listen_port added when it
        /// carries no port of its own.
        #[arg(long)]
        endpoint: Option<String>,
        #[arg(long)]
        export: bool,
        #[arg(long)]
        qr: bool,
        /// Swap dark/light QR modules. A dark-themed terminal renders "dark" modules as the
        /// foreground text color and "light" ones as the background - visually the opposite of
        /// standard (dark-on-light) QR polarity. Confirmed: the official WireGuard app's own
        /// scanner rejects that, while AmneziaWG's and a plain camera don't care either way.
        #[arg(long)]
        invert: bool,
        #[arg(long, default_value = "slipmesh.yaml")]
        config: PathBuf,
    },
}

pub fn run(command: Command) -> Result<()> {
    match command {
        Command::Add {
            if_,
            name,
            allowed_ips,
            public_key,
            endpoint,
            export,
            qr,
            invert,
            keep_private,
            config,
        } => rw_add(
            &if_,
            &name,
            &allowed_ips,
            public_key.as_deref(),
            endpoint.as_deref(),
            export,
            qr,
            invert,
            keep_private,
            &config,
        ),
        Command::Del { if_, name, config } => rw_del(&if_, &name, &config),
        Command::Inspect {
            if_,
            name,
            private_key,
            endpoint,
            export,
            qr,
            invert,
            config,
        } => rw_inspect(
            &if_,
            &name,
            private_key.as_ref(),
            endpoint.as_deref(),
            export,
            qr,
            invert,
            &config,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn rw_add(
    if_: &str,
    name: &str,
    allowed_ips: &str,
    public_key: Option<&str>,
    endpoint: Option<&str>,
    export: bool,
    qr: bool,
    invert: bool,
    keep_private: bool,
    config_path: &Path,
) -> Result<()> {
    let (mut yaml, file) = read_slipmesh(config_path)?;
    let topology = file.topology();
    let document = pool_document(&file, topology, if_)?;
    // Minted and written in along with the client: a pool key minted for the exported config has
    // to be the one `generate` puts on the wire, not a key that dies with this process.
    let (secrets, minted) = settle_secrets(&mut yaml, &file)?;

    let added = roadwarrior::add(
        topology,
        &secrets,
        if_,
        name,
        allowed_ips,
        public_key,
        endpoint,
        export,
        qr,
        keep_private,
    )?;
    edit::add_client(&mut yaml, document, &added.client)?;
    write_edits(config_path, &yaml)?;
    report_minted(&minted, config_path);
    println!(
        "added {name:?} to roadwarriors pool {if_:?} in {}",
        config_path.display()
    );

    if let Some((_, text)) = added.config {
        if export {
            println!("\n{text}");
        }
        if qr {
            println!("\n{}", roadwarrior::render_qr(&text, invert)?);
        }
    }
    Ok(())
}

fn rw_del(if_: &str, name: &str, config_path: &Path) -> Result<()> {
    let (mut yaml, file) = read_slipmesh(config_path)?;
    let topology = file.topology();
    let document = pool_document(&file, topology, if_)?;

    let (index, client) = roadwarrior::find_client(topology, if_, name)?;
    edit::remove_client(&mut yaml, document, index)?;
    write_edits(config_path, &yaml)?;
    println!(
        "removed {name:?} (public_key {:?}) from roadwarriors pool {if_:?} in {}",
        client.public_key,
        config_path.display()
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn rw_inspect(
    if_: &str,
    name: &str,
    private_key: Option<&PrivateKey>,
    endpoint: Option<&str>,
    export: bool,
    qr: bool,
    invert: bool,
    config_path: &Path,
) -> Result<()> {
    let (_, file) = read_slipmesh(config_path)?;
    let topology = file.topology();
    let (secrets, minted) = mint::resolve(topology);
    // Only this pool's: a config rendered with a key that is never written down would not connect.
    anyhow::ensure!(
        !minted.pools.iter().any(|pool| pool.name == if_),
        "{} lacks the secrets of pool {if_:?} - `slipmesh mint` creates and writes them",
        config_path.display()
    );

    let text = roadwarrior::inspect(topology, &secrets, if_, name, private_key, endpoint)?;

    // Inspecting is pointless with no output at all - default to --export if neither flag was
    // given, unlike rw-add (where registering a client without ever displaying it is legitimate).
    let export = export || !qr;
    if export {
        println!("{text}");
    }
    if qr {
        println!("\n{}", roadwarrior::render_qr(&text, invert)?);
    }
    Ok(())
}

/// The position of pool `name`'s document, erring with the pools there are.
fn pool_document(
    file: &SlipmeshFile,
    topology: &mesh_config::MeshConfig,
    name: &str,
) -> Result<usize> {
    roadwarrior::find_pool(topology, name)?;
    file.pool_document(name)
        .with_context(|| format!("pool {name:?} is in the topology but in no document"))
}

/// The whole topology's secrets, with whatever they had to mint recorded in `yaml`, in the fields
/// it belongs to, and named in the second value - empty when nothing was minted.
fn settle_secrets(
    yaml: &mut MultiDoc,
    file: &SlipmeshFile,
) -> Result<(sliplib_mesh::secrets::ResolvedSecrets, String)> {
    let (resolved, minted) = mint::resolve(file.topology());
    edit::record(yaml, file, &minted)?;
    Ok((resolved, minted.routes().join(", ")))
}

fn report_minted(routes: &str, config_path: &Path) {
    if !routes.is_empty() {
        println!("minted {routes} into {}", config_path.display());
    }
}
