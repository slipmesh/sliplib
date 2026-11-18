//! `slipmesh.yaml`: what each document in it is, which hosts it reaches, and what one host ends up
//! with.
//!
//! Every document carries a `slipmesh:` block saying what it is for. `network` is the topology,
//! exactly one of it. `roadwarriors` is one pool per document. `patch` is a document in the format
//! of whatever configures a host, aimed at hosts by `include`/`exclude`; what it holds is for that
//! tool to read. The block is for the file's reader and never reaches a host.

use crate::mesh_config::{self, MeshConfig, RoadwarriorPool};
use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use sliplib_yaml::{MultiDoc, Value};
use std::fmt;

/// The top-level block of every document: what the document is for and which hosts it reaches.
pub const META_KEY: &str = "slipmesh";

#[derive(Deserialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
enum Kind {
    Network,
    Roadwarriors,
    /// Read only to refuse it, naming where its ruleset goes.
    Nftables,
    Patch,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Kind::Network => "network",
            Kind::Roadwarriors => "roadwarriors",
            Kind::Nftables => "nftables",
            Kind::Patch => "patch",
        }
    }
}

/// A document's `slipmesh:` block. Strict, so that a misspelled `include` is an error rather than
/// a document that silently reaches every host.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Meta {
    kind: Kind,
    include: Option<Vec<String>>,
    exclude: Option<Vec<String>>,
}

/// One document of `slipmesh.yaml` with its `slipmesh:` block read out of it.
struct Document {
    meta: Meta,
    /// The document without the block, its lines kept so that an error names the line of the file.
    inner: sliplib_yaml::Document,
}

impl std::ops::Deref for Document {
    type Target = sliplib_yaml::Document;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl Document {
    fn read(mut inner: sliplib_yaml::Document) -> Result<Self> {
        let line = inner.line;
        let meta = inner
            .take(META_KEY)
            .context("no `slipmesh:` block saying what the document is for")
            .and_then(|meta| {
                sliplib_yaml::from_value::<Meta>(meta).context("its `slipmesh:` block")
            })
            .with_context(|| format!("the document starting at line {line}"))?;
        Ok(Self { meta, inner })
    }

    /// The document as `T`, with an error that names the field and the line of the file.
    fn deserialize<T: serde::de::DeserializeOwned>(&self) -> Result<T> {
        self.inner.deserialize().with_context(|| self.at())
    }

    fn at(&self) -> String {
        format!(
            "the `kind: {}` document at line {}",
            self.meta.kind.name(),
            self.line
        )
    }

    fn reaches(&self, host: &str) -> bool {
        self.meta
            .include
            .as_ref()
            .is_none_or(|hosts| hosts.iter().any(|h| h == host))
            && !self.meta.exclude.iter().flatten().any(|h| h == host)
    }
}

/// A `patch` document: one in the format of whatever configures the hosts it reaches. It reads as
/// where it stands in the file, for a refusal to name.
pub struct Patch(Document);

/// Which of two `patch` documents that reach one host is laid over the other: the greater over the
/// lesser, and of two equal ones the later in the file.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Rank(bool);

impl Patch {
    /// The document without its `slipmesh:` block.
    pub fn value(&self) -> &Value {
        &self.0.value
    }

    /// A document that names its hosts in `include` ranks above one for every host, wherever the
    /// two stand in the file.
    pub fn rank(&self) -> Rank {
        Rank(self.0.meta.include.is_some())
    }
}

impl fmt::Display for Patch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0.at())
    }
}

pub struct SlipmeshFile {
    network: Value,
    network_index: usize,
    hosts: Vec<String>,
    pools: Vec<Document>,
    patches: Vec<Patch>,
    topology: MeshConfig,
}

impl SlipmeshFile {
    pub fn parse(raw: &str) -> Result<Self> {
        Self::read(&MultiDoc::parse(raw)?)
    }

    /// `slipmesh.yaml` as `yaml` holds it - for a caller that goes on to edit it.
    pub fn read(yaml: &MultiDoc) -> Result<Self> {
        let mut networks = Vec::new();
        let mut pools = Vec::new();
        let mut patches = Vec::new();
        for document in yaml.documents().iter().cloned() {
            let document = Document::read(document)?;
            match document.meta.kind {
                Kind::Network => networks.push(document),
                Kind::Roadwarriors => pools.push(document),
                Kind::Nftables => bail!(
                    "{}: the ruleset is now the file {} of a `kind: patch` document holding \
                     `ExtensionServiceConfig` `nftables` - aimed at hosts the same way",
                    document.at(),
                    crate::nftables::TEMPLATE_PATH
                ),
                Kind::Patch => patches.push(Patch(document)),
            }
        }

        ensure!(
            networks.len() == 1,
            "slipmesh.yaml needs exactly one `kind: network` document, found {}",
            networks.len()
        );
        let network = networks.remove(0);
        for untargeted in std::iter::once(&network).chain(&pools) {
            ensure!(
                untargeted.meta.include.is_none() && untargeted.meta.exclude.is_none(),
                "{}: `include`/`exclude` aim a document at hosts, and this kind is not aimed",
                untargeted.at()
            );
        }
        ensure!(
            network.value.get("roadwarriors").is_none(),
            "{}: `roadwarriors` belongs in its own `kind: roadwarriors` document",
            network.at()
        );
        let topology: MeshConfig = network.deserialize()?;
        let hosts: Vec<String> = topology.nodes.into_iter().map(|n| n.name).collect();

        for targeted in patches.iter().map(|patch| &patch.0) {
            if let Some(include) = &targeted.meta.include {
                ensure!(
                    !include.is_empty(),
                    "{}: `include: []` reaches no host - leave `include` out to reach every host",
                    targeted.at()
                );
            }
            for host in targeted
                .meta
                .include
                .iter()
                .chain(&targeted.meta.exclude)
                .flatten()
            {
                ensure!(
                    hosts.contains(host),
                    "{}: names host {host:?}, which the network document does not have",
                    targeted.at()
                );
            }
        }

        for pool in &pools {
            pool.deserialize::<RoadwarriorPool>()?;
        }
        let topology = model(&network.value, &pools)?;
        Ok(Self {
            network_index: network.index,
            network: network.inner.value,
            hosts,
            pools,
            patches,
            topology,
        })
    }

    /// The position of the `network` document in the file - where an edit to the topology goes.
    pub fn network_document(&self) -> usize {
        self.network_index
    }

    /// The position of the `roadwarriors` document holding pool `name` - where an edit to that
    /// pool goes.
    pub fn pool_document(&self, name: &str) -> Option<usize> {
        self.pools
            .iter()
            .find(|p| p.value.get("name").and_then(Value::as_str) == Some(name))
            .map(|p| p.index)
    }

    /// The hosts `network` names, in its order.
    pub fn hosts(&self) -> Vec<&str> {
        self.hosts.iter().map(String::as_str).collect()
    }

    /// The whole topology: `network` with every pool.
    pub fn topology(&self) -> &MeshConfig {
        &self.topology
    }

    pub fn into_topology(self) -> MeshConfig {
        self.topology
    }

    /// The topology as one host sees it.
    pub fn effective_for(&self, host: &str) -> Result<MeshConfig> {
        self.ensure_host(host)?;
        model(&self.network, &self.pools)
    }

    /// Every `patch` document, in the order of the file.
    pub fn patches(&self) -> &[Patch] {
        &self.patches
    }

    /// The `patch` documents that reach `host`, in the order of the file.
    pub fn patches_for(&self, host: &str) -> Result<Vec<&Patch>> {
        self.ensure_host(host)?;
        Ok(self
            .patches
            .iter()
            .filter(|patch| patch.0.reaches(host))
            .collect())
    }

    fn ensure_host(&self, host: &str) -> Result<()> {
        ensure!(
            self.hosts.iter().any(|h| h == host),
            "host {host:?} is not in the network document"
        );
        Ok(())
    }
}

/// `network` with every pool, as one validated model.
fn model(network: &Value, pools: &[Document]) -> Result<MeshConfig> {
    let mut value = network.clone();
    let mapping = value
        .as_mapping_mut()
        .context("the network document is not a mapping")?;
    let pools = pools.iter().map(|p| p.value.clone()).collect();
    mapping.insert("roadwarriors".into(), Value::Sequence(pools));
    let model: MeshConfig = sliplib_yaml::from_value(value).context("assembling the topology")?;
    mesh_config::validate(&model)?;
    Ok(model)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NETWORK: &str = r#"slipmesh:
  kind: network
cluster:
  bgp_as: 64512
  loopback_networks: {ipv4: "198.51.100.0/25", ipv6: "3fff::/32"}
nodes:
  - {name: node-a, node_id: "198.51.100.1"}
  - {name: node-b, node_id: "198.51.100.2"}
  - {name: node-c, node_id: "198.51.100.3"}
"#;

    const KUBELET: &str = r#"slipmesh:
  kind: patch
apiVersion: v1alpha1
kind: KubeletConfig
extraArgs:
    rotate-server-certificates: "true"
"#;

    fn file(documents: &[&str]) -> String {
        documents.join("---\n")
    }

    fn pool(name: &str, port: u16) -> String {
        format!(
            "slipmesh:\n  kind: roadwarriors\nname: {name}\nnode_hostnames: [node-a]\naddress: \"203.0.113.1/27\"\nlisten_port: {port}\nclients: []\n"
        )
    }

    fn ruleset(meta: &str, table: &str) -> String {
        format!(
            "slipmesh:\n  kind: patch\n{meta}apiVersion: v1alpha1\nkind: ExtensionServiceConfig\nname: nftables\nconfigFiles:\n  - mountPath: /etc/talos-extensions/nftables.conf.j2\n    content: |\n      table inet {table} {{}}\n"
        )
    }

    fn error(raw: &str) -> String {
        match SlipmeshFile::parse(raw) {
            Ok(_) => panic!("parsed, but should have been refused:\n{raw}"),
            Err(err) => format!("{err:#}"),
        }
    }

    #[test]
    fn hosts_come_from_the_network_document_in_its_order() {
        let parsed = SlipmeshFile::parse(NETWORK).unwrap();
        assert_eq!(parsed.hosts(), ["node-a", "node-b", "node-c"]);
    }

    #[test]
    fn a_file_without_a_network_document_is_refused() {
        assert!(error(KUBELET).contains("network"));
    }

    #[test]
    fn a_file_with_two_network_documents_is_refused() {
        assert!(error(&file(&[NETWORK, NETWORK])).contains("network"));
    }

    #[test]
    fn a_document_without_a_slipmesh_block_is_refused_by_its_line() {
        let raw = file(&[NETWORK, "stray: 1\n"]);
        let line = raw[..raw.find("stray").unwrap()].matches('\n').count() + 1;
        let err = error(&raw);
        assert!(err.contains(&format!("line {line}")), "{err}");
        assert!(err.contains("slipmesh"), "{err}");
    }

    #[test]
    fn a_document_of_only_comments_is_not_one() {
        SlipmeshFile::parse(&file(&[NETWORK, "# end of file\n"])).unwrap();
    }

    #[test]
    fn an_unknown_kind_is_refused_with_the_known_ones() {
        let err = error(&file(&[NETWORK, "slipmesh:\n  kind: patches\na: 1\n"]));
        assert!(err.contains("patches"), "{err}");
        assert!(err.contains("roadwarriors"), "{err}");
    }

    #[test]
    fn a_misspelled_meta_key_is_refused() {
        let err = error(&file(&[
            NETWORK,
            "slipmesh:\n  kind: patch\n  tagrets: [node-a]\napiVersion: v1alpha1\nkind: KubeletConfig\n",
        ]));
        assert!(err.contains("tagrets"), "{err}");
    }

    #[test]
    fn an_empty_include_is_refused() {
        let err = error(&file(&[
            NETWORK,
            "slipmesh:\n  kind: patch\n  include: []\napiVersion: v1alpha1\nkind: KubeletConfig\n",
        ]));
        assert!(err.contains("include"), "{err}");
    }

    #[test]
    fn include_on_a_network_document_is_refused() {
        let network = NETWORK.replace(
            "  kind: network\n",
            "  kind: network\n  include: [node-a]\n",
        );
        let err = error(&network);
        assert!(
            err.contains("the `kind: network` document at line 1: `include`"),
            "{err}"
        );
    }

    #[test]
    fn exclude_on_a_roadwarriors_document_is_refused() {
        let pool = pool("plain", 51820).replace(
            "  kind: roadwarriors\n",
            "  kind: roadwarriors\n  exclude: [node-b]\n",
        );
        let err = error(&file(&[NETWORK, &pool]));
        assert!(
            err.contains("the `kind: roadwarriors` document at line 11: `include`/`exclude`"),
            "{err}"
        );
    }

    #[test]
    fn an_unknown_host_in_include_or_exclude_is_refused_by_name() {
        for meta in ["  include: [node-z]\n", "  exclude: [node-z]\n"] {
            let err = error(&file(&[NETWORK, &ruleset(meta, "t")]));
            assert!(err.contains("node-z"), "{err}");
        }
    }

    #[test]
    fn an_unknown_field_is_refused_at_the_line_it_is_written_on() {
        let network = NETWORK.replace(
            "  bgp_as: 64512\n",
            "  bgp_as: 64512\n  metrics_port: 9586\n",
        );
        let raw = file(&[KUBELET, &network]);
        let line = raw[..raw.find("  metrics_port").unwrap()]
            .matches('\n')
            .count()
            + 1;
        let err = error(&raw);
        assert!(err.contains("cluster"), "{err}");
        assert!(err.contains("metrics_port"), "{err}");
        assert!(err.contains(&format!("line {line}")), "{err}");
    }

    #[test]
    fn a_network_document_with_its_meta_block_last_still_parses() {
        let network = format!(
            "{}slipmesh:\n  kind: network\n",
            NETWORK
                .strip_prefix("slipmesh:\n  kind: network\n")
                .unwrap()
        );
        SlipmeshFile::parse(&network).unwrap();
    }

    #[test]
    fn a_patch_without_include_reaches_every_host() {
        let parsed = SlipmeshFile::parse(&file(&[NETWORK, KUBELET])).unwrap();
        for host in ["node-a", "node-b", "node-c"] {
            assert_eq!(parsed.patches_for(host).unwrap().len(), 1, "{host}");
        }
    }

    #[test]
    fn a_patch_with_include_reaches_only_its_hosts() {
        let patch = KUBELET.replace("  kind: patch\n", "  kind: patch\n  include: [node-b]\n");
        let parsed = SlipmeshFile::parse(&file(&[NETWORK, &patch])).unwrap();
        assert!(parsed.patches_for("node-a").unwrap().is_empty());
        assert_eq!(parsed.patches_for("node-b").unwrap().len(), 1);
    }

    #[test]
    fn a_host_in_exclude_does_not_get_the_patch() {
        let patch = KUBELET.replace("  kind: patch\n", "  kind: patch\n  exclude: [node-c]\n");
        let parsed = SlipmeshFile::parse(&file(&[NETWORK, &patch])).unwrap();
        assert_eq!(parsed.patches_for("node-a").unwrap().len(), 1);
        assert!(parsed.patches_for("node-c").unwrap().is_empty());
    }

    #[test]
    fn the_patches_of_a_host_come_in_the_order_of_the_file() {
        // The one that names its host stands first and ranks above the other: sorted by rank,
        // it would come last.
        let install = "slipmesh:\n  kind: patch\n  include: [node-a]\napiVersion: v1alpha1\nkind: UnattendedInstallConfig\n";
        let parsed = SlipmeshFile::parse(&file(&[NETWORK, install, KUBELET])).unwrap();
        let kinds: Vec<_> = parsed
            .patches_for("node-a")
            .unwrap()
            .into_iter()
            .map(|patch| patch.value()["kind"].as_str().unwrap())
            .collect();
        assert_eq!(kinds, ["UnattendedInstallConfig", "KubeletConfig"]);
    }

    #[test]
    fn what_a_patch_holds_is_not_the_loaders_to_refuse() {
        let without_a_kind = "slipmesh:\n  kind: patch\nmachine:\n  install: {disk: /dev/vda}\n";
        let over_a_generated_one = "slipmesh:\n  kind: patch\napiVersion: v1alpha1\nkind: ExtensionServiceConfig\nname: router\nenvironment:\n  - RUST_LOG=debug\nconfigFiles:\n  - mountPath: /etc/talos-extensions/router.yaml\n    content: x\n";
        let parsed =
            SlipmeshFile::parse(&file(&[NETWORK, without_a_kind, over_a_generated_one])).unwrap();
        assert_eq!(parsed.patches().len(), 2);
    }

    #[test]
    fn a_patch_that_names_its_hosts_ranks_above_one_for_every_host() {
        let named = KUBELET.replace("  kind: patch\n", "  kind: patch\n  include: [node-a]\n");
        let excluding = KUBELET.replace("  kind: patch\n", "  kind: patch\n  exclude: [node-c]\n");
        let parsed = SlipmeshFile::parse(&file(&[NETWORK, &named, KUBELET, &excluding])).unwrap();
        let [named, for_all, excluding] = parsed.patches() else {
            panic!("three patch documents were written");
        };
        assert!(named.rank() > for_all.rank());
        assert_eq!(excluding.rank(), for_all.rank());
    }

    #[test]
    fn a_patch_reads_as_its_kind_and_the_line_it_starts_at() {
        let parsed = SlipmeshFile::parse(&file(&[NETWORK, KUBELET])).unwrap();
        assert_eq!(
            parsed.patches()[0].to_string(),
            "the `kind: patch` document at line 11"
        );
    }

    #[test]
    fn pools_are_gathered_in_file_order() {
        let parsed = SlipmeshFile::parse(&file(&[
            NETWORK,
            &pool("second", 51821),
            &pool("first", 51820),
        ]))
        .unwrap();
        let names: Vec<_> = parsed
            .topology()
            .roadwarriors
            .iter()
            .map(|p| p.name.as_str())
            .collect();
        assert_eq!(names, ["second", "first"]);
    }

    #[test]
    fn documents_are_found_by_their_position_in_the_file() {
        // A document of only comments holds a position too.
        let raw = file(&[
            NETWORK,
            &pool("first", 51820),
            "# nothing here yet\n",
            &pool("second", 51821),
        ]);
        let parsed = SlipmeshFile::parse(&raw).unwrap();
        assert_eq!(parsed.network_document(), 0);
        assert_eq!(parsed.pool_document("first"), Some(1));
        assert_eq!(parsed.pool_document("second"), Some(3));
        assert_eq!(parsed.pool_document("third"), None);
    }

    #[test]
    fn two_pools_of_one_name_are_refused() {
        let err = error(&file(&[
            NETWORK,
            &pool("plain", 51820),
            &pool("plain", 51821),
        ]));
        assert!(err.contains("plain"), "{err}");
    }

    #[test]
    fn pools_or_a_ruleset_inside_the_network_document_are_refused_by_their_kind() {
        for (section, kind) in [
            ("roadwarriors: []\n", "kind: roadwarriors"),
            ("nftables: {ruleset: x}\n", "nftables"),
        ] {
            let err = error(&format!("{NETWORK}{section}"));
            assert!(err.contains(kind), "{err}");
        }
    }

    #[test]
    fn a_kind_nftables_document_is_refused_with_where_its_ruleset_goes() {
        let err = error(&file(&[
            NETWORK,
            "slipmesh:\n  kind: nftables\nruleset: |\n  table inet t {}\n",
        ]));
        assert!(
            err.contains("the `kind: nftables` document at line 11"),
            "{err}"
        );
        assert!(err.contains("kind: patch"), "{err}");
        assert!(
            err.contains("/etc/talos-extensions/nftables.conf.j2"),
            "{err}"
        );
    }

    #[test]
    fn the_topology_carries_every_pool() {
        let parsed = SlipmeshFile::parse(&file(&[
            NETWORK,
            &pool("plain", 51820),
            &ruleset("  include: [node-a]\n", "t"),
        ]))
        .unwrap();
        let topology = parsed.topology();
        assert_eq!(topology.roadwarriors.len(), 1);
    }

    #[test]
    fn the_network_document_is_found_by_its_position_in_the_file() {
        let parsed = SlipmeshFile::parse(&file(&[KUBELET, NETWORK])).unwrap();
        assert_eq!(parsed.network_document(), 1);
    }

    #[test]
    fn every_host_sees_the_whole_topology() {
        let parsed = SlipmeshFile::parse(&file(&[NETWORK, &pool("first", 51820)])).unwrap();
        let effective = parsed.effective_for("node-b").unwrap();
        assert_eq!(effective.roadwarriors.len(), 1);
        assert_eq!(&effective, parsed.topology());
        assert_eq!(parsed.into_topology(), effective);
    }

    #[test]
    fn a_host_the_network_document_does_not_have_is_refused() {
        let parsed = SlipmeshFile::parse(NETWORK).unwrap();
        assert!(parsed.effective_for("node-x").is_err());
        assert!(parsed.patches_for("node-x").is_err());
    }
}
