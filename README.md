# sliplib

Rust crates shared by the slipmesh components: the Talos extension daemons and their config
generator, the RouterOS tool and the Kubernetes operators. With them comes [`slipmesh`](#slipmesh),
the command-line tool for `slipmesh.yaml` itself.

| crate | what it gives |
| --- | --- |
| [`sliplib-net`](net) | CIDR parsing and arithmetic on `ipnet`'s types, and the mesh MTU's floor and annotation |
| [`sliplib-fs`](fs) | Files written whole, and only when their text changes |
| [`sliplib-yaml`](yaml) | Multi-document YAML files read into values, edited and turned back into text |
| [`sliplib-runtime`](runtime) | Logging and shutdown set up the same way for every daemon and tool, and a daemon's Prometheus endpoint |
| [`sliplib-secrets`](secrets) | Secrets `Debug` never prints, and WireGuard keys: decoding, generation, the public key |
| [`sliplib-config`](config) | `slipmesh.yaml`, its schema and its loader, and what every platform's daemons share of their configs, with its checks |
| [`sliplib-mesh`](mesh) | A node's `awg` and `router` configs, computed from `slipmesh.yaml` |

The crates and the tool are one workspace with one version and one tag. A change to one crate moves
the tag for all of them; for a consumer of another crate that update changes nothing.

## `slipmesh`

`slipmesh` works on the files of `slipmesh.yaml` whatever platform its nodes run, and is the one
tool that writes into them: the tools that configure a node only read them. It takes each file as
plain text: a file kept encrypted is decrypted by whatever runs the tool. It reads [format version
2](docs/slipmesh-yaml.md), in one file or [several](docs/slipmesh-yaml.md#several-files), and
refuses a file of version 1, which [the conversion
document](docs/slipmesh-yaml-v1-to-v2.md#converting-a-file-by-hand) turns into version 2.

```bash
slipmesh check --in mesh.yaml secrets.yaml talos.yaml
slipmesh mint --in mesh.yaml talos.yaml --edit secrets.yaml
slipmesh fmt mesh.yaml secrets.yaml talos.yaml
slipmesh manifest --in mesh.yaml > mesh-object.yaml
slipmesh rekey --node node-a --in mesh.yaml talos.yaml --edit secrets.yaml
```

`--in` takes several files and may be repeated. A command that writes reads the file of `--edit`
too, given in `--in` or not, and writes into it alone, replacing it in place; one that is not there
yet is created, its owner's alone, holding one `network` document. It refuses an `--edit` file
that does not hold exactly one `network` document, a change to a value another file holds, a value
a file laid over the `--edit` one takes away, and a write that moves the nodes or the links, naming
the file.

Writing anything rewrites the whole file from its values, in the layout of the YAML serializer.
Comments outside block scalars are not kept; a document that holds nothing - only comments, empty,
or `~` - is dropped; an anchor and its aliases are written out as independent copies. So the first
write to a file written by hand changes most of its lines; [`fmt`](#fmt) makes that change alone.
A command with nothing to write leaves the file as it is.

### `check`

`check` reads the files through the loader every tool reads them with, then computes every
node's `awg` and `router` configs and checks each the way its node would. It prints `<node>: ok`
for a node that passes and, for one that does not, the first thing that refuses it on a line of
its own. It goes through every node whatever the others gave, and exits 1 when anything was
refused. What the loader refuses is one refusal for the whole file, with no node checked, even
when it concerns one node - a `node_id` that is not an address, is `0.0.0.0` or is wider than a
network it is added to, two links on one port. It writes nothing.

What leaves no network to compute from is one refusal for the file as well: a file without
`mesh.interface_prefix`, naming `mint`, which writes it. A refusal that `mint` cures says so.

Where nothing was refused, it then prints a warning, on a line that starts with `warning:`, for
every link that loses data packets it should deliver. A warning refuses nothing and leaves the
exit code as it is. There are two, both for a link with `random_trailers` on: one of `h1` to `h3`
is a range, with the share of data packets dropped at most where the bytes read are random; and
the link has no `header_protection_key` while one of `s1` to `s3` stands 1 to 15 bytes above `s4`.
A file with a refusal gets none: what a link loses is counted from settings that pass the checks.

It reads the block of a `platform`, `template`, `patch` or `vars` document and not its body: that
of the first three is the platform's tool's own check, such as the Talos generator's `--check`, and
that of `vars` is passed on as written.

### `mint`

Keys cannot be derived, so `mint` creates them once and writes them into the fields an operator
would otherwise fill in by hand: the `wireguard.private_key` of every node on a link, and the
fields `jc`, `jmin`, `jmax`, `s1`, `s2` and `h1` to `h4` of the `obfuscation` of every link of type
`amneziawg` that leaves them unset, once `mesh.amneziawg.obfuscation` is laid under its own. A
link of type `wireguard` gets no obfuscation, and a node on no link no key. It writes the reference
value of `mesh.interface_prefix`, `mesh-`, where the files leave it out, and prints each entry it
wrote into - `nodes.node-a.wireguard.private_key`, `mesh.links.a-b.amneziawg.obfuscation` - never a
value.

A magic header is created outside every header the link already holds, its own and the ones its
type's obfuscation gives it. When those leave fewer values than the headers it lacks, `mint` writes
nothing and names the entry, `mesh.links.a-b.amneziawg.obfuscation`: narrow the ranges written for
it or for its type, or write all four of its headers.

`s1` and `s2` are created so that neither stands 1 to 15 bytes above the `s4` the link holds, its
own or its type's: such an offset is what [`check`](#check) warns about on a link
with `random_trailers` on and no `header_protection_key`, and either can be switched after the
offsets are written. They are also created so that `s2` is not `s1` + 56, which would give the
two handshake messages one length, whichever of the two the link already holds.

Before it writes, `mint` checks every node on the files as they would become, as `check` does, and
writes nothing when one would be refused: a value that is written is never minted again. Where a
setting of the files' own narrows what fits beside it, a further run draws again.

Written, a value is the operator's like any other: `mint` never replaces one, and deleting one
gives that node or link a new identity on the next run, and every peer of it a config that changes
with it. Only what was created is written: a field set on the link or on its type is not copied
down.

The file holds private keys in the clear; keep it where such files are kept.

### `fmt`

`fmt` rewrites each file it is given in the form every writing command leaves it in and changes no
value, laying nothing over anything: it reads the text it is about to write and refuses to write it
if any document's values differ from the file's. What a hand-written file loses is its layout, its
comments outside block scalars, its documents that hold nothing - only comments, empty, or `~` -
and the link between an anchor and its aliases, each of which becomes a copy. Committed on its own,
that rewrite keeps a later `mint` or `rekey` down to the lines it edits. A file already in that
form is not touched. One whose blocks do not read, or of another version or revision, is refused as
it is, and then no file is written. One line says whether each file was rewritten.

### `rekey`

`rekey --node <name>` gives one node a new `wireguard.private_key`, whether or not it had one,
writing its `wireguard` block where the node has none. Every peer of the node dials it by the
public half of that key, so the config of each of them changes with it and has to reach its node in
the same pass. It refuses a node on no link, which nothing dials, and checks, as `mint` does, that
the files with the new key still give a network before it writes it.

### `manifest`

`manifest` prints the cluster's `Mesh` object, `slipmesh.net/v1alpha1`, cluster-scoped and called
`mesh`: every node of the network by its name, with its `endpoint` as the files write it and none
for a node without one. The agents in the cluster read it, and the egress agent takes every
endpoint out of its lists. It reads the files the way `check` does and writes nothing; apply its
output whenever a node is added or its endpoint changes:

```yaml
apiVersion: slipmesh.net/v1alpha1
kind: Mesh
metadata:
  name: mesh
spec:
  nodes:
    node-a:
      endpoint:
        ipv4: 192.0.2.1
    node-c: {}
```

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

CIDR on [`ipnet`](https://crates.io/crates/ipnet)'s types, re-exported so one version of `ipnet`
reaches every component. On top of its methods:

- `parse`, `parse_ipv4` and `parse_ipv6` read `"<address>/<prefix length>"`, host bits kept, with
  errors that name the input;
- `exclude(net, excluded)`: the rest of `net`, as the fewest networks, in address order;
- `subtract(included, excluded)`: every address of `included` in none of `excluded`, as the fewest
  networks, IPv4 first, then IPv6, each in address order;
- `Family`, IPv4 or IPv6;
- with the feature `netlink`, Linux only, `route`: kernel routes by table and protocol tag in
  either family (`RtClient::route_replace`, `route_del`, `routes`), the main table's default route
  of a family and the link and addresses it leaves through (`default_route`, `uplink`), and a
  wake-up on every link, route and address change (`link_route_and_address_changes`). Another
  daemon's route to the same destination and table stays beside ours when the metrics differ;
  without a metric of its own a route takes the kernel's default, 0 for IPv4 and 1024 for IPv6;
- `mtu::MIN_IPV6_MTU`, 1280, the least MTU of a link that carries IPv6, and
  `mtu::MESH_MTU_ANNOTATION`, the annotation of a node's `Node` object that carries the mesh MTU.

### `sliplib-fs`

A file written through a temporary one beside it, so a reader sees the old text or the new one.

- `write_if_changed` writes a file unless it already holds the text, and says whether it wrote. A
  file it cannot read, such as one that is not UTF-8, is written over. Its directory has to exist:
  a missing one is refused, naming the file.
- `replace_file` replaces a file that exists, keeping its permissions and following a symlink.
- `read_or_empty` reads a file that may not exist.
- `Error`: the step that failed, naming its path.

### `sliplib-yaml`

Every read and edit of a YAML file, and the text to write back, so a component deals in its own
types; `sliplib-fs` writes it. What the documents mean is the caller's.

- `MultiDoc`: `read` or `parse` a multi-document file, a YAML tag refused, then `documents()`,
  each with its position, the line it starts on and its value, or their `values()`.
  `Document::take` removes a top-level key; `Document::deserialize` reads the rest, an error naming
  the line. `document_mut` edits a value and `text` gives the file's text back, without its
  comments or documents of only comments.
- `read`: a file of one document, into a type, a YAML tag refused.
- `merge`: JSON Merge Patch (RFC 7396), key order kept; `merge_keeping` keeps a `null` where the
  caller says; `join` puts documents together.
- `from_value_at`: a value laid from several texts, read whole, a refusal (`Refused`) giving the
  path of its key; `written_by` the last layer holding that path or its deepest part, and
  `Document::line_at` the line of a path in a document's text.
- `Error`: what each of them refuses, naming the file or the document's line.
- `Without`: a mapping as a type with some keys passed over, an unknown key's error naming its line.
  `strict`: a struct read refusing a key it has no field for, the refusal's path reaching the key.
- `Value`, `Mapping`, `from_str`, `from_value`, `to_string` and `to_value`, re-exported from
  `yaml_serde`.

### `sliplib-config`

`slipmesh.yaml`, and what every platform's daemons share of their configs. A key of one daemon
alone, such as its metrics listener, is its platform's tool's.

Each check and the loader returns an error of its own, one variant per refusal: `v2::Error`,
`v2::Invalid`, `v2::NameError`, those of `v2::groups` and `v2::network`, `awg::Invalid`,
`router::Invalid`, `router::InterfacePatternError` and `ObfuscationError`. A variant's text is one line.
A cause it names itself is a field, and one of its own line is its `source`, never both: the whole
chain prints the whole message, each part once.

- `v2`: [format version 2](docs/slipmesh-yaml.md).
  - `Sources::read` reads files as [several files](docs/slipmesh-yaml.md#several-files) are
    read: each once, by its canonical path, in the byte order of those paths, an `edit` file among
    them read as empty and `is_new` where it is not there yet. `check_blocks` refuses what a file
    is refused for alone: a block, a version or a revision. `File` (`open` of
    paths, `read` of `Sources`, `parse` of one text) refuses what its specification has the loader
    refuse, as `v2::Error`; a file of version 1 as `VersionOne`, naming the conversion document. A
    refusal of a document names its file and line; one of the network or the groups as laid
    (`Laid`) names the key's line in the last document that wrote it, or, where the refusal is of a
    mapping as a whole, the mapping's; one about no document (`Files`) names the files read. Each
    names a file once: a document's `include` and `exclude` are refused as `Reach`, naming the
    document, and the groups' own definitions as `Groups`.
  - `network()` gives what is in force under each node's `routing` and in each link's block;
    `laid()` the `network` documents laid over each other, for a write.
  - A node gives its `underlay` and `private_key`; a `Link` whether a node is an end (`has_end`)
    and the `other_end`; a `LinkKind` its `port`, `obfuscation` (empty
    on `wireguard`) and `persistent_keepalive`.
  - `network_documents` are where the network's layers stand, for a write.
    `InterfacePrefix::REFERENCE` is what `slipmesh mint` writes where `mesh.interface_prefix` is
    unset.
  - `templates`, `templates_for` and `patches` give `AimedDocument`s with their `Aim`: what each
    reaches and its rank. `platform` gives a `PlatformDocument`. `vars_for` gives the `vars` that
    reach a node, laid over each other by rank.
  - A body is its platform's tool's: `read` gives it as the tool's type, a refusal naming the
    document and the line. A template or patch also gives its `value`.
- `awg`: `AwgConfig` and `validate`. `InterfaceEntry::keepalive_of` is a peer's keepalive: none
  for one without an `endpoint`; else its own, or 25 on a plain interface and 20 to 30 on an
  obfuscated one. `PeerEntry::allowed_ips_or_all` is a peer's own, or `0.0.0.0/0` and `::/0`.
- `router`: `RouterConfig` and `validate`. `Loopbacks` is the node's pair, written as a list of
  the two. `InterfacePattern` is an entry of `ospf_interfaces` or
  `direct_interfaces`, an interface name or a prefix and `*`: `matches` a name, `name` gives an
  exact one, `is_router_loopback` tells the entry of `ROUTER_LOOPBACK`.
- `Obfuscation`, AmneziaWG's device-level parameters, with the checks the kernel module would fail
  on. `trailer_loss` says how a tunnel with `random_trailers` on loses data packets, as
  `TrailerLoss`. `bytes_into_clear_header` says whether an offset reads a data packet's clear
  header.
- `Prefix`, `Prefix4` and `Prefix6`: an address and its prefix length, host bits kept, read and
  written as `"<address>/<prefix length>"`.
- `Range`: a number or `lo-hi` the module draws from. A range is written as a string, which a
  daemon built before ranges refuses; nothing here checks a node's release.
- `mesh_object`: `MeshSpec`, what [`slipmesh manifest`](#manifest) writes, and with the `kube`
  feature the `Mesh` custom resource the agents read and its CRD, `Mesh::crd()`. A binary that
  enables `kube` also enables one `v1_*` feature of `k8s-openapi`.

### `sliplib-mesh`

A node's `awg` and `router` configs, computed from the file `sliplib-config` reads, for every tool
of every platform.

A refusal is a `render::Error`, naming the node and the config, with its cause as its `source`, or
an `mtu::MtuError`. Files that read but give no mesh are refused as `network::Error`; `Mesh::node`
refuses an unknown name as `network::UnknownNode`, and `Mesh::node_on` a node of another platform
too, as `network::NodeError`.

- `Mesh`: the loader's network (`network()`), its interface prefix and `links_pattern`. `Mesh::of` refuses a
  `v2::File` without `mesh.interface_prefix`. `addresses` gives what a node's `node_id` gives it:
  loopbacks, link-local, tunnel IPv4. `direct_interfaces` appends the links' pattern unless listed,
  where the links carry IPv4. `network::node_key` and `network::link_obfuscation` name where the
  files hold a key and an obfuscation.
- `render::node_configs`: a node's `AwgConfig` and `RouterConfig`, each checked as the node checks
  it. It refuses a missing key of the node, unless it is on no link, or of a peer, and an
  `amneziawg` obfuscation that lacks one of `jc`, `jmin`, `jmax`, `s1`, `s2` and `h1` to `h4`; the
  refusal's `secrets::MissingSecret` names `slipmesh mint`. Nothing of links the node is not on is
  refused. A dialed peer gets its link's keepalive, its type's default included; a peer nobody
  dials gets none. An `amneziawg` interface gets every obfuscation value the daemon sends, one
  left unset as `0` or `false`; the decoys and the header protection key only where set. No
  metrics listener: the Talos generator adds it.
- `mint::create`, behind the `mint` feature, off by default: the secrets the file lacks, by entry,
  no key for a node on no link, or `NoHeaderLeft` where an entry's headers leave no room. Only
  `slipmesh` builds with it.
- `mtu::mesh_mtu`: the largest MTU a full-size packet crosses every link's underlay at
  unfragmented, or `mesh.mtu` where it fits, counting every link's `s4`. It refuses a link limit
  below 1280, a `mesh.mtu` above a link's limit, each naming the link, a `mesh.mtu` below 1280, and
  one set on a mesh without links.
- `addressing`: the address a `node_id` gives, the link-local of a loopback, an endpoint to dial.
- `warnings::warnings`: each link's `TrailerLoss`, in order. A `Warning` displays as the line a
  tool prints, and `concerns` tells whether it is about a node.

### `sliplib-runtime`

`init_tracing()` logs to stdout by `RUST_LOG`, INFO where it is unset or empty, with no timestamp
or colour: Talos's service log and the container runtime stamp each line and store it as text, and
Talos starts an extension service with no environment. Call it first in `main`.

`serve(name, shutdown_timeout, work)` runs a daemon under
[`tokio-graceful-shutdown`](https://crates.io/crates/tokio-graceful-shutdown). Its handlers are in
before `work` starts, and `work`'s `Shutdown::requested()` completes on SIGTERM or SIGINT. Work
still running `shutdown_timeout` later is dropped and `serve` fails: keep it below the sender's wait
before SIGKILL. A failure is logged with its whole `anyhow` chain:

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

`serve` is the default `serve` feature; a tool that only logs takes `default-features = false`.

With the `metrics` feature, on Linux, `metrics` serves a daemon's Prometheus endpoint.
`metrics::bind(listen)` binds even while the address is on no interface yet, such as a loopback
another service creates, and answers once it appears. `metrics::serve(listener, connections,
pages)` answers `GET` on every path `pages` returns a renderer for, rendering on each request; any
other request is a 404 that renders nothing, a page that fails to render is a 500, logged, and past
`connections` at once a client waits. A connection that waits a few seconds for a request head, its
first or its next, is cut, so an idle client holds no place. `Page::openmetrics(&registry)` encodes a
[`prometheus-client`](https://crates.io/crates/prometheus-client) registry with the content type
Prometheus parses it by, and `Page::text(body)` is plain text. A renderer is boxed, so each path can
be an `async` block of its own:

```rust,no_run
use std::num::NonZeroUsize;

use prometheus_client::registry::Registry;
use sliplib_runtime::metrics::{self, Page, Rendering};

async fn run() -> anyhow::Result<()> {
    let listener = metrics::bind("192.0.2.1:9500".parse()?)?;
    metrics::serve(listener, NonZeroUsize::new(16).unwrap(), |path: &str| -> Option<Rendering<anyhow::Error>> {
        match path {
            "/metrics" => Some(Box::pin(async { Ok(Page::openmetrics(&Registry::default())?) })),
            "/prefixes" => Some(Box::pin(async { Ok(Page::text("192.0.2.0/24\n".to_owned())) })),
            _ => None,
        }
    })
    .await;
    Ok(())
}
```

### `sliplib-secrets`

`Secret` holds a secret, such as AmneziaWG's header protection key, as the text it was given, so it
is written back unchanged. `Debug` prints `<redacted>`; the text leaves through `expose()` and
serde.

A WireGuard key is base64 of 32 bytes, as configs and RouterOS carry it. `wireguard::PrivateKey`
is kept as a `Secret` is:

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

`==` compares keys as X25519 uses them: RouterOS returns the clamped form of a key generated
unclamped, and the two are equal.

## Development

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
# the tests that need a network namespace each; a user namespace cannot load `dummy`
sudo modprobe dummy
cargo test --tests --all-features --config "target.'cfg(target_os = \"linux\")'.runner = 'unshare -r'" -- --include-ignored
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
fail each time; [`.cargo/mutants.toml`](.cargo/mutants.toml) lists what it skips and why. Only the
network namespace tests catch the mutants of `sliplib_net::route`, so give them a namespace and
the ignored tests:

```bash
CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER='unshare -r' cargo mutants -- -- --include-ignored
```

The variable names the host's target triple.

What the files give a node is pinned value by value, each in a test of the rule that computes it:
those of [`mesh/src/render.rs`](mesh/src/render.rs) for a node's two configs, and those of
[`config/src/awg.rs`](config/src/awg.rs) and [`config/src/router.rs`](config/src/router.rs) for the
keys the configs are written and read under.

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
