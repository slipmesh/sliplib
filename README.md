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

After the nodes it prints a warning, on a line that starts with `warning:`, for every link that
loses data packets it should deliver. A warning refuses nothing and leaves the exit code as it is.
There are two, both for a link with `random_trailers` on: one of `h1` to `h3` is a range, with the
share of data packets dropped at most; and the link has no `header_protection_key` while one of
`s1` to `s3` stands 1 to 15 bytes above `s4`.

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

A magic header is created outside every header the link or pool already holds, its own and the
ones the network's `obfuscation` gives it. When those leave fewer values than the headers it lacks,
`mint` writes nothing and names the entry, `mesh.links[node-a|node-b].obfuscation`: narrow the
ranges written for it or for the network, or write all four of its headers.

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
pool's secrets matter to them. `rw add` creates nothing but the client's key pair. Both refuse a
pool whose obfuscation the kernel module would refuse, with what a `check` of its node says.

The config carries what the pool and its clients have to share - `S1` to `S4`, `H1` to `H4`, and
`HeaderProtectionKey` and `RandomTrailers` when the pool sets them - and `Jc`, `Jmin`, `Jmax` for
the client's own handshakes. A client has to be new enough for what the pool sets: `amneziawg-tools`
reads `S3` and `S4` from its 2025 releases of 1.0 on, a header written as a range and the header
protection key from 3.0 on, and the trailer switch from 3.1 on; an application with a parser of its
own may differ. An older client cannot be given such a pool. The header protection key is the same
in every client's config of a pool, so it is as secret as all of them together.

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
  networks, in address order;
- `reserved(address)` says whether an address is one no host takes on an interface where it stands
  with that prefix length: the network and broadcast addresses of an IPv4 prefix shorter than /31,
  the Subnet-Router anycast address of an IPv6 prefix shorter than /127. `ipnet` leaves the two
  IPv4 ones out only when it lists the hosts of a network, and has no such rule for IPv6.

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

Every check and the loader return an error type of their own, one variant for each refusal:
`v1::slipmesh_file::Error`, `v1::mesh_config::Invalid`, `awg::Invalid`, `router::Invalid`,
`ObfuscationError`, `MetricsError`, `talos::Error`. A variant's text is one line of the refusal,
and what it was caused by is its `source`, so printing the whole chain gives the whole message.

- `slipmesh_file::SlipmeshFile` reads `slipmesh.yaml` of either version (`parse`, `read`): a file
  whose `network` document says no `version` with the reader of version 1, any other with that of
  version 2. A refusal is the reader's own.
- `v1::slipmesh_file`: `SlipmeshFile` reads `slipmesh.yaml` from its text (`parse`) or from a
  `MultiDoc` (`read`): the `slipmesh:` block of every document, the one `network` document, one pool per
  `roadwarriors` document, and `patch` documents aimed at hosts by `include`/`exclude`. It gives the
  hosts in order, the topology, and the `patch` documents as written: all of them (`patches`) or
  those that reach one host (`patches_for`), in the order of the file. A `Patch` gives its
  document, its `rank` among the documents that reach the same host, and reads as where it stands
  in the file. What a `patch` holds is for the tool of its platform to read and merge.
- `v1::mesh_config`: the topology - `MeshConfig` with its cluster settings, nodes, mesh links,
  roadwarriors pools and bypass entries - and `validate`, the checks across them.
  `missing_defaults` lists the network settings the topology leaves to a default that a node is
  told, each with that default.
- `v2`: `slipmesh.yaml` of [format version 2](docs/slipmesh-yaml.md), which no tool reads yet.
  `v2::File` reads the files as a whole (`parse`, `read`) and refuses whatever its specification
  has the loader refuse, as `v2::Error`. It gives the `network` with what is in force for each
  node and each link written where they hold their own: a node's `routing`, a link's block of its
  type. The `template` and `patch` documents come with their `Aim`: the nodes each reaches, and
  its rank among those that reach one node (`templates_for`, `patches_for`). The settings of a
  platform (`talos_settings`, `routeros_settings`) and the keys of a `template`
  (`TemplateDocument::read`) are read when asked for, so a tool reads only its own platform's.
  The text of a template and the body of a `patch` are their platform's tool's to read.
- `nftables::TEMPLATE_PATH`, where the `nftables` service reads its ruleset template.
- `talos`: `Identity` is what Talos keys a document by (`apiVersion`, `kind`, `name`).
  `ExtensionServiceConfig` and `ConfigFile` build and find the document that mounts a service's
  files; `sets_file` says whether a document sets one of them, and `contents_as_text` turns a file's
  contents written as YAML into the string Talos takes. `merge` merges two documents of one identity
  as a JSON Merge Patch, a `$patch: delete` on either side replaced by the overlay.
  `kube_node_config` is an empty `KubeNodeConfig` and `is_kube_node_config` recognises one;
  `annotate` adds the annotations of one namespace to it, refusing a document that deletes it or
  sets an annotation in that namespace itself.
- `awg`: `AwgConfig` with its interfaces and peers, and `validate`. A peer's `persistent_keepalive`
  is a number of seconds or a range of them. For a peer that has none,
  `InterfaceEntry::keepalive_of` gives the default of what the interface speaks: 25 where it sets
  no obfuscation, 20 to 30 where it sets any. A config holds a keepalive only where it differs
  from that.
- `router`: `RouterConfig` with its identity, iBGP peers, BFD settings, announcements and bypass
  sources, and `validate`.
- `Obfuscation`, AmneziaWG's device-level parameters, with the checks the kernel module would
  otherwise fail on. `Obfuscation::trailer_loss` says how a tunnel with `random_trailers` on loses
  data packets, as `TrailerLoss`: a share of them where one of `h1` to `h3` is a range, or by
  their own header where an offset reads it. `MetricsConfig` is where a daemon serves its
  Prometheus endpoint.
- `Range`, the type of the magic headers and the timing and padding values of `Obfuscation`. The
  kernel module draws each of them from a range, written as one number or as `lo-hi`. A number
  is rendered into a node's config as a number and a range as a string, which a daemon built
  before ranges refuses: write a range only for nodes that run a release that reads it. No tool
  here checks that.

### `sliplib-mesh`

What a node's daemons are configured with, computed from the file `sliplib-config` reads, for
every tool that configures a node of any platform.

A refusal is a `render::Error`, which names the node and which of its configs it is about, or an
`mtu::MtuError`; the cause is the `source` of each. A file that reads and gives no network to
compute from is refused as a `model::Error`.

- `model::Network` is what the configs are computed from, whichever version of the format the file
  is of: every node with the addresses derived for it, every link with the obfuscation and the
  keepalive in force on it, the route classes, the ports of the metrics listeners, and what only a
  file of version 1 describes, its pools and bypass lists. `model::lower` gives it for a
  `SlipmeshFile`; `v1::lower` and `v2::lower` are the two it picks between. The lowering of
  version 2 refuses two nodes that get one address and a link-local no interface takes. The
  metrics ports of a file of version 2 are the `talos` platform's settings, which the lowering
  does not read: the Talos generator sets them on the network.
- `secrets::read` takes the secrets the network holds: each node's private key, each pool's
  private key, and each link's and pool's obfuscation. It never creates one: a key the file lacks
  is absent from `ResolvedSecrets`, while every link and pool gets an obfuscation entry, empty
  when nothing is set or it is `plain`. `secrets::read_pools` takes those of the pools from a
  topology of version 1 itself, for a client's config.
- `render::node_configs` gives a node's `AwgConfig` and `RouterConfig`, each checked the way the
  node checks it. It needs the node's own key always; for each of its links, the peer's key and
  the link's obfuscation; for each pool on it, the pool's key and obfuscation. It refuses what of
  these is missing, or an obfuscation not `plain` without one of `jc`, `jmin`, `jmax`, `s1`, `s2`
  and `h1` to `h4`, naming what lacks what; it refuses nothing for the secrets of nodes, links and
  pools the node is not on. Such a refusal gives its `secrets::MissingSecret` through
  `render::Error::missing_secret`, so a tool can add how its user creates the secret. A peer the
  node dials is given the keepalive of its link where that differs from what
  `InterfaceEntry::keepalive_of` gives without it. The metrics listeners go to the nodes of
  `talos`, and to every node of a file of version 1, which names no platform.
- The `mint` feature, off by default, adds `mint::create`: whatever secrets the file lacks,
  created, each by the entry it belongs to, or `NoHeaderLeft` when an entry's magic headers leave
  no room for the ones it lacks. `slipmesh` builds with it; a tool that only reads the file leaves
  it off, and that function is not in its build.
- `mtu::mesh_mtu` is the MTU of every mesh interface: the largest at which a full-size packet
  crosses every link's underlay unfragmented, or the one the file sets for the mesh when it fits.
  It reads every link's obfuscation through its `s4`, links the node is not on included; a missing
  entry counts as no junk.
- `addressing` gives a node's IPv4 loopback from its `node_id`, the link-local of a loopback, and
  an endpoint to dial.
- `warnings::warnings` lists what the tools accept and tell about: for every link, in order, each
  `TrailerLoss` of its obfuscation. A `Warning` reads as the line a tool prints and says whether it
  `concerns` a node, so a tool run for some nodes can tell about their links alone.

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

[`mesh/tests/golden.rs`](mesh/tests/golden.rs) pins what a file of format version 1 gives its
nodes: the configs of every node of the two fixtures under
[`mesh/tests/golden/`](mesh/tests/golden) are compared byte for byte with the files beside each,
with derived public keys named by their owner. `v1` sets every key of the format, and
`v1-alternate` takes the other side of each key a file sets once. The Talos generator pins its
patch files on copies of the same fixtures, and both tests hold the hash of each, so the copies
change together. A change meant to move the configs records the new ones, and the difference is
reviewed with the rest of the pull request; the test refuses to record in CI:

```bash
UPDATE_EXPECT=1 cargo test -p sliplib-mesh --test golden
```

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
