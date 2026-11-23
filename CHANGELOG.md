# Changelog

All notable changes to this project will be documented in this file.

This project adheres to [Keep a Changelog](https://keepachangelog.com/en/1.0.0/)
and follows [Semantic Versioning](https://semver.org/).

## [0.22.0] - 2026-11-23

### Breaking changes 💥

- Read each type from its own text, with sliplib-yaml's own error (#88)
- Give the tools one implementation per shared job (#89)

## [0.21.0] - 2026-11-23

### Breaking changes 💥

- Take any loopback network a node_id fits, and refuse a node_id that does not fit (#77)
- Take an endpoint name as written and drop the MTU refusals that only changed a message (#79)
- Read the secrets from the model, not from a copy of them (#80)
- Leave the Talos settings, templates, metrics and the daemons' own keys to their platform's tools (#82)
- Compute the configs from the loader's network, not from a copy of it (#83)
- Name the mint hint once and drop what sliplib no longer needs (#84)

### Documentation 📚

- Close the follow-ups of the format documents (#75)
- Say where the generated nftables document stands in a patch file (#76)
- Cut the comments of the secrets change (#81)
- Cut sliplib's comments to what the names do not say (#85)

### Miscellaneous 🧹

- Remove the fuzz target of the config reader (#78)

## [0.20.0] - 2026-11-21

### Breaking changes 💥

- Compute a node's configs from one model of the network, from files of either version (#70)
- Read only files of format version 2 (#73)

### Added ✨

- Read the names, the blocks and the groups of format version 2 (#63)
- Read the network and the platform documents of format version 2 (#66)
- Read the files of format version 2 as a whole (#67)
- Check, mint, rekey and fmt a file of format version 2 (#71)

### Build system 🛠️

- Raise the minor version for a breaking change before 1.0 (#61)
- Update rust crate clap to 4.6.9 (main) (#68)
- Update rust crate nix to 0.32.0 (main) (#69)

### CI/CD ⚙️

- Move the shared workflows to v4.0.2 (#62)

### Tests ✅

- Send the test's SIGTERM through nix, not a kill process (#65)
- Pin on format version 2 what only version 1 pinned (#72)

## [0.19.0] - 2026-11-18

### Breaking changes 💥

- Give a peer its keepalive in the awg config (#59)

### Added ✨

- Warn about a link that loses data packets to random trailers (#58)

### Build system 🛠️

- Update non-major (main) (#45)

## [0.18.0] - 2026-11-18

### Breaking changes 💥

- Leave what a patch document holds to its platform's tool (#53)
- Return typed errors from sliplib-config and sliplib-mesh (#54)
- Leave the Talos documents to the Talos generator (#55)

### CI/CD ⚙️

- Open the release PR as the organisation's ci app (#50)

### Documentation 📚

- Describe the slipmesh.yaml format version 2 (#51)

### Tests ✅

- Pin what a version 1 file gives its nodes (#52)

## [0.17.0] - 2026-11-17

### Breaking changes 💥

- Take magic headers and tuning values as ranges (#47)
- Treat header protection as the interface's, not a peer's (#48)

### Miscellaneous 🧹

- Put breaking changes in a changelog section of their own (#46)

## [0.16.0] - 2026-11-15

### Breaking changes 💥

- Create and write the secrets and defaults of slipmesh.yaml (#42)

### Added ✨

- Add the rw commands and fmt (#43)

## [0.15.0] - 2026-11-15

### Added ✨

- Add the slipmesh tool, which checks slipmesh.yaml (#40)

### CI/CD ⚙️

- Call the shared workflows at slipmesh/.github@v3.2.2 (#38)

### Fixed 🐛

- Name the node whose key does not decode (#39)

## [0.14.0] - 2026-11-15

### Added ✨

- Compute a node's awg and router configs (#36)

## [0.13.0] - 2026-11-15

### Added ✨

- Read slipmesh.yaml (#34)

## [0.12.0] - 2026-11-11

### Added ✨

- Merge talos documents and annotate a KubeNodeConfig (#32)

### Tests ✅

- Tidy up fixtures (#31)

## [0.11.0] - 2026-11-10

### Added ✨

- Carry an mtu for each awg interface (#29)

## [0.10.0] - 2026-11-10

### Added ✨

- Add the patch format as sliplib-config (#27)

### Documentation 📚

- Say that sliplib-yaml writes a file back from its values (#26)

## [0.9.0] - 2026-11-10

### Breaking changes 💥

- Read and write through yaml_serde only (#24)

## [0.8.0] - 2026-11-10

### Breaking changes 💥

- Take the serde model from yaml-rt (#21)

### Added ✨

- Read a single-document file as a type (#22)

### Fixed 🐛

- Indent sequences in added values as a hand-written file does (#20)

## [0.7.0] - 2026-11-10

### Added ✨

- Read, patch and write multi-document yaml files (#18)

## [0.6.0] - 2026-11-09

### Added ✨

- Add cidr parsing and exclusion on ipnet's types (#16)

## [0.5.1] - 2026-11-09

### Fixed 🐛

- Log at info when rust_log is unset (#14)

## [0.5.0] - 2026-11-09

### Added ✨

- Put serve behind a default feature (#12)

## [0.4.0] - 2026-11-09

### Added ✨

- Run a daemon's work until sigterm through serve (#10)

## [0.3.0] - 2026-11-08

### Added ✨

- Add the tracing setup every daemon and tool shares (#8)

### CI/CD ⚙️

- Check licenses through slipmesh/.github@v3.1.0 (#7)

## [0.2.0] - 2026-11-08

### Added ✨

- Add a secret type for secrets that are not keys (#5)

## [0.1.0] - 2026-11-08

### Added ✨

- Add wireguard keys with a private key that never prints (#2)

### Miscellaneous 🧹

- Initial commit
