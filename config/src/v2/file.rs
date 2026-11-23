//! The files read as a whole: the blocks, the network in force, the groups as nodes, and each
//! platform document with its aim.

use super::block::{Aimed, Block, PlatformBlock, Rank};
use super::check::Invalid;
use super::groups::{Groups, GroupsError, Resolved, Selection};
use super::network::{AMNEZIAWG, Network, WIREGUARD};
use super::{Name, Platform};
use serde::de::DeserializeOwned;
use sliplib_yaml::{Document, Mapping, MultiDoc, Value, merge};
use std::fmt;

/// Of the syntax.
pub const VERSION: u64 = 2;

/// Of the rules that turn the files into configs.
pub const REVISION: u64 = 1;

const NETWORK: &str = "network";
const GROUPS: &str = "groups";
const PLATFORM: &str = "platform";
const TEMPLATE: &str = "template";
const PATCH: &str = "patch";

/// The key of every document's block.
pub const META_KEY: &str = "slipmesh";

type Source = Box<dyn std::error::Error + Send + Sync>;

/// A document's kind and the line it starts at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    kind: &'static str,
    line: usize,
}

impl Place {
    pub(crate) fn new(kind: &'static str, line: usize) -> Self {
        Self { kind, line }
    }
}

impl fmt::Display for Place {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "the `kind: {}` document at line {}",
            self.kind, self.line
        )
    }
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BlockError {
    #[error("no `slipmesh:` block saying what the document is for")]
    Missing,
    #[error("its `slipmesh:` block")]
    Malformed(#[source] Source),
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error(transparent)]
    Yaml(Source),
    #[error(
        "{place} has no `version`, or one below {VERSION}, so the files are of format version 1, which this release does not read - convert them to version {VERSION} as `docs/slipmesh-yaml-v1-to-v2.md` of sliplib says under \"Converting a file by hand\""
    )]
    VersionOne { place: Place },
    #[error(
        "{place} has `version: {version}`, and this tool reads format version {VERSION} - a newer format needs a newer tool"
    )]
    NewerVersion { place: Place, version: u64 },
    #[error(
        "{place} is of revision {revision}, and this tool is built for revision {REVISION} - update the tool"
    )]
    NewerRevision { place: Place, revision: u64 },
    #[error(
        "{place} is of revision {revision}, and this tool is built for revision {REVISION} - move the files to revision {REVISION}, together with every other tool"
    )]
    OlderRevision { place: Place, revision: u64 },
    #[error("the document starting at line {line}")]
    Block {
        line: usize,
        #[source]
        source: BlockError,
    },
    #[error("{place}")]
    Body {
        place: Place,
        #[source]
        source: Source,
    },
    #[error("there is no `kind: network` document, and without one there is no network")]
    NoNetwork,
    #[error(
        "{count} `kind: {kind}` documents, the first two at lines {first} and {second}: this release reads one and lays no two over each other - write them as one document"
    )]
    Several {
        kind: &'static str,
        count: usize,
        first: usize,
        second: usize,
    },
    #[error(transparent)]
    Invalid(#[from] Invalid),
    #[error(transparent)]
    Groups(#[from] GroupsError),
    #[error(
        "two `kind: platform` documents for `{platform}`, at lines {first} and {second}, and a platform has at most one - write their keys in one"
    )]
    PlatformTwice {
        platform: Platform,
        first: usize,
        second: usize,
    },
    #[error(
        "{place} is written for `{platform}`, whose tool takes no document of this kind - take it out"
    )]
    NotTaken { place: Place, platform: Platform },
    #[error(
        "{place} is written for `{platform}` and reaches the node {node:?}, a `{other}` node - take the node out with `exclude`, or aim the document at nodes of `{platform}` alone"
    )]
    OtherPlatform {
        place: Place,
        platform: Platform,
        node: Name,
        other: Platform,
    },
}

struct Read {
    block: Block,
    /// Without the block, its lines kept for an error to name.
    body: Document,
}

impl Read {
    fn block(mut body: Document) -> Result<Self, Error> {
        let line = body.line;
        let block = match body.value.get(META_KEY) {
            None => Err(BlockError::Missing),
            Some(_) => Block::read(&body).map_err(|source| BlockError::Malformed(source.into())),
        }
        .map_err(|source| Error::Block { line, source })?;
        body.take(META_KEY);
        Ok(Self { block, body })
    }
}

fn body<T: DeserializeOwned>(document: &Document, kind: &'static str) -> Result<T, Error> {
    document.deserialize().map_err(|source| Error::Body {
        place: Place::new(kind, document.line),
        source: source.into(),
    })
}

/// Before anything else, so a newer format is not refused for a key unknown here: every version,
/// then every revision.
fn generation(yaml: &MultiDoc) -> Result<(), Error> {
    let networks: Vec<(Place, &Value)> = yaml
        .documents()
        .iter()
        .filter_map(|document| Some((document.line, document.value.get(META_KEY)?)))
        .filter(|(_, block)| block.get("kind").and_then(Value::as_str) == Some(NETWORK))
        .map(|(line, block)| (Place::new(NETWORK, line), block))
        .collect();
    let written =
        |block: &Value, key: &str| block.get(key).filter(|value| !value.is_null()).cloned();
    for (place, block) in &networks {
        // A number of another type is left to the block, which refuses it.
        match written(block, "version").map(|version| version.as_u64()) {
            None => {
                return Err(Error::VersionOne {
                    place: place.clone(),
                });
            }
            Some(Some(version)) if version < VERSION => {
                return Err(Error::VersionOne {
                    place: place.clone(),
                });
            }
            Some(Some(version)) if version > VERSION => {
                return Err(Error::NewerVersion {
                    place: place.clone(),
                    version,
                });
            }
            Some(_) => {}
        }
    }
    for (place, block) in &networks {
        match written(block, "revision").and_then(|revision| revision.as_u64()) {
            Some(revision) if revision > REVISION => {
                return Err(Error::NewerRevision {
                    place: place.clone(),
                    revision,
                });
            }
            Some(revision) if revision < REVISION => {
                return Err(Error::OlderRevision {
                    place: place.clone(),
                    revision,
                });
            }
            _ => {}
        }
    }
    Ok(())
}

fn at_most_one(
    mut documents: Vec<Document>,
    kind: &'static str,
) -> Result<Option<Document>, Error> {
    match documents.len() {
        0 | 1 => Ok(documents.pop()),
        count => {
            let mut lines: Vec<usize> = documents.iter().map(|document| document.line).collect();
            lines.sort_unstable();
            Err(Error::Several {
                kind,
                count,
                first: lines[0],
                second: lines[1],
            })
        }
    }
}

/// Those a node may hold its own of.
const NODE_LISTS: [&str; 3] = ["direct_interfaces", "learn", "announce"];

/// `network` holds no `null`. Without a mapping of its own, the network's.
fn lay(network: &Value, own: Option<&Value>) -> Value {
    match own {
        Some(own) if own.is_mapping() => merge(network.clone(), own),
        _ => network.clone(),
    }
}

/// Laid over the empty tree, so no `null` is left, with `routing` laid under each node's and
/// `mesh.<type>` under each link's block. The own values are taken as written: a `null` there takes
/// the network's away, which reading the two apart could not tell from a key left out.
fn in_force(written: &Value) -> Value {
    let mut tree = merge(Value::Null, written);
    let routing = tree.get("routing");
    let lists: Mapping = NODE_LISTS
        .iter()
        .filter_map(|key| Some(((*key).into(), routing?.get(key)?.clone())))
        .collect();
    let lists = Value::Mapping(lists);
    let mesh = tree.get("mesh");
    let of_types =
        [WIREGUARD, AMNEZIAWG].map(|kind| (kind, mesh.and_then(|mesh| mesh.get(kind)).cloned()));

    let written_nodes = written.get("nodes").and_then(Value::as_mapping);
    if let Some(nodes) = tree.get_mut("nodes").and_then(Value::as_mapping_mut) {
        for (name, node) in nodes.iter_mut() {
            let own = written_nodes
                .and_then(|nodes| nodes.get(name))
                .and_then(|node| node.get("routing"));
            if let Some(node) = node.as_mapping_mut() {
                node.insert("routing".into(), lay(&lists, own));
            }
        }
    }
    let written_links = written
        .get("mesh")
        .and_then(|mesh| mesh.get("links"))
        .and_then(Value::as_mapping);
    let links = tree
        .get_mut("mesh")
        .and_then(|mesh| mesh.get_mut("links"))
        .and_then(Value::as_mapping_mut);
    for (name, link) in links.into_iter().flat_map(|links| links.iter_mut()) {
        let Some(link) = link.as_mapping_mut() else {
            continue;
        };
        let kind = link.get("type").and_then(Value::as_str);
        let Some((kind, defaults)) = of_types.iter().find(|(name, _)| Some(*name) == kind) else {
            continue;
        };
        if link.get(*kind).is_some_and(Value::is_mapping) {
            let own = written_links
                .and_then(|links| links.get(name))
                .and_then(|link| link.get(*kind));
            let defaults = defaults.clone().unwrap_or(Value::Null);
            link.insert((*kind).into(), lay(&defaults, own));
        }
    }
    tree
}

/// The nodes a document reaches, and its rank. It displays as its place, for a refusal.
#[derive(Debug, Clone, PartialEq)]
pub struct Aim {
    place: Place,
    rank: Rank,
    reach: Vec<Name>,
}

impl Aim {
    pub fn rank(&self) -> Rank {
        self.rank
    }

    pub fn reaches(&self, node: &str) -> bool {
        self.reach.iter().any(|reached| reached.as_str() == node)
    }
}

impl fmt::Display for Aim {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.place.fmt(f)
    }
}

impl Aimed {
    fn aim(self, place: Place, network: &Network, groups: &Resolved) -> Result<Aim, Error> {
        let platform = self.platform;
        if platform == Platform::Routeros {
            return Err(Error::NotTaken { place, platform });
        }
        let of_the_platform: Vec<Name> = network
            .nodes
            .iter()
            .filter(|(_, node)| node.platform == platform)
            .map(|(name, _)| name.clone())
            .collect();
        let rank = Rank::of(self.order, &self.include);
        let selection = Selection {
            include: self.include,
            exclude: self.exclude,
        };
        let reach = groups.reach(&selection, &of_the_platform, &place.to_string())?;
        let of_another = reach
            .iter()
            .map(|node| (node, network.nodes[node].platform))
            .find(|(_, other)| *other != platform);
        if let Some((node, other)) = of_another {
            return Err(Error::OtherPlatform {
                place,
                platform,
                node: node.clone(),
                other,
            });
        }
        Ok(Aim { place, rank, reach })
    }
}

/// A `template` or `patch`, whose body is its platform's tool's.
pub struct AimedDocument {
    aim: Aim,
    body: Document,
}

// The aim alone: the body may hold secrets in a format this crate does not read.
impl fmt::Debug for AimedDocument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AimedDocument")
            .field("aim", &self.aim)
            .finish_non_exhaustive()
    }
}

impl AimedDocument {
    pub fn aim(&self) -> &Aim {
        &self.aim
    }

    /// Without the `slipmesh:` block.
    pub fn value(&self) -> &Value {
        &self.body.value
    }

    /// A refusal names the document and the line.
    pub fn read<T: DeserializeOwned>(&self) -> Result<T, Error> {
        body(&self.body, self.aim.place.kind)
    }
}

/// Whose body is its platform's tool's.
pub struct PlatformDocument {
    place: Place,
    body: Document,
}

// The place alone, as for `AimedDocument`.
impl fmt::Debug for PlatformDocument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PlatformDocument")
            .field("place", &self.place)
            .finish_non_exhaustive()
    }
}

impl PlatformDocument {
    pub fn place(&self) -> &Place {
        &self.place
    }

    /// A refusal names the document and the line.
    pub fn read<T: DeserializeOwned>(&self) -> Result<T, Error> {
        body(&self.body, PLATFORM)
    }
}

/// The last by rank last.
fn reaching<'a>(documents: &'a [AimedDocument], node: &str) -> Vec<&'a AimedDocument> {
    let mut reaching: Vec<&AimedDocument> = documents
        .iter()
        .filter(|document| document.aim.reaches(node))
        .collect();
    // Stable: of two documents of one rank, the later read stays the later.
    reaching.sort_by_key(|document| document.aim.rank());
    reaching
}

pub struct File {
    network: Network,
    network_document: usize,
    platforms: Vec<(Platform, PlatformDocument)>,
    templates: Vec<AimedDocument>,
    patches: Vec<AimedDocument>,
}

impl fmt::Debug for File {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("File")
            .field("network", &self.network)
            .field("platforms", &self.platforms)
            .field("templates", &self.templates)
            .field("patches", &self.patches)
            .finish()
    }
}

impl File {
    pub fn parse(raw: &str) -> Result<Self, Error> {
        Self::read(&MultiDoc::parse(raw).map_err(|source| Error::Yaml(source.into()))?)
    }

    pub fn read(yaml: &MultiDoc) -> Result<Self, Error> {
        generation(yaml)?;
        let mut documents = yaml
            .documents()
            .iter()
            .cloned()
            .map(Read::block)
            .collect::<Result<Vec<Read>, Error>>()?;
        // Stable: documents of one `order` keep the order they were read in.
        documents.sort_by_key(|document| document.block.order());

        let mut networks = Vec::new();
        let mut groups = Vec::new();
        let mut platforms: Vec<(Platform, PlatformDocument)> = Vec::new();
        let mut aimed_documents = Vec::new();
        for Read { block, body } in documents {
            match block {
                Block::Network(_) => networks.push(body),
                Block::Groups(_) => groups.push(body),
                Block::Platform(PlatformBlock { platform, .. }) => {
                    if let Some((_, first)) = platforms.iter().find(|(of, _)| *of == platform) {
                        let first = first.body.line;
                        return Err(Error::PlatformTwice {
                            platform,
                            first: first.min(body.line),
                            second: first.max(body.line),
                        });
                    }
                    let place = Place::new(PLATFORM, body.line);
                    platforms.push((platform, PlatformDocument { place, body }));
                }
                Block::Template(aimed) => aimed_documents.push((TEMPLATE, aimed, body)),
                Block::Patch(aimed) => aimed_documents.push((PATCH, aimed, body)),
            }
        }

        let written = at_most_one(networks, NETWORK)?.ok_or(Error::NoNetwork)?;
        let network_document = written.index;
        // Read as written first: from the text, an error names the line.
        body::<Network>(&written, NETWORK)?;
        let network: Network =
            sliplib_yaml::from_value(in_force(&written.value)).map_err(|source| Error::Body {
                place: Place::new(NETWORK, written.line),
                source: source.into(),
            })?;
        network.check()?;

        let nodes: Vec<Name> = network.nodes.keys().cloned().collect();
        let groups = match at_most_one(groups, GROUPS)? {
            Some(document) => body::<Groups>(&document, GROUPS)?,
            None => Groups::default(),
        }
        .resolve(&nodes)?;

        let mut templates = Vec::new();
        let mut patches = Vec::new();
        for (kind, aimed, body) in aimed_documents {
            let aim = aimed.aim(Place::new(kind, body.line), &network, &groups)?;
            let document = AimedDocument { aim, body };
            if kind == TEMPLATE {
                templates.push(document);
            } else {
                patches.push(document);
            }
        }
        Ok(Self {
            network,
            network_document,
            platforms,
            templates,
            patches,
        })
    }

    /// What is in force under each node's `routing` and in each link's block.
    pub fn network(&self) -> &Network {
        &self.network
    }

    /// Its position in the file, for a write.
    pub fn network_document(&self) -> usize {
        self.network_document
    }

    pub fn platform(&self, platform: Platform) -> Option<&PlatformDocument> {
        self.platforms
            .iter()
            .find(|(of, _)| *of == platform)
            .map(|(_, document)| document)
    }

    /// In the order of the documents.
    pub fn templates(&self) -> &[AimedDocument] {
        &self.templates
    }

    /// The last by rank last: of one daemon's, the node gets the last.
    pub fn templates_for(&self, node: &str) -> Vec<&AimedDocument> {
        reaching(&self.templates, node)
    }

    /// In the order of the documents.
    pub fn patches(&self) -> &[AimedDocument] {
        &self.patches
    }
}

#[cfg(test)]
mod tests {
    use super::super::network::LinkKind;
    use super::super::testing::example;
    use super::*;
    use crate::Range;

    const BLOCK: &str = "{kind: network, version: 2, revision: 1}";

    fn yaml(text: &str) -> Value {
        sliplib_yaml::from_str(text).unwrap()
    }

    /// The example's network as a document, after `edit`.
    fn network_edited(block: &str, edit: impl FnOnce(&mut Value)) -> String {
        let mut tree = example();
        edit(&mut tree);
        let mut document = Mapping::new();
        document.insert(META_KEY.into(), yaml(block));
        document.extend(tree.as_mapping().unwrap().clone());
        sliplib_yaml::to_string(&Value::Mapping(document)).unwrap()
    }

    /// The example's network as a document, with `overlay` laid over it.
    fn network(overlay: &str) -> String {
        network_edited(BLOCK, |tree| *tree = merge(tree.clone(), &yaml(overlay)))
    }

    fn file(documents: &[&str]) -> Result<File, Error> {
        File::parse(&documents.join("\n---\n"))
    }

    fn refusal(documents: &[&str]) -> Error {
        file(documents).expect_err("the files read")
    }

    /// The line the document `index` of `documents` starts at once they are joined into one text.
    fn line(documents: &[&str], index: usize) -> usize {
        let before: String = documents[..index]
            .iter()
            .map(|document| format!("{document}\n---\n"))
            .collect();
        before.matches('\n').count() + 1
    }

    fn place(kind: &'static str, documents: &[&str], index: usize) -> Place {
        Place::new(kind, line(documents, index))
    }

    fn name(text: &str) -> Name {
        Name::try_from(text.to_owned()).unwrap()
    }

    /// A `template` for the router, with `block` as the keys of its block besides the kind.
    fn template(block: &str) -> String {
        format!(
            "slipmesh: {{kind: template, {block}}}\ndaemon: router\nbase: sha256:{}\ncontent: text\n",
            "0".repeat(64)
        )
    }

    /// A `patch` named `name`, with `block` as the keys of its block besides the kind.
    fn patch(block: &str, name: &str) -> String {
        format!("slipmesh: {{kind: patch, {block}}}\nkind: Example\nname: {name}\n")
    }

    /// The nodes `aim` reaches, in the order of the network.
    fn reached<'a>(file: &'a File, aim: &Aim) -> Vec<&'a str> {
        file.network()
            .nodes
            .keys()
            .map(Name::as_str)
            .filter(|node| aim.reaches(node))
            .collect()
    }

    fn names(names: &[Name]) -> Vec<&str> {
        names.iter().map(Name::as_str).collect()
    }

    fn body_names(documents: &[&AimedDocument]) -> Vec<String> {
        documents
            .iter()
            .map(|document| document.value()["name"].as_str().unwrap().to_owned())
            .collect()
    }

    #[test]
    fn the_example_of_the_specification_reads_with_its_nodes_and_links_in_order() {
        let file = file(&[&network("{}")]).unwrap();
        let network = file.network();
        let nodes: Vec<&Name> = network.nodes.keys().collect();
        assert_eq!(
            names(&nodes.into_iter().cloned().collect::<Vec<_>>()),
            ["node-a", "node-b", "node-c", "router1"]
        );
        let links: Vec<Name> = network.mesh.links.keys().cloned().collect();
        assert_eq!(names(&links), ["a-b", "a-c", "a-r1"]);
    }

    #[test]
    fn a_node_without_lists_of_its_own_has_the_networks() {
        let file = file(&[&network("{}")]).unwrap();
        let node = &file.network().nodes["node-a"].routing;
        assert_eq!(node.direct_interfaces, ["lan0"]);
        assert_eq!(node.learn, file.network().routing.learn);
        assert_eq!(node.announce, file.network().routing.announce);
        assert!(!node.learn.is_empty() && !node.announce.is_empty());
    }

    #[test]
    fn a_nodes_own_list_replaces_the_networks_and_leaves_the_others() {
        let file = file(&[&network(
            "{nodes: {node-b: {routing: {direct_interfaces: [lan1, lan2]}}}}",
        )])
        .unwrap();
        let node = &file.network().nodes["node-b"].routing;
        assert_eq!(node.direct_interfaces, ["lan1", "lan2"]);
        assert_eq!(node.learn, file.network().routing.learn);
        // The example's own: a node that takes neither range.
        let router = &file.network().nodes["router1"].routing;
        assert_eq!(router.direct_interfaces, ["bridge-lan"]);
        assert!(router.learn.is_empty() && router.announce.is_empty());
    }

    #[test]
    fn a_null_among_a_nodes_lists_takes_the_networks_away_for_that_node() {
        let text = network_edited(BLOCK, |tree| {
            tree["nodes"]["node-b"]["routing"] = yaml("{learn: null}");
        });
        let file = file(&[&text]).unwrap();
        let node = &file.network().nodes["node-b"].routing;
        assert!(node.learn.is_empty());
        assert_eq!(node.announce, file.network().routing.announce);
        assert!(!file.network().nodes["node-a"].routing.learn.is_empty());
    }

    #[test]
    fn a_node_whose_routing_is_written_with_nothing_after_it_has_the_networks() {
        let text = network_edited(BLOCK, |tree| {
            tree["nodes"]["node-b"]["routing"] = Value::Null;
        });
        let file = file(&[&text]).unwrap();
        let node = &file.network().nodes["node-b"].routing;
        assert_eq!(node.direct_interfaces, ["lan0"]);
    }

    #[test]
    fn two_nodes_of_one_name_are_refused() {
        let twice = format!(
            "{}  node-a: {{platform: talos, node_id: 0.0.0.9, underlay: {{wan: {{}}}}}}\n",
            network("{}")
        );
        assert!(
            twice.trim_end().ends_with("underlay: {wan: {}}}"),
            "{twice}"
        );
        let refusal = refusal(&[&twice]);
        assert!(matches!(refusal, Error::Yaml(_)), "{refusal:?}");
    }

    #[test]
    fn a_null_in_the_networks_own_routing_is_the_key_left_out() {
        let text = network_edited(BLOCK, |tree| {
            tree["routing"]["learn"] = Value::Null;
        });
        let file = file(&[&text]).unwrap();
        assert!(file.network().routing.learn.is_empty());
        assert!(file.network().nodes["node-a"].routing.learn.is_empty());
    }

    #[test]
    fn an_end_written_null_is_the_end_left_out() {
        let three = network_edited(BLOCK, |tree| {
            tree["mesh"]["links"]["a-b"]["ends"]["node-c"] = Value::Null;
        });
        let file = file(&[&three]).unwrap();
        assert_eq!(
            names(&file.network().mesh.links["a-b"].ends),
            ["node-a", "node-b"]
        );
        let none = network_edited(BLOCK, |tree| {
            let ends = &mut tree["mesh"]["links"]["a-b"]["ends"];
            *ends = yaml("{node-a: null, node-b: null}");
        });
        let refusal = refusal(&[&none]);
        assert!(
            matches!(&refusal, Error::Body { place, .. } if *place == Place::new(NETWORK, 1)),
            "{refusal:?}"
        );
    }

    #[test]
    fn an_underlay_entry_written_null_is_the_entry_left_out() {
        let text = network_edited(BLOCK, |tree| {
            tree["nodes"]["node-b"]["underlay"]["wan"] = Value::Null;
        });
        let refusal = refusal(&[&text]);
        let none = Invalid::Underlay {
            node: name("node-b"),
            entries: 0,
        };
        assert!(
            matches!(&refusal, Error::Invalid(invalid) if *invalid == none),
            "{refusal:?}"
        );
    }

    #[test]
    fn a_node_a_link_or_a_class_written_null_is_the_one_left_out() {
        let text = network_edited(BLOCK, |tree| {
            tree["nodes"]["node-x"] = Value::Null;
            tree["mesh"]["links"]["a-c"] = Value::Null;
            tree["routing"]["classes"]["egress"] = Value::Null;
        });
        let file = file(&[&text]).unwrap();
        let network = file.network();
        assert_eq!(
            names(&network.nodes.keys().cloned().collect::<Vec<_>>()),
            ["node-a", "node-b", "node-c", "router1"]
        );
        assert_eq!(
            names(&network.mesh.links.keys().cloned().collect::<Vec<_>>()),
            ["a-b", "a-r1"]
        );
        assert!(network.routing.classes.is_empty());
    }

    #[test]
    fn a_group_written_null_is_the_group_left_out() {
        let groups = "slipmesh: {kind: groups}\ncore: [node-a]\nedge: null\n";
        let documents = [
            network("{}"),
            groups.to_owned(),
            patch("platform: talos, include: [edge]", "one"),
        ];
        let documents: Vec<&str> = documents.iter().map(String::as_str).collect();
        let unknown = GroupsError::UnknownName {
            whose: place(PATCH, &documents, 2).to_string(),
            name: name("edge"),
        };
        let refusal = refusal(&documents);
        assert!(
            matches!(&refusal, Error::Groups(groups) if *groups == unknown),
            "{refusal:?}"
        );
    }

    #[test]
    fn an_exclude_written_null_in_a_group_is_the_exclude_left_out() {
        let groups = "slipmesh: {kind: groups}\ncore: {include: [node-a, node-b], exclude: null}\n";
        let file = file(&[
            &network("{}"),
            groups,
            &patch("platform: talos, include: [core]", "one"),
        ])
        .unwrap();
        assert_eq!(
            reached(&file, file.patches()[0].aim()),
            ["node-a", "node-b"]
        );
    }

    fn amneziawg<'a>(file: &'a File, link: &str) -> &'a super::super::network::AmneziawgLink {
        match &file.network().mesh.links[link].kind {
            LinkKind::Amneziawg(block) => block,
            LinkKind::Wireguard(_) => panic!("{link} is of type wireguard"),
        }
    }

    fn wireguard<'a>(file: &'a File, link: &str) -> &'a super::super::network::WireguardLink {
        match &file.network().mesh.links[link].kind {
            LinkKind::Wireguard(block) => block,
            LinkKind::Amneziawg(_) => panic!("{link} is of type amneziawg"),
        }
    }

    #[test]
    fn a_link_has_the_keepalive_of_its_type_unless_it_writes_its_own() {
        let file = file(&[&network(
            "{mesh: {wireguard: {persistent_keepalive: 10}, amneziawg: {persistent_keepalive: 21-29}}}",
        )])
        .unwrap();
        assert_eq!(wireguard(&file, "a-r1").persistent_keepalive, Some(10));
        assert_eq!(
            amneziawg(&file, "a-b").persistent_keepalive,
            Some(Range::new(21, 29).unwrap())
        );
        // The example's own.
        assert_eq!(
            amneziawg(&file, "a-c").persistent_keepalive,
            Some(Range::new(15, 20).unwrap())
        );
    }

    #[test]
    fn a_link_without_a_keepalive_anywhere_has_none() {
        let file = file(&[&network("{}")]).unwrap();
        assert_eq!(wireguard(&file, "a-r1").persistent_keepalive, None);
        assert_eq!(amneziawg(&file, "a-b").persistent_keepalive, None);
    }

    #[test]
    fn a_null_at_a_links_keepalive_takes_the_networks_away_for_that_link() {
        let text = network_edited(BLOCK, |tree| {
            tree["mesh"]["amneziawg"]["persistent_keepalive"] = yaml("21-29");
            tree["mesh"]["links"]["a-b"]["amneziawg"]["persistent_keepalive"] = Value::Null;
        });
        let file = file(&[&text]).unwrap();
        assert_eq!(amneziawg(&file, "a-b").persistent_keepalive, None);
    }

    #[test]
    fn a_links_obfuscation_is_laid_over_the_networks_field_by_field() {
        let file = file(&[&network(
            "{mesh: {links: {a-b: {amneziawg: {obfuscation: {s4: 20, jc: 4}}}}}}",
        )])
        .unwrap();
        let obfuscation = &amneziawg(&file, "a-b").obfuscation.as_ref().unwrap().0;
        // Its own, over the network's 16; its own, which the network does not set; the network's.
        assert_eq!(obfuscation.s4, Some(20));
        assert_eq!(obfuscation.jc, Some(4));
        assert_eq!(obfuscation.s3, Some(40));
        assert_eq!(obfuscation.random_trailers, Some(true));
    }

    #[test]
    fn a_link_that_writes_no_obfuscation_has_the_networks() {
        let file = file(&[&network("{}")]).unwrap();
        let obfuscation = &amneziawg(&file, "a-b").obfuscation.as_ref().unwrap().0;
        assert_eq!(obfuscation.s4, Some(16));
    }

    #[test]
    fn a_null_at_a_field_of_a_links_obfuscation_drops_that_field_for_the_link() {
        let text = network_edited(BLOCK, |tree| {
            tree["mesh"]["links"]["a-b"]["amneziawg"]["obfuscation"] = yaml("{s3: null}");
        });
        let file = file(&[&text]).unwrap();
        let dropped = &amneziawg(&file, "a-b").obfuscation.as_ref().unwrap().0;
        assert_eq!(dropped.s3, None);
        assert_eq!(dropped.s4, Some(16));
        let kept = &amneziawg(&file, "a-c").obfuscation.as_ref().unwrap().0;
        assert_eq!(kept.s3, Some(40));
    }

    #[test]
    fn a_null_in_the_defaults_of_a_type_is_the_key_left_out() {
        let text = network_edited(BLOCK, |tree| {
            tree["mesh"]["amneziawg"]["obfuscation"]["s3"] = Value::Null;
        });
        let file = file(&[&text]).unwrap();
        assert_eq!(
            amneziawg(&file, "a-b").obfuscation.as_ref().unwrap().0.s3,
            None
        );
    }

    #[test]
    fn a_link_takes_nothing_from_the_defaults_of_another_type() {
        let file = file(&[&network(
            "{mesh: {amneziawg: {persistent_keepalive: 21-29}}}",
        )])
        .unwrap();
        assert_eq!(wireguard(&file, "a-r1").persistent_keepalive, None);
    }

    #[test]
    fn the_network_document_is_known_by_its_position_in_the_file() {
        let settings = "slipmesh: {kind: platform, platform: talos}\nmetrics: {}\n";
        // An empty document counts: the position is that of the file, which an edit goes by.
        let file = file(&[settings, "", &network("{}")]).unwrap();
        assert_eq!(file.network_document(), 2);
    }

    #[test]
    fn a_document_without_its_block_is_refused_by_its_line() {
        let documents = [network("{}"), "kind: Example\n".to_owned()];
        let documents: Vec<&str> = documents.iter().map(String::as_str).collect();
        let line = line(&documents, 1);
        let refusal = refusal(&documents);
        assert!(
            matches!(refusal, Error::Block { line: at, source: BlockError::Missing } if at == line),
            "{refusal:?}"
        );
    }

    #[test]
    fn files_print_with_no_private_key_and_no_body_of_a_platform() {
        let key = sliplib_secrets::wireguard::PrivateKey::generate();
        let text = network_edited(BLOCK, |tree| {
            tree["nodes"]["node-a"]["wireguard"] =
                yaml(&format!("{{private_key: {}}}", key.expose()));
        });
        let tokens = [(); 3].map(|()| sliplib_secrets::wireguard::PrivateKey::generate());
        let file = file(&[
            &text,
            &format!(
                "slipmesh: {{kind: platform, platform: talos}}\ntoken: {}\n",
                tokens[0].expose()
            ),
            &format!(
                "{}token: {}\n",
                template("platform: talos"),
                tokens[1].expose()
            ),
            &format!(
                "{}token: {}\n",
                patch("platform: talos", "one"),
                tokens[2].expose()
            ),
        ])
        .unwrap();
        let printed = format!("{file:?}");
        for secret in std::iter::once(&key).chain(&tokens) {
            assert!(!printed.contains(secret.expose()), "{printed}");
        }
        for kind in ["\"platform\"", "\"template\"", "\"patch\""] {
            assert!(printed.contains(kind), "{kind}: {printed}");
        }
    }

    #[test]
    fn a_block_that_does_not_read_is_refused_by_its_line() {
        for block in [
            "slipmesh: {kind: roadwarriors}\nname: pool\n",
            "slipmesh: {kind: groups, platform: talos}\ncore: [node-a]\n",
        ] {
            let documents = [network("{}").as_str(), block].map(str::to_owned);
            let documents: Vec<&str> = documents.iter().map(String::as_str).collect();
            let line = line(&documents, 1);
            let refusal = refusal(&documents);
            assert!(
                matches!(refusal, Error::Block { line: at, source: BlockError::Malformed(_) } if at == line),
                "{block}: {refusal:?}"
            );
        }
    }

    #[test]
    fn a_document_reads_beside_top_level_keys_that_are_not_strings() {
        let network = network("{}");
        for other in ["5: x\n", "? [a, b]\n: c\n"] {
            let patch = format!("{META_KEY}: {{kind: patch, platform: talos}}\n{other}");
            file(&[&network, &patch]).unwrap();
        }
    }

    #[test]
    fn a_value_of_a_block_that_does_not_fit_is_refused_by_its_key_and_line() {
        let wrong_version = network_edited("{kind: network, version: abc, revision: 1}", |_| ());
        let network = network("{}");
        for (documents, key, written) in [
            ([wrong_version.as_str(), ""], "version", "version: abc"),
            (
                [
                    network.as_str(),
                    "slipmesh:\n  kind: platform\n  platform: nope\n",
                ],
                "platform",
                "platform: nope",
            ),
            (
                [
                    network.as_str(),
                    "slipmesh:\n  kind: template\n  platform: talos\n  include: 5\n",
                ],
                "include",
                "include: 5",
            ),
        ] {
            let line = file_line(&documents, written);
            let refusal = refusal(&documents);
            let Error::Block {
                source: BlockError::Malformed(source),
                ..
            } = &refusal
            else {
                panic!("{refusal:?}");
            };
            let source = source.to_string();
            assert!(
                source.starts_with(&format!("{META_KEY}.{key}: "))
                    && source.contains(&format!(" at line {line} ")),
                "{source}"
            );
        }
    }

    /// The line of `documents` joined that holds `written`.
    fn file_line(documents: &[&str], written: &str) -> usize {
        let text = documents.join("\n---\n");
        text.lines()
            .position(|line| line.trim_start() == written)
            .unwrap()
            + 1
    }

    #[test]
    fn the_talos_documents_of_the_example_reach_the_nodes_they_say() {
        let file = file(&[
            include_str!("../../../docs/example/mesh.yaml"),
            include_str!("../../../docs/example/talos.yaml"),
        ])
        .unwrap();
        let daemons: Vec<&str> = file
            .templates()
            .iter()
            .map(|template| template.value()["daemon"].as_str().unwrap())
            .collect();
        assert_eq!(daemons, ["router", "firewall"]);
        for template in file.templates() {
            assert_eq!(
                reached(&file, template.aim()),
                ["node-a", "node-b", "node-c"]
            );
        }
        let reach: Vec<Vec<&str>> = file
            .patches()
            .iter()
            .map(|patch| reached(&file, patch.aim()))
            .collect();
        assert_eq!(reach, [vec!["node-a", "node-b", "node-c"], vec!["node-b"]]);
        let levels = |node: &str| -> Vec<String> {
            file.patches()
                .iter()
                .filter(|patch| patch.aim().reaches(node))
                .map(|patch| patch.value()["environment"][0].as_str().unwrap().to_owned())
                .collect()
        };
        assert_eq!(
            levels("node-b"),
            ["EXAMPLE_LEVEL=info", "EXAMPLE_LEVEL=debug"]
        );
        assert_eq!(levels("node-c"), ["EXAMPLE_LEVEL=info"]);
        assert!(levels("router1").is_empty());
    }

    #[test]
    fn another_version_is_refused_before_a_key_this_one_does_not_know() {
        let newer = network_edited(
            "{kind: network, version: 3, revision: 1, layers: 2}",
            |_| {},
        );
        let refusal = refusal(&[&newer]);
        assert!(
            matches!(&refusal, Error::NewerVersion { place, version: 3 } if *place == Place::new(NETWORK, 1)),
            "{refusal:?}"
        );
    }

    #[test]
    fn a_network_document_without_a_version_or_below_2_is_of_version_1() {
        let v1 = "# version 1\n\nslipmesh: {kind: network}\ncluster:\n  bgp_as: 64500\n  \
            loopback_networks: {ipv4: 198.51.100.0/25, ipv6: '3fff:0:0:1::/64'}\nnodes:\n  - \
            {name: node-a, node_id: 0.0.0.1}\n";
        let of_v1 = File::parse(v1).expect_err("version 1 reads");
        assert!(
            matches!(&of_v1, Error::VersionOne { place } if *place == Place::new(NETWORK, 3)),
            "{of_v1:?}"
        );
        for block in [
            "{kind: network, revision: 1}",
            "{kind: network, version: null, revision: 1}",
            "{kind: network, version: 0, revision: 1}",
            "{kind: network, version: 1, revision: 1}",
        ] {
            let refusal = refusal(&[&network_edited(block, |_| {})]);
            assert!(
                matches!(&refusal, Error::VersionOne { place } if *place == Place::new(NETWORK, 1)),
                "{block}: {refusal:?}"
            );
        }
    }

    #[test]
    fn the_version_of_every_network_document_is_checked_before_any_revision() {
        let documents = [
            network_edited("{kind: network, version: 2, revision: 2}", |_| {}),
            network_edited("{kind: network, version: 3, revision: 1}", |_| {}),
        ];
        let documents: Vec<&str> = documents.iter().map(String::as_str).collect();
        let at = place(NETWORK, &documents, 1);
        let refusal = refusal(&documents);
        assert!(
            matches!(&refusal, Error::NewerVersion { place, version: 3 } if *place == at),
            "{refusal:?}"
        );
    }

    #[test]
    fn another_revision_is_refused_naming_which_side_to_update() {
        let at = Place::new(NETWORK, 1);
        let newer = network_edited("{kind: network, version: 2, revision: 2}", |_| {});
        let of_newer = refusal(&[&newer]);
        assert!(
            matches!(&of_newer, Error::NewerRevision { place, revision: 2 } if *place == at),
            "{of_newer:?}"
        );
        let older = network_edited("{kind: network, version: 2, revision: 0}", |_| {});
        let of_older = refusal(&[&older]);
        assert!(
            matches!(&of_older, Error::OlderRevision { place, revision: 0 } if *place == at),
            "{of_older:?}"
        );
    }

    #[test]
    fn a_network_document_needs_its_revision() {
        assert!(file(&[&network_edited(BLOCK, |_| {})]).is_ok());
        let refusal = refusal(&[&network_edited("{kind: network, version: 2}", |_| {})]);
        assert!(
            matches!(
                refusal,
                Error::Block {
                    line: 1,
                    source: BlockError::Malformed(_)
                }
            ),
            "{refusal:?}"
        );
    }

    #[test]
    fn files_without_a_network_are_refused() {
        let refusal = refusal(&["slipmesh: {kind: groups}\n"]);
        assert!(matches!(refusal, Error::NoNetwork), "{refusal:?}");
    }

    #[test]
    fn a_second_network_or_groups_document_is_refused_by_their_lines_in_the_files() {
        // The second is set first by its order; the lines come as they stand in the files.
        let networks = [
            network("{}"),
            network_edited(
                "{kind: network, version: 2, revision: 1, order: 10}",
                |_| {},
            ),
        ];
        let networks: Vec<&str> = networks.iter().map(String::as_str).collect();
        let lines = (line(&networks, 0), line(&networks, 1));
        let two_networks = refusal(&networks);
        assert!(
            matches!(two_networks, Error::Several { kind: NETWORK, count: 2, first, second } if (first, second) == lines),
            "{two_networks:?}"
        );
        let groups = [
            network("{}"),
            "slipmesh: {kind: groups}\ncore: [node-a]\n".to_owned(),
            "slipmesh: {kind: groups, order: 10}\ncore: [node-a]\n".to_owned(),
        ];
        let groups: Vec<&str> = groups.iter().map(String::as_str).collect();
        let lines = (line(&groups, 1), line(&groups, 2));
        let two_groups = refusal(&groups);
        assert!(
            matches!(two_groups, Error::Several { kind: GROUPS, count: 2, first, second } if (first, second) == lines),
            "{two_groups:?}"
        );
    }

    #[test]
    fn a_key_the_network_does_not_have_is_refused_by_its_line() {
        let refusal = refusal(&[&network("{mesh: {colour: red}}")]);
        assert!(
            matches!(&refusal, Error::Body { place, .. } if *place == Place::new(NETWORK, 1)),
            "{refusal:?}"
        );
    }

    #[test]
    fn what_is_wrong_across_the_network_is_refused() {
        let refusal = refusal(&[&network("{nodes: {node-c: {node_id: 0.0.0.1}}}")]);
        let twice = Invalid::NodeIdTwice {
            id: std::net::Ipv4Addr::new(0, 0, 0, 1),
            first: name("node-a"),
            second: name("node-c"),
        };
        assert!(
            matches!(&refusal, Error::Invalid(invalid) if *invalid == twice),
            "{refusal:?}"
        );
    }

    #[test]
    fn a_document_reaches_the_nodes_of_its_platform_when_it_names_none() {
        let file = file(&[&network("{}"), &patch("platform: talos", "one")]).unwrap();
        assert_eq!(
            reached(&file, file.patches()[0].aim()),
            ["node-a", "node-b", "node-c"]
        );
        assert!(!file.patches()[0].aim().reaches("node-x"));
    }

    #[test]
    fn a_document_reaches_what_its_include_names_less_what_its_exclude_names() {
        let groups = "slipmesh: {kind: groups}\ncore: [node-a, node-b]\n";
        let file = file(&[
            &network("{}"),
            groups,
            &patch(
                "platform: talos, include: [core, node-c], exclude: [node-a]",
                "one",
            ),
        ])
        .unwrap();
        assert_eq!(
            reached(&file, file.patches()[0].aim()),
            ["node-b", "node-c"]
        );
    }

    #[test]
    fn a_name_that_is_neither_a_node_nor_a_group_is_refused_naming_the_document() {
        let documents = [
            network("{}"),
            patch("platform: talos, include: [node-x]", "one"),
        ];
        let documents: Vec<&str> = documents.iter().map(String::as_str).collect();
        let unknown = GroupsError::UnknownName {
            whose: place(PATCH, &documents, 1).to_string(),
            name: name("node-x"),
        };
        let refusal = refusal(&documents);
        assert!(
            matches!(&refusal, Error::Groups(groups) if *groups == unknown),
            "{refusal:?}"
        );
    }

    #[test]
    fn a_document_left_reaching_no_node_is_refused() {
        let documents = [
            network("{}"),
            template("platform: talos, include: [node-a], exclude: [node-a]"),
        ];
        let documents: Vec<&str> = documents.iter().map(String::as_str).collect();
        let empty = GroupsError::Empty(place(TEMPLATE, &documents, 1).to_string());
        let refusal = refusal(&documents);
        assert!(
            matches!(&refusal, Error::Groups(groups) if *groups == empty),
            "{refusal:?}"
        );
    }

    #[test]
    fn a_document_that_reaches_a_node_of_another_platform_is_refused() {
        let by_name = [
            network("{}"),
            patch("platform: talos, include: [node-a, router1]", "one"),
        ];
        // Inside a group `all` is every node, whatever the platform of the document that names it.
        let by_group = [
            network("{}"),
            "slipmesh: {kind: groups}\neveryone: [all]\n".to_owned(),
            patch("platform: talos, include: [everyone]", "one"),
        ];
        for documents in [&by_name[..], &by_group[..]] {
            let documents: Vec<&str> = documents.iter().map(String::as_str).collect();
            let at = place(PATCH, &documents, documents.len() - 1);
            let refusal = refusal(&documents);
            assert!(
                matches!(&refusal, Error::OtherPlatform { place, platform: Platform::Talos, node, other: Platform::Routeros }
                    if *place == at && node.as_str() == "router1"),
                "{refusal:?}"
            );
        }
    }

    #[test]
    fn a_node_of_another_platform_taken_out_with_exclude_is_no_error() {
        let groups = "slipmesh: {kind: groups}\neveryone: [all]\n";
        let file = file(&[
            &network("{}"),
            groups,
            &patch(
                "platform: talos, include: [everyone], exclude: [router1]",
                "one",
            ),
        ])
        .unwrap();
        assert_eq!(
            reached(&file, file.patches()[0].aim()),
            ["node-a", "node-b", "node-c"]
        );
    }

    #[test]
    fn a_template_or_a_patch_for_routeros_is_refused() {
        for (kind, document) in [
            (TEMPLATE, template("platform: routeros")),
            (PATCH, patch("platform: routeros", "one")),
        ] {
            let documents = [network("{}"), document];
            let documents: Vec<&str> = documents.iter().map(String::as_str).collect();
            let at = place(kind, &documents, 1);
            let refusal = refusal(&documents);
            assert!(
                matches!(&refusal, Error::NotTaken { place, platform: Platform::Routeros } if *place == at),
                "{refusal:?}"
            );
        }
    }

    #[test]
    fn the_groups_are_checked_against_the_nodes() {
        let refusal = refusal(&[
            &network("{}"),
            "slipmesh: {kind: groups}\nnode-a: [node-b]\n",
        ]);
        let like_a_node = GroupsError::NamedLikeANode(name("node-a"));
        assert!(
            matches!(&refusal, Error::Groups(groups) if *groups == like_a_node),
            "{refusal:?}"
        );
    }

    #[test]
    fn a_key_a_group_does_not_take_is_refused_by_its_line() {
        let documents = [
            network("{}"),
            "slipmesh: {kind: groups}\ncore: {include: [node-a], only: [node-b]}\n".to_owned(),
        ];
        let documents: Vec<&str> = documents.iter().map(String::as_str).collect();
        let at = place(GROUPS, &documents, 1);
        let refusal = refusal(&documents);
        assert!(
            matches!(&refusal, Error::Body { place, .. } if *place == at),
            "{refusal:?}"
        );
    }

    #[test]
    fn templates_come_by_order_then_those_for_all_then_as_read() {
        // A template's body is its tool's: `name` tells them apart here.
        let named = |block: &str, name: &str| {
            format!("slipmesh: {{kind: template, {block}}}\nname: {name}\n")
        };
        let file = file(&[
            &network("{}"),
            &named("platform: talos, include: [node-a]", "narrow"),
            &named("platform: talos", "for-all"),
            &named("platform: talos, include: all", "for-all-later"),
            &named(
                "platform: talos, include: [node-a], order: 2000",
                "narrow-last",
            ),
            &named("platform: talos, order: 2000", "for-all-last"),
            &named("platform: talos, order: 10", "first"),
            &named("platform: talos, include: [node-b]", "another-node"),
        ])
        .unwrap();
        assert_eq!(
            body_names(&file.templates_for("node-a")),
            [
                "first",
                "for-all",
                "for-all-later",
                "narrow",
                "for-all-last",
                "narrow-last"
            ]
        );
    }

    #[test]
    fn every_patch_is_listed_in_the_order_of_the_documents() {
        let file = file(&[
            &network("{}"),
            &patch("platform: talos", "second"),
            &patch("platform: talos, order: 10", "first"),
        ])
        .unwrap();
        let all: Vec<&AimedDocument> = file.patches().iter().collect();
        assert_eq!(body_names(&all), ["first", "second"]);
    }

    #[test]
    fn a_patch_gives_its_document_without_the_block() {
        let file = file(&[&network("{}"), &patch("platform: talos", "one")]).unwrap();
        assert_eq!(
            file.patches()[0].value(),
            &yaml("{kind: Example, name: one}")
        );
    }

    #[test]
    fn templates_come_ranked_with_their_bodies() {
        let file = file(&[
            &network("{}"),
            &template("platform: talos, include: [node-a]"),
            &template("platform: talos"),
        ])
        .unwrap();
        let for_a = file.templates_for("node-a");
        assert_eq!(for_a.len(), 2);
        assert!(for_a[0].aim().rank() < for_a[1].aim().rank());
        assert_eq!(reached(&file, for_a[1].aim()), ["node-a"]);
        assert_eq!(file.templates_for("node-b").len(), 1);
        assert_eq!(file.templates().len(), 2);
        assert_eq!(for_a[1].value()["content"], yaml("text"));
    }

    #[test]
    fn a_body_of_a_template_or_a_platform_is_taken_as_written() {
        let documents = [
            network("{}"),
            "slipmesh: {kind: template, platform: talos}\ndaemon: none\ncolour: red\n".to_owned(),
            "slipmesh: {kind: platform, platform: talos}\nlisten: true\n".to_owned(),
            "slipmesh: {kind: platform, platform: routeros}\nmetrics: {}\n".to_owned(),
        ];
        let documents: Vec<&str> = documents.iter().map(String::as_str).collect();
        let file = file(&documents).unwrap();
        assert_eq!(
            file.templates()[0].value(),
            &yaml("{daemon: none, colour: red}")
        );
        let talos = file.platform(Platform::Talos).unwrap();
        assert_eq!(talos.read::<Value>().unwrap(), yaml("{listen: true}"));
        assert_eq!(*talos.place(), place(PLATFORM, &documents, 2));
        let routeros = file.platform(Platform::Routeros).unwrap();
        assert_eq!(routeros.read::<Value>().unwrap(), yaml("{metrics: {}}"));
        assert_eq!(*routeros.place(), place(PLATFORM, &documents, 3));
    }

    #[derive(serde::Deserialize, Debug)]
    #[serde(deny_unknown_fields)]
    struct Known {
        #[expect(dead_code, reason = "only refusals are read")]
        known: u8,
    }

    #[test]
    fn a_body_refused_when_read_names_its_document_and_the_line_of_the_key() {
        let documents = [
            network("{}"),
            "slipmesh: {kind: template, platform: talos}\nknown: 1\nunknown: 2\n".to_owned(),
            "slipmesh: {kind: platform, platform: talos}\nknown: 1\nunknown: 2\n".to_owned(),
        ];
        let documents: Vec<&str> = documents.iter().map(String::as_str).collect();
        let file = file(&documents).unwrap();
        let refusals = [
            (
                1,
                TEMPLATE,
                file.templates()[0].read::<Known>().unwrap_err(),
            ),
            (
                2,
                PLATFORM,
                file.platform(Platform::Talos)
                    .unwrap()
                    .read::<Known>()
                    .unwrap_err(),
            ),
        ];
        for (index, kind, refusal) in refusals {
            let key_line = format!("at line {}", line(&documents, index) + 2);
            assert!(
                matches!(&refusal, Error::Body { place: at, source }
                    if *at == place(kind, &documents, index)
                        && source.to_string().contains(&key_line)),
                "{key_line}: {refusal:?}"
            );
        }
    }

    #[test]
    fn an_aim_reads_as_where_its_document_stands() {
        let documents = [network("{}"), patch("platform: talos", "one")];
        let documents: Vec<&str> = documents.iter().map(String::as_str).collect();
        let file = file(&documents).unwrap();
        assert_eq!(
            file.patches()[0].aim().to_string(),
            place(PATCH, &documents, 1).to_string()
        );
    }

    #[test]
    fn a_platform_without_its_document_has_none() {
        let file = file(&[&network("{}")]).unwrap();
        assert!(file.platform(Platform::Talos).is_none());
    }

    #[test]
    fn a_second_platform_document_for_one_platform_is_refused_by_both_lines_in_the_files() {
        // The second is set first by its order; the lines come as they stand in the files.
        let late = "slipmesh: {kind: platform, platform: talos}\nmetrics: {}\n";
        let early = "slipmesh: {kind: platform, platform: talos, order: 10}\nmetrics: {}\n";
        let documents = [network("{}").as_str(), late, early].map(str::to_owned);
        let documents: Vec<&str> = documents.iter().map(String::as_str).collect();
        let lines = (line(&documents, 1), line(&documents, 2));
        let refusal = refusal(&documents);
        assert!(
            matches!(refusal, Error::PlatformTwice { platform: Platform::Talos, first, second }
                if (first, second) == lines),
            "{refusal:?}"
        );
    }
}
