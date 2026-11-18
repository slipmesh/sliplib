# `slipmesh.yaml` from version 1 to version 2

Version 1 is the format every release so far reads; [version 2](slipmesh-yaml.md) is the one its
document describes. This one says what differs between them and how a file is converted.

## Differences from version 1

In general:

- The network is one tree: `addressing`, `routing`, `mesh` and `nodes`. Version 1 spread it over
  `cluster`, `bfd`, `obfuscation`, `nodes` and `mesh`.
- What is named is a mapping by name: nodes, links, groups, a node's underlay entries, route
  classes. Version 1 held nodes and links in lists, and a link had no name.
- The network may be written in several files, laid over each other by one rule. Version 1 read one
  file.
- A document may set its place among the others with `order`, which ranks before everything else.
  In version 1 a document had the place it stood in.
- The format says its `version` and `revision`.
- Every node names its `platform`, and a document written for one platform reaches only that
  platform's nodes.
- `include` and `exclude` take groups, and `all`, as well as nodes. Groups are a kind of document of
  their own, and a group may name other groups and take nodes out. A document left reaching no node
  once `exclude` is taken out is refused.
- Parsing is strict at every depth. Version 1 refused an unknown key in the `slipmesh:` block, at
  the top of the `network` document and in its `cluster` block, and passed over one in a node, a
  link, `mesh`, `bfd`, `obfuscation`, and the loopback and tunnel networks. A misspelled key that
  version 1 ignored there is refused now.
- The AS number is required. Version 1 read a missing one as 0, which it refused only when a node's
  configs were computed.
- A node's endpoint says its family by its key, and takes no port. Version 1 dialed a port written
  there, and counted the MTU of the link as if the endpoint were a host name; without
  `address_family` it counted a host name for IPv6.
- One rule for names, which version 1 did not have: ASCII letters, digits, `-`, `_` and `.`, with a
  letter or a digit first and last. `all` and `none` are reserved.
- New refusals among the nodes, none of which version 1 checked: two nodes with the same loopback
  address; an IPv6 loopback network longer than /96; two nodes with the same address on the links,
  link-local or IPv4; a link-local whose low 64 bits are all zeros.
- The network of the link-local addresses on the links is a key of its own,
  `addressing.tunnel.link_local`, written with or without an IPv4 network beside it. Version 1 took
  it from `tunnel_networks.ipv6`, which came only together with `ipv4`, and did not check that it
  lay inside `fe80::/10`.
- Two links between one pair of nodes are refused.
- With `addressing.tunnel.ipv4`, the pattern of the links' interfaces no longer has to stand in
  `direct_interfaces`: it is added at the end of the list. Version 1 refused a file without it.
- Every link has a `type`, and the keys of that type stand in a block named after it. A link that
  version 1 marked `plain` is of type `wireguard`, any other of type `amneziawg`.
- A link of a type that the tool of one of its ends does not serve is refused, which makes a link to
  a `routeros` node one of type `wireguard`. Version 1 did not know a node's platform and rendered
  an obfuscated link to such a node with obfuscation on one end alone.
- The keepalive of a link is a key, `persistent_keepalive`. Version 1 had none and sent one every
  25 seconds on either type; left out, it is now still 25 on a link of type `wireguard`, and `20-30`
  on one of type `amneziawg`.
- BFD is on where its block is. Version 1 had `bfd.enable`.
- The numbers of route classes are checked against each other and against `200`. Version 1 checked
  one number, against `0` to `4`.
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

### What leaves the file

Version 1 also held two parts that describe what runs over the mesh rather than the mesh:
roadwarriors pools, one `roadwarriors` document each, and the lists of destinations that leave the
mesh through a node, under the key `bypass` of the `network` document with
`cluster.bypass_refresh_interval_secs`. Version 2 has neither. Both become resources in the
Kubernetes cluster, read by the operators that serve them: a list becomes an egress list, and a pool
a roadwarriors pool with its clients.

**Planned:** the operators, and the command each of them has to write its resources out of a
version 1 file, once.

### Converting a file by hand

No command converts a file. It is edited by hand, and the tools then show that nothing changed that
was not meant to.

1. Before editing, leave nothing pending: `slipmesh-taloscfg generate --diff` prints no change to a
   patch file, and `slipmesh-routeros --node <name> --check --diff` plans none on its device.
   Whatever shows later then comes from the conversion.
2. Move what the file's comments say to where it will be kept: the first write by `slipmesh` drops
   them.
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
   5. Move `awg_metrics_port` and `router_metrics_port` into a new `platform` document for `talos`,
      as `metrics.mesh.port` and `metrics.router.port`. Delete `cluster`.
   6. Turn `nodes` into a mapping: each node's `name` becomes its key. A name outside
      [the rule](slipmesh-yaml.md#names), or `all` or `none`, has to change, and so do the names of the interfaces
      named after it on the node's peers. Give every node its `platform`. Its `mesh_private_key`
      becomes `wireguard.private_key`, and its `direct_interfaces` moves under its `routing`.
   7. Give every node one `underlay` entry, named as you like. Its `mtu` is the node's
      `uplink_mtu`, left out where that was. Its `endpoint` holds the node's `endpoint` without any
      port, under the key of its family: the family of the address, or for a host name the family it
      resolves to first. Version 1 counted a host name without `address_family` for IPv6; where it
      resolves to IPv4, the MTU of the mesh may now come out up to 20 higher, so write a `mesh.mtu`
      of the value it had: the MTU of a working mesh does not move in a conversion. A node without
      `endpoint` gets an entry without one.
   8. Give every node outside the cluster those two ranges belong to, a `routeros` router for one,
      `learn: []` and `announce: []` under its `routing`.
   9. Turn `mesh.links` into a mapping, giving each link a name. Its `pair` becomes `ends`, each node
      with `{}`. A link with `plain: true` becomes `type: wireguard`, with its `port` in a
      `wireguard` block. Any other becomes `type: amneziawg`, with its `port` and `obfuscation` in an
      `amneziawg` block. A link that reaches a `routeros` node has to be of type `wireguard`.
   10. Delete `bypass`, `bypass_refresh_interval_secs` and every `roadwarriors` document.
   11. Turn each `patch` that carries a template into a `template` document, `daemon: router` or
       `daemon: firewall`, keeping its `include` and `exclude`. The template's text becomes
       `content`. `base` is the hash of the reference that text was made from. Where that is not
       known, the hash printed by `slipmesh-taloscfg template router` or
       `slipmesh-taloscfg template firewall` is the current reference's: it silences the warning
       without anything having been compared.
   12. What else such a `patch` held: its other `configFiles` stay in a `patch` of the same
       document, which sets `configFiles` only, and none at a path the generator writes, the
       template's among them. Any other key of the `router` document version 1 already passed over;
       of the `nftables` document the generator now writes the rest itself, and the key has no place
       in the files.
   13. Give every remaining `patch` its `platform`, drop the `exclude` that kept it from the nodes of
       other platforms, take a node of another platform out of its `include`, and delete the
       `mikrotik` document.
   14. Where several documents name the same set of nodes, a group in a `groups` document may name
       it once. Where the keys or the obfuscation should be kept apart, move them into a file of
       their own, laid over the rest: `nodes.<name>.wireguard` and
       `mesh.links.<name>.amneziawg.obfuscation` keep their paths there.

   Where the pattern of the links' interfaces stands last in a `direct_interfaces`, it may stay or be
   taken out: the configs are the same either way. Where it stands before other entries, taking it
   out moves it to the end, and the node's routing configuration changes with it.
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
