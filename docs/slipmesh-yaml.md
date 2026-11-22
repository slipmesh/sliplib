# `slipmesh.yaml`, format version 2

`slipmesh.yaml` describes one slipmesh network: its nodes, the links between them and the routing
over those links. It is the single source: every tool that configures a node reads it itself, and
none reads what another tool produced. What runs over the mesh - access for clients from outside,
its rules, which traffic leaves through which node - is described where it runs; [What leaves the
file](slipmesh-yaml-v1-to-v2.md#what-leaves-the-file) says where the two such parts of version 1 go.

This is the specification of the format. A part no tool serves yet is marked **Planned** where it
will stand, as [Planned](#planned) explains. sliplib v0.19 and the releases before it read version
1, which [Differences from version 1](slipmesh-yaml-v1-to-v2.md#differences-from-version-1) starts
from; from v0.20.0 on, the tools read version 2 alone. What this document says a tool does, v0.20.0
does.

## The files

A network is written in one file, which a tool takes with `--config`; [several
files](#several-files) are **Planned**. A tool takes the file as plain text: a file kept encrypted
is decrypted by whatever runs the tool, and the format knows nothing of how. The nodes' private keys
are written in the clear, so the file that holds them is kept where such files are kept; with
several files it could be a file of its own.

People write the documents. Only [`slipmesh`](../README.md#slipmesh) writes values into them - the
keys nobody can derive, and reference values - and it checks the files as a whole. Every other
tool only reads; where something it needs is missing, it refuses and names what writes it.

### Documents and the `slipmesh:` block

A file is a stream of YAML documents separated by `---`. Each has a top-level `slipmesh:` block
that says what it is; the rest of the document is its body.

```yaml
slipmesh:
  kind: patch
  platform: talos
  include: [workers]
apiVersion: v1alpha1
kind: ExtensionServiceConfig
name: example
```

The keys of the block:

| key | type | where | what it is |
| --- | --- | --- | --- |
| `kind` | string | every document | What the document is: one of the kinds below. |
| `version`, `revision` | integer | `network` | [The format's version and revision](#version-and-revision). |
| `platform` | `talos` or `routeros` | the kinds for one platform | [The platform](#aiming-a-document-at-nodes) whose tool serves the document. One value. |
| `include`, `exclude` | list of names, or `all` and `none` | the kinds aimed at nodes | [The nodes the document reaches](#aiming-a-document-at-nodes). |
| `order` | integer, 0 to 65535 | every document, optional, `1000` when left out | [Where the document stands](#several-files) among all the documents read. |

The kinds:

| kind | how many | `include`, `exclude` | `platform` | what it holds |
| --- | --- | --- | --- | --- |
| [`network`](#the-network-document) | one; **Planned:** several, laid over each other | not taken | not taken | The mesh: its addressing, routing, links and nodes. |
| [`groups`](#groups) | at most one; **Planned:** several, laid over each other | not taken | not taken | Named sets of nodes, for documents to be aimed at. |
| [`platform`](#platform) | at most one for a platform | not taken | required | Settings only that platform's tool reads. |
| [`template`](#template) | any | optional | required | The template a daemon on the node makes its config from. |
| [`patch`](#patch) | any | optional | required | A document in the platform's own format. |

The block is strict: a key it does not know, or one its kind does not take, is an error, so a
misspelled `include` cannot aim a document at every node. A document without the block is an
error; one that holds nothing - empty, only comments, or `~` - is skipped.

### Several files

A tool reads one file, given with `--config`. It holds one `network` document, which is required,
and at most one `groups` document: a second of either is refused, naming the lines of the first two.
Every `network` document carries `version` and `revision`.

Before the body of any document is parsed, the documents are sorted by `order`, `1000` where it is
left out; documents of one `order` keep the place they have in the file. That is "the order of the
documents" below. Where documents are merged or chosen between for a node, their
[rank](#aiming-a-document-at-nodes) puts one more rule between `order` and that place.

**Planned:** several files, which the rest of this section describes as they will be read, and
which the tools do not read yet.

A tool takes its files with `--in`, which takes several values and may be repeated:
`--in cfg/*.yaml` reads every such file of the directory. A command that writes also reads the file
of its [`--edit`](#what-slipmesh-writes). Every path is first brought to its canonical form, and
the files are read sorted by those paths, byte by byte. Neither the order on the command line nor
the way a path is written matters: one file under two spellings, such as `a.yaml` and `./a.yaml`,
or reached through a symbolic link, is one file, read once. An `--edit` file that does not exist
yet is taken at the canonical path of its directory. Files in one directory therefore sort by
name, and files in different directories by where the directories are, which differs between
machines.

The documents are then sorted by `order`, those of one `order` keeping the order they were read in,
by file and then by place in the file. A document that has to come first or last whatever the
paths say sets its `order`.

The `network` documents, any number in one file or in several, are
[laid over each other](#laying-one-value-over-another) in that order from an empty tree, and so
are the `groups` documents. The result, one network and one set of groups, is what is checked: a
single document need not be valid alone, so one file may hold the architecture of the mesh and
another, laid over it, its secrets. The format does not know which keys are secret: a file of
secrets is an ordinary layer. Documents of the other kinds are not laid over each other, only
collected in the same order.

### Laying one value over another

One rule lays a value over another wherever the format does so: JSON Merge Patch,
[RFC 7396](https://www.rfc-editor.org/rfc/rfc7396), the way Helm lays values files over each
other.

- A mapping is laid over a mapping key by key, and the rule applies again to each key's value.
- A scalar or a list replaces what was there, whole: an appended list would leave no way to take
  an element out.
- `null` removes the key it stands at.

It is used in three places:

- **Between documents, Planned with [several files](#several-files):** the `network` documents in
  order over an empty tree, and the `groups` documents likewise, so a `null` in any of them, the
  first included, means the key is absent, but for the two cases below.
- **For a node:** `nodes.<name>.routing` over [`routing`](#routing), by
  [the same path](#a-nodes-routing). A `null` at one of its keys takes the network's value away
  for that node.
- **For a link:** `mesh.links.<name>.<type>` over `mesh.<type>`, the defaults of the type: today
  the [keepalive](#keepalive) of both types and the [obfuscation](#obfuscation) of `amneziawg`. A
  `null` at any depth of it, a key of the block or a field of its `obfuscation`, takes the
  network's value away for that link: it is how a link drops
  one field of `mesh.amneziawg.obfuscation`. A `null` at a field `slipmesh mint` writes leaves the
  link without it, and the link is refused as any link that lacks the field is; `slipmesh mint`
  then writes that field over the `null`, as it writes into any link that lacks it once the
  defaults are laid under. Where the link's whole `obfuscation` is `null`, which takes away every
  field laid under it, `mint` writes the fields it mints and a `null` at each other field of
  `mesh.amneziawg.obfuscation`, so the link goes without those still.

Every key of these two places takes `null`, besides the type its table gives.

A mapping keeps the order of its keys: a key stands where it first appears as the documents are laid
over each other, and one that a `null` removed and a later document writes again goes last. The
order reaches the node as written: a node's iBGP peers follow the order of `nodes`, and its
interfaces that of `mesh.links`.

A key with nothing after it is `null` in YAML: `node-a:` alone under a link's `ends` removes that
end, so an end with no keys is written `node-a: {}`.

### Version and revision

Both are required, in the `slipmesh:` block of every `network` document and nowhere else:

```yaml
slipmesh:
  kind: network
  version: 2
  revision: 1
```

`version` is the syntax of the format as a whole, 2 in this document. It changes only when the
syntax is replaced, which means converting every file. A `network` document without `version` is
version 1, which [Converting a file by hand](slipmesh-yaml-v1-to-v2.md#converting-a-file-by-hand)
turns into version 2. A tool refuses it, and a `version` that is an integer below 2 or `null`,
naming that document and pointing at that conversion. A `version` that is no integer is refused by
the block as a value of the wrong type.

`revision` is the generation of the rules that turn the files into configs. It grows when the same
data has to give a different result, for instance when what a document configures on a node passes
from one component to another. A tool is built for exactly one revision: an older tool refuses
files of a newer one, a newer tool files of an older one, and each says which of the two to
update. Such a change is switched on by one deliberate edit of the number, made together with
moving every tool to the release built for it, not by whichever tool is upgraded first.

| revision | what it means |
| --- | --- |
| 1 | the rules this document describes |

Nothing else moves either number. A key, a kind or a value that a tool begins to serve, a
[Planned](#planned) part among them, arrives with a release of the tools that read it, since
[strict parsing](#strict-parsing) refuses what a tool does not know. Within one `version` a key is
not renamed: a new name is added beside the old one, both are accepted, and writing both is an
error. The old name leaves with the next `version`.

### Names

Nodes, groups, links, a node's underlay entries and route classes are named by the keys of the
mappings that hold them, under one rule: ASCII letters, digits, `-`, `_` and `.`, with a letter or
a digit first and last. Names become parts of interface names, of names in a node's routing
configuration and of annotation keys, which take no more. A class name is at most 48 characters,
since it ends the key of a node annotation, which takes 63, and a node's name is bounded by the
[interface names](#nodes) it becomes part of. No other name has a limit.

Nodes and groups share one namespace, so a group named like a node is an error. `all` and `none`
are reserved: no node or group takes either. Every other kind of name is unique within its own
mapping.

### Aiming a document at nodes

`include` in a document's block lists the nodes it reaches, and `exclude` takes nodes back out.
Each is a list of names, a name being a node or a [group](#groups). `all` is a group nobody defines:
every node, and for a document of one platform every node of that platform; it stands wherever a
group name can. `include` left out means `all` and `exclude` left out means `none`;
`include: all` and `exclude: none`, the word as the whole value, mean the same.

- A name that is neither a node nor a group is an error.
- `include: []` is an error, and so is a document left reaching no node once `exclude` is taken
  out.
- `exclude` only removes: a name in it that `include` did not bring in changes nothing. "A group
  but one of its nodes" is `include: [workers]` with `exclude: [node-c]`.

Where several documents of one kind reach a node and are merged or chosen between, they rank by
three things, each breaking the ties the one before leaves:

1. `order`, lower first;
2. the documents for all before the others. A document is for all when its `include` is left out,
   is `all`, or is a list whose only name is `all`; any other list is narrower. `exclude` leaves
   the rank as it is;
3. the order they were read in.

The last by rank wins. Among documents of one `order`, one aimed at some nodes therefore comes after
the one for all wherever it stands, and an `order` set on purpose moves either past the other.

Only `template` and `patch` take the two keys, both optional. The other kinds are about no node in
particular, and there the keys are an error.

A document written in the terms of one platform names it with `platform`, one value: a [platform of
the nodes](#who-serves-a-document). Its `all`, wherever it stands in its own `include` or `exclude`,
is every node of that platform: `include: [all, node-a]` reaches those nodes and `node-a`, but ranks
as aimed at some nodes, since its `include` is not `all` alone, and `exclude: [all]` leaves the
document reaching no node, which is refused. Every node the document is left reaching once `exclude`
is taken out has to be of that platform, and a node of another makes the document an error, not
something passed over. Inside a group there is no platform, and `all` there is every node: a group
that names it holds nodes of every platform, and makes such a document an error in the same way.

### Strict parsing

All tools read the files through one loader, in `sliplib-config`. What it checks in a document
depends on whose the document is:

- **Every tool, in every document:** the `slipmesh:` block - the kind, the keys it takes, the names
  in `include` and `exclude` - and the version and revision.
- **Every tool, in the network and the groups:** every key and every value at any depth. Every
  tool serves both.
- **The tool of a platform, in that platform's documents:** every key and every value of the body,
  an unknown key being an error at any depth.
- **A tool, in a document of another platform:** the block alone, so a key added to the body does
  not concern it.
- **`slipmesh check`, in all of it:** every body strictly, but the text of a `template` and the
  body of a `patch`, which it leaves to the platform's tool. It is the one check of the files as a
  whole, and the one to run after an edit by hand.

The tools are built with the format they read, so a key a tool does not know is a mistake: passed
over, it would silently stop configuring what it was written for. A body written in a platform's
own language - the document in a `patch`, the text of a `template` - is checked as far as the tool
can, as the sections of those kinds say.

A new key in the network, or a new kind, therefore needs a release of every tool, since they share
the loader. A new key in a platform's document needs a release of that platform's tool and of
`slipmesh`.

### Planned

What no tool does yet is not in the format: there is no key and no parsing for it, and strict
parsing refuses a file that writes it, as it refuses a key nobody meant. This document names each
such part where it will stand, with **Planned**, its path in the tree and what it is for. Its full
description comes with the release that serves it, and the shape around it leaves room for it
without moving the version or the revision.

### Who refuses, and how far

- **The loader**, shared by every tool, refuses what is wrong in any `slipmesh:` block or in the
  network, such as a link of a [type](#meshlinks) that the tool of one of its ends does not serve.
  Every tool refuses, for every node.
- **The computation of a node's configs**, in `sliplib-mesh`, refuses what is wrong for one node:
  a missing secret, an obfuscation the kernel module would refuse, an interface name too long. It
  stops that node and the nodes it has a link with, since each end of a link needs the other; a
  tool still configures the rest. [The MTU of the mesh](#the-mtu-of-the-mesh) is the exception: it
  is computed from every link, so what it refuses stops every node.
- **A platform's tool** refuses what is wrong in a document only it serves, such as a `template`.

`slipmesh check` makes all three, for every node. A refusal listed below is the loader's unless the
section names another.

### Required values, defaults and reference values

A value that two components have to agree on, or that decides who serves what, has no default in
any tool: a node's platform, the number of a route class. Left out, it is an error that names the
key. A key whose absence means a feature is off may be left out: no metrics port, no listener; no
`routing.bfd`, no BFD.

These keys default to a value, each with one consumer chain and nothing to agree with:

| key | default | |
| --- | --- | --- |
| `order` of a `slipmesh:` block | `1000` | the place of every document that does not set one |
| `addressing.tunnel.link_local` | `fe80::/64` | the link-local prefix of every interface |
| `nodes.<name>.underlay.<entry>.mtu` | `1500` | the MTU of Ethernet |
| `mesh.<type>.persistent_keepalive` and a link's own | `25` on `wireguard`, `20-30` on `amneziawg` | the interval WireGuard's own tools suggest behind a NAT, on `amneziawg` drawn around it at each timer |
| `routing.bfd.min_rx_ms`, `min_tx_ms`, `multiplier` | `300`, `300`, `5` | a link is a tunnel, some of them long: detection in about 1.5 s, where a tighter timer would flap |

For an absent required key `slipmesh mint` may write a *reference value*: the value a network takes
unless it has a reason for another. There is one, `mesh-` for `mesh.interface_prefix`. Written, it
is a value like any other, and no tool falls back to it, which is why a tool refuses files still
missing the key and names `mint`.

`mint` never writes what a person chooses: a node's platform or `node_id`, a port, the AS number,
the networks, the number of a route class, who is in a group. Where one of those is missing, the
refusal names the key.

### Who serves a document

Every node names its `platform`, the tool that configures it. That tool reads the files itself and
serves, on its own nodes, the documents that reach them.

| `platform` | tool |
| --- | --- |
| `talos` | the Talos generator, `slipmesh-taloscfg` |
| `routeros` | the RouterOS tool, `slipmesh-routeros` |

A setting that the tools of more than one platform need stands in the network: `announce` is one,
since every platform builds a node's announcements from it. A key only one platform understands
stands in that platform's documents. A document without `platform`, such as the network or the
groups, reaches nodes of every platform, and each tool takes the nodes that are its own. The tools
do not know of each other: what a platform's tool does not implement reaches no device of that
platform, and nothing reports it, unless the description of the key says otherwise. [Who serves
what](#who-serves-what) says what each tool serves.

A new platform is a new value of `platform` and a tool that serves it. The loader learns the value,
so it arrives with a release of every tool, and refuses a link of a type the platform does not
serve that reaches one of its nodes, as the [table of link types](#meshlinks) shows for the
platforms there are.

## The network document

The network is one tree, held whole by one `network` document; several, laid over each other, are
[Planned](#several-files). It is about no node in particular, and the tool of every
platform serves it.

```yaml
slipmesh:
  kind: network
  version: 2
  revision: 1
addressing:
  loopback: {ipv4: 198.51.100.0/25, ipv6: "3fff:0:0:1::/64"}
routing:
  bgp: {as: 64500}
mesh:
  interface_prefix: mesh-
nodes:
  node-a:
    platform: talos
    node_id: 0.0.0.1
    underlay:
      wan: {endpoint: {ipv4: 192.0.2.1}}
```

| key | type | required | what it is |
| --- | --- | --- | --- |
| `addressing` | mapping | yes | [The networks the nodes' addresses are taken from](#addressing). |
| `routing` | mapping | yes | [The routing over the mesh](#routing), for every node. |
| `mesh` | mapping | yes | [The links](#mesh): their interfaces, their MTU, the defaults of their types, the links themselves. |
| `nodes` | mapping | yes | [The nodes](#nodes), at least one. |

A value that is one for each address family is a mapping by family, `ipv4` and `ipv6`. A list of
prefixes is one list, in which each prefix says its own family. A family a key does not take yet is
refused.

### `addressing`

The networks from which a node's addresses are derived by its `node_id`, as
[What a node gets](#what-a-node-gets) says.

| key | type | required | what it is |
| --- | --- | --- | --- |
| `loopback` | mapping | yes | `ipv4` and `ipv6`, both required: the networks of the nodes' loopback addresses. |
| `tunnel` | mapping | no | The networks of the addresses on the links' interfaces: `ipv4` and `link_local`, each optional. |

`tunnel.ipv4` is for a node that has to send IPv4 from a link, as when it masquerades what it
forwards into one: a link's interface has no IPv4 address otherwise. With it, every link's
interface carries an address from it, derived the way the IPv4 loopback is, and these addresses are
announced as [`direct_interfaces`](#routing) says.

`tunnel.link_local` is the network of the IPv6 link-local address of every link's interface, on
which OSPFv3 runs. It lies inside `fe80::/10`, and one outside it is refused; left out, it is
`fe80::/64`. Either of the two keys may be written without the other.

**Planned:** `addressing.tunnel.ipv6`, a routable IPv6 address on the links' interfaces.

### `routing`

The routing over the mesh. A node holds its own value for some of these keys under
[`nodes.<name>.routing`](#a-nodes-routing).

| key | type | required | default | what it is |
| --- | --- | --- | --- | --- |
| `bgp` | mapping | yes | | Its one key, `as`, an integer from 1 to 4294967295, is the AS number of the mesh, under which the nodes speak iBGP to each other. |
| `bfd` | mapping | no | none | [BFD on the links](#routingbfd). |
| `direct_interfaces` | list of strings | no | empty | The interfaces whose connected IPv4 networks a node announces. |
| `learn` | list of strings | no | empty | IPv4 prefixes: a node announces the routes its kernel holds inside them. |
| `announce` | list of strings | no | empty | IPv4 prefixes a node announces as its own. |
| `classes` | mapping | no | none | [Classes of routes](#routingclasses): named protocol numbers of routes that something other than the router installs. |

Besides its own loopback, a node comes to announce an IPv4 prefix in three ways:

- `direct_interfaces`, for a network connected on a real interface. An entry is an interface name
  or a pattern with `*`; one that matches no interface of a node announces nothing there.
- `learn`, for a range whose parts the nodes get at run time, each routing its own part without an
  address of it on any interface, as with the pod range of a Kubernetes cluster. A route inside a
  `learn` prefix is announced whoever installed it.
- `announce`, for a prefix the node answers for without having it connected, as with the service
  range of such a cluster: the node holds it on its loopback and announces it as written.

An entry of `learn` or `announce` that is not an IPv4 prefix is refused. A node that has no part in
those ranges, such as a router outside the cluster they belong to, holds `learn: []` and
`announce: []` in its own `routing`, and neither takes those routes in nor announces them.

With `addressing.tunnel.ipv4`, the pattern of the links' interfaces - the
[interface prefix](#mesh) and `*` - is added at the end of every node's `direct_interfaces`, unless
the list already holds that exact string. The IPv4 addresses on the links are therefore announced
if and only if `addressing.tunnel.ipv4` is set, and `direct_interfaces` lists only what the writer
of the files adds. The list is in order, and the order reaches the node: the reference router
template of `talos` makes one `protocol direct` of each entry. The pattern written last changes
nothing; written before other entries, it keeps its place.

**Planned:**

- `routing.bgp.sessions`: named sessions, internal and external, in one place. An internal entry
  names `reflectors` and their `clients`, for a mesh in which a session between every two nodes is
  too many. An external one names the `nodes` that hold it and the `neighbor`, with its `as` and
  address. A session may carry its own `as`, the number the mesh speaks to that neighbour under.
  Without entries every node holds a session with every other, as now.
- `routing.ospf`: settings of OSPFv3, such as the default cost of a link.
- IPv6 prefixes in `learn` and `announce`.

#### `routing.bfd`

Bidirectional Forwarding Detection notices a dead link within seconds, where the routing protocol
alone takes its dead interval. With the block, every link has a BFD session; without it, none
has. `bfd: {}` turns it on with the defaults.

| key | type | required | default | what it is |
| --- | --- | --- | --- | --- |
| `min_rx_ms` | integer, at least 1 | no | `300` | The least interval between received control packets, in milliseconds. |
| `min_tx_ms` | integer, at least 1 | no | `300` | The least interval between sent control packets, in milliseconds. |
| `multiplier` | integer, 1 to 255 | no | `5` | How many packets missed in a row take the link down. |

There are no other settings: these three are what the BFD of every platform takes, so the two ends
of a link are always configured alike.

#### `routing.classes`

A class is a named protocol number, `proto` in `ip route`, for a set of routes that something
other than the router installs in a node's kernel, such as the routes that lead out through a node.
The router does not read it today.

```yaml
routing:
  classes:
    egress:
      protocol: 152
```

| key | type | required | what it is |
| --- | --- | --- | --- |
| `protocol` | integer, 0 to 255 | yes | The protocol number of the class's routes in a node's kernel. |

A class takes any [name](#names). On `talos` a node is told each class's number as the annotation
`slipmesh.net/route_proto_id.<name>`, for whatever installs that class's routes there; without the
class there is no such annotation. Refused, over all classes together: two classes with the same
number; `0` to `4`, the kernel's own; `200`, which the routes to the clients of roadwarriors pools
carry.

**Planned:** the router carrying a class's routes over the mesh to the other nodes, telling them
from its other kernel routes by the number; a class's own `table`, the kernel routing table its
routes go into on the receiving nodes; its `families`; and `rules`, a mapping of named routing
rules, each with a priority, a mark, which is a string of `0x` and hexadecimal digits, and where it
sends. A class without a table puts its routes in `main`.

### `nodes`

The nodes of the network, a mapping by name with at least one entry.

```yaml
nodes:
  node-a:
    platform: talos
    node_id: 0.0.0.1
    underlay:
      wan: {endpoint: {ipv4: 192.0.2.1}}
    wireguard:
      private_key: AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=
```

| key | type | required | what it is |
| --- | --- | --- | --- |
| `platform` | `talos` or `routeros` | yes | [Who configures the node](#who-serves-a-document). Any other value is an error. |
| `node_id` | string in the form of an IPv4 address | yes | The node's identity, from which [its addresses are derived](#what-a-node-gets). Unique among nodes, and so are the addresses derived from it. |
| `underlay` | mapping | yes | [What the node reaches the world through](#underlay), exactly one entry. |
| `wireguard` | mapping | written by `slipmesh mint` | Its one key, `private_key`, is a WireGuard private key: the node's one identity on every link of type `wireguard` or `amneziawg`. |
| `routing` | mapping | no | [The node's own routing values](#a-nodes-routing). |

A node's name is part of an interface name on every node it has a link with, and an interface name
holds 15 bytes at most: with the interface prefix `mesh-`, 10 for the name. A longer one is refused
once the node is on a link, when the configs of the node at the other end are computed.

Nobody writes `wireguard.private_key` by hand: `slipmesh mint` creates it for a node on a link of a
type that takes it, and `slipmesh rekey` replaces it. Until it is written, neither that node nor
any node it has a link with can be configured, since each end of a link needs the public half of
the other's key. Their configs are refused when computed, naming `mint`.

#### `underlay`

The interfaces a node reaches the world through are not the mesh's: it neither configures nor
addresses them. `underlay` states, in one entry by name, what the links need to know of them.

| key | type | required | default | what it is |
| --- | --- | --- | --- | --- |
| `endpoint` | mapping | no | none | The address at which the node's peers reach it, by family: `ipv4` or `ipv6`, one of them, holding an IP address of that family or a host name. |
| `mtu` | integer | no | `1500` | The MTU of the path, which bounds every link the node is on. Set it where the path is narrower, as behind a provider's tunnel or over PPPoE. |

The key under `endpoint` is the family the links to the node are dialed over, and the
[MTU](#the-mtu-of-the-mesh) is counted for it; an address of the other family under it is an error.
A host name says no family, so the key says it. A peer dials the first address the resolver
returns, and a `routeros` device resolves the name itself, so a name with addresses of both
families has to resolve to the family of its key first, or the links carry an MTU counted for the
other family's header. `endpoint` takes no port, since each link adds its own; one written with a
port is refused. A host name is read as follows: labels of ASCII
letters, digits and `-` joined by dots, as [RFC 1123](https://www.rfc-editor.org/rfc/rfc1123) has
its labels, none empty, longer than 63 bytes or starting or ending with `-`; at most 253 bytes in
all, the limit of [RFC 1035](https://www.rfc-editor.org/rfc/rfc1035); and the last label not all
digits, which RFC 1123 does not require. Anything that is neither such a name nor an IP address is
refused.

An entry without `endpoint` is a node behind NAT or without a fixed address. It is never dialed: it
dials the other end of each of its links.

Refused: a node with no `underlay` entry or with more than one, and an `endpoint` with both
families.

**Planned:** a second entry, such as a network inside a site with a larger MTU, which a link picks
at its [end](#meshlinks); and both families in one `endpoint`.

#### A node's routing

`nodes.<name>.routing` holds the node's own values of [`routing`](#routing) by the same path,
[laid over](#laying-one-value-over-another) the network's. Today it takes `direct_interfaces`,
`learn` and `announce`. Each is a list, so the node's list replaces the network's rather than
extending it: the node gets exactly what it wrote, nothing with `[]`, and nothing with `null`.

### `mesh`

The links: the names of their interfaces, their MTU, the defaults of their types and the links
themselves.

| key | type | required | default | what it is |
| --- | --- | --- | --- | --- |
| `interface_prefix` | string | yes | | What the name of every interface a tool creates for a link starts with. `slipmesh mint` writes the reference value, `mesh-`. |
| `mtu` | integer | no | the least limit of the links | Lowers [the MTU of the mesh](#the-mtu-of-the-mesh). |
| `wireguard` | mapping | no | none | The defaults of links of type `wireguard`. Its one key is [`persistent_keepalive`](#keepalive). |
| `amneziawg` | mapping | no | none | The defaults of links of type `amneziawg`: [`persistent_keepalive`](#keepalive), and `obfuscation`, [the obfuscation](#obfuscation) every such link starts from. |
| `links` | mapping | no | none | [The links](#meshlinks), by name. |

`interface_prefix` is not empty and takes ASCII letters, digits, `-`, `_` and `.`. With `*` after
it, it is the pattern by which routing and `direct_interfaces` find the links' interfaces. On
`talos` the node is told it as the annotation `slipmesh.net/interface_prefix.mesh`.

#### `mesh.links`

The links of the mesh, a mapping by name; the name appears in messages and configures nothing. A
link's `type` says what carries it, its `ends` name the nodes it joins, and the keys of its type
stand in a block named after the type.

```yaml
mesh:
  links:
    a-b:
      type: amneziawg
      ends: {node-a: {}, node-b: {}}
      amneziawg:
        port: 51830
        obfuscation:
          jc: 4
    a-r1:
      type: wireguard
      ends: {node-a: {}, router1: {}}
      wireguard:
        port: 51832
```

| key | type | required | what it is |
| --- | --- | --- | --- |
| `type` | `wireguard` or `amneziawg` | yes | What carries the link. |
| `ends` | mapping | yes | The link's ends by node name, each `{}` today. |
| `wireguard` | mapping | with `type: wireguard` | The keys of a WireGuard link. |
| `amneziawg` | mapping | with `type: amneziawg` | The keys of an AmneziaWG link. |

Both types today are tunnels: exactly two ends, and on each an interface the tool creates, named by
the interface prefix and the name of the other end. Both take each end's key from the node's
`wireguard.private_key`.

| type | its block | what it is |
| --- | --- | --- |
| `wireguard` | `port`, an integer from 1 to 65535, required: the UDP port both ends listen on. `persistent_keepalive`, optional: the link's [keepalive](#keepalive), over `mesh.wireguard.persistent_keepalive`. | Plain WireGuard. |
| `amneziawg` | `port` and `persistent_keepalive`, as for `wireguard`, the keepalive over `mesh.amneziawg.persistent_keepalive`; `obfuscation`, a mapping, optional: the [obfuscation](#obfuscation) fields of this link, laid over `mesh.amneziawg.obfuscation`. | WireGuard whose packets AmneziaWG keeps from looking like WireGuard. |

What stands outside the blocks holds for every type: the `ends` of a link; the `underlay` of a node,
which says where and over what its peers reach it; and `addressing.tunnel`, the MTU of the mesh and
OSPFv3, which run over every link.

Which platform serves which type:

| type | `talos` | `routeros` |
| --- | --- | --- |
| `wireguard` | served | served |
| `amneziawg` | served | refused |

A new type is a new value of `type` with a block of its own: on a link, and under `mesh` or on a
node where the type needs keys there. Like any new key, it reaches the tools with a release of
theirs.

Refused:

- a name in `ends` that is not a node, and a link of a tunnel type without exactly two ends;
- a second link between the same two nodes;
- a block of a type other than the link's, and a link without the block of its own type;
- a `port` that another link of the same node uses;
- a link of a type that the tool of one of its ends does not serve, so that no link is configured at
  one end alone. The error names the type and the link. A link that reaches a `routeros` node is
  therefore of type `wireguard`;
- with [the MTU of the mesh](#the-mtu-of-the-mesh), so stopping every node: a link with no
  `endpoint` at either end, which neither could dial, and a link whose two `endpoint`s are of
  different families, where the family would depend on which end dialed first;
- when the configs of its two nodes are computed: a link of type `amneziawg` that lacks one of the
  fields `slipmesh mint` writes, once `mesh.amneziawg.obfuscation` is laid under its own.

**Planned:**

- At a link's end, `underlay`, the node's entry the link goes through, and `interface`, the name of
  its interface. A second link between the same two nodes needs it, since the default name would be
  the same.
- Type `ethernet`: an interface the node already has, and two ends or more, as on a VLAN. The tool
  creates nothing and only runs routing over it.
- A link's own `routing`, such as the OSPF cost and BFD of that link, and its own `mtu`.
- `mesh.<type>.fwmark`, and the same key in a link's block: the mark an interface puts on its own
  outer packets, by which the node's routing rules and firewall tell them apart. A string, `0x` and
  hexadecimal digits. Not served on `routeros`.
- A preshared key of a link: a secret in the block of its type, the same on both ends, which
  `slipmesh mint` creates.

#### Keepalive

`persistent_keepalive` is how long, in seconds, nothing may pass between the two ends of a link, in
either direction, before an end sends its peer a packet, so that a NAT on the way keeps the link's
mapping open. It stands in the block of a link's type, laid over `mesh.<type>.persistent_keepalive`,
the network's value.

```yaml
mesh:
  amneziawg:
    persistent_keepalive: 15-20
  links:
    a-r1:
      type: wireguard
      ends: {node-a: {}, router1: {}}
      wireguard:
        port: 51832
        persistent_keepalive: 10
```

| type | value | left out |
| --- | --- | --- |
| `wireguard` | a number, 0 to 65535 | `25` |
| `amneziawg` | a number, 0 to 65535, or a range `lo-hi` [as in the obfuscation](#obfuscation) | `20-30` |

`0` switches it off. From a range the kernel module draws an interval each time it sets the timer;
a range starts at 1 or above, since a draw of 0 would send at once. An end sends the keepalive to a
peer it dials, one whose `underlay` has an `endpoint`; an end that only waits to be dialed sends
none. `talos` serves it on both types, and `routeros` on `wireguard`, the one type it serves. An
`awg` daemon older than the one that takes a peer's keepalive from its config entry sends 25 to
every peer it dials, whatever the file says.

The generator writes `persistent_keepalive` into a peer's entry only where the value differs from
what the consumer gives a peer without it: 25 on a link of type `wireguard`, `20-30` on one of type
`amneziawg`.

It is not `keepalive_timeout` of the obfuscation: that one is how long the module waits, once data
came in, before it answers with an empty packet.

#### Obfuscation

The AmneziaWG parameters that keep the packets of a link of type `amneziawg` from looking like
WireGuard. They stand under `obfuscation` in the link's `amneziawg` block, laid over
`mesh.amneziawg.obfuscation`, and both ends of the link get the result, so they agree by
themselves. A link of type `wireguard` has none.

`slipmesh mint` writes nine fields, with values of its own for each link, into every link of type
`amneziawg` that lacks them once the defaults are laid under its own: `jc`, `jmin`, `jmax`, `s1`,
`s2` and `h1` to `h4`. The others are written by hand where they are wanted. Every field is
optional, and one left out leaves that part of the tunnel as WireGuard has it.

| key | type | what it is |
| --- | --- | --- |
| `jc` | integer, 0 to 65535 | How many junk packets go out before a handshake. |
| `jmin`, `jmax` | integer, 0 to 65535 | The least and the greatest size of a junk packet. `jmin` above `jmax` is an error where `jc` and `jmax` are both above 0, the case in which the module draws each size between them. |
| `s1`, `s2`, `s3`, `s4` | integer, 0 to 65535 | Bytes of junk added to a packet of each type: handshake initiation, handshake response, cookie reply, data. `s4` is on every data packet and lowers [the MTU of the mesh](#the-mtu-of-the-mesh). |
| `h1`, `h2`, `h3`, `h4` | number or range, up to 4294967295 | The value that stands in place of the WireGuard message type of the same four packet types. No two of the four may share a value, one left out counting as its WireGuard type, 1 to 4. |
| `i1` to `i5` | string | Decoys: packets sent ahead of a handshake, each described in tags under this table. |
| `header_protection_key` | string | A secret: base64 of 32 bytes that encrypts every handshake and cookie message whole, and the 16-byte header of a data packet - its type, key index and counter - for every peer of the interface. Each of `s1` to `s4` has to be at least 12, the room its nonce takes. |
| `content_padding_addition` | number or range, up to 65535 | Bytes added to a data packet in place of the padding it would otherwise get: WireGuard's own, or the random trailer when `random_trailers` is on. `0` or the key left out leaves the packet that padding. |
| `rekey_after_time`, `rekey_timeout`, `reject_after_time`, `keepalive_timeout`, `max_handshake_attempts` | number or range, up to 65535 | The timing values: four timers and a count that WireGuard fixes, each described under this table. `0` or the key left out leaves one to the module's constant. |
| `random_trailers` | boolean | A trailer of random length on outgoing packets. Both ends of a tunnel set it, or neither. |
| `disable_cookies` | boolean | No cookie replies. |

An obfuscation the kernel module would refuse is refused when the configs of the link's two nodes
are computed. One in `mesh.amneziawg.obfuscation` therefore stops every node with a link of type
`amneziawg` that does not set the field itself.

**Decoys.** Each of `i1` to `i5` describes one UDP packet: its tags, joined in the order they are
written. Every decoy that is set goes out ahead of each handshake initiation, `i1` first.

| tag | what it gives |
| --- | --- |
| `<b 0x…>` | The bytes written, in hexadecimal: `0x` in lower case and an even number of digits, at least two. |
| `<c>` | 4 bytes, in network byte order: a count of the decoys, which starts from a random number at each handshake. |
| `<t>` | 4 bytes, in network byte order: the Unix time, in seconds. |
| `<r N>` | `N` random bytes. |
| `<rc N>` | `N` random Latin letters. |
| `<rd N>` | `N` random digits. |

```yaml
obfuscation:
  i1: "<b 0xabcd><c><t><r 16>"
```

A packet is at most 65535 bytes. No tool reads the text: it reaches the kernel module as written,
and the module refuses an unknown or malformed tag when the node's interface is configured. Text
outside the tags is passed over, so a text with no tag at all is accepted and sends nothing.

**A number or a range.** Ten of the fields take either: `h1` to `h4`, and the six from
`content_padding_addition` to `max_handshake_attempts`. A range is a string `lo-hi`: two decimal
numbers around one dash and nothing else, both ends included, `lo` not above `hi`. A number is the
range of that one value. The kernel module draws a value from a range, for a header with every
packet it sends. A write by `slipmesh` leaves a range a string and a number a number.

```yaml
obfuscation:
  h1: 1101-1199
  h2: 2202
  rekey_after_time: 100-140
```

A range reaches a node's daemon as that string, and a daemon of a release that knows no ranges
refuses its config; no tool checks which release a node runs. Refused whatever they hold: two of
`h1` to `h4` that share a value, and a range of a timing value that starts at 0. `0` alone is valid
there, and a range of `content_padding_addition` may start at 0.

`mint` writes numbers, so every header it creates is a single value, outside every header the link
already holds, its own and the ones the defaults give it. When those leave fewer values than the
headers the link lacks, `mint` writes nothing and names the link.

**The timing values.** Five of the fields move what WireGuard fixes: four timers, in seconds, and a
count. `0` or the key left out leaves each to the module's constant.

| key | constant | what it moves |
| --- | --- | --- |
| `rekey_after_time` | 120 | The age of a key at which the end that started its handshake begins a new one as it sends a packet. Three times it is also the wait before what is left of a peer's keys is wiped once a handshake was given up. With `0` or the key left out the module takes 180 for that wait, not the 120 above, so it lasts 540 seconds. |
| `rekey_timeout` | 5 | The wait before an unanswered handshake is sent again, and the least time between two handshakes started towards one peer. |
| `max_handshake_attempts` | 18 | An unanswered handshake is sent again until its repeats pass this number. Then the packets waiting for the peer are dropped. |
| `keepalive_timeout` | 10 | When data came in and nothing went back for this long, an empty packet goes out. When data went out and nothing came back for this long plus `rekey_timeout`, a new handshake starts. |
| `reject_after_time` | 180 | Three times it is how long after a handshake a peer's keys are wiped from memory. It less `keepalive_timeout` and `rekey_timeout` is the age of a key at which the end that started its handshake begins a new one as it receives a packet. |

A key stops being used 180 seconds after its handshake, whatever `reject_after_time` says. Refused:

- a `rekey_after_time` of 180 or more, by the high end of its range: a renewal drawn at 180 or later
  comes after the key is gone;
- a `reject_after_time` that is not above `keepalive_timeout` plus `rekey_timeout`, each counted by
  the low end of its range and by its constant where it is `0` or left out: the age at which a
  packet received renews a key would be zero or less, and every handshake would be followed by
  another.

Not checked: a `reject_after_time` far from 180 moves the timers that depend on it away from the
moment a key stops being used.

**Header protection.** `header_protection_key` is the interface's: the module reads no switch for
one peer. Once the key is set, every handshake, cookie and data packet to and from every peer of
that interface is protected; junk packets and decoys are not. Every peer needs the same key and the
same `s1` to `s4`. A key taken out of the files stays in force on a node until its interface is
created anew: the module cannot take it off.

**Random trailers.** `random_trailers` has to be the same on both ends of a tunnel. An end without
it requires the exact length of a handshake or cookie message, and drops what an end with it sends.
Data packets pass either way.

With `random_trailers` on, the receiving module does not tell the types of message apart by their
length. It reads the four bytes at the offsets `s1`, `s2` and `s3` in turn, where a handshake
initiation, a handshake response and a cookie reply carry `h1`, `h2` and `h3`, and a data packet
whose bytes there happen to fall inside that header is taken for that message, and dropped. What
decides is the `random_trailers` of the receiving end, whatever the packet carries:
`content_padding_addition` does not change it. A handshake response can be taken for another
message the same way, which delays the link by one `rekey_timeout`.

Where the four bytes at each offset are random, the share of data packets dropped is at most the
number of values in `h1`, `h2` and `h3` together, divided by 2³², 4294967296; `h4` does not count.
It is less for short packets: a packet shorter than the message of an offset, that message with its
`s` bytes ahead of it, cannot be taken for it. A single value in each of the three costs 3 in 2³²;
ranges some 58, 340 and 337 million values wide cost about 16%.

The bytes are random with a `header_protection_key`. Without one, a data packet is `s4` random
bytes and then its 16-byte header in the clear: its type, its key index, which stays the same for
the life of a key, and its counter. An `s1`, `s2` or `s3` 1 to 15 above `s4` reads those fields,
and what is dropped is then decided by the packet's own key index and counter, not by chance. 8
above `s4` reads the low half of the counter: the packets whose counter is a value of that header
are dropped. 4 above reads the key index: where it is a value of the header, every data packet
under that key is dropped until the next handshake.

The tools accept both cases and refuse nothing. Once `mesh.amneziawg.obfuscation` is laid under
the link's own, `slipmesh check` and each generator warn for every link where `random_trailers` is
on and:

- one of `h1` to `h3` is a range. The warning names the link and the share;
- the link has no `header_protection_key`, and one of `s1` to `s3` is 1 to 15 above `s4`. The
  warning names the link and the offset, and gives no share.

With `random_trailers` on, keep `h1` to `h3` single values or narrow ranges, or put the range in
`h4`; and set a `header_protection_key`, or keep each of `s1` to `s3` from being 1 to 15 above `s4`.

### What a node gets

`sliplib-mesh` computes this from the network, the same for every platform, and a tool turns the
result into what its platform takes.

**Loopback addresses.** One IPv4 and one IPv6, by which the node is known inside the mesh. Each is
a network of `addressing.loopback` with `node_id` in its host part: the IPv4 one takes as many low
bits of `node_id` as the network leaves free, the IPv6 one all 32 as its low 32 bits. With the
networks `198.51.100.0/25` and `3fff:0:0:1::/64`, a `node_id` of `0.0.0.5` gives `198.51.100.5`
and `3fff:0:0:1::5`. A unique `node_id` does not make the addresses unique: in a /25, `0.0.0.5` and
`0.0.0.133` give the same IPv4 loopback. Two nodes with the same address in either family are
refused, and so is an IPv6 loopback network longer than /96, which leaves no room for the 32 bits.

**One interface for each link.** On each end of a link of either type, it is named by
`mesh.interface_prefix` and the name of the other end, listens on the `port` of the link's block,
and has the node's `wireguard.private_key`. Its one peer is the other end, known by the public half
of that node's key, and dialed at that node's `endpoint` and that `port` when it has an `endpoint`.
The peer is allowed every address: what goes into a link is decided by routing, not by the tunnel.

**The addresses on a link.** An IPv6 link-local, the same on every link of the node, on which
OSPFv3 runs. It is the network of [`addressing.tunnel.link_local`](#addressing) with `node_id` in
its host part, derived as the IPv4 loopback is: the network's own bits, then as many low bits of
`node_id` as the network leaves free, at most all 32 as the address's low 32 bits, and zeros
between. With the default `fe80::/64`, a `node_id` of `0.0.0.5` gives `fe80::5`. With
`addressing.tunnel.ipv4` the interface also carries the IPv4 address derived from it.

A network that leaves fewer than 32 bits free takes only the low bits of `node_id`, so those bits
have to differ between nodes: two nodes with the same derived address on the links, link-local or
IPv4, are refused, across the whole network and not only between the ends of one link. Unlike the
IPv6 loopback, `link_local` may be longer than /96. Every node gets such a link-local address, a
node on no link too, and such a node is refused for a duplicate or for a host part of zeros as any
other.

**A host part of zeros.** An address whose host part is all zeros is refused only where the prefix
it stands with on its interface makes it special. On an IPv4 prefix shorter than /31 the host part
of all zeros names the network and that of all ones its broadcast address
([RFC 1812, section 4.2.3](https://www.rfc-editor.org/rfc/rfc1812#section-4.2.3)), and neither is
taken. On an IPv6 prefix shorter than /127 the host part of all zeros is the Subnet-Router anycast
address ([RFC 4291, section 2.6.1](https://www.rfc-editor.org/rfc/rfc4291#section-2.6.1)), and is
not taken. A /31 ([RFC 3021](https://www.rfc-editor.org/rfc/rfc3021)), a /127
([RFC 6164](https://www.rfc-editor.org/rfc/rfc6164)), a /32 and a /128 have no such address. The
tools put the loopbacks on their interface as /32 and /128 and the IPv4 address of a link as /32,
so none of them is refused for its bits; the link-local of a link stands as /64, so one whose low
64 bits are all zeros is refused.

**Routing.** Every node runs OSPFv3 over its links, which carries the IPv6 loopbacks, and an iBGP
session with every other node between those loopbacks, under `routing.bgp.as`. iBGP carries IPv4
prefixes only: the node's IPv4 loopback, what its `direct_interfaces`, `learn` and `announce` name,
and on `talos` whatever else the node's router template exports, such as routes the node installs
for what runs over the mesh. With `routing.bfd`, a BFD session watches each link.

#### The MTU of the mesh

Every interface of every link gets one MTU, the largest at which a full-size packet still crosses
the underlay of every link in one piece. Each link has a limit, counted by what its type adds to a
packet. For `wireguard` and `amneziawg` it is

```text
U - 32 - 8 - IP - S4
```

where `U` is the smaller underlay `mtu` of its two ends, 32 WireGuard's header and tag, 8 the UDP
header, `IP` 20 or 40 by the family the link is dialed over, and `S4` the `s4` of an `amneziawg`
link once the defaults are laid under its own, 0 for a `wireguard` one. Two nodes on Ethernet,
dialed over IPv6 and with no `s4`, give 1420. The MTU of the mesh is the least of the limits of all
links, whatever their type. `mesh.mtu` lowers it, and one above it is refused. A result below 1280,
the least IPv6 allows a link, is an error, and so is a `mesh.mtu` with no links, which has no
interface to give it to.

Two refusals name the node and the key of the underlay `mtu` that causes them, such as
`underlay.wan.mtu`:

- An underlay `mtu` of either end below the least the family the link is dialed over allows a link:
  68 for IPv4 ([RFC 791, section 3.1](https://www.rfc-editor.org/rfc/rfc791#section-3.1)), 1280 for
  IPv6 ([RFC 8200, section 5](https://www.rfc-editor.org/rfc/rfc8200#section-5)).
- An underlay `mtu` of the narrower end that the bytes the link adds, `32 + 8 + IP + S4` of the
  formula above, reach or exceed: no room is left for a packet.

## `groups`

Named sets of nodes, for documents to be [aimed at](#aiming-a-document-at-nodes). They are not the
architecture of the mesh but what other documents name, so they can be written beside those
documents, in the same file or, once several files are read, in their own. The block of a `groups`
document takes `kind` and `order` alone. One is read; several, laid over each other into one set of
groups, are [Planned](#several-files). The loader turns a group into its nodes, so every tool
understands it; a group configures nothing by itself.

```yaml
slipmesh:
  kind: groups
core: [node-a, node-b, node-c]
workers: {include: [core], exclude: [node-c]}
```

The body maps a group's name to its value: a list of names, short for `include`, or a mapping with
`include` and `exclude`, the keys of a document's block. A name is a node or another group, and
`all` may stand there too. A group stands for what its `include` reaches less what its `exclude`
reaches, whatever the order in the lists: `workers` above is `node-a` and `node-b`.

Refused:

- a name that is neither a node nor a group, and a group named like a node;
- a group that reaches itself through the groups it names;
- a group left with no node, an empty list among them.

There is no mark before a name to take it out, since `!` starts a tag in YAML: `exclude` is the one
way to remove. A link's `ends` name nodes, never a group.

## Platform documents

The documents of a platform are written in its terms and name it in their `slipmesh:` block. They
may stand in the files of the network or in files of their own. Today the one such platform is
`talos`: `routeros` has no settings, templates or documents of its own and needs no `platform`
document. One is taken, and the RouterOS tool and `slipmesh check` refuse any key in its body; a
`template` or a `patch` that names `routeros` is refused.

### `platform`

Settings that only one platform's tool reads: at most one document for a platform, named by
`platform` in its block, taking neither `include` nor `exclude`. That platform's tool and
`slipmesh check` read its body; the tools of other platforms read its block alone, so a new key
here needs no release of theirs.

```yaml
slipmesh:
  kind: platform
  platform: talos
metrics:
  mesh: {port: 9586}
  router: {port: 9324}
```

The settings of `talos`:

| key | type | required | what it is |
| --- | --- | --- | --- |
| `metrics` | mapping | no | The Prometheus metrics of every `talos` node, by what serves them: `mesh`, the node's links, and `router`, its routing. Each holds `port`, an integer from 1 to 65535, required in it: the TCP port it is served on. |

A node listens on its own IPv4 loopback, so the metrics are reachable inside the mesh and nowhere
else. The ports are two because two daemons serve them and cannot share one. An entry left out
means no listener; a `port` of `0` is an error, since leaving the entry out is how to have none.

### `template`

The text from which a daemon on the node makes its own configuration - the routing daemon its
routing configuration, the firewall daemon its ruleset - filling it in with that node's values.

```yaml
slipmesh:
  kind: template
  platform: talos
daemon: router
base: sha256:0000000000000000000000000000000000000000000000000000000000000000
content: |
  router id {{ loopback }};
  protocol device {
  }
```

| key | where | type | required | what it is |
| --- | --- | --- | --- | --- |
| `platform` | block | `talos` | yes | The platform whose daemon takes the template. |
| `include`, `exclude` | block | lists of names | no | The nodes the template is for. Left out, every node of the platform. |
| `daemon` | body | `router` or `firewall` | yes | The daemon the template is for. |
| `base` | body | string | yes | The reference template this text was made from: `sha256:` and the 64 hexadecimal digits of the SHA-256 of that reference's text. |
| `content` | body | string | yes | The text of the template. |

Templates are not merged: where more than one for a daemon reaches a node, the node gets the last
of them by [rank](#aiming-a-document-at-nodes).

A daemon's reference template comes with the Talos generator, which prints a `template` document
holding it, `base` included, to add to the files and edit there:

```bash
slipmesh-taloscfg template router
slipmesh-taloscfg template firewall
```

The text in the files is the one a node gets, and a newer reference does not replace it; `base` is
how that is noticed. The generator holds one reference text for each daemon, its current one. When
the hash of that text is not the one `base` names, it warns and prints the difference between the
text in the files and the current reference, which the text can then be brought up to, with `base`
set to the hash the command above prints. The warning stops nothing.

What the Talos generator refuses:

- a `talos` node that no `router` template reaches, since the routing daemon does not run without
  one;
- files in which no document is a `firewall` template. A node that none of them reaches is not an
  error: the generator writes no `nftables` document for it, and the node gets no ruleset;
- a template its daemon would fail on: one that does not compile, or that reads a variable the
  daemon does not supply, in any branch and not only in those a node takes. A `router` template is
  also filled in with the config of every node it reaches; a `firewall` template is not, since its
  values exist only on the node.

Those checks of the text are the generator's, `slipmesh-taloscfg generate --check`.
`slipmesh check` checks the keys of the document and does not read the text.

### `patch`

A document in the platform's own format, carried to the nodes it reaches - for `talos`, a document
of the machine configuration - for what this format has no key for.

```yaml
slipmesh:
  kind: patch
  platform: talos
  include: [node-b]
apiVersion: v1alpha1
kind: ExtensionServiceConfig
name: example
environment:
  - EXAMPLE_LEVEL=debug
```

| key | where | type | required | what it is |
| --- | --- | --- | --- | --- |
| `platform` | block | `talos` | yes | The platform whose document this is. |
| `include`, `exclude` | block | lists of names | no | The nodes the document is for. Left out, every node of the platform. |
| the rest | body | | | The document itself. For `talos` it has to have a `kind`, by which documents are told apart. |

For a node, the `patch` documents of one identity - for `talos` the `apiVersion`, `kind` and `name`
of the document - are merged into one in the order of their [rank](#aiming-a-document-at-nodes), by
[the one rule](#laying-one-value-over-another). Talos' own merge appends to a list; this one does
not.

A `talos` document with `$patch: delete` is not merged: it deletes the document of its identity.
Where it comes after others, the result is the deletion; where another comes after it, that one
replaces the result outright. The generator refuses one that deletes the `KubeNodeConfig`, which
carries the node's annotations.

A file that a `talos` document carries in `configFiles[].content` may be written as YAML, a mapping
or a list, in place of a string. The node gets that YAML's text, and each of its fields is a field
of `slipmesh.yaml` like any other.

The generator makes some documents of a node itself: the `ExtensionServiceConfig` documents `awg`,
`router` and `nftables`, written in that order after the `patch` documents, and the annotations
under `slipmesh.net/` in the node's `KubeNodeConfig`.
A `patch` may add files to those three, and annotations of its own to the `KubeNodeConfig`. The
generator refuses:

- a `patch` of one of the three that sets anything but `configFiles`, or a file at a path the
  generator writes there;
- an annotation under `slipmesh.net/`.

`slipmesh check` reads the block of a `patch` and not its body. What the body means is the
platform's, and the generator checks what it knows of it.

### `vars`

**Planned:** a kind of document holding values for templates - what differs between networks or
between nodes while the text of a template stays the same - aimed at nodes with `include` and
`exclude`, and read by a template by name.

## Who serves what

What each tool does with each part of the files under revision 1. `slipmesh check` checks all of
it; the column of `slipmesh` says what it writes. How each tool is run, and what it writes or
changes, is in its own documentation.

| part | `slipmesh` writes | on `talos`, `slipmesh-taloscfg` | on `routeros`, `slipmesh-routeros` |
| --- | --- | --- | --- |
| `addressing`, `nodes`, `mesh` | keys, obfuscation, the reference value | Links of both types: the node's link interfaces with their obfuscation, keepalive, addresses and MTU; the node's annotations. | Links of type `wireguard`, the only type it serves: interfaces, peers with their keepalive, addresses, MTU. |
| `routing.bgp` | | OSPFv3 and iBGP. | The same. |
| `routing.direct_interfaces`, `learn`, `announce` | | As the router template has it. The reference one announces the connected networks of the interfaces, the kernel routes inside `learn`, and `announce` as written. | The connected networks of matching interfaces, the connected networks inside `learn`, and `announce` as written. |
| `routing.bfd` | | A BFD session on every link. | The same. |
| `routing.classes` | | Each class's `protocol` as an annotation. | Not served. |
| `groups` | | Understood, as by every tool. | The same. |
| `platform` | | The metrics listeners. | No settings. |
| `template` | | Checked, and given to the node's daemon. | Refused. |
| `patch` | | Merged, and carried into the node's machine configuration. | Refused. |

On `talos` the generator tells a node these values as annotations under `slipmesh.net/`:
`interface_prefix.mesh`; `route_proto_id.<name>` for each route class; `endpoint`, the address or
host name under the node's `endpoint`, none for a node without one; and `mesh_mtu`, the MTU of the
mesh as computed.

## Example

[`example/`](example) holds a whole network in version 2, written in three files as it will be once
[several files](#several-files) are read:

- [`mesh.yaml`](example/mesh.yaml), the architecture: three `talos` nodes and a `routeros` one,
  three links and a route class;
- [`secrets.yaml`](example/secrets.yaml), laid over it: the nodes' keys and what `slipmesh mint`
  wrote into the links' obfuscation, with a header protection key written by hand;
- [`talos.yaml`](example/talos.yaml), the documents of `talos`: its settings, two templates, two
  patches, and two groups: `workers`, which one patch is aimed with, and `core`, which `workers` is
  made of.

A tool reads one file today, so the three are given to it as one, their documents in that order,
which is what laying `secrets.yaml` over `mesh.yaml` comes to. They hold no Planned part, since a
file with one is refused. One link has a header, a timer and its [keepalive](#keepalive) written as
ranges.

Their networks and addresses are from the ranges reserved for documentation, but for the
link-local `fe80::` of the links; a `node_id` is an identity, not an address. Their AS number is
one reserved for documentation, their names stand for nothing, and their keys are a letter
repeated.

## What `slipmesh` writes

`slipmesh` is the only tool that writes into the files, and it writes values, not documents:

- `mint` creates what cannot be derived and is missing: a node's `wireguard.private_key`, and the
  fields of [the obfuscation](#obfuscation) it writes into a link of type `amneziawg`. It also
  writes the [reference value](#required-values-defaults-and-reference-values) of
  `mesh.interface_prefix`;
- `rekey` replaces one node's `wireguard.private_key`.

A command that writes replaces the one file it reads, given with `--config`, with what it
creates in it.

**Planned:** with [several files](#several-files), a command that writes takes one file with
`--edit` and writes what it creates there. It reads the file too, among the files of `--in`, so it
sees what was already written in it; the file may stand in `--in` as well, and is one file, read
once, either way. An `--edit` file that does not exist yet is created, holding one `network`
document with the `version` and `revision` of the ones read.

```bash
slipmesh mint --in mesh.yaml --edit secrets.yaml
```

With several files, refused:

- an `--edit` file that exists and does not hold exactly one `network` document, since the command
  would not know which to write into;
- a change to a value already set, as `rekey` makes, where the value lies in a file other than the
  `--edit` one. The refusal names that file: written into the `--edit` file, the new value would lie
  under or over the old one by the order of the documents, not by the intent of the command.

The commands are described [with the tool](../README.md#slipmesh). Everything else - nodes, links,
groups, templates, every document - is written by hand.

### A write rewrites the whole file

A command that writes reads its file into values, changes the ones it is about, and writes
the file back from the values, in the layout of the YAML serializer. The values survive. Of a file
written by hand, this does not:

- its layout: indentation, flow collections such as `{a: 1}` and `[a, b]`, quoting, blank lines;
- every comment outside a block scalar. A `#` line inside the `|` text of a template is part of that
  text, and stays;
- every document that holds nothing: empty, only comments, or `~`;
- the link between an anchor and its aliases: each alias becomes a copy of its own;
- the base a number was written in: `0x5DC` comes back as `1500`. A value that has to read the way
  it was written is a string, where its key takes one.

So the first write to a file written by hand changes most of its lines. `slipmesh fmt` makes that
change alone: it rewrites a file in the form every write leaves it in and changes no value. It takes
the file given with `--config` and rewrites it in place. **Planned:** with several files, it takes
the files to rewrite as its arguments, not `--in` and `--edit`, and rewrites each in place, laying
nothing over anything. It reads back the text it is about to write, and refuses to write it if the
values of any document differ from the file's. Run once and committed on its own, it keeps every
later write down to the lines that write is about. What a comment said has to move elsewhere first,
for instance into a README beside the files.

A command with nothing to write leaves the file as it is. Before it replaces the file, a command
other than `fmt` checks that the files, with the result in its place, still read as a valid
network. Every command replaces the file in one step, keeping its permissions.
