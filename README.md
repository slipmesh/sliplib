# sliplib

Rust crates shared by the slipmesh components: the Talos extension daemons and their config
generator, the RouterOS tool and the Kubernetes operators.

| crate | what it gives |
| --- | --- |
| [`sliplib-net`](net) | CIDR parsing and arithmetic on `ipnet`'s types |
| [`sliplib-yaml`](yaml) | Multi-document YAML files read, edited and written, comments and layout kept |
| [`sliplib-runtime`](runtime) | Logging and shutdown set up the same way for every daemon and tool |
| [`sliplib-secrets`](secrets) | Secrets `Debug` never prints, and WireGuard keys: decoding, generation, the public key |
| [`sliplib-config`](config) | The patch format: Talos documents and the daemons' configs, with their checks |

The crates are one workspace with one version and one tag. A change to one crate moves the tag for
all of them; for a consumer of another crate that update changes nothing.

## Using a crate

Depend on the crates you need by git tag; nothing is published to crates.io:

```toml
[dependencies]
sliplib-secrets = { git = "https://github.com/slipmesh/sliplib", tag = "v0.1.0" }
```

### `sliplib-net`

CIDR for every component, on the types of [`ipnet`](https://crates.io/crates/ipnet), which this
crate re-exports: a component takes `IpNet`, `Ipv4Net` and `Ipv6Net` from here rather than from
`ipnet` itself, so one change of the version reaches all of them. Containment, masks and
aggregation are `ipnet`'s own methods. On top of them:

- `parse`, `parse_ipv4` and `parse_ipv6` read `"<address>/<prefix length>"` as a config writes it, keeping the
  host bits, with errors that name the input;
- `exclude(net, excluded)` is what `ipnet` lacks: the rest of `net` without `excluded`, as the fewest
  networks, in address order.

### `sliplib-yaml`

Every read, edit and write of a YAML file goes through here, so a component deals in its own types
and values and never in YAML text or files. The crate knows YAML and files, not what the documents
mean.

- `MultiDoc` is a multi-document file: `read` from disk or `parse` from text, then `documents()`,
  each with its position in the file, the line its content starts on and its value. A document of
  only comments is skipped but keeps its position. `Document::take` removes a top-level key, and
  `Document::deserialize` reads the rest into a type, with errors naming the line of the file.
- `MultiDoc::document_mut` edits a document's value, and `MultiDoc::text` writes the file back. The
  values are what is written: comments, and documents of only comments, are not kept.
- `read` reads a file of one document into a type.
- `merge` is JSON Merge Patch (RFC 7386) with key order kept; `join` and `append` put documents
  together.
- `replace_file` replaces a file atomically, keeping its permissions and following a symlink;
  `write_if_changed` writes only what differs, creating the directory; `read_or_empty` reads a file
  that may not exist yet.
- `Value`, `Mapping`, `from_str`, `from_value`, `to_string` and `to_value` are `yaml_serde`'s,
  re-exported so that a component needs no YAML crate of its own.

### `sliplib-config`

The patch format: what the config generator writes for a node, and what its readers take - Talos,
the daemons on the node, and the RouterOS tool. Types and their checks only; reading and writing
the files is `sliplib-yaml`'s.

- `talos`: `Identity` is what Talos keys a document by (`apiVersion`, `kind`, `name`).
  `ExtensionServiceConfig` and `ConfigFile` build and find the document that mounts a service's
  files; `sets_file` says whether a document sets one of them, and `contents_as_text` turns a file's
  contents written as YAML into the string Talos takes.
- `awg`: `AwgConfig` with its interfaces and peers, and `validate`.
- `router`: `RouterConfig` with its identity, iBGP peers, BFD settings, announcements and bypass
  sources, and `validate`.
- `Obfuscation`, AmneziaWG's device-level parameters, with the checks the kernel module would
  otherwise fail on; `MetricsConfig`, where a daemon serves its Prometheus endpoint.

### `sliplib-runtime`

`init_tracing()` logs to stdout through `tracing`, filtered by `RUST_LOG`, with no timestamp and no
colour: Talos's service log and the container runtime stamp every line themselves and store it as text.
Without `RUST_LOG`, or with it empty, it logs INFO and above: Talos starts an extension service with
no environment of its own.
Call it once, first thing in `main`.

`serve(name, shutdown_timeout, work)` runs a daemon's work under
[`tokio-graceful-shutdown`](https://crates.io/crates/tokio-graceful-shutdown). It installs the SIGTERM
and SIGINT handlers before the work starts, and the work gets a `Shutdown` whose `requested()`
completes on either signal. Work that has not returned `shutdown_timeout` after that is dropped and
`serve` fails, so the timeout is set below what the sender of the signal waits before SIGKILL. A
failed work is logged with its whole `anyhow` chain:

```rust,no_run
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    sliplib_runtime::init_tracing();
    sliplib_runtime::serve("daemon", Duration::from_secs(5), async |shutdown| {
        tokio::select! {
            result = run() => result,
            () = shutdown.requested() => Ok(()),
        }
    })
    .await
}

async fn run() -> anyhow::Result<()> {
    std::future::pending().await
}
```

`serve` is the default `serve` feature. A tool that only logs takes the crate with
`default-features = false`, and none of the shutdown dependencies come with it.

### `sliplib-secrets`

`Secret` holds a secret as the text it was given, a password or AmneziaWG's header protection key,
so it is written back unchanged, and prints as `<redacted>` through `Debug`: a structure that holds
one can be logged or shown in a diff without leaking it. The text leaves through `expose()`, and
through serde into the config it is written to.

A WireGuard key is the base64 text of 32 bytes, the form configs and RouterOS carry it in.
`wireguard::PrivateKey` is a private key kept the way a `Secret` is:

```rust
use sliplib_secrets::wireguard::{PrivateKey, decode_key};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let key = PrivateKey::generate();
    let public = key.public_key()?;
    let bytes: [u8; 32] = decode_key(&public)?;
    assert_eq!(format!("{key:?}"), "<redacted>");
    println!("{public} is {} bytes", bytes.len());
    Ok(())
}
```

`==` compares two private keys as WireGuard uses them: RouterOS returns a key in the clamped form
X25519 reduces it to, which differs as text from the key as generated, and the two are equal.

## Development

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo deny --locked check licenses bans sources
```

CI runs the same on every pull request and every push to `main`, for the x86-64 and arm64 musl
targets, and `cargo audit`, which also runs weekly. The licenses and sources a dependency may come
with are the ones [`deny.toml`](deny.toml) allows. Code style: [docs/CODESTYLE.md](docs/CODESTYLE.md).

Before a pull request, mutation testing runs locally; it takes longer than CI should:

```bash
cargo mutants
```

[`cargo-mutants`](https://mutants.rs) changes the code one place at a time and expects a test to
fail each time; [`.cargo/mutants.toml`](.cargo/mutants.toml) lists what it skips and why.

## Releasing

Releases come from `main` only. Run the Release PR workflow, with the version or empty to compute it
from the conventional commits since the last tag:

```bash
gh workflow run release-pr.yml -f version=v0.1.0
```

It opens `chore: release vX.Y.Z` with the workspace version moved and `CHANGELOG.md` regenerated by
git-cliff. Merging it tags the merge commit and creates the GitHub Release from that changelog
section; consumers then move their tag.

## License

MIT OR Apache-2.0, at your option: [LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE).
The crates depend on `x25519-dalek` and `curve25519-dalek` (BSD-3-Clause): a binary built with them
carries their copyright notice.
