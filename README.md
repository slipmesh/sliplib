# sliplib

Rust crates shared by the slipmesh components: the Talos extension daemons and their config
generator, the RouterOS tool and the Kubernetes operators. With them comes [`slipmesh`](#slipmesh),
the command-line tool for `slipmesh.yaml` itself.

| crate | what it gives |
| --- | --- |
| [`sliplib-net`](net) | CIDR parsing and arithmetic on `ipnet`'s types |
| [`sliplib-yaml`](yaml) | Multi-document YAML files read into values, edited and written back |
| [`sliplib-runtime`](runtime) | Logging and shutdown set up the same way for every daemon and tool |
| [`sliplib-secrets`](secrets) | Secrets `Debug` never prints, and WireGuard keys: decoding, generation, the public key |
| [`sliplib-config`](config) | `slipmesh.yaml` and the patch format: the schema and its loader, Talos documents and the daemons' configs, with their checks |
| [`sliplib-mesh`](mesh) | A node's `awg` and `router` configs, computed from `slipmesh.yaml` |

The crates and the tool are one workspace with one version and one tag. A change to one crate moves
the tag for all of them; for a consumer of another crate that update changes nothing.

## `slipmesh`

`slipmesh` works on `slipmesh.yaml` whatever platform its nodes run, and is the one tool that
writes into it: the tools that configure a node only read the file. It takes the file as plain
text: a file kept encrypted is decrypted by whatever runs the tool.

```bash
slipmesh check --config slipmesh.yaml
slipmesh mint --config slipmesh.yaml
slipmesh fmt --config slipmesh.yaml
slipmesh rekey --node node-a --config slipmesh.yaml
slipmesh rw add --if plain --name laptop --allowed-ips 203.0.113.5/32 --export --config slipmesh.yaml
```

`--config` is `slipmesh.yaml` of the working directory unless given. A command that writes
replaces the file in place, so it can be given as an editor command, with `--config` as its last
word and the path after it:

```bash
EDITOR="slipmesh mint --config"
```

Writing anything rewrites the whole file from its values, in the layout of the YAML serializer.
Comments outside block scalars are not kept; a document that holds nothing - only comments, empty,
or `~` - is dropped; an anchor and its aliases are written out as independent copies. So the first
write to a file written by hand changes most of its lines; [`fmt`](#fmt) makes that change alone.
A command with nothing to write leaves the file as it is.

### `check`

`check` reads the whole file through the loader every tool reads it with, then computes every
node's `awg` and `router` configs and checks each the way its node would. It prints `<node>: ok`
for a node that passes and, for one that does not, the first thing that refuses it on a line of
its own. It goes through every node whatever the others gave, and exits 1 when anything was
refused. What the loader refuses is one refusal for the whole file, with no node checked, even
when it concerns one node - a `node_id` that is not an address, two links on one port. It writes
nothing.

It also refuses a file that leaves `cluster.interface_prefix.mesh` or
`cluster.route_proto_id.egress` to its default, and still checks its nodes: a tool that publishes
them to the nodes takes them only as written. A refusal that `mint` cures says so.

What only one platform's tool knows is that tool's own check - the Talos generator's `--check`
for the templates and patches of a Talos node.

### `mint`

Keys cannot be derived, so `mint` creates them once and writes them into the fields an operator
would otherwise fill in by hand: `nodes[].mesh_private_key`, a pool document's `private_key`, and
the fields `jc`, `jmin`, `jmax`, `s1`, `s2` and `h1` to `h4` of the `obfuscation` of a link or pool
that leaves them unset. A `plain` link or pool gets no obfuscation. It writes the defaults of
`cluster.interface_prefix.mesh` (`mesh-`) and `cluster.route_proto_id.egress` (`152`) where the
file leaves them out, and prints each entry it wrote into - `nodes[node-a].mesh_private_key`,
`mesh.links[node-a|node-b].obfuscation` - never a value.

Before it writes, `mint` checks every node on the file as it would become, as `check` does, and
writes nothing when one would be refused: a value that is written is never minted again. Where a
setting of the file's own narrows what fits beside it, a further run draws again.

Written, a value is the operator's like any other: `mint` never replaces one, and deleting one
gives that node, link or pool a new identity on the next run, and every peer of it a config that
changes with it. Only what was created is written: a field set on the entry or in the global
`obfuscation` is not copied down.

The file holds private keys in the clear; keep it where such files are kept.

### `fmt`

`fmt` rewrites the file in the form every writing command leaves it in and changes no value: it
reads the text it is about to write and refuses to write it if any document's values differ from
the file's. What a hand-written file loses is its layout, its comments outside block scalars, its
documents that hold nothing - only comments, empty, or `~` - and the link between an anchor and its
aliases, each of which becomes a copy. Committed on its own, that rewrite keeps a later `rekey` or
`rw add` down to the lines it edits. A file already in that form is not touched, and one that does
not read is refused as it is. One line says whether the file was rewritten.

### `rekey`

`rekey --node <name>` gives one node a new mesh key, whether or not it had one. Every peer of the
node dials it by the public half of that key, so the config of each of them changes with it and
has to reach its node in the same pass.

### `rw add`, `rw del`, `rw inspect`

The clients of a roadwarriors pool, each in the pool's own document. `--if` names the pool,
`--name` the client:

```bash
slipmesh rw add --if plain --name laptop --allowed-ips 203.0.113.5/32 --export --qr
slipmesh rw inspect --if plain --name laptop --qr
slipmesh rw del --if plain --name laptop
```

`rw add` generates the client's key pair, writes the public half into the pool and prints a
ready-to-import config with `--export`, a QR code of it in the terminal with `--qr`; `--invert`
swaps the code's dark and light for a terminal theme whose polarity a scanner refuses. A generated
private key goes into that config, or with `--keep-private` into the file, and nowhere else: with
none of `--export`, `--qr` and `--keep-private` the key would be lost, and `rw add` refuses to
generate one. `--public-key` registers a client that has its own key pair. `--allowed-ips` takes
networks separated by commas; an address without a prefix length is that one host.

With `--keep-private` the entry holds the private key in place of the public one, never both, and
`rw inspect` renders the config in full from it. Without it, `rw inspect` leaves a placeholder for
the key unless `--private-key` gives it back, and refuses one that is not the client's. `rw
inspect` prints the config when neither `--export` nor `--qr` is given, and writes nothing.

A pool whose key or obfuscation is not written takes no client and shows none: `rw add` and
`rw inspect` refuse it, whatever else they were asked for, until `mint` has written them. Only that
pool's secrets matter to them. `rw add` creates nothing but the client's key pair.

The config's `Endpoint` is the first of the pool's `node_hostnames`, with the pool's
`listen_port`. `--endpoint` picks another of them, or names anything else - a DNS name in front of
the pool, say - taken as written, with the pool's port when it carries none. Its `DNS` is the
pool's `dns`, else address 10 of `cluster.service_subnet`, else absent.

### Installing

A release carries the tool built for Linux, statically linked, as `slipmesh-linux-amd64` and
`slipmesh-linux-arm64`, with their checksums in `SHA256SUMS`:

```bash
gh release download --repo slipmesh/sliplib --pattern slipmesh-linux-amd64 --pattern SHA256SUMS
sha256sum --check --ignore-missing SHA256SUMS
install -D -m 0755 slipmesh-linux-amd64 "$HOME/.local/bin/slipmesh"
```

On arm64, `arm64` takes the place of `amd64` in each line. From a checkout,
`cargo install --locked --path slipmesh` builds the tool for the host it runs on, linked the way
that host links, not the static binary of a release.

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
- `merge` is JSON Merge Patch (RFC 7386) with key order kept; `join` puts documents together.
- `replace_file` replaces a file atomically, keeping its permissions and following a symlink;
  `write_if_changed` writes only what differs, creating the directory; `read_or_empty` reads a file
  that may not exist yet.
- `Value`, `Mapping`, `from_str`, `from_value`, `to_string` and `to_value` are `yaml_serde`'s,
  re-exported so that a component needs no YAML crate of its own.

### `sliplib-config`

`slipmesh.yaml`, which every tool reads, and the patch format: what the config generator writes for
a node, and what its readers take - Talos, the daemons on the node, and the RouterOS tool. Types and
their checks only; reading and writing the files is `sliplib-yaml`'s.

- `slipmesh_file`: `SlipmeshFile` reads `slipmesh.yaml` from its text (`parse`) or from a `MultiDoc`
  (`read`): the `slipmesh:` block of every document, the one `network` document, one pool per
  `roadwarriors` document, and `patch` documents aimed at hosts by `include`/`exclude`. It gives the
  hosts in order, the topology, and the Talos documents of one host (`patches_for`), merged defaults
  first. `GENERATED` names the `ExtensionServiceConfig`s generated from the network, which a patch
  may only add files to.
- `mesh_config`: the topology - `MeshConfig` with its cluster settings, nodes, mesh links,
  roadwarriors pools and bypass entries - and `validate`, the checks across them.
  `missing_defaults` lists the network settings the topology leaves to a default that a node is
  told, each with that default.
- `nftables::TEMPLATE_PATH`, where the `nftables` service reads its ruleset template.
- `talos`: `Identity` is what Talos keys a document by (`apiVersion`, `kind`, `name`).
  `ExtensionServiceConfig` and `ConfigFile` build and find the document that mounts a service's
  files; `sets_file` says whether a document sets one of them, and `contents_as_text` turns a file's
  contents written as YAML into the string Talos takes. `merge` merges two documents of one identity
  as a JSON Merge Patch, a `$patch: delete` on either side replaced by the overlay.
  `kube_node_config` is an empty `KubeNodeConfig` and `is_kube_node_config` recognises one;
  `annotate` adds the annotations of one namespace to it, refusing a document that deletes it or
  sets an annotation in that namespace itself.
- `awg`: `AwgConfig` with its interfaces and peers, and `validate`.
- `router`: `RouterConfig` with its identity, iBGP peers, BFD settings, announcements and bypass
  sources, and `validate`.
- `Obfuscation`, AmneziaWG's device-level parameters, with the checks the kernel module would
  otherwise fail on; `MetricsConfig`, where a daemon serves its Prometheus endpoint.

### `sliplib-mesh`

What a node's daemons are configured with, computed from the topology `sliplib-config` reads, for
every tool that configures a node of any platform.

- `secrets::read` takes the secrets the file sets: each node's mesh private key, each pool's private
  key, and each link's and pool's obfuscation layered over the topology's. It never creates one: a
  key the file lacks is absent from `ResolvedSecrets`, while every link and pool gets an obfuscation
  entry, empty when nothing is set or it is `plain`.
- `render::node_configs` gives a node's `AwgConfig` and `RouterConfig`, each checked the way the
  node checks it. It needs the node's own mesh key always; for each of its links, the peer's key
  and the link's obfuscation; for each pool on it, the pool's key and obfuscation. It refuses what
  of these is missing, or an obfuscation not `plain` without one of `jc`, `jmin`, `jmax`, `s1`,
  `s2` and `h1` to `h4`, naming what lacks what; it refuses nothing for the secrets of nodes, links
  and pools the node is not on. Such a refusal carries `secrets::MissingSecret` in its chain
  (`anyhow::Error::is`), so a tool can add how its user creates the secret.
- The `mint` feature, off by default, adds `mint::create`: whatever secrets the file lacks,
  created, each by the entry it belongs to. `slipmesh` builds with it; a tool that only reads the
  file leaves it off, and that function is not in its build.
- `mtu::mesh_mtu` is the MTU of every mesh interface: the largest at which a full-size packet
  crosses every link's underlay unfragmented, or `cluster.mesh_mtu` when it fits. It reads every
  link's obfuscation through its `s4`, links the node is not on included; a missing entry counts as
  no junk.
- `addressing` gives a node's IPv4 loopback from its `node_id`, the link-local of a loopback, and
  an endpoint to dial.

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
with are the ones [`deny.toml`](deny.toml) allows; [`about.toml`](about.toml) repeats the licenses
for the notices a release attaches, rendered through [`about.hbs`](about.hbs), and CI renders them
on every pull request. Code style: [docs/CODESTYLE.md](docs/CODESTYLE.md).

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
git-cliff. Merging it releases it: that push alone builds `slipmesh-linux-{amd64,arm64}`, tags the
commit once the build succeeds, and attaches them with a `SHA256SUMS` and the notices of what is
linked into them to the GitHub Release, whose text is that changelog section. Consumers of the
crates then move their tag.

## License

MIT OR Apache-2.0, at your option: [LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE).
The crates depend on `x25519-dalek` and `curve25519-dalek` (BSD-3-Clause): a binary built with them
carries their copyright notice. A released `slipmesh` comes with the notices of everything linked
into it: `THIRD-PARTY-NOTICES`, `COPYRIGHT-rust-std.html` and `COPYRIGHT-musl`, attached to its
release.
