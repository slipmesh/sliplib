# `slipmesh.yaml`, format version 2

`slipmesh.yaml` describes one slipmesh network: its nodes, the links between them, and the
routing over those links. It is the single source for that. Every tool that configures a node
reads it itself, and none reads what another tool produced.

What runs over the mesh is not the format's to describe: access for clients from outside, the
rules for it, the choice of which traffic leaves through which node. Those are described where
they run. Version 1 held two such parts, and [What leaves the file](#what-leaves-the-file) says
where they go.

This document is the specification of the format. It describes what the tools do, and names the
parts that are only **Planned** where they will stand: what no tool does is not in the format yet,
and [Planned](#planned) says how such a part arrives.

No released tool reads version 2 yet. The tools released so far read version 1, the format that
[Differences from version 1](#differences-from-version-1) starts from. Where this document says
what a tool does, it means the first release that reads version 2.

## The files

A network may be written in one file or in several, and a tool reads them all as one. It takes
each as plain text: a file kept encrypted is decrypted by whatever runs the tool, and the format
knows nothing of how. The private keys of the nodes are written in the clear, so the file that
holds them is kept where such files are kept; [several files](#several-files) let that be a file of
its own.

People write the documents. One tool, [`slipmesh`](../README.md#slipmesh), writes values into
them - the keys nobody can derive, and reference values - and checks the files as a whole. Every
other tool only reads: where something a tool needs is missing, the tool refuses and names what
writes it. [What `slipmesh` writes](#what-slipmesh-writes) says what a write does to a file
written by hand.

### Documents and the `slipmesh:` block

A file is a stream of YAML documents separated by `---`. Every document has a top-level
`slipmesh:` block that says what the document is; the rest of the document is its body.

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
| [`network`](#the-network-document) | at least one, laid over each other | not taken | not taken | The mesh: its addressing, routing, links and nodes. |
| [`groups`](#groups) | any, laid over each other | not taken | not taken | Named sets of nodes, for documents to be aimed at. |
| [`platform`](#platform) | at most one for a platform | not taken | required | Settings only that platform's tool reads. |
| [`template`](#template) | any | optional | required | The template a daemon on the node makes its config from. |
| [`patch`](#patch) | any | optional | required | A document in the platform's own format. |

The block is strict: a key it does not know, or one its kind does not take, is an error, so a
misspelled `include` cannot quietly aim a document at every node. A document without the block is
an error. A document that holds nothing - empty, only comments, or `~` - is skipped.

### Several files

A tool takes its files with `--in`, which takes several values and may be repeated:
`--in cfg/*.yaml` reads every such file of the directory. A command that writes reads the file of
its [`--edit`](#what-slipmesh-writes) as well. The tool first brings every path given to `--in`
and `--edit` to its canonical form, and sorts the files by those paths, byte by byte. That is the
order the files are read in. Neither the order on the command line nor the way a path is written
matters: one file under two spellings, such as `a.yaml` and `./a.yaml`, or reached through a
symbolic link, is one file, read once. A file of `--edit` that does not exist yet is taken at the
canonical path of its directory.

Files in one directory therefore sort by name. Files in different directories sort by where those
directories are, which differs from one machine to another.

Once every file is read, and before the body of any document is parsed, the documents are sorted
by the `order` of their `slipmesh:` block, `1000` where it is left out. Documents of the same
`order` keep the order they were read in: by file, then by their place in the file. This is what
"the order of the documents" means below. Where documents are merged or chosen between for a node,
their [rank](#aiming-a-document-at-nodes) puts one more rule between `order` and the order they
were read in. A document that has to come first or last whatever the paths of the files say so
with its `order`.

The `network` documents are [laid over each other](#laying-one-value-over-another) in that order,
starting from an empty tree, and so are the `groups` documents. There may be any number of each,
in different files or in one, and the result is one network and one set of groups. That result is
what is checked: a single document need not be valid alone, so one file may hold the architecture
of the mesh and another, laid over it, its secrets. Every `network` document carries `version` and
`revision`, and they are the same in all of them. Without any `network` document there is no
network, which is an error.

Documents of the other kinds are not laid over each other. They are collected in the same order,
and where some of them are merged or chosen between for a node, they
[rank](#aiming-a-document-at-nodes) by it.

What is laid over what is decided by `order` and the canonical paths of the files, and by nothing
else. The format does not know which of its keys are secret: a file of secrets is an ordinary layer that
holds them.

### Laying one value over another

One rule lays a value over another wherever the format does so. It is JSON Merge Patch,
[RFC 7396](https://www.rfc-editor.org/rfc/rfc7396), the way values files are laid over each other
in Helm:

- a mapping is laid over a mapping key by key, and the rule applies again to each key's value;
- a scalar or a list replaces what was there, whole. A list is not appended to, since an
  appended list would leave no way to take an element out;
- `null` removes the key it stands at.

The rule is used in three places:

- **Between documents.** The `network` documents are laid in order over an empty tree, and so are
  the `groups` documents, so a `null` in any of them, the first included, means the key is absent.
- **For a node.** A key under [`routing`](#routing) is the value of the network, and
  `nodes.<name>.routing` holds the node's own [by the same path](#a-nodes-routing).
- **For a link.** `mesh.<type>` holds the defaults of a link type, and `mesh.links.<name>.<type>`
  the link's own: today, the [keepalive](#keepalive) of both types, and the
  [obfuscation](#obfuscation) of a link of type `amneziawg`.

A key written with nothing after it is `null` in YAML. `node-a:` alone under a link's `ends`
therefore removes that end rather than naming it: an end with no keys is written `node-a: {}`.

### Version and revision

Both stand in the `slipmesh:` block of every `network` document and nowhere else, and both are
required:

```yaml
slipmesh:
  kind: network
  version: 2
  revision: 1
```

`version` is the syntax of the format as a whole. This document describes version 2. It changes
only when the syntax itself is replaced, which means converting every file. A `network` document
without `version` belongs to version 1, which this document does not describe:
[Converting a file by hand](#converting-a-file-by-hand) turns one into version 2.

`revision` is the generation of the rules that turn the files into configs. It grows when the same
data has to give a different result, for instance when what a document configures on a node passes
from one component to another. A tool is built for exactly one revision and accepts no other. An
older tool refuses files of a newer revision, a newer tool refuses files of an older one, and each
says which of the two to update. Such a change is therefore switched on by one deliberate edit of
this number, made together with moving every tool to the release built for it, and not by
whichever tool happens to be upgraded first.

| revision | what it means |
| --- | --- |
| 1 | the rules this document describes |

Nothing else raises either number:

- A key, a kind or a value that a tool begins to serve - a [Planned](#planned) part among them -
  arrives with a release of the tools that read it, which [strict parsing](#strict-parsing) makes
  necessary: a tool refuses what it does not know. Neither number moves.
- Within one `version` a key is not renamed. A new name is added beside the old one, both are
  accepted, and writing both is an error. The old name leaves with the next `version`.

### Names

Nodes, node groups, links, the underlay entries of a node and route classes are named by the keys
of the mappings that hold them, and one rule holds for all of them: ASCII letters, digits, `-`, `_`
and `.`, with a letter or a digit first and last. Names become parts of interface names, of names
in a node's routing configuration and of annotation keys, which take no more. A class name is at
most 48 characters, since it ends the key of a node annotation, which takes 63.

Node names and group names share one namespace. A group named like a node is an error, so a name
in a list of nodes never means two things. `all` and `none` are reserved: no node or group takes
either. Each of the other kinds of name is unique among its own kind, which its mapping already
makes it.

### Aiming a document at nodes

A document for some nodes names them in its `slipmesh:` block:

- `include` lists the nodes the document reaches;
- `exclude` takes nodes back out.

Each is a list of names, and a name is a node of the network or a [group](#groups). `all` is a
group of its own that nobody defines: every node, and for a document of one platform every node of
that platform. It stands wherever a group name can.

`include` left out means `all`, and `exclude` left out means `none`. Both words may be written as
the whole value of their key, `include: all` and `exclude: none`, and mean the same as leaving the
key out.

- A name that is neither a node nor a group is an error.
- `include: []` is an error, and so is a document left reaching no node once `exclude` is taken
  out: neither is what anyone means to write.
- `exclude` only removes. A name in it that `include` did not bring in changes nothing.
- "A group but one of its nodes" is written with the two keys: `include: [workers]` and
  `exclude: [node-c]`.

Where several documents of one kind reach a node and are merged or chosen between, they rank by
three things, each breaking the ties the one before leaves:

1. `order`, lower first;
2. the documents for all before the others. A document is for all when its `include` is left
   out, is the word `all`, or is a list whose only name is `all`: one set of nodes written two
   ways ranks the same. Any other list is narrower;
3. the order they were read in.

`exclude` narrows the nodes a document reaches and leaves its rank as it is. So among documents of
one `order`, one aimed at some nodes comes after the one for all, wherever it stands, and an
`order` set on purpose moves either past the other. The last by rank wins.

The kinds that take the two keys, `template` and `patch`, take them as optional. The kinds that take
neither are about no node in particular, and there the keys are an error.

A document written in the terms of one platform names it with `platform` in its block, one value:
a [platform of the nodes](#who-serves-a-document). It reaches nodes of that platform alone. With
an `include` other than `all`, every node it is left reaching once `exclude` is taken out has to be of that platform:
a node of another is an error, not something passed over.

### Strict parsing

All tools read the files through one loader, the one in `sliplib-config`. What is checked in a
document depends on whose it is:

- **Every tool, in every document:** the `slipmesh:` block - the kind, the keys that kind takes,
  the names in `include` and `exclude` - and the version and revision.
- **Every tool, in the network and the groups:** every key and every value, at any depth, once
  the `network` documents and the `groups` documents are each laid over each other. Every tool
  serves both.
- **The tool of a platform, in that platform's documents:** every key and every value of the body.
  An unknown key is an error at any depth.
- **A tool, in a document of another platform:** nothing beyond the block. It does not read the
  body, so a key added to another platform's document does not concern it.
- **`slipmesh check`, in all of it:** every body strictly, but the text of a `template` and the
  body of a `patch`, which it leaves to the platform's tool. It is the one check of the files as a
  whole, and the one to run after an edit by hand.

The tools are built with the format they read, so a key a tool does not know is a mistake: passed
over, it would silently stop configuring what it was written for. A body written in a platform's
own language - the document inside a `patch`, the text of a `template` - is checked as far as the
tool can check it; the sections of those kinds say how far.

What follows for releases:

- a new key in the network, or a new kind, needs a release of every tool, since they share the
  loader and all serve the network;
- a new key in a platform's document needs a release of that platform's tool and of `slipmesh`;
- none of them moves `version` or `revision`.

### Planned

What no tool does yet is not in the format the tools read: no key, no parsing. A file that writes
such a part is refused by strict parsing, the same as a key nobody meant. This document names each
such part where it will stand, with the word **Planned**, its path in the tree and what it is for;
its full description comes with the release that serves it. The shape around it is laid out so
that it can be added without moving the version or the revision.

### Who refuses, and how far

A refusal comes from one of three places, and how much it stops depends on which:

- **The loader**, shared by every tool, refuses what is wrong in the `slipmesh:` block of any
  document or in the network, which every tool serves, such as a link of a [type](#meshlinks)
  that the tool of one of its ends does not serve. It stops everything: every tool refuses, for
  every node.
- **The computation of a node's configs**, in `sliplib-mesh`, refuses what is wrong for one node:
  a missing secret, an obfuscation the kernel module would refuse, an interface name too long. It
  stops that node and the nodes it has a link with, since each end of a link needs the other. A
  tool still configures the rest. [The MTU of the mesh](#the-mtu-of-the-mesh) is the exception:
  it is computed from every link, so what it refuses stops every node.
- **A platform's tool** refuses what is wrong in a document only it serves, such as a `template`.
  The tools of other platforms do not read it.

`slipmesh check` makes all three, for every node. Where a section below lists what is refused, it
is the loader that refuses it unless the section names another.

### Required values, defaults and reference values

A value that two components have to agree on, or that decides who serves what, is written in the
files and has no default in any tool: a node's platform, the number of a route class. Left out, it
is an error that names the key.

A key may be left out where its absence means the feature is off: no metrics port, no listener;
no `routing.bfd`, no BFD.

A few keys default to a value. Each has one consumer chain and nothing to agree with, and there
are no others:

| key | default | |
| --- | --- | --- |
| `nodes.<name>.underlay.<entry>.mtu` | `1500` | the MTU of Ethernet |
| `routing.bfd.min_rx_ms`, `min_tx_ms`, `multiplier` | `300`, `300`, `5` | a link is a tunnel, some of them long: detection in about 1.5 s, where a tighter timer would flap |

For a required key that is absent `slipmesh mint` may write a *reference value*: the value a
network takes unless it has a reason for another. There is one such key, `mesh.interface_prefix`,
and its reference value is `mesh-`. Written, it is a value like any other, and no tool falls back
to it - which is why a tool refuses files where the key is still missing, and names `mint`.

`mint` writes reference values and creates secrets. It never writes what a person chooses: a
node's platform or `node_id`, a port, the AS number, the networks, the number of a route class,
who is in a group. Where one of those is missing, the refusal names the key.

### Who serves a document

Every node names its `platform`, and the platform is the tool that configures the node. That
tool reads the files itself and serves, on its own nodes, the documents that reach them.

| `platform` | tool |
| --- | --- |
| `talos` | the Talos generator, `slipmesh-taloscfg` |
| `routeros` | the RouterOS tool, `slipmesh-routeros` |

A setting that the tools of more than one platform need stands in the network, not in a
platform's document: `announce` is one, since every platform builds a node's announcements from
it. A key only one platform understands stands in a document of that platform.

A document may reach nodes of more than one platform, and each tool takes from it the nodes that
are its own. The tools do not know of each other: what a platform's tool does not implement
reaches no device of that platform, and nothing reports it, unless a rule of the format says
otherwise where it describes the key. [Who serves what](#who-serves-what) says what each tool
serves under the revision.

A new platform is a new value of `platform`, and a tool that serves it. The loader learns the
value, so it arrives with a release of every tool; neither the version nor the revision moves. It
serves the link types it serves, and the loader refuses a link of another type that reaches one of
its nodes, as the [table of link types](#meshlinks) shows for the platforms there are.

## The network document

The network is one tree. A file may hold it whole in one `network` document, or several files may
hold parts of it that are [laid over each other](#several-files). It is about no node in
particular: the document takes neither `include` nor `exclude`. The tool of every platform serves
it, so a new key here needs a release of them all.

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

Where a value is one for each address family, its key is a mapping by family, `ipv4` and `ipv6`.
Where it is a list of prefixes, it is one list, and each prefix says its own family. A family a
key does not take yet is refused.

### `addressing`

The networks from which the tool derives a node's addresses by its `node_id`, as
[What a node gets](#what-a-node-gets) says.

| key | type | required | what it is |
| --- | --- | --- | --- |
| `loopback` | mapping | yes | `ipv4` and `ipv6`, both required: the networks of the nodes' loopback addresses. |
| `tunnel` | mapping | no | The networks of the addresses on the links' interfaces: `ipv4` and `link_local`, each optional. |

`tunnel.ipv4` is for a node that has to send IPv4 from a link, as when it masquerades what it
forwards into one: a link's interface has no IPv4 address otherwise. With it, every link's
interface of a node carries an address from `tunnel.ipv4`, derived from `node_id` the way the
IPv4 loopback is, and the addresses on the links are announced as
[`direct_interfaces`](#routing) says.

`tunnel.link_local` is the network of the IPv6 link-local address of every link's interface, on
which OSPFv3 runs. It is a network inside `fe80::/10`, and one outside it is refused. Left out, it
is `fe80::/64`. It does not depend on `tunnel.ipv4`: either may be written without the other.

**Planned:** `addressing.tunnel.ipv6`, a routable IPv6 address on the links' interfaces.

### `routing`

The routing over the mesh, for every node. A node holds its own value for some of these keys
under [`nodes.<name>.routing`](#a-nodes-routing).

| key | type | required | default | what it is |
| --- | --- | --- | --- | --- |
| `bgp` | mapping | yes | | Its one key, `as`, an integer from 1 to 4294967295, is the AS number of the mesh, under which the nodes speak iBGP to each other. |
| `bfd` | mapping | no | none | [BFD on the links](#routingbfd). |
| `direct_interfaces` | list of strings | no | empty | The interfaces whose connected IPv4 networks a node announces. |
| `learn` | list of strings | no | empty | IPv4 prefixes: a node announces the routes its kernel holds inside them. |
| `announce` | list of strings | no | empty | IPv4 prefixes a node announces as its own. |
| `classes` | mapping | no | none | [Classes of routes](#routingclasses) that the mesh carries and something other than the router installs. |

`direct_interfaces`, `learn` and `announce` are the three ways a node comes to announce an IPv4
prefix to the mesh, besides its own loopback.

- `direct_interfaces` is for a network the node has connected on a real interface. An entry is
  the name of an interface or a pattern with `*`, and one that matches no interface of a node
  announces nothing there.
- `learn` is for a range whose parts the nodes get at run time, each node routing its own part
  without an address of it on any interface, as with the pod range of a Kubernetes cluster. A
  route inside a `learn` prefix is announced whoever installed it.
- `announce` is for a prefix the node answers for without having it connected, as with the
  service range of such a cluster: the node holds it on its loopback and announces it as
  written.

A node that has no part in what `learn` and `announce` name, such as a router outside the
Kubernetes cluster whose ranges they are, holds `learn: []` and `announce: []` in its own
`routing`: it neither takes those routes in nor announces them.

An entry of `learn` or `announce` that is not an IPv4 prefix is refused.

The tool derives the addresses on the links, so announcing them is the tool's business too. With
`addressing.tunnel.ipv4`, the pattern of the links' interfaces - the
[interface prefix](#mesh) and `*` - is added at the end of every node's `direct_interfaces`,
unless the list already holds that exact string. The IPv4 addresses on the links are therefore
announced if and only if `addressing.tunnel.ipv4` is set, and `direct_interfaces` lists only what the
writer of the files adds.

`direct_interfaces` is a list in order, and the order reaches the node: the reference router
template of `talos` makes one `protocol direct` of each entry, in that order. Writing the pattern
in the list is valid. Written last, it changes nothing; written before other entries, it keeps the
place it was written in.

**Planned:**

- `routing.bgp.sessions`: named sessions, internal and external, in one place. An internal entry
  names `reflectors` and their `clients`, for a mesh in which a session between every two nodes is
  too many. An external one names the `nodes` that hold it and the `neighbor`, with its `as` and
  address. A session may carry its own `as`, the number the mesh speaks to that neighbour under.
  Without entries every node holds a session with every other, as now.
- `routing.ospf`: settings of OSPFv3, such as the default cost of a link.
- IPv6 prefixes in `learn` and `announce`.

#### `routing.bfd`

Bidirectional Forwarding Detection on the links. It notices a dead link within seconds, where
the routing protocol alone takes its dead interval. The block present, every link has a BFD
session; absent, none has. `bfd: {}` turns it on with the defaults.

| key | type | required | default | what it is |
| --- | --- | --- | --- | --- |
| `min_rx_ms` | integer, at least 1 | no | `300` | The least interval between received control packets, in milliseconds. |
| `min_tx_ms` | integer, at least 1 | no | `300` | The least interval between sent control packets, in milliseconds. |
| `multiplier` | integer, 1 to 255 | no | `5` | How many packets missed in a row take the link down. |

There are no other settings. These three are what the BFD of every platform takes, so the two
ends of a link are always configured alike.

#### `routing.classes`

A class is a set of routes that something other than the router installs in a node's kernel and
the mesh carries to the other nodes: the routes that lead out through a node, for one. The router
of a node tells them from its other kernel routes by their protocol number, `proto` in `ip route`.

```yaml
routing:
  classes:
    egress:
      protocol: 152
```

| key | type | required | what it is |
| --- | --- | --- | --- |
| `protocol` | integer, 0 to 255 | yes | The protocol number of the class's routes in a node's kernel. |

The name of a class is any [name](#names). On `talos` a node is told each class's number as the
annotation `slipmesh.net/route_proto_id.<name>`, for whatever installs that class's routes there.
Without the class there is no such annotation.

The numbers are checked over all classes together. Refused: two classes with the same number;
`0` to `4`, the kernel's own; `200`, the number the routes to the clients of roadwarriors pools
carry.

**Planned:** a class's own `table`, the kernel routing table its routes go into on the receiving
nodes; its `families`; and `rules`, a mapping of named routing rules, each with a priority, a
mark, which is a string of `0x` and hexadecimal digits, and where it sends. A class without a table puts its
routes in `main`.

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

The node's name is part of an interface name on every node it has a link with, and an interface
name holds 15 bytes at most. With the interface prefix `mesh-` that leaves a name 10; a longer
one is refused once the node is on a link, when the configs of the node at its other end are
computed.

`wireguard.private_key` is a secret nobody writes by hand: `slipmesh mint` creates it, for a node
that is on a link of a type that takes it, and `slipmesh rekey` replaces it. Until it is written,
such a node cannot be configured, and neither can any node it has a link with, since each end of a
link needs the public half of the other's key. The tools refuse such a node when its configs are
computed, and name `mint`.

#### `underlay`

The interfaces a node reaches the world through are not the mesh's: it neither configures nor
addresses them. `underlay` states the facts about them that the links need, one entry by name.

| key | type | required | default | what it is |
| --- | --- | --- | --- | --- |
| `endpoint` | mapping | no | none | The address at which the node's peers reach it, by family: `ipv4` or `ipv6`, one of them, holding an IP address of that family or a host name. |
| `mtu` | integer | no | `1500` | The MTU of the path, which bounds every link the node is on. Set it where the path is narrower, as behind a provider's tunnel or over PPPoE. |

The key under `endpoint` is the family the links to the node are dialed over, and the
[MTU](#the-mtu-of-the-mesh) is counted for it. An address under the key of the other family is
an error. A host name says no family, so the key says it: a peer dials the first address the
resolver returns for the name, and a `routeros` device resolves the name itself, so a name with
addresses of both families has to resolve to the family of its key first. Otherwise the links to
the node carry an MTU counted for the other family's header.

`endpoint` takes no port, since each link adds its own, and one written with a port is refused.

An entry without `endpoint` is a node behind NAT or without a fixed address. It is never dialed:
it dials the other end of each of its links.

Refused: a node with no `underlay` entry or with more than one, and an `endpoint` with both
families.

**Planned:** a second entry, such as a network inside a site with a larger MTU, which a link
picks at its [end](#meshlinks); and both families in one `endpoint`.

#### A node's routing

`nodes.<name>.routing` holds the node's own values of [`routing`](#routing) by the same path, laid
over the network's [by the one rule](#laying-one-value-over-another). Today it takes
`direct_interfaces`, `learn` and `announce`. Each is a list, so the node's list replaces the
network's and does not extend it: a node that writes one gets exactly what it wrote, with `[]`
nothing, and with `null` the key is gone, which is nothing too.

### `mesh`

Everything about the links: the names of their interfaces, their MTU, the defaults of their types
and the links themselves.

| key | type | required | default | what it is |
| --- | --- | --- | --- | --- |
| `interface_prefix` | string | yes | | What the name of every interface a tool creates for a link starts with. `slipmesh mint` writes the reference value, `mesh-`. |
| `mtu` | integer | no | the least limit of the links | Lowers [the MTU of the mesh](#the-mtu-of-the-mesh). |
| `wireguard` | mapping | no | none | The defaults of links of type `wireguard`. Its one key is [`persistent_keepalive`](#keepalive). |
| `amneziawg` | mapping | no | none | The defaults of links of type `amneziawg`: [`persistent_keepalive`](#keepalive), and `obfuscation`, [the obfuscation](#obfuscation) every such link starts from. |
| `links` | mapping | no | none | [The links](#meshlinks), by name. |

`interface_prefix` takes ASCII letters, digits, `-`, `_` and `.`, and is not empty. With `*` after
it, it is also the pattern by which routing and `direct_interfaces` find the links' interfaces.
On `talos` the node is told the prefix as the annotation `slipmesh.net/interface_prefix.mesh`.

#### `mesh.links`

The links of the mesh, a mapping by name. A link's `type` says what carries it, its `ends` name
the nodes it joins, and the keys that belong to its type stand in a block named after the type.

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

The name of a link names it in messages; nothing is configured from it.

There are two types today, and both are a tunnel: exactly two ends, and on each an interface the
tool creates, named by the interface prefix and the name of the other end.

| type | its block | what it is |
| --- | --- | --- |
| `wireguard` | `port`, an integer, required: the UDP port both ends listen on. `persistent_keepalive`, optional: the link's [keepalive](#keepalive), over `mesh.wireguard.persistent_keepalive`. | Plain WireGuard. |
| `amneziawg` | `port` and `persistent_keepalive`, as for `wireguard`, the keepalive over `mesh.amneziawg.persistent_keepalive`; `obfuscation`, a mapping, optional: the [obfuscation](#obfuscation) fields of this link, laid over `mesh.amneziawg.obfuscation`. | WireGuard whose packets AmneziaWG keeps from looking like WireGuard. |

Both take each end's key from the node's `wireguard.private_key`. What stays outside the blocks
holds for every type: the `ends` of a link; the `underlay` of a node, which says where and over
what its peers reach it; and `addressing.tunnel`, the MTU of the mesh and OSPFv3, which run over
every link.

Which platform serves which type:

| type | `talos` | `routeros` |
| --- | --- | --- |
| `wireguard` | served | served |
| `amneziawg` | served | refused |

A new type is a new value of `type` and a block of its own: on a link, and under `mesh` or on a
node where the type needs keys there. Nothing else changes, and neither the version nor the
revision moves. Like any new key, it reaches the tools with a release of theirs.

What is refused:

- a name in `ends` that is not a node, and a link of a tunnel type without exactly two ends;
- a second link between the same two nodes;
- a block of a type other than the link's, and a link without the block of its own type;
- a `port` that another link of the same node uses;
- a link of a type that the tool of one of its ends does not serve, so that no link is configured
  at one end alone. The error names the type and the link. Today the RouterOS tool serves
  `wireguard` alone, so a link that reaches a `routeros` node is of type `wireguard`;
- with [the MTU of the mesh](#the-mtu-of-the-mesh), so stopping every node: a link with no
  `endpoint` on either end, which neither could dial, and a link whose two `endpoint`s are of
  different families, where the family would depend on which end dialed first;
- when the configs of its two nodes are computed: a link of type `amneziawg` that lacks one of the
  fields `slipmesh mint` writes, once `mesh.amneziawg.obfuscation` is laid under its own.

`mint` writes a link's obfuscation and never its `port`: which port a link takes is decided by
whoever writes the files.

**Planned:**

- At a link's end, `underlay`, the node's entry the link goes through, and `interface`, the name of
  its interface. A second link between the same two nodes needs it, since the default name would
  be the same.
- Type `ethernet`: an interface the node already has, and two ends or more, as on a VLAN. The tool
  creates nothing and only runs routing over it.
- A link's own `routing`, such as the OSPF cost and BFD of that link, and its own `mtu`.
- `mesh.<type>.fwmark`, and the same key in a link's block: the mark an interface puts on its own
  outer packets, by which the node's routing rules and firewall tell them apart. A string, `0x` and
  hexadecimal digits. Not served on `routeros`.
- A preshared key of a link: a secret in the block of its type, the same on both ends, which
  `slipmesh mint` creates.

#### Keepalive

`persistent_keepalive` is how long, in seconds, nothing may pass between the two ends of a link,
in either direction, before an end sends its peer a packet, so that a NAT on the way keeps the
link's mapping open. It stands in the block of a
link's type, and `mesh.<type>.persistent_keepalive` is the network's value,
[laid under](#laying-one-value-over-another) the link's own.

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

`0` switches the keepalive off. From a range the kernel module draws an interval each time it sets
the timer. A range starts at 1 or above, since a draw of 0 would send at once.

An end sends it to a peer it dials, one whose `underlay` has an `endpoint`. An end that only waits
to be dialed, by a peer without one, sends none.

It is not `keepalive_timeout` of the obfuscation: that one is how long the module waits, once data
came in, before it answers with an empty packet; this one is the interval that keeps a NAT mapping
alive.

`talos` serves it on both types, and `routeros` on `wireguard`, the one type it serves.

#### Obfuscation

The AmneziaWG parameters of a link of type `amneziawg`: what keeps its packets from looking like
WireGuard. They stand under `obfuscation` in the link's `amneziawg` block. The one under
`mesh.amneziawg` is the default of every such link, the link's own is
[laid over it](#laying-one-value-over-another), and both ends of the link get the result. A link of
type `wireguard` has none.

`slipmesh mint` writes nine of the fields into every link of type `amneziawg` that lacks them once
the defaults are laid under its own, with values of its own for each link: `jc`, `jmin`, `jmax`,
`s1`, `s2` and `h1` to `h4`. The others are written by hand where they are wanted. Every field is
optional, and one left out leaves that part of the tunnel as WireGuard has it.

| key | type | what it is |
| --- | --- | --- |
| `jc` | integer, 0 to 65535 | How many junk packets go out before a handshake. |
| `jmin`, `jmax` | integer, 0 to 65535 | The least and the greatest size of a junk packet. `jmin` above `jmax` is an error where `jc` and `jmax` are both above 0, the case in which the module draws each size between them. |
| `s1`, `s2`, `s3`, `s4` | integer, 0 to 65535 | Bytes of junk added to a packet of each type: handshake initiation, handshake response, cookie reply, data. `s4` is on every data packet and lowers [the MTU of the mesh](#the-mtu-of-the-mesh). |
| `h1`, `h2`, `h3`, `h4` | number or range, up to 4294967295 | The value that stands in place of the WireGuard message type of the same four packet types. No two of the four may share a value, one left out counting as its WireGuard type, 1 to 4. |
| `i1` to `i5` | string | Decoys: packets sent ahead of a handshake, each described in tags under this table. |
| `header_protection_key` | string | A secret: base64 of 32 bytes that encrypts the type and reserved fields of the packet header, for every peer of the interface. Each of `s1` to `s4` has to be at least 12, the room its nonce takes. |
| `content_padding_addition` | number or range, up to 65535 | Bytes added to a data packet in place of the padding it would otherwise get: WireGuard's own, or the random trailer when `random_trailers` is on. `0` or the key left out leaves the packet that padding. |
| `rekey_after_time`, `rekey_timeout`, `reject_after_time`, `keepalive_timeout`, `max_handshake_attempts` | number or range, up to 65535 | The timing values: four timers and a count that WireGuard fixes, each described under this table. `0` or the key left out leaves one to the module's constant. |
| `random_trailers` | boolean | A trailer of random length on outgoing packets. Both ends of a tunnel set it, or neither. |
| `disable_cookies` | boolean | No cookie replies. |

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
numbers around one dash and nothing else, both ends included, `lo` not above `hi`. The kernel
module draws a value from it, for a header with every packet it sends. A number is the range of
that one value. A write by `slipmesh` leaves both as they were written, a range a string and a
number a number.

```yaml
obfuscation:
  h1: 1101-1199
  h2: 2202
  rekey_after_time: 100-140
```

A range reaches a node's daemon as that string, and a daemon of a release that knows no ranges
refuses its config. No tool checks which release a node runs.

`mint` writes numbers, so every header it creates is a single value. A header it creates lies
outside every header the link already holds, its own and the ones the defaults give it. When those
leave fewer values than the headers the link lacks, `mint` writes nothing and names the link.

Two ranges are refused whatever they hold: two of `h1` to `h4` that share a value, and a range of
a timing value that starts at 0. `0` alone is valid there, and a range of
`content_padding_addition` may start at 0.

**The timing values.** Five of the fields move what WireGuard fixes: four timers, in seconds, and
a count. `0` or the key left out leaves each to the module's constant.

| key | constant | what it moves |
| --- | --- | --- |
| `rekey_after_time` | 120 | The age of a key at which the end that started its handshake begins a new one as it sends a packet. Three times it is also the wait before what is left of a peer's keys is wiped once a handshake was given up. With `0` or the key left out the module takes 180 for that wait, not the 120 above, so it lasts 540 seconds. |
| `rekey_timeout` | 5 | The wait before an unanswered handshake is sent again, and the least time between two handshakes started towards one peer. |
| `max_handshake_attempts` | 18 | An unanswered handshake is sent again until its repeats pass this number. Then the packets waiting for the peer are dropped. |
| `keepalive_timeout` | 10 | When data came in and nothing went back for this long, an empty packet goes out. When data went out and nothing came back for this long plus `rekey_timeout`, a new handshake starts. |
| `reject_after_time` | 180 | Three times it is how long after a handshake a peer's keys are wiped from memory. It less `keepalive_timeout` and `rekey_timeout` is the age of a key at which the end that started its handshake begins a new one as it receives a packet. |

A key stops being used 180 seconds after its handshake, and `reject_after_time` does not move that
moment. Two settings are refused:

- a `rekey_after_time` of 180 or more, by the high end of its range: a renewal drawn at 180 or
  later comes after the key is gone;
- a `reject_after_time` that is not above `keepalive_timeout` plus `rekey_timeout`: the age at
  which a packet received renews a key would be zero or less, and every handshake would be
  followed by another. Each of the three counts by the low end of its range, and one that is `0`
  or left out by its constant.

Not checked: a `reject_after_time` far from 180 moves the timers that depend on it away from the
moment a key stops being used, which stays where it is.

**Header protection.** `header_protection_key` is the interface's: the module reads no switch for
one peer. Once the key is set, every handshake, cookie and data packet to and from every peer of
that interface is protected. Junk packets and the decoys of `i1` to `i5` are not. Every peer needs
the same key and the same `s1` to `s4`. A key taken out of the files stays in force on a node
until its interface is created anew: the module cannot take it off.

**Random trailers.** `random_trailers` has to be the same on both ends of a tunnel. An end without
it requires the exact length of a handshake or cookie message, and drops what an end with it
sends. Data packets pass either way.

**Random trailers and header ranges.** With `random_trailers` on, the receiving module does not
tell the types of message apart by their length. It reads the four bytes at the offsets `s1`, `s2`
and `s3` in turn, where a handshake initiation, a handshake response and a cookie reply carry
`h1`, `h2` and `h3`. A data packet whose bytes there happen to fall inside the header of that
offset is taken for that message, and dropped. What decides is the `random_trailers` of the
receiving end, whatever the packet carries: `content_padding_addition` does not change it. A
handshake response can be taken for another message the same way, which delays the link by one
`rekey_timeout`.

The share of data packets dropped is at most the number of values in `h1`, `h2` and `h3`
together, divided by 2³², 4294967296. It is less for short packets: a packet shorter than the
message of an offset, that message with its `s` bytes ahead of it, cannot be taken for it. `h4`
does not count. A single value
in each of the three costs 3 in 2³²; ranges some 58, 340 and 337 million values wide cost about
16%.

The tools accept the combination. `slipmesh check` and each generator warn for every link where
`random_trailers` is on and one of `h1` to `h3` is a range, once `mesh.amneziawg.obfuscation` is
laid under the link's own, and the warning names the link and the share. With `random_trailers`
on, keep `h1` to `h3` single values or narrow ranges, or put the range in `h4`.

The two ends of a link get the same fields, so they agree by themselves.

An obfuscation the kernel module would refuse is refused when the configs of the link's two nodes
are computed. One in `mesh.amneziawg.obfuscation` therefore stops every node with a link of type
`amneziawg` that does not set the field itself.

### What a node gets

A tool does not work any of this out itself. `sliplib-mesh` computes it from the network, the same
for every platform, and a tool turns the result into what its platform takes.

**Loopback addresses.** One IPv4 and one IPv6, by which the node is known inside the mesh. Each
is a network of `addressing.loopback` with `node_id` in its host part: the IPv4 one takes as many
low bits of `node_id` as the network leaves free, the IPv6 one takes all 32 as its low 32 bits.
With the networks `198.51.100.0/25` and `3fff:0:0:1::/64`, a `node_id` of `0.0.0.5` gives
`198.51.100.5` and `3fff:0:0:1::5`.

A unique `node_id` does not make the addresses unique: in a /25, `0.0.0.5` and `0.0.0.133` give the
same IPv4 loopback. Two nodes with the same address in either family are refused. So is an IPv6
loopback network longer than /96, which leaves no room for the 32 bits.

**One interface for each link.** For a link of either type today, on each end it is named by
`mesh.interface_prefix` and the name of the other end, listens on the `port` of the link's block,
and has the node's `wireguard.private_key`. Its one peer is the other end, known by the public
half of that node's key and dialed at that node's `endpoint` and that `port` when it has an
`endpoint`. The peer is allowed every address: what goes into a link is decided by routing, not by
the tunnel.

**The addresses on a link.** An IPv6 link-local, the same on every link of the node, on which
OSPFv3 runs. It is the network of [`addressing.tunnel.link_local`](#addressing) with `node_id` in
its host part, the way the IPv4 loopback is derived: the network's own bits, then as many low bits
of `node_id` as the network leaves free, at most all 32 as the address's low 32 bits, and zeros
between. With the default `fe80::/64`, a `node_id` of `0.0.0.5` gives `fe80::5`. With
`addressing.tunnel.ipv4` the interface also carries the IPv4 address derived from it.

A network that leaves fewer than 32 bits free takes only the low bits of `node_id`, so those bits
have to differ between nodes. Two nodes with the same derived address on the links, link-local or
IPv4, are refused, across the whole network and not only between the two ends of one link. A
network of any length is taken: unlike the IPv6 loopback, `link_local` may be longer than /96.

**A host part of zeros.** An address whose host part is all zeros is refused only where the prefix
it stands with on its interface makes it special. On an IPv4 prefix shorter than /31 the host part
of all zeros names the network and that of all ones its broadcast address
([RFC 1812, section 4.2.3](https://www.rfc-editor.org/rfc/rfc1812#section-4.2.3)), and neither
is taken. On an IPv6 prefix shorter than /127 the host part of all zeros is the Subnet-Router
anycast address ([RFC 4291, section 2.6.1](https://www.rfc-editor.org/rfc/rfc4291#section-2.6.1)),
and is not taken. A /31 ([RFC 3021](https://www.rfc-editor.org/rfc/rfc3021)), a /127
([RFC 6164](https://www.rfc-editor.org/rfc/rfc6164)), a /32 and a /128 have no such address. The
tools put the loopbacks on their interface as /32 and /128, and the IPv4 address of a link as /32,
so none of them is refused for its bits. The link-local of a link stands as /64, so one whose low
64 bits are all zeros is refused.

#### The MTU of the mesh

Every interface of every link gets one MTU, the largest at which a full-size packet still crosses
the underlay of every link in one piece. Each link has a limit, and how it is counted depends on
the link's type, by what the type adds to a packet. For `wireguard` and `amneziawg` it is

```text
U - 32 - 8 - IP - S4
```

where `U` is the smaller underlay `mtu` of its two ends, 32 is WireGuard's header and tag, 8 the
UDP header, `IP` 20 or 40 by the family the link is dialed over, and `S4` the `s4` of an
`amneziawg` link, once the defaults are laid under its own, and 0 for a `wireguard` one. Two nodes
on Ethernet, dialed over IPv6 and with no `s4`, give 1420. The MTU of the mesh is the least of the
limits of all links, whatever their type. `mesh.mtu` lowers it and cannot raise it, and a result
below 1280, the least IPv6 allows a link, is an error. So is a `mesh.mtu` with no links, which has
no interface to give it to. The MTU is computed from every link, and what it refuses stops every
node.

**Routing.** Every node runs OSPFv3 over its links, which carries the IPv6 loopbacks, and an iBGP
session with every other node between those loopbacks, under `routing.bgp.as`. iBGP carries IPv4
prefixes only: the node's IPv4 loopback, what its `direct_interfaces`, `learn` and `announce`
name, and on `talos` whatever else the node's router template exports, such as routes the node
installs for what runs over the mesh. With `routing.bfd`, a BFD session watches each link.

## `groups`

Named sets of nodes, for documents to be [aimed at](#aiming-a-document-at-nodes). Groups are not
the architecture of the mesh: they are what other documents name, and they can be written beside
those documents, in the same file or in their own. The block of a `groups` document takes `kind`
and `order` alone. Like the `network` documents, any number of `groups` documents are
[laid over each other](#several-files) into one set of groups. The loader turns a group into its
nodes, so every tool understands it, and a group configures nothing by itself.

```yaml
slipmesh:
  kind: groups
core: [node-a, node-b, node-c]
workers: {include: [core], exclude: [node-c]}
```

The body is a mapping from the name of a group to its value. The value is a list of names, which
is short for `include`, or a mapping with `include` and `exclude`, the keys of a document's block.
A name is a node or another group, and `all` may stand there too. A group stands for its nodes:
what its `include` reaches less what its `exclude` reaches, whatever the order in the lists.
`workers` above is `node-a` and `node-b`.

Refused:

- a name that is neither a node nor a group, and a group named like a node;
- a group that reaches itself through the groups it names;
- a group left with no node, an empty list among them.

There is no mark before a name to take it out: `!` starts a tag in YAML, and `exclude` is the one
way to remove.

A link's `ends` name nodes, never a group.

## Platform documents

The documents of a platform are written in its terms and name it in their `slipmesh:` block. They
may stand in the files of the network or in files of their own. Today that platform is `talos`:
`routeros` has no settings, templates or documents of its own, and a `template` or a `patch` that
names it is refused.

### `platform`

Settings that only one platform's tool reads. There is at most one for a platform, named by
`platform` in its block, and the document takes neither `include` nor `exclude`. That platform's
tool and `slipmesh check` read its body. The tools of other platforms read its block alone, so a
new key here needs no release of theirs.

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

`routeros` has no settings of its own, and needs no `platform` document.

### `template`

The text from which a daemon on the node makes its own configuration: the routing daemon its
routing configuration, the firewall daemon its ruleset. The daemon fills the template in on the
node, with that node's values.

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

Templates are not merged. Where more than one document for a daemon reaches a node, that node gets
the last of them by [rank](#aiming-a-document-at-nodes). So among templates of one `order`, one
aimed at some nodes wins over the one for all, wherever it stands.

A daemon's reference template comes with the Talos generator, which prints a `template` document
holding it, `base` included, to add to the files and edit there:

```bash
slipmesh-taloscfg template router
slipmesh-taloscfg template firewall
```

The text in the files is the one a node gets, and a newer reference does not replace it. `base` is
how that is noticed. The generator holds one reference text for each daemon, its current one.
When the hash of that text is not the one `base` names, it warns and prints the difference between
the text in the files and the current reference. The text can then be brought up to it and `base`
set to the hash the command above prints. The warning stops nothing.

What the Talos generator refuses:

- a `talos` node that no `router` template reaches, since the routing daemon does not run without
  one;
- files in which no document is a `firewall` template. A node that none of them reaches is not an
  error: the generator writes no `nftables` document for it, and the node gets no ruleset;
- a template its daemon would fail on: one that does not compile, or that reads a variable the
  daemon does not supply, in any branch and not only in those a node takes. A `router` template is
  also filled in with the config of every node it reaches. A `firewall` template is not, since its
  values exist only on the node.

Those checks of the text are the generator's, `slipmesh-taloscfg generate --check`. `slipmesh
check` checks the keys of the document and does not read the text.

### `patch`

A document in the platform's own format, carried to the nodes it reaches: for `talos`, a document
of the machine configuration. It is for what this format has no key for.

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
of the document - are merged into one in the order of their [rank](#aiming-a-document-at-nodes),
so among documents of one `order` the more specific one wins wherever it stands. They merge by
[the one rule](#laying-one-value-over-another). Talos' own merge appends to a list; this one does
not.

A `talos` document with `$patch: delete` is not merged: it deletes the document of its identity.
Where it comes after others, the result is the deletion; where another comes after it, that one
replaces the result outright. The generator refuses one that deletes the `KubeNodeConfig`, which
carries the node's annotations.

A file that a `talos` document carries in `configFiles[].content` may be written as YAML, a mapping
or a list, in place of a string. The node gets that YAML's text, and each of its fields is a field
of `slipmesh.yaml` like any other.

Some documents of a node the generator makes itself: the `ExtensionServiceConfig` documents `awg`,
`router` and `nftables`, and the annotations under `slipmesh.net/` in the node's `KubeNodeConfig`.
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
it; the column of `slipmesh` says what the tool writes.

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

On `talos` the generator also tells a node values as annotations under `slipmesh.net/`:
`interface_prefix.mesh`; `route_proto_id.<name>` for each route class; `endpoint`, the value of the
node's `endpoint`; and `mesh_mtu`, the MTU of the mesh as computed.

How each tool is run, and what it writes or changes, is in its own documentation.

## Example

[`example/`](example) holds a whole network in version 2, in three files read together:

- [`mesh.yaml`](example/mesh.yaml), the architecture: three `talos` nodes and a `routeros` one,
  three links and a route class;
- [`secrets.yaml`](example/secrets.yaml), laid over it: the nodes' keys and what `slipmesh mint`
  wrote into the links' obfuscation, with a header protection key written by hand;
- [`talos.yaml`](example/talos.yaml), the documents of `talos`: its settings, two templates, two
  patches, and the two groups the patches are aimed with.

They lie in one directory and sort in that order, so `secrets.yaml` is laid over `mesh.yaml`. The
same documents in one file would give the same configs.

The files hold no Planned part, since a file with one is refused: those are named in their own
sections above. Of the keys there are they hold all but four of the six padding and timing values
of [the obfuscation](#obfuscation), which are written as the two they hold are. One link has a
header, a timer and its [keepalive](#keepalive) written as ranges.

Their networks and addresses are from the ranges reserved for documentation, but for the
link-local `fe80::` of the links; a `node_id` is an identity, not an address. Their AS number is
one reserved for documentation, their names stand for nothing, and their keys are a letter
repeated.

## What `slipmesh` writes

`slipmesh` is the only tool that writes into the files, and it writes values, not documents:

- `mint` creates what cannot be derived and is missing: a node's `wireguard.private_key`, and the
  fields `jc`, `jmin`, `jmax`, `s1`, `s2` and `h1` to `h4` of the obfuscation of a link of type
  `amneziawg`. It also writes the
  [reference value](#required-values-defaults-and-reference-values) of `mesh.interface_prefix`;
- `rekey` replaces one node's `wireguard.private_key`.

A command that writes takes one file with `--edit`, and writes what it creates there. The file is
read as well, among the files of `--in`, so the command sees what was already written in it. It
may stand in `--in` too, or not: either way it is [one file](#several-files), read once. A file of
`--edit` that does not exist yet is created.

```bash
slipmesh mint --in mesh.yaml --edit secrets.yaml
```

What is refused:

- a file of `--edit` that exists and does not hold exactly one `network` document, since the
  command would not know which to write into;
- a change to a value that is already set, as `rekey` makes, where the value lies in a file other
  than the one of `--edit`. The refusal names that file. Written into the file of `--edit`, the new
  value would lie under or over the old one by the order of the documents, not by the intent of
  the command.

The commands are described [with the tool](../README.md#slipmesh). Everything else - nodes, links,
groups, templates, every document - is written by hand.

### A write rewrites the whole file

A command that writes reads the file of its `--edit` into values, changes the ones it is about, and writes the
file back from the values, in the layout of the YAML serializer. The values survive. Of a file
written by hand, this does not:

- its layout: indentation, flow collections such as `{a: 1}` and `[a, b]`, quoting, blank lines;
- every comment outside a block scalar. A `#` line inside the `|` text of a template is part of
  that text, and stays;
- every document that holds nothing: empty, only comments, or `~`;
- the link between an anchor and its aliases: each alias becomes a copy of its own;
- the base a number was written in: `0x5DC` comes back as `1500`. A value that has to read the
  way it was written is a string, where its key takes one.

So the first write to a file written by hand changes most of its lines. `slipmesh fmt` makes that
change alone: it rewrites a file in the form every write leaves it in and changes no value. It
takes the files to rewrite as its arguments, not `--in` and `--edit`, and rewrites each in place;
nothing is laid over anything. It reads back the text it is about to write, and refuses to write
it if the values of any document differ from the file's. Run once and committed on its own, it
keeps every later write down to the lines that write is about. What a comment said has to move
elsewhere first, for instance into a README beside the files.

A command with nothing to write leaves the file as it is. Before it replaces the file, a command
other than `fmt` checks that the files, with the result in its place, still read as a valid
network. Every command replaces the file in one step, keeping its permissions.

### Writing into a file kept encrypted

Whatever decrypts a file for editing hands its editor command one path, that of the decrypted
copy, as the last word. `--edit` takes that path, and the other files are given with `--in`
before it. The file itself is not among them: the copy stands in its place.

```bash
EDITOR="slipmesh mint --in mesh.yaml --edit"
```

The copy lies in a temporary directory, not beside the file, and its place among the other files
depends on where that directory is, which differs from one machine to another. Where the files set
the same keys, the file kept encrypted gives its documents an `order`, so that their place does not
depend on where the copy lies.

## Differences from version 1

Version 1 is the format every release so far reads.

What changed in general:

- The network is one tree: `addressing`, `routing`, `mesh` and `nodes`. Version 1 spread it over
  `cluster`, `bfd`, `obfuscation`, `nodes` and `mesh`.
- What is named is a mapping by name: nodes, links, groups, a node's underlay entries, route
  classes. Version 1 held nodes and links in lists, and a link had no name.
- The network may be written in several files, laid over each other by one rule. Version 1 read
  one file.
- A document may set its place among the others with `order`, which ranks before everything
  else. In version 1 a document had the place it stood in.
- The format says its `version` and `revision`.
- Every node names its `platform`, and a document written for one platform reaches only that
  platform's nodes.
- `include` and `exclude` take groups, and `all`, as well as nodes. Groups are a kind of document
  of their own, and a group may name other groups and take nodes out. A document left reaching no
  node once `exclude` is taken out is refused.
- Parsing is strict at every depth. Version 1 refused an unknown key in the `slipmesh:` block,
  at the top of the `network` document and in its `cluster` block, and passed over one in a node,
  a link, `mesh`, `bfd`, `obfuscation`, and the loopback and tunnel networks. A misspelled key
  that version 1 ignored there is refused now.
- The AS number is required. Version 1 read a missing one as 0, which it refused only when a
  node's configs were computed.
- A node's endpoint says its family by its key, and takes no port. Version 1 dialed a port
  written there, and counted the MTU of the link as if the endpoint were a host name; without
  `address_family` it counted a host name for IPv6.
- One rule for names, which version 1 did not have: ASCII letters, digits, `-`, `_` and `.`,
  with a letter or a digit first and last. `all` and `none` are reserved.
- New refusals among the nodes: two nodes with the same loopback address; an IPv6 loopback
  network longer than /96; two nodes with the same address on the links, link-local or IPv4; a
  link-local whose low 64 bits are all zeros. Version 1 checked none of them.
- The network of the link-local addresses on the links is a key of its own,
  `addressing.tunnel.link_local`, written with or without an IPv4 network beside it. Version 1
  took it from `tunnel_networks.ipv6`, which came only together with `ipv4`, and did not check
  that it lay inside `fe80::/10`.
- Two links between one pair of nodes are refused.
- With `addressing.tunnel.ipv4`, the pattern of the links' interfaces no longer has to stand in
  `direct_interfaces`: it is added at the end of the list. Version 1 refused a file without it.
- Every link has a `type`, and the keys of that type stand in a block named after it. A link that
  version 1 marked `plain` is of type `wireguard`, any other of type `amneziawg`.
- A link of a type that the tool of one of its ends does not serve is refused, which makes a link
  to a `routeros` node one of type `wireguard`. Version 1 did not know a node's platform and
  rendered an obfuscated link to such a node with obfuscation on one end alone.
- The keepalive of a link is a key, `persistent_keepalive`. Version 1 had none and sent one every
  25 seconds on either type; left out, it is now still 25 on a link of type `wireguard`, and
  `20-30` on one of type `amneziawg`.
- BFD is on where its block is. Version 1 had `bfd.enable`.
- The numbers of route classes are checked against each other and against `200`. Version 1
  checked one number, against `0` to `4`.
- Without the class `egress` a `talos` node gets no annotation `slipmesh.net/route_proto_id.egress`.
  Version 1 always set it, to 152 when the file had no value.
- `slipmesh mint` writes one reference value, that of `mesh.interface_prefix`. The second one
  version 1 wrote, 152 for `cluster.route_proto_id.egress`, has no successor: the number of a route
  class is written by hand.

The kinds:

| version 1 | version 2 |
| --- | --- |
| `network`, `patch` | The same kinds, with the changes below. |
| `roadwarriors` | Gone: [it leaves the file](#what-leaves-the-file). |
| | New: `groups`, `platform` and `template`. |
| `nftables`, kept only to be refused with a hint | Gone: an unknown kind like any other. |
| A `patch` naming the `ExtensionServiceConfig` `router`, to give it `router.conf.j2` | A `template` with `daemon: router`. |
| A `patch` that is the whole `ExtensionServiceConfig` `nftables`, with `nftables.conf.j2` | A `template` with `daemon: firewall`. The generator makes the document, and a `patch` may only add files to it. |
| A `patch` naming the `ExtensionServiceConfig` `mikrotik`, with how to reach the device | Gone from the files. The RouterOS tool takes that from its environment. |
| `exclude` of the non-Talos nodes on every `patch` | Not needed: a `patch` names its `platform` and reaches no other. |

The keys of the `network` document:

| version 1 | version 2 |
| --- | --- |
| `cluster` | Gone: its keys move to `addressing`, `routing`, `mesh` and the `platform` document of `talos`. |
| `cluster.bgp_as` | `routing.bgp.as`, now required. |
| `cluster.loopback_networks` | `addressing.loopback`. |
| `cluster.tunnel_networks.ipv4` | `addressing.tunnel.ipv4`. |
| `cluster.tunnel_networks.ipv6` | `addressing.tunnel.link_local`, the same network. |
| `cluster.interface_prefix.mesh` | `mesh.interface_prefix`. |
| `cluster.mesh_mtu` | `mesh.mtu`. |
| `cluster.direct_interfaces` | `routing.direct_interfaces`. |
| `cluster.pod_subnet`, one prefix | `routing.learn`, a list of prefixes. |
| `cluster.service_subnet`, one prefix, announced under the label `k8s-services` | `routing.announce`, a list of prefixes. There is no label. |
| `cluster.route_proto_id.egress` | `routing.classes.egress.protocol`. |
| `cluster.awg_metrics_port`, `cluster.router_metrics_port` | `metrics.mesh.port` and `metrics.router.port` in the [`platform`](#platform) document of `talos`. |
| `cluster.bypass_refresh_interval_secs`, `bypass` | Gone: [they leave the file](#what-leaves-the-file). |
| `bfd` | `routing.bfd`, without `enable`. |
| `obfuscation` | `mesh.amneziawg.obfuscation`. |
| `nodes`, a list | `nodes`, a mapping by name; `name` is gone. |
| `nodes[].endpoint`, `address_family`, `uplink_mtu` | `nodes.<name>.underlay.<entry>.endpoint.<family>` and `mtu`. |
| `nodes[].mesh_private_key` | `nodes.<name>.wireguard.private_key`. |
| `nodes[].direct_interfaces` | `nodes.<name>.routing.direct_interfaces`; the node's `routing` also takes `learn` and `announce`. |
| `mesh.links`, a list | `mesh.links`, a mapping by name. |
| `mesh.links[].pair` | `ends`, a mapping by node name. |
| `mesh.links[].port`, `obfuscation`, `plain` | `type`, required; `port` and `obfuscation` in the block of the type. `plain: true` is `type: wireguard`, and `plain` is gone. |

### What leaves the file

Version 1 also held two parts that describe what runs over the mesh rather than the mesh:
roadwarriors pools, one `roadwarriors` document each, and the lists of destinations that leave the
mesh through a node, under the key `bypass` of the `network` document with
`cluster.bypass_refresh_interval_secs`. Version 2 has neither. Both become resources in the
Kubernetes cluster, where the operators that serve them read them: a list becomes an egress list,
and a pool a roadwarriors pool with its clients.

**Planned:** the operators, and the command each of them has to write its resources out of a
version 1 file, once.

### Converting a file by hand

No command converts a file. It is edited by hand, and the tools then show that nothing changed
that was not meant to.

1. Before the file is edited, leave nothing pending: `slipmesh-taloscfg generate --diff` prints no
   change to a patch file, and `slipmesh-routeros --node <name> --check --diff` plans none on its
   device. Whatever shows later then comes from the conversion.
2. Move what the file's comments say to where it will be kept: the first write by `slipmesh`
   drops them.
3. Have the pools and the lists written out as resources of the cluster, as
   [What leaves the file](#what-leaves-the-file) says. **Planned.**
4. Edit the file:
   1. In the `slipmesh:` block of the `network` document, write `version: 2` and `revision: 1`.
   2. Write `addressing`: `loopback` is `cluster.loopback_networks`. Where the file has
      `cluster.tunnel_networks`, `tunnel.ipv4` is its `ipv4`, and `tunnel.link_local` its `ipv6`,
      moved as it is: the link-local addresses are derived from it as before.
   3. Write `routing`: `bgp.as` is `cluster.bgp_as`; `direct_interfaces` is the list of the same
      name; `learn` holds `pod_subnet` and `announce` holds `service_subnet`, each in a list. The
      label `k8s-services` has no place in version 2. `bfd` moves here without `enable`: where
      `enable` was `false` or absent, delete the block. `classes.egress.protocol` is
      `cluster.route_proto_id.egress`.
   4. Write `mesh`: `interface_prefix` is `cluster.interface_prefix.mesh`, `mtu` is
      `cluster.mesh_mtu`, and `amneziawg.obfuscation` is the network's `obfuscation`. To keep the
      keepalive of the links of type `amneziawg` where it was, write
      `amneziawg.persistent_keepalive: 25`.
   5. Move `awg_metrics_port` and `router_metrics_port` into a new `platform` document for
      `talos`, as `metrics.mesh.port` and `metrics.router.port`. Delete `cluster`.
   6. Turn `nodes` into a mapping: each node's `name` becomes its key. A name outside
      [the rule](#names), or `all` or `none`, has to change, and so do the names of the interfaces
      named after it on the node's peers. Give every node its `platform`. Its `mesh_private_key`
      becomes `wireguard.private_key`, and its `direct_interfaces` moves under its `routing`.
   7. Give every node one `underlay` entry, named as you like. Its `mtu` is the node's
      `uplink_mtu`, left out where that was. Its `endpoint` holds the node's `endpoint` without any
      port, under the key of its family: the family of the address, or for a host name the family
      it resolves to first. Version 1 counted a host name without `address_family` for IPv6; where
      it resolves to IPv4, the MTU of the mesh may now come out up to 20 higher: write a `mesh.mtu`
      of the value it had, since the MTU of a working mesh does not move in a conversion. A node
      without `endpoint` gets an entry without one.
   8. Give every node outside the cluster those two ranges belong to, a `routeros` router for one,
      `learn: []` and `announce: []` under its `routing`.
   9. Turn `mesh.links` into a mapping, giving each link a name. Its `pair` becomes `ends`, each
      node with `{}`. A link with `plain: true` becomes `type: wireguard`, with its `port` in a
      `wireguard` block. Any other becomes `type: amneziawg`, with its `port` and `obfuscation` in
      an `amneziawg` block. A link that reaches a `routeros` node has to be of type `wireguard`.
   10. Delete `bypass`, `bypass_refresh_interval_secs` and every `roadwarriors` document.
   11. Turn each `patch` that carries a template into a `template` document, `daemon: router` or
       `daemon: firewall`, and keep its `include` and `exclude`. The template's text becomes
       `content`. `base` is the hash of the reference that text was made from. Where that is not
       known, the hash printed by `slipmesh-taloscfg template router` or
       `slipmesh-taloscfg template firewall` is the current reference's: it silences the warning
       without anything having been compared.
   12. What else such a `patch` held: its other `configFiles` stay in a `patch` of the same
       document, which sets `configFiles` only, and none at a path the generator writes, the
       template's among them. Any other key of the `router` document version 1 already passed
       over; of the `nftables` document the generator now writes the rest itself, and the key has
       no place in the files.
   13. Give every remaining `patch` its `platform`, drop the `exclude` that kept it from the nodes
       of other platforms, take a node of another platform out of its `include`, and delete the
       `mikrotik` document.
   14. Where several documents name the same set of nodes, a group in a `groups` document may
       name it once.
       Where the keys or the obfuscation should be kept apart, move them into a file of their own,
       laid over the rest: `nodes.<name>.wireguard` and `mesh.links.<name>.amneziawg.obfuscation`
       keep their paths there.

   Where the pattern of the links' interfaces stands last in a `direct_interfaces`, it may stay or
   be taken out: the configs are the same either way. Where it stands before other entries, taking
   it out moves it to the end, and the node's routing configuration changes with it.
5. Run `slipmesh check`. It parses every document strictly, but the text of a `template` and the
   body of a `patch`, which the generator checks, and it checks every node's configs.
6. Run the two commands of step 1 again. They show these differences and no other:
   - in the patch file of every `talos` node, the `awg` config holds no pool interfaces,
     `router.yaml` holds no `bypass`, and its `announce` holds no label, so the node's routing
     configuration loses the comment the label made;
   - a node given `announce: []` in step 4.8 no longer announces the service range;
   - unless step 4.4 wrote `persistent_keepalive: 25`, the keepalive of every link of type
     `amneziawg` is `20-30` where it was 25;
   - where a node was renamed in step 4.6, the interfaces named after it, and what refers to them.

   Anything else is a mistake in the conversion. The generator no longer writes a patch file for a
   node of another platform, which never used it: delete the one version 1 left.
7. Run `slipmesh mint`, which writes nothing where nothing is missing, then `slipmesh fmt` on each
   file, and commit that rewrite on its own.
