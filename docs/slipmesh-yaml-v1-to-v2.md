# `slipmesh.yaml` from version 1 to version 2

Version 1 is the format sliplib v0.19 and the releases before it read. [Version
2](slipmesh-yaml.md), which its document describes, is the one from v0.20.0 on, and those releases
refuse a file of version 1. This document says what differs between them and how a file is
converted.

## Differences from version 1

In general:

- The network is one tree: `addressing`, `routing`, `mesh` and `nodes`. Version 1 spread it over
  `cluster`, `bfd`, `obfuscation`, `nodes` and `mesh`.
- What is named is a mapping by name: nodes, links, groups, a node's underlay entries, route
  classes. Version 1 held nodes and links in lists, and a link had no name.
- The network may be written in several `network` documents and several files, laid over each
  other by one rule. Version 1 read one file and one network document.
- A document may set its place among the others with `order`, which ranks before everything else.
  In version 1 a document had the place it stood in.
- The format says its `version` and `revision`.
- Every node names its `platform`. A document written for one platform is for that platform's
  nodes, and a node of another that its `include` reaches makes it an error.
- `include` and `exclude` take groups, and `all`, as well as nodes. Groups are a kind of document of
  their own, and a group may name other groups and take nodes out. A document left reaching no node
  once `exclude` is taken out is refused.
- Parsing is strict at every depth. Version 1 refused an unknown key in the `slipmesh:` block, at
  the top of the `network` document and in its `cluster` block, and passed over one in a node, a
  link, `mesh`, `bfd`, `obfuscation`, and the loopback and tunnel networks. A misspelled key that
  version 1 ignored there is refused now.
- The AS number is required. Version 1 read a missing one as 0, which it refused only when a node's
  configs were computed.
- A node's endpoint says its family by its key, and holds no port. Version 1 dialed a port written
  there, and counted the MTU of the link as if the endpoint were a host name; without
  `address_family` it counted a host name for IPv6. The loader takes a name as written, so step 4.7
  takes a port and the brackets of an IPv6 address out, and step 6 shows one left behind in every
  peer's endpoint for that node.
- One rule for names, which version 1 did not have: ASCII letters, digits, `-`, `_` and `.`, with a
  letter or a digit first and last. `all` and `none` are reserved.
- New refusals among the nodes, none of which version 1 checked: a `node_id` wider than the host
  part of a network it is added to, which version 1 cut to the bits that network leaves free; and
  `0.0.0.0`.
- The network of the link-local addresses on the links is a key of its own,
  `addressing.tunnel.link_local`, written with or without an IPv4 network beside it. Version 1 took
  it from `tunnel_networks.ipv6`, which came only together with `ipv4`, and did not check that it
  lay inside `fe80::/10`.
- Two links between one pair of nodes are refused, and so is a link's `port` of 0.
- A node's private key is created only for a node on a link. Version 1 required one of every
  node.
- With `addressing.tunnel.ipv4`, the pattern of the links' interfaces no longer has to stand in
  `direct_interfaces`: it is added at the end of the list. Version 1 refused a file without it.
- Every link has a `type`, and the keys of that type stand in a block named after it. A link that
  version 1 marked `plain` is of type `wireguard`, any other of type `amneziawg`.
- A link of a type that the tool of one of its ends does not serve is refused, which makes a link to
  a `routeros` node one of type `wireguard`. Version 1 did not know a node's platform and rendered
  an obfuscated link to such a node with obfuscation on one end alone.
- The keepalive of a link is a key, `persistent_keepalive`. Version 1 had none: the `awg` daemon
  that reads a peer's keepalive from its config entry sends a keepalive every 25 seconds to a peer
  it dials on an interface without obfuscation and every `20-30` on one with it, and a RouterOS
  device sends every 25; a file of version 1 gets that. Version 2 gives the same where no key is
  written, and the generator writes the key only where a link's value differs from it. The
  [specification](slipmesh-yaml.md#keepalive) says what an older daemon does.
- BFD is on where its block is. Version 1 had `bfd.enable`.
- The numbers of route classes are checked against each other and against `200`. Version 1 checked
  one number, against `0` to `4`.
- Without the class `egress` a `talos` node gets no annotation `slipmesh.net/route_proto_id.egress`.
  Version 1 always set it: its generator refused a file without the number and named
  `slipmesh mint`, which wrote 152.
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
| A `patch` that is a `RoutingRuleConfig`, reaching every Talos node | A rule of a route class. The generator makes the document. |
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
| `cluster.awg_metrics_port`, `cluster.router_metrics_port` | `metrics.mesh.port` and `metrics.router.port` in the [`platform`](slipmesh-yaml.md#platform) document of `talos`. |
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

## What leaves the file

Version 1 also held two parts that describe what runs over the mesh rather than the mesh:
roadwarriors pools, one `roadwarriors` document each, and the lists of destinations that leave the
mesh through a node, under the key `bypass` of the `network` document with
`cluster.bypass_refresh_interval_secs`. Version 2 has neither. Both become resources in the
Kubernetes cluster, read by the operators that serve them: a list becomes an egress list, and a pool
a roadwarriors pool with its clients.

The `slipmesh rw` commands of v0.19 and before edit its pools; from v0.20.0 on there are none.

**Planned:** the operators, and the command each of them has to write its resources out of a
version 1 file, once.

## Converting a file by hand

No command converts a file. It is edited by hand, and the tools then show that nothing changed that
was not meant to. Step 1 runs the tools of the last release that reads version 1, built on sliplib
v0.19, on the file as it is; steps 5 to 7 run those built on v0.20.0 or later, on the converted
file. Every node of a network converted this way gets the configs version 1 gave it, but for the
differences step 6 lists. [`mesh/tests/golden.rs`](../mesh/tests/golden.rs) pins that for two
networks, all but the metrics listeners, which the Talos generator writes: what else step 6 shows
comes from the conversion.

1. Before editing, leave nothing pending: `slipmesh-taloscfg generate --diff` prints no change to a
   patch file, and `slipmesh-routeros --node <name> --check --diff` plans none on its device.
   Whatever shows later then comes from the conversion. The patch files these tools leave are the
   ones the last release that reads version 1 gives: keep them, for a rollback and for step 6 to be
   compared against.
2. Move what the file's comments say to where it will be kept: the first write by `slipmesh` drops
   them.
3. Have the pools and the lists written out as resources of the cluster, as
   [What leaves the file](#what-leaves-the-file) says. **Planned:** until the operators exist, a
   file with pools or `bypass` cannot be converted. A file with neither skips this step.
4. Edit the file. A key a file of version 1 writes as `null` is deleted, not moved: version 1
   read `null` as the key left out, and version 2 lays it as deleting the key. Moved as it is, a
   node's `direct_interfaces: null`, which gave the node the cluster's list, leaves it announcing no
   interfaces, and a field of a link's `obfuscation` written `null`, which took the network's
   value, leaves the link without that field.
   1. In the `slipmesh:` block of the `network` document, write `version: 2` and `revision: 1`.
   2. Write `addressing`: `loopback` is `cluster.loopback_networks`. Where the file has
      `cluster.tunnel_networks`, `tunnel.ipv4` is its `ipv4`, and `tunnel.link_local` its `ipv6`,
      moved as it is: the link-local addresses are derived from it as before. Version 1 took a
      network that step 5 refuses: an `ipv6` of `tunnel_networks` outside `fe80::/10`. Such a
      network has to change, and the addresses derived from it move with it.
   3. Write `routing`: `bgp.as` is `cluster.bgp_as`; `direct_interfaces` is the list of the same
      name; `learn` holds `pod_subnet` and `announce` holds `service_subnet`, each in a list. The
      label `k8s-services` has no place in version 2. `bfd` moves here without `enable`: where
      `enable` was `false` or absent, delete the block. `classes.egress.protocol` is
      `cluster.route_proto_id.egress`, which a file that passes step 1 always has, and its `table`
      the table the `RoutingRuleConfig` patches look up other than `main`, if there is one. An
      entry of a
      `direct_interfaces`, here or a node's, that is a CIDR or holds a character other than ASCII
      letters, digits, `-`, `_` and `.` before a trailing `*`, as `?` is, does not load: rewrite it
      as interface names, or prefixes and `*`.
   4. Write `mesh`: `interface_prefix` is `cluster.interface_prefix.mesh`, `mtu` is
      `cluster.mesh_mtu`, and `amneziawg.obfuscation` is the network's `obfuscation`.
   5. Move `awg_metrics_port` and `router_metrics_port` into a new `platform` document for `talos`,
      as `metrics.mesh.port` and `metrics.router.port`. Delete `cluster`.
   6. Turn `nodes` into a mapping: each node's `name` becomes its key. A name outside [the
      rule](slipmesh-yaml.md#names), or `all` or `none`, has to change, and so do the names of the
      interfaces named after it on the node's peers. Give every node its `platform`. Its
      `mesh_private_key` becomes `wireguard.private_key`, and its `direct_interfaces` moves under
      its `routing`, but for a `null` one, which is deleted. A `node_id` wider than the host part
      of a network it is added to, which version 1 cut there, and `0.0.0.0`, are refused by
      step 5. Give such a node the `node_id` whose bits version 1 kept in its IPv4 loopback, the
      old one cut to the host bits of `loopback.ipv4`, where that value is not `0.0.0.0` and fits
      every network the node's addresses are derived from; otherwise any value that does.
      `198.51.100.133` under a /25 becomes `0.0.0.5`. The node's IPv4 loopback stays only in the
      first case.
   7. Give every node one `underlay` entry, named as you like. Its `mtu` is the node's `uplink_mtu`,
      left out where that was. Its `endpoint` holds the node's `endpoint` without any port and
      without the brackets of an IPv6 address, under the key of its family: the family of the
      address, or for a host name the family it resolves to first. Without `address_family`, version
      1 counted for IPv6 every endpoint that is not a bare address, a host name or an address with a
      port; where it is IPv4, the MTU of the mesh may now come out up to 20 higher, so write a
      `mesh.mtu` of the value it had: the MTU of a working mesh does not move in a conversion.
      Version 1 dialed a port written in the endpoint; a peer now dials the link's `port`. Where the
      two differed, as behind a port forward, forward the link's `port` to the node before the
      conversion. A node without `endpoint` gets an entry without one.
   8. Give every node outside the cluster those two ranges belong to, a `routeros` router for one,
      `learn: []` and `announce: []` under its `routing`.
   9. Turn `mesh.links` into a mapping, giving each link a name. Its `pair` becomes `ends`, each node
      with `{}`. A link with `plain: true` becomes `type: wireguard`, with its `port` in a
      `wireguard` block. Any other becomes `type: amneziawg`, with its `port` and `obfuscation` in an
      `amneziawg` block, the fields of `obfuscation` written `null` deleted. A link that reaches a
      `routeros` node has to be of type `wireguard`.
   10. Delete `bypass`, `bypass_refresh_interval_secs` and every `roadwarriors` document.
   11. Turn each `patch` that carries a template into a `template` document, `daemon: router` or
       `daemon: firewall`, keeping its `include` and `exclude`. The template's text becomes
       `content`. `base` is the hash of the reference that text was made from. Where that is not
       known, the hash printed by `slipmesh-taloscfg template router` or
       `slipmesh-taloscfg template firewall` is the current reference's: it silences the warning
       without anything having been compared.
   12. What else such a `patch` held: its other `configFiles` stay in a `patch` of the same
       document, which sets `configFiles` only, and none at a path the generator writes, the
       template's among them. Version 1 already refused any other key in a `patch` of the `router`
       document. The rest of the `nftables` document the generator now writes itself, so its other
       keys have no place in the files.
   13. Turn into a rule of a route class each `patch` that is a `RoutingRuleConfig` reaching every
       node of `talos`, an `exclude` of nodes of other platforms aside, and holding nothing but
       `name`, `table`, `fwMark`, `fwMask` and a `dst` of `0.0.0.0/0` or `::/0`. The rule goes into
       the class whose `table` it looks up, and one that looks up `main` into the class of the rule
       it lets packets past, `egress` for the reference setup. Its `priority` is the `name`; its
       own name is free. `fwMark` and `fwMask` become its `mark`, in hexadecimal,
       `0x100000/0x100000` for `1048576` and `1048576`, the mask left out where it was absent or
       every bit. `table: main` becomes `lookup: main`, and the class's own table is left out. The
       `dst` of the rules of one class gives the class its `families`: `[ipv4]` for `0.0.0.0/0`,
       `[ipv6]` for `::/0`, the key left out for none. Rules of one class that differ in `dst`
       cannot share it. Delete the `patch`. A `RoutingRuleConfig` with any other key, reaching some
       nodes of `talos` only, or differing in `dst` from the other rules of its class, stays a
       `patch`.
   14. Give every remaining `patch` its `platform`, drop the `exclude` that kept it from the nodes of
       other platforms, take a node of another platform out of its `include`, and delete the
       `mikrotik` document.
   15. Where several documents name the same set of nodes, a group in a `groups` document may name
       it once. Where the keys or the obfuscation should be kept apart, move them into a file of
       their own, laid over the rest:
       `nodes.<name>.wireguard` and `mesh.links.<name>.amneziawg.obfuscation` keep their paths
       there.

   Where the pattern of the links' interfaces stands last in a `direct_interfaces`, it may stay or be
   taken out: the configs are the same either way. Where it stands before other entries, taking it
   out moves it to the end, and the node's routing configuration changes with it.
5. Run `slipmesh check`. It parses every document strictly, but the bodies of `platform`,
   `template` and `patch` documents, which the platform's tool checks, and it checks every node's
   configs.
6. Run the two commands of step 1 again, with the tools built on v0.20.0 or later, and compare the
   patch files, and the plan `slipmesh-routeros` shows, with what step 1 left. Nothing of the mesh
   changes of itself: not the addresses, the keys, the link-local addresses of the tunnels, the MTU,
   OSPF, BFD or the iBGP sessions to the routers. The differences are of two kinds, and of no
   other.

   What version 2 does not carry over, as [What leaves the file](#what-leaves-the-file) says:
   - in the patch file of every `talos` node, the `awg` config holds no pool interfaces,
     `router.yaml` holds no `bypass`, and its `announce` holds no label, so the node's routing
     configuration loses the comment the label made.

   What step 4 changed on purpose:
   - a node given `learn: []` in step 4.8 no longer announces what it held inside the pod range,
     and one given `announce: []` no longer announces the service range;
   - where a node's endpoint had a port or the brackets of an IPv6 address, its annotation
     `slipmesh.net/endpoint` carries the bare address or name; where that port differed from the
     link's `port`, every peer's endpoint for that node, in the `awg` config and on a RouterOS peer;
   - where step 4.2 changed a network, the addresses derived from it;
   - where a node was renamed in step 4.6, the interfaces named after it, and what refers to them;
   - where step 4.6 gave a node another `node_id`, each address derived from it that the new one
     changes, and what other nodes hold of it, the address of its iBGP sessions among them: its
     IPv6 loopback where its network kept more bits of the old `node_id` than the new one has; its
     IPv4 loopback where the new `node_id` is not the one version 1 kept there; its link-local
     where it changes with them, which a `link_local` network that took only the kept low bits
     leaves as it was;
   - where step 4.9 made a link to a `routeros` node, which version 1 could have marked as
     obfuscated, a link of type `wireguard`, the interface of its `talos` end has no obfuscation,
     and the peer's keepalive is 25 where an obfuscated link gave it `20-30`;
   - where step 4.13 turned a `patch` into a rule, its `RoutingRuleConfig` document stands after the
     generated documents of the node's patch file, not among the documents of the `patch`es, with
     the same contents;
   - where step 4.11 turned a `patch` of `router` or `nftables` into a template, the files of that
     document come in the generator's order and not the `patch`'s: the generator's config, then the
     template, then the files the `patch` adds, so a file the `patch` listed before the template
     stands after it now. The `nftables` document of a node's patch file stands after the `router`
     document, not among the documents of the `patch`es, and has none of the keys besides
     `configFiles` that step 4.12 dropped, `environment` for one;
   - where the pattern of the links' interfaces stood before other entries of a `direct_interfaces`
     and step 4 took it out, it stands last, and the node's routing configuration changes with it.

   Any other difference is an error in the conversion, or in this document: do not roll out until
   it is explained. The generator no longer writes a patch file for a node of another platform,
   which never used it: delete the one version 1 left.
7. Run `slipmesh mint`, which writes nothing where nothing is missing, then `slipmesh fmt` on each
   file, and commit that rewrite on its own.
