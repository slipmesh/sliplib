//! `slipmesh.yaml` of format version 2 read as a whole: every block, the network with what is in
//! force for each node and each link, the groups as their nodes, and the documents of a platform
//! with the nodes each reaches.

use super::block::{Block, Exclude, Include, Order, Rank};
use super::check::Invalid;
use super::groups::{Groups, GroupsError, Resolved, Selection};
use super::network::{AMNEZIAWG, Network, WIREGUARD};
use super::platform::{RouterosSettings, TalosSettings, Template};
use super::{Name, Platform};
use crate::slipmesh_file::{BlockError, META_KEY, Place};
use serde::de::DeserializeOwned;
use sliplib_yaml::{Document, Mapping, MultiDoc, Value, merge};
use std::fmt;

/// The syntax of the format this module reads.
pub const VERSION: u64 = 2;

/// The generation of the rules that turn the files into configs, for which this code is built.
pub const REVISION: u64 = 1;

const NETWORK: &str = "network";
const GROUPS: &str = "groups";
const PLATFORM: &str = "platform";
const TEMPLATE: &str = "template";
const PATCH: &str = "patch";

type Source = Box<dyn std::error::Error + Send + Sync>;

/// Why the files do not read.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The text is not a stream of YAML documents.
    #[error(transparent)]
    Yaml(Source),
    #[error(
        "{place} has no `version`, or one below {VERSION}, so it is of format version 1, which the reader of version {VERSION} does not take - convert the files to version {VERSION} as `docs/slipmesh-yaml-v1-to-v2.md` of sliplib says under \"Converting a file by hand\""
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
    /// The body of the document at `place` does not read as its kind.
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
    #[error("{place}, with each node's and each link's own values laid over the network's")]
    InForce {
        place: Place,
        #[source]
        source: Source,
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

/// One document with its `slipmesh:` block read out of it.
struct Read {
    block: Block,
    /// The document without the block, its lines kept so that an error names the line of the file.
    body: Document,
}

impl Read {
    fn block(mut body: Document) -> Result<Self, Error> {
        let line = body.line;
        let block = body
            .take(META_KEY)
            .ok_or(BlockError::Missing)
            .and_then(|block| {
                sliplib_yaml::from_value::<Block>(block)
                    .map_err(|source| BlockError::Malformed(source.into()))
            })
            .map_err(|source| Error::Block { line, source })?;
        Ok(Self { block, body })
    }
}

fn body<T: DeserializeOwned>(document: &Document, kind: &'static str) -> Result<T, Error> {
    document.deserialize().map_err(|source| Error::Body {
        place: Place::new(kind, document.line),
        source: source.into(),
    })
}

/// Refuses files of another version or revision before anything else of them is read, so that a
/// file of a newer format is not refused for a key this code does not know: the version of every
/// `network` document first, then the revision of each.
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

/// The one document of `kind` among `documents`, or none.
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

/// The lists of `routing` that a node may hold its own value of.
const NODE_LISTS: [&str; 3] = ["direct_interfaces", "learn", "announce"];

/// `own` laid over `network`, which holds no `null`. Without a mapping of its own, a node or a
/// link has the network's values.
fn lay(network: &Value, own: Option<&Value>) -> Value {
    match own {
        Some(own) if own.is_mapping() => merge(network.clone(), own),
        _ => network.clone(),
    }
}

/// The network document laid over the empty tree, so that no `null` is left in it, with what is in
/// force written where each node and each link holds its own values: `nodes.<name>.routing` over
/// the same keys of `routing`, and `mesh.links.<name>.<type>` over `mesh.<type>`. Those own values
/// are taken as written: a `null` among them takes the network's away there, which reading the two
/// apart could not tell from a key left out.
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

/// The nodes a document of a platform reaches, and where it stands among the others that reach
/// one of them. It reads as where the document stands in the files, for a refusal to name.
#[derive(Debug, Clone, PartialEq)]
pub struct Aim {
    place: Place,
    platform: Platform,
    rank: Rank,
    reach: Vec<Name>,
}

impl Aim {
    pub fn platform(&self) -> Platform {
        self.platform
    }

    /// Of two documents of one kind that reach one node, the greater is laid over the lesser or
    /// chosen, and of two equal ones the later read.
    pub fn rank(&self) -> Rank {
        self.rank
    }

    /// The nodes the document reaches, in the order of the network.
    pub fn nodes(&self) -> &[Name] {
        &self.reach
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

/// What the block of a `template` or a `patch` says of where it goes.
struct Aimed {
    platform: Platform,
    include: Include,
    exclude: Exclude,
    order: Order,
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
        Ok(Aim {
            place,
            platform,
            rank,
            reach,
        })
    }
}

/// A `template` document: the text a daemon on the nodes it reaches makes its configuration
/// from.
pub struct TemplateDocument {
    aim: Aim,
    body: Document,
}

// By hand: a `Document` holds the text of every file, the private keys among it.
impl fmt::Debug for TemplateDocument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TemplateDocument")
            .field("aim", &self.aim)
            .field("value", &self.body.value)
            .finish()
    }
}

impl TemplateDocument {
    pub fn aim(&self) -> &Aim {
        &self.aim
    }

    /// The keys of the document. The text among them is for the daemon's own tool to check.
    pub fn read(&self) -> Result<Template, Error> {
        body(&self.body, TEMPLATE)
    }
}

/// A `patch` document: one in the format of the platform it names, for that platform's tool to
/// read.
pub struct PatchDocument {
    aim: Aim,
    body: Document,
}

// The aim alone: a patch is in its platform's format, whose secrets this one cannot tell apart.
impl fmt::Debug for PatchDocument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PatchDocument")
            .field("aim", &self.aim)
            .finish_non_exhaustive()
    }
}

impl PatchDocument {
    pub fn aim(&self) -> &Aim {
        &self.aim
    }

    /// The document without its `slipmesh:` block.
    pub fn value(&self) -> &Value {
        &self.body.value
    }
}

/// Those of `documents` that reach `node`, the last by rank last.
fn reaching<'a, T>(documents: &'a [T], aim: impl Fn(&T) -> &Aim, node: &str) -> Vec<&'a T> {
    let mut reaching: Vec<&T> = documents
        .iter()
        .filter(|document| aim(document).reaches(node))
        .collect();
    // Stable: of two documents of one rank, the later read stays the later.
    reaching.sort_by_key(|document| aim(document).rank());
    reaching
}

pub struct File {
    network: Network,
    platforms: Vec<(Platform, Document)>,
    templates: Vec<TemplateDocument>,
    patches: Vec<PatchDocument>,
}

impl fmt::Debug for File {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let platforms: Vec<(Platform, &Value)> = self
            .platforms
            .iter()
            .map(|(platform, document)| (*platform, &document.value))
            .collect();
        f.debug_struct("File")
            .field("network", &self.network)
            .field("platforms", &platforms)
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
        let mut platforms: Vec<(Platform, Document)> = Vec::new();
        let mut aimed = Vec::new();
        for Read { block, body } in documents {
            match block {
                Block::Network { .. } => networks.push(body),
                Block::Groups { .. } => groups.push(body),
                Block::Platform { platform, .. } => {
                    if let Some((_, first)) = platforms.iter().find(|(of, _)| *of == platform) {
                        return Err(Error::PlatformTwice {
                            platform,
                            first: first.line.min(body.line),
                            second: first.line.max(body.line),
                        });
                    }
                    platforms.push((platform, body));
                }
                Block::Template {
                    platform,
                    include,
                    exclude,
                    order,
                } => aimed.push((
                    TEMPLATE,
                    Aimed {
                        platform,
                        include,
                        exclude,
                        order,
                    },
                    body,
                )),
                Block::Patch {
                    platform,
                    include,
                    exclude,
                    order,
                } => aimed.push((
                    PATCH,
                    Aimed {
                        platform,
                        include,
                        exclude,
                        order,
                    },
                    body,
                )),
            }
        }

        let written = at_most_one(networks, NETWORK)?.ok_or(Error::NoNetwork)?;
        // Read as written first: from the text, an error names the line.
        body::<Network>(&written, NETWORK)?;
        let network: Network =
            sliplib_yaml::from_value(in_force(&written.value)).map_err(|source| {
                Error::InForce {
                    place: Place::new(NETWORK, written.line),
                    source: source.into(),
                }
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
        for (kind, aimed, body) in aimed {
            let aim = aimed.aim(Place::new(kind, body.line), &network, &groups)?;
            if kind == TEMPLATE {
                templates.push(TemplateDocument { aim, body });
            } else {
                patches.push(PatchDocument { aim, body });
            }
        }
        Ok(Self {
            network,
            platforms,
            templates,
            patches,
        })
    }

    /// The network, with what is in force for each node under its `routing` and for each link in
    /// the block of its type.
    pub fn network(&self) -> &Network {
        &self.network
    }

    /// The settings of `platform`: the body of its `platform` document, and what a platform has
    /// without one where the files hold none.
    fn settings<T: DeserializeOwned + Default>(&self, platform: Platform) -> Result<T, Error> {
        match self.platforms.iter().find(|(of, _)| *of == platform) {
            Some((_, document)) => body(document, PLATFORM),
            None => Ok(T::default()),
        }
    }

    pub fn talos_settings(&self) -> Result<TalosSettings, Error> {
        self.settings(Platform::Talos)
    }

    /// Nothing, or the refusal of a key: `routeros` has no settings.
    pub fn routeros_settings(&self) -> Result<RouterosSettings, Error> {
        self.settings(Platform::Routeros)
    }

    /// Every `template` document, in the order of the documents.
    pub fn templates(&self) -> &[TemplateDocument] {
        &self.templates
    }

    /// The `template` documents that reach `node`, the last by rank last: of those for one
    /// daemon the node gets the last.
    pub fn templates_for(&self, node: &str) -> Vec<&TemplateDocument> {
        reaching(&self.templates, TemplateDocument::aim, node)
    }

    /// Every `patch` document, in the order of the documents.
    pub fn patches(&self) -> &[PatchDocument] {
        &self.patches
    }

    /// The `patch` documents that reach `node`, in the order their platform's tool lays them
    /// over each other.
    pub fn patches_for(&self, node: &str) -> Vec<&PatchDocument> {
        reaching(&self.patches, PatchDocument::aim, node)
    }
}

#[cfg(test)]
mod tests {
    use super::super::network::LinkKind;
    use super::super::platform::Daemon;
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

    fn names(names: &[Name]) -> Vec<&str> {
        names.iter().map(Name::as_str).collect()
    }

    fn patch_names(patches: &[&PatchDocument]) -> Vec<String> {
        patches
            .iter()
            .map(|patch| patch.value()["name"].as_str().unwrap().to_owned())
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
        assert_eq!(names(file.patches()[0].aim().nodes()), ["node-a", "node-b"]);
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
    fn files_print_with_no_private_key_and_no_patch_body() {
        let key = sliplib_secrets::wireguard::PrivateKey::generate();
        let text = network_edited(BLOCK, |tree| {
            tree["nodes"]["node-a"]["wireguard"] =
                yaml(&format!("{{private_key: {}}}", key.expose()));
        });
        let token = sliplib_secrets::wireguard::PrivateKey::generate();
        let file = file(&[
            &text,
            "slipmesh: {kind: platform, platform: talos}\nmetrics: {}\n",
            &template("platform: talos"),
            &format!(
                "{}token: {}\n",
                patch("platform: talos", "one"),
                token.expose()
            ),
        ])
        .unwrap();
        let printed = format!("{file:?}");
        for secret in [key.expose(), token.expose()] {
            assert!(!printed.contains(secret), "{printed}");
        }
        for value in ["\"metrics\"", "\"text\"", "\"patch\""] {
            assert!(printed.contains(value), "{value}: {printed}");
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
    fn the_talos_documents_of_the_example_reach_the_nodes_they_say() {
        let file = file(&[
            include_str!("../../../docs/example/mesh.yaml"),
            include_str!("../../../docs/example/talos.yaml"),
        ])
        .unwrap();
        let daemons: Vec<Daemon> = file
            .templates()
            .iter()
            .map(|template| template.read().unwrap().daemon)
            .collect();
        assert_eq!(daemons, [Daemon::Router, Daemon::Firewall]);
        for template in file.templates() {
            assert_eq!(
                names(template.aim().nodes()),
                ["node-a", "node-b", "node-c"]
            );
        }
        let reached: Vec<Vec<&str>> = file
            .patches()
            .iter()
            .map(|patch| names(patch.aim().nodes()))
            .collect();
        assert_eq!(
            reached,
            [vec!["node-a", "node-b", "node-c"], vec!["node-b"]]
        );
        let levels = |node: &str| -> Vec<String> {
            file.patches_for(node)
                .iter()
                .map(|patch| patch.value()["environment"][0].as_str().unwrap().to_owned())
                .collect()
        };
        assert_eq!(
            levels("node-b"),
            ["EXAMPLE_LEVEL=info", "EXAMPLE_LEVEL=debug"]
        );
        assert_eq!(levels("node-c"), ["EXAMPLE_LEVEL=info"]);
        assert!(file.patches_for("router1").is_empty());
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
        let v1 = include_str!("../../../mesh/tests/golden/v1/slipmesh.yaml");
        let line = MultiDoc::parse(v1).unwrap().documents()[0].line;
        let of_v1 = File::parse(v1).expect_err("version 1 reads");
        assert!(
            matches!(&of_v1, Error::VersionOne { place } if *place == Place::new(NETWORK, line)),
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
            names(file.patches()[0].aim().nodes()),
            ["node-a", "node-b", "node-c"]
        );
        assert_eq!(file.patches()[0].aim().platform(), Platform::Talos);
        assert_eq!(patch_names(&file.patches_for("node-b")), ["one"]);
        assert!(file.patches_for("router1").is_empty());
        assert!(file.patches_for("node-x").is_empty());
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
        assert_eq!(names(file.patches()[0].aim().nodes()), ["node-b", "node-c"]);
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
            names(file.patches()[0].aim().nodes()),
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
    fn patches_come_by_order_then_those_for_all_then_as_read() {
        let file = file(&[
            &network("{}"),
            &patch("platform: talos, include: [node-a]", "narrow"),
            &patch("platform: talos", "for-all"),
            &patch("platform: talos, include: all", "for-all-later"),
            &patch(
                "platform: talos, include: [node-a], order: 2000",
                "narrow-last",
            ),
            &patch("platform: talos, order: 2000", "for-all-last"),
            &patch("platform: talos, order: 10", "first"),
            &patch("platform: talos, include: [node-b]", "another-node"),
        ])
        .unwrap();
        assert_eq!(
            patch_names(&file.patches_for("node-a")),
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
        let all: Vec<&PatchDocument> = file.patches().iter().collect();
        assert_eq!(patch_names(&all), ["first", "second"]);
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
    fn templates_come_ranked_and_read_as_their_keys() {
        let file = file(&[
            &network("{}"),
            &template("platform: talos, include: [node-a]"),
            &template("platform: talos"),
        ])
        .unwrap();
        let for_a = file.templates_for("node-a");
        assert_eq!(for_a.len(), 2);
        assert!(for_a[0].aim().rank() < for_a[1].aim().rank());
        assert_eq!(names(for_a[1].aim().nodes()), ["node-a"]);
        assert_eq!(file.templates_for("node-b").len(), 1);
        assert_eq!(file.templates().len(), 2);
        let read = for_a[1].read().unwrap();
        assert_eq!(
            (read.daemon, read.content.as_str()),
            (Daemon::Router, "text")
        );
    }

    #[test]
    fn a_template_with_a_key_it_does_not_take_is_refused_when_read() {
        let documents = [
            network("{}"),
            format!("{}colour: red\n", template("platform: talos")),
        ];
        let documents: Vec<&str> = documents.iter().map(String::as_str).collect();
        let at = place(TEMPLATE, &documents, 1);
        let file = file(&documents).unwrap();
        let refusal = file.templates()[0].read().unwrap_err();
        assert!(
            matches!(&refusal, Error::Body { place, .. } if *place == at),
            "{refusal:?}"
        );
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
    fn the_settings_of_talos_are_those_of_its_platform_document() {
        let settings =
            "slipmesh: {kind: platform, platform: talos}\nmetrics: {mesh: {port: 9586}}\n";
        let file = file(&[&network("{}"), settings]).unwrap();
        let metrics = file.talos_settings().unwrap().metrics.unwrap();
        assert_eq!(metrics.mesh.unwrap().port.get(), 9586);
        assert_eq!(metrics.router, None);
    }

    #[test]
    fn a_platform_without_its_document_has_no_settings() {
        let file = file(&[&network("{}")]).unwrap();
        assert_eq!(file.talos_settings().unwrap(), TalosSettings::default());
        assert_eq!(
            file.routeros_settings().unwrap(),
            RouterosSettings::default()
        );
    }

    #[test]
    fn a_key_the_settings_of_a_platform_do_not_have_is_refused_when_read() {
        let documents = [
            network("{}"),
            "slipmesh: {kind: platform, platform: talos}\nlisten: true\n".to_owned(),
            "slipmesh: {kind: platform, platform: routeros}\nmetrics: {}\n".to_owned(),
        ];
        let documents: Vec<&str> = documents.iter().map(String::as_str).collect();
        // Both load: the tool of another platform reads the block alone.
        let file = file(&documents).unwrap();
        let of_talos = place(PLATFORM, &documents, 1);
        let talos = file.talos_settings().unwrap_err();
        assert!(
            matches!(&talos, Error::Body { place, .. } if *place == of_talos),
            "{talos:?}"
        );
        let of_routeros = place(PLATFORM, &documents, 2);
        let routeros = file.routeros_settings().unwrap_err();
        assert!(
            matches!(&routeros, Error::Body { place, .. } if *place == of_routeros),
            "{routeros:?}"
        );
    }

    #[test]
    fn an_empty_platform_document_of_routeros_is_taken() {
        let of_routeros = "slipmesh: {kind: platform, platform: routeros}\n";
        let file = file(&[&network("{}"), of_routeros]).unwrap();
        assert_eq!(
            file.routeros_settings().unwrap(),
            RouterosSettings::default()
        );
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
