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
| [`sliplib-config`](config) | `slipmesh.yaml`, its schema and its loader, and the configs of the daemons, each with its checks |
| [`sliplib-mesh`](mesh) | A node's `awg` and `router` configs, computed from `slipmesh.yaml` |

The crates and the tool are one workspace with one version and one tag. A change to one crate moves
the tag for all of them; for a consumer of another crate that update changes nothing.

## `slipmesh`

`slipmesh` works on `slipmesh.yaml` whatever platform its nodes run, and is the one tool that
writes into it: the tools that configure a node only read the file. It takes the file as plain
text: a file kept encrypted is decrypted by whatever runs the tool. It reads [format version
2](docs/slipmesh-yaml.md), in one file given with `--config`, and refuses a file of version 1,
which [the conversion document](docs/slipmesh-yaml-v1-to-v2.md#converting-a-file-by-hand) turns
into version 2.

```bash
slipmesh check --config slipmesh.yaml
slipmesh mint --config slipmesh.yaml
slipmesh fmt --config slipmesh.yaml
slipmesh rekey --node node-a --config slipmesh.yaml
```

`--config` is `slipmesh.yaml` of the working directory unless given. A command that writes
replaces the file in place.

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
when it concerns one node - a `node_id` that is not an address, is `0.0.0.0` or is wider than a
network it is added to, two links on one port. It writes nothing.

What leaves no network to compute from is one refusal for the file as well: a file without
`mesh.interface_prefix`, naming `mint`, which writes it. It reads the
settings of each platform and the keys of each `template`, refuses each of those that does not
read, and still checks the nodes. A refusal that `mint` cures says so.

Where nothing was refused, it then prints a warning, on a line that starts with `warning:`, for
every link that loses data packets it should deliver. A warning refuses nothing and leaves the
exit code as it is. There are two, both for a link with `random_trailers` on: one of `h1` to `h3`
is a range, with the share of data packets dropped at most where the bytes read are random; and
the link has no `header_protection_key` while one of `s1` to `s3` stands 1 to 15 bytes above `s4`.
A file with a refusal gets none: what a link loses is counted from settings that pass the checks.

What only one platform's tool knows is that tool's own check - the Talos generator's `--check`
for the text of a template and the body of a `patch` of a Talos node.

### `mint`

Keys cannot be derived, so `mint` creates them once and writes them into the fields an operator
would otherwise fill in by hand: the `wireguard.private_key` of every node on a link, and the
fields `jc`, `jmin`, `jmax`, `s1`, `s2` and `h1` to `h4` of the `obfuscation` of every link of type
`amneziawg` that leaves them unset, once `mesh.amneziawg.obfuscation` is laid under its own. A
link of type `wireguard` gets no obfuscation, and a node on no link no key. It writes the reference
value of `mesh.interface_prefix`, `mesh-`, where the file leaves it out, and prints each entry it
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

Before it writes, `mint` checks every node on the file as it would become, as `check` does, and
writes nothing when one would be refused: a value that is written is never minted again. Where a
setting of the file's own narrows what fits beside it, a further run draws again.

Written, a value is the operator's like any other: `mint` never replaces one, and deleting one
gives that node or link a new identity on the next run, and every peer of it a config that changes
with it. Only what was created is written: a field set on the link or on its type is not copied
down.

The file holds private keys in the clear; keep it where such files are kept.

### `fmt`

`fmt` rewrites the file in the form every writing command leaves it in and changes no value: it
reads the text it is about to write and refuses to write it if any document's values differ from
the file's. What a hand-written file loses is its layout, its comments outside block scalars, its
documents that hold nothing - only comments, empty, or `~` - and the link between an anchor and its
aliases, each of which becomes a copy. Committed on its own, that rewrite keeps a later `mint` or
`rekey` down to the lines it edits. A file already in that form is not touched, and one that does
not read is refused as it is. One line says whether the file was rewritten.

### `rekey`

`rekey --node <name>` gives one node a new `wireguard.private_key`, whether or not it had one,
writing its `wireguard` block where the node has none. Every peer of the node dials it by the
public half of that key, so the config of each of them changes with it and has to reach its node in
the same pass. It refuses a node on no link, which nothing dials, and checks, as `mint` does, that
the file with the new key still gives a network before it writes it.

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

`slipmesh.yaml`, which every tool reads, and the daemons' configs: what a config generator writes
for a node from it, and what its readers take - the daemons on the node, and the RouterOS tool.
Types and their checks only; reading and writing the files is `sliplib-yaml`'s.

Every check and the loader return an error type of their own, one variant for each refusal:
`v2::Error`, `v2::Invalid`, `v2::NameError` and those of `v2::groups`, `v2::network` and
`v2::platform`, `awg::Invalid`, `router::Invalid` and `LoopbackAddressError`, `ObfuscationError`,
`MetricsError`. A variant's text is one line of the refusal. Where that line names its cause
itself - an address that does not parse, with why - the variant holds the cause as a field and has
no `source`; where the cause is a line of its own, it is the variant's `source`. A variant never
does both, so printing the whole chain gives the whole message, each part once.

- `v2`: `slipmesh.yaml` of [format version 2](docs/slipmesh-yaml.md).
  `v2::File` reads the files as a whole (`parse`, `read`) and refuses whatever its specification
  has the loader refuse, as `v2::Error`. Among them are a text that is not YAML, and a `network`
  document whose `version` is missing, below 2 or `null`: such files are of version 1, refused as
  `VersionOne`, naming the document that converts them. `missing_reference_values` lists the required keys the
  files lack that `slipmesh mint` writes, and `network_document` says where the network stands
  in the file, for a write. It gives the `network` with what is in force for each
  node and each link written where they hold their own: a node's `routing`, a link's block of its
  type. The `template` and `patch` documents come with their `Aim`: the nodes each reaches, and
  its rank among those that reach one node (`templates_for`, `patches_for`). The settings of a
  platform (`talos_settings`, `routeros_settings`) and the keys of a `template`
  (`TemplateDocument::read`) are read when asked for, so a tool reads only its own platform's.
  The text of a template and the body of a `patch` are their platform's tool's to read. This
  release reads one `network` and one `groups` document, and refuses a second, which the format
  lays over the first, until the tools read several files.
- `nftables::TEMPLATE_PATH`, where the `nftables` service reads its ruleset template.
- `awg`: `AwgConfig` with its interfaces and peers, and `validate`. A peer's `persistent_keepalive`
  is a number of seconds or a range of them. `InterfaceEntry::keepalive_of` gives what a peer ends
  up with: none for a peer without an `endpoint`, which is only waited for; for a dialed one its
  own, or 25 on an interface that sets nothing of the obfuscation and 20 to 30 on one that sets
  any. Whoever writes a config leaves out a keepalive that does not change what that gives;
  `validate` takes one written all the same.
- `router`: `RouterConfig` with its identity, iBGP peers, BFD settings, announcements and bypass
  sources, and `validate`.
- `Obfuscation`, AmneziaWG's device-level parameters, with the checks the kernel module would
  otherwise fail on. `Obfuscation::trailer_loss` says how a tunnel with `random_trailers` on loses
  data packets, as `TrailerLoss`: a share of them where one of `h1` to `h3` is a range, or by
  their own header where an offset reads it. `Obfuscation::bytes_into_clear_header` says whether
  an offset does, for the warning and for whatever creates an offset. `MetricsConfig` is where a
  daemon serves its Prometheus endpoint.
- `Range`, the type of the magic headers and the timing and padding values of `Obfuscation`. The
  kernel module draws each of them from a range, written as one number or as `lo-hi`. A number
  is rendered into a node's config as a number and a range as a string, which a daemon built
  before ranges refuses: write a range only for nodes that run a release that reads it. No tool
  here checks that.

### `sliplib-mesh`

What a node's daemons are configured with, computed from the file `sliplib-config` reads, for
every tool that configures a node of any platform.

A refusal is a `render::Error`, which names the node and which of its configs it is about, with
its cause as its `source`, or an `mtu::MtuError`. Files that read and give no network to compute
from are refused as this crate's `v2::Error`; `sliplib-config`'s refuses those that do not read.
`Network::node` refuses a name the network does not hold as `model::UnknownNode`.

- `model::Network`, what the configs are computed from: each node with its derived addresses,
  platform and key; each link with its keepalive and obfuscation in force, the obfuscation empty on
  a `wireguard` link; the route classes. `v2::lower` gives it from a `v2::File`, refusing one
  without `mesh.interface_prefix`. `model::node_key` and `model::link_obfuscation` name where the
  files hold a node's key and a link's obfuscation.
- `render::node_configs` gives a node's `AwgConfig` and `RouterConfig` from the network, each
  checked as the node checks it. It refuses a missing key of the node, unless it is on no link, or
  of a peer, and a link of type `amneziawg` whose obfuscation lacks one of `jc`, `jmin`, `jmax`,
  `s1`, `s2` and `h1` to `h4`, naming what lacks what; nothing of nodes and links the node is not
  on. Such a refusal gives its `secrets::MissingSecret` through `render::Error::missing_secret`, for
  a tool to say how to create it. A dialed peer gets its link's keepalive where that differs from
  what `InterfaceEntry::keepalive_of` gives without it. No metrics listener: a tool for Talos adds
  it from the `talos` settings.
- The `mint` feature, off by default, adds `mint::create`: whatever secrets the file lacks, created,
  each by the entry it belongs to, and no key for a node on no link, or `NoHeaderLeft`
  when an entry's magic headers leave no room for the ones it lacks. `slipmesh` builds with it; a
  tool that only reads the file leaves it off, and that function is not in its build.
- `mtu::mesh_mtu`, the MTU of every mesh interface: the largest at which a full-size packet crosses
  every link's underlay unfragmented, or `mesh.mtu` where it fits. It counts every link's `s4`, 0
  where unset. It refuses a link whose limit is below 1280, the least IPv6 allows a link, and a
  `mesh.mtu` above a link's limit, each naming the link; and a `mesh.mtu` below 1280 or set on a
  mesh without links.
- `addressing` gives the address a `node_id` gives in a network, the link-local of a loopback, and
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

[`mesh/tests/golden.rs`](mesh/tests/golden.rs) pins what the files give their nodes. Under
[`mesh/tests/golden/`](mesh/tests/golden), `v1` and `v1-alternate` hold the configs that two
networks of format version 1 gave every node, with derived public keys named by their owner; the
first set every key of that format, the second the other side of each key a file sets once. `v2`
and `v2-alternate` are the same two networks converted to version 2 by hand, and every node of each
gets those configs, but for what [a conversion
changes](docs/slipmesh-yaml-v1-to-v2.md#converting-a-file-by-hand). The files hold values: the
test reads each into its type and writes it back, and the text has to come out the same. A change
of what a node gets fails the twins; a change of how a config is written fails the read-back until
the files are rewritten in the new form, which the pull request then shows.

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
