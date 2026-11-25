//! The files read as a whole: the blocks, the network in force, the groups as nodes, and each
//! platform document with its aim.

use super::block::{Aimed, Block, PlatformBlock, Rank};
use super::check::Invalid;
use super::groups::{Groups, GroupsError, Resolved, Selection};
use super::network::{AMNEZIAWG, Network, WIREGUARD};
use super::{Name, Platform};
use serde::de::DeserializeOwned;
use sliplib_yaml::{Document, Mapping, MultiDoc, Refused, Value, merge, merge_keeping};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

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

/// Where a document starts: its file, none for a text given as it is, and its line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Start {
    file: Option<Arc<Path>>,
    line: usize,
}

impl Start {
    pub fn file(&self) -> Option<&Path> {
        self.file.as_deref()
    }

    pub fn line(&self) -> usize {
        self.line
    }
}

impl fmt::Display for Start {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}", self.line)?;
        match &self.file {
            Some(file) => write!(f, " of {}", file.display()),
            None => Ok(()),
        }
    }
}

/// A document's kind and where it starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    kind: &'static str,
    start: Start,
}

impl Place {
    /// Of a text given as it is.
    #[cfg(test)]
    pub(crate) fn new(kind: &'static str, line: usize) -> Self {
        Self {
            kind,
            start: Start { file: None, line },
        }
    }

    pub fn start(&self) -> &Start {
        &self.start
    }
}

impl fmt::Display for Place {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "the `kind: {}` document at {}", self.kind, self.start)
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
    #[error("reading {}", path.display())]
    Reading {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{} names no file", path.display())]
    NoFileName { path: PathBuf },
    #[error(
        "{} is a link to {}, which is not there - create the target, or give its path instead",
        path.display(),
        target.display()
    )]
    Dangling { path: PathBuf, target: PathBuf },
    #[error("reading {}", shown(paths))]
    Files {
        paths: Vec<PathBuf>,
        #[source]
        source: Box<Error>,
    },
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
    #[error("the document starting at {start}")]
    Block {
        start: Start,
        #[source]
        source: BlockError,
    },
    #[error("{place}")]
    Body {
        place: Place,
        #[source]
        source: Source,
    },
    /// Of the `network` or the `groups` documents laid together: `at` is the key's line in the
    /// last that wrote it.
    #[error("the `kind: {kind}` documents laid together, at {at}")]
    Laid {
        kind: &'static str,
        at: Start,
        #[source]
        source: Refused,
    },
    #[error("there is no `kind: network` document, and without one there is no network")]
    NoNetwork,
    #[error(transparent)]
    Invalid(#[from] Invalid),
    #[error(transparent)]
    Groups(#[from] GroupsError),
    #[error(
        "two `kind: platform` documents for `{platform}`, at {first} and {second}, and a platform has at most one - write their keys in one"
    )]
    PlatformTwice {
        platform: Platform,
        first: Start,
        second: Start,
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

fn shown(paths: &[PathBuf]) -> String {
    let shown: Vec<String> = paths
        .iter()
        .map(|path| path.display().to_string())
        .collect();
    shown.join(", ")
}

/// A file read, or a text given as it is.
pub struct SourceFile {
    path: Option<Arc<Path>>,
    yaml: MultiDoc,
    new: bool,
}

impl SourceFile {
    /// As given; none for a text given as it is.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// An `edit` file not there yet, read as empty.
    pub fn is_new(&self) -> bool {
        self.new
    }

    pub fn yaml(&self) -> &MultiDoc {
        &self.yaml
    }

    pub fn yaml_mut(&mut self) -> &mut MultiDoc {
        &mut self.yaml
    }
}

pub struct Sources {
    files: Vec<SourceFile>,
    edit: Option<usize>,
}

/// A file as given, by its canonical path.
struct Given<'a> {
    canonical: PathBuf,
    path: &'a Path,
    edit: bool,
    new: bool,
}

/// Where a file not there yet would stand: its name in its directory's canonical path.
fn not_there(path: &Path) -> Result<PathBuf, Error> {
    let name = path.file_name().ok_or_else(|| Error::NoFileName {
        path: path.to_owned(),
    })?;
    if let Ok(target) = std::fs::read_link(path) {
        return Err(Error::Dangling {
            path: path.to_owned(),
            target,
        });
    }
    let directory = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty());
    let directory =
        std::fs::canonicalize(directory.unwrap_or(Path::new("."))).map_err(|source| {
            Error::Reading {
                path: path.to_owned(),
                source,
            }
        })?;
    Ok(directory.join(name))
}

fn given_edit(path: &Path) -> Result<Given<'_>, Error> {
    let (canonical, new) = match std::fs::canonicalize(path) {
        Ok(canonical) => (canonical, false),
        Err(missing) if missing.kind() == std::io::ErrorKind::NotFound => (not_there(path)?, true),
        Err(source) => {
            return Err(Error::Reading {
                path: path.to_owned(),
                source,
            });
        }
    };
    Ok(Given {
        canonical,
        path,
        edit: true,
        new,
    })
}

/// One not there is refused unless it is the `edit` file not there yet.
fn given_input<'a>(path: &'a Path, edit: Option<&Given>) -> Result<Given<'a>, Error> {
    let canonical = match std::fs::canonicalize(path) {
        Ok(canonical) => canonical,
        Err(source) => {
            let new_edit = edit.filter(|edit| edit.new).map(|edit| &edit.canonical);
            match not_there(path) {
                Ok(canonical) if Some(&canonical) == new_edit => {
                    return Ok(Given {
                        canonical,
                        path,
                        edit: false,
                        new: true,
                    });
                }
                _ => {
                    return Err(Error::Reading {
                        path: path.to_owned(),
                        source,
                    });
                }
            }
        }
    };
    Ok(Given {
        canonical,
        path,
        edit: false,
        new: false,
    })
}

impl Sources {
    /// Each file once, whatever the path it is reached by, in the byte order of the canonical paths;
    /// `edit` among them, read too if it is there.
    pub fn read<P: AsRef<Path>>(inputs: &[P], edit: Option<&Path>) -> Result<Self, Error> {
        let edit = edit.map(given_edit).transpose()?;
        let mut given = inputs
            .iter()
            .map(|input| given_input(input.as_ref(), edit.as_ref()))
            .collect::<Result<Vec<Given>, Error>>()?;
        given.extend(edit);
        // `Path` compares by components, so `a/b` would come before `a-b`. Stable, so a file is
        // named as it was first given.
        given.sort_by(|a, b| {
            a.canonical
                .as_os_str()
                .as_encoded_bytes()
                .cmp(b.canonical.as_os_str().as_encoded_bytes())
        });
        given.dedup_by(|later, first| {
            let same = later.canonical == first.canonical;
            first.edit |= same && later.edit;
            same
        });
        let edit = given.iter().position(|file| file.edit);
        let files = given
            .into_iter()
            .map(|file| {
                let yaml = match file.new {
                    true => MultiDoc::parse(""),
                    false => MultiDoc::read(file.path),
                }
                .map_err(|source| Error::Yaml(source.into()))?;
                Ok(SourceFile {
                    path: Some(file.path.into()),
                    yaml,
                    new: file.new,
                })
            })
            .collect::<Result<_, Error>>()?;
        Ok(Self { files, edit })
    }

    /// Its position in [`Sources::files`].
    pub fn edit(&self) -> Option<usize> {
        self.edit
    }

    /// One text, of no file.
    pub fn parse(text: &str) -> Result<Self, Error> {
        let yaml = MultiDoc::parse(text).map_err(|source| Error::Yaml(source.into()))?;
        Ok(Self {
            files: vec![SourceFile {
                path: None,
                yaml,
                new: false,
            }],
            edit: None,
        })
    }

    /// In the order they are read.
    pub fn files(&self) -> &[SourceFile] {
        &self.files
    }

    pub fn files_mut(&mut self) -> &mut [SourceFile] {
        &mut self.files
    }
}

/// A document by its file, in the order of [`Sources::files`], and its position there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DocumentId {
    pub file: usize,
    pub document: usize,
}

/// A document as read, before its block is.
struct Raw<'a> {
    id: DocumentId,
    file: &'a Option<Arc<Path>>,
    document: &'a Document,
}

impl Raw<'_> {
    fn start(&self) -> Start {
        Start {
            file: self.file.clone(),
            line: self.document.line,
        }
    }

    fn place(&self, kind: &'static str) -> Place {
        Place {
            kind,
            start: self.start(),
        }
    }
}

/// A document whose block is read.
struct Read {
    block: Block,
    document: Taken,
}

/// Without its block, its lines kept for an error to name.
struct Taken {
    id: DocumentId,
    start: Start,
    body: Document,
}

impl Read {
    fn block(raw: &Raw) -> Result<Self, Error> {
        let mut body = raw.document.clone();
        let start = raw.start();
        let block = match body.value.get(META_KEY) {
            None => Err(BlockError::Missing),
            Some(_) => Block::read(&body).map_err(|source| BlockError::Malformed(source.into())),
        }
        .map_err(|source| Error::Block {
            start: start.clone(),
            source,
        })?;
        body.take(META_KEY);
        let document = Taken {
            id: raw.id,
            start,
            body,
        };
        Ok(Self { block, document })
    }
}

impl Taken {
    fn place(&self, kind: &'static str) -> Place {
        Place {
            kind,
            start: self.start.clone(),
        }
    }
}

fn body<T: DeserializeOwned>(document: &Document, place: &Place) -> Result<T, Error> {
    document.deserialize().map_err(|source| Error::Body {
        place: place.clone(),
        source: source.into(),
    })
}

/// Before anything else, so a newer format is not refused for a key unknown here: every version,
/// then every revision.
fn generation(documents: &[Raw]) -> Result<(), Error> {
    let networks: Vec<(Place, &Value)> = documents
        .iter()
        .filter_map(|raw| Some((raw, raw.document.value.get(META_KEY)?)))
        .filter(|(_, block)| block.get("kind").and_then(Value::as_str) == Some(NETWORK))
        .map(|(raw, block)| (raw.place(NETWORK), block))
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

/// Below `nodes.<name>.routing` and `mesh.links.<name>.<type>`, a `null` takes the network's value
/// away, so laying one document over another keeps it.
fn keeps_null(path: &[&Value]) -> bool {
    let in_routing =
        matches!(path, [nodes, _, routing, _, ..] if *nodes == "nodes" && *routing == "routing");
    let in_link = matches!(path, [mesh, links, _, kind, _, ..]
        if *mesh == "mesh" && *links == "links" && (*kind == WIREGUARD || *kind == AMNEZIAWG));
    in_routing || in_link
}

/// The documents of one kind, in their order.
struct Layers<'a> {
    first: &'a Taken,
    later: &'a [Taken],
}

impl Layers<'_> {
    /// From an empty tree.
    fn laid(&self, keeps: &dyn Fn(&[&Value]) -> bool) -> Value {
        std::iter::once(self.first)
            .chain(self.later)
            .fold(Value::Null, |tree, layer| {
                merge_keeping(tree, &layer.body.value, keeps)
            })
    }

    /// A refusal names the last layer that wrote the key, and the key's line there.
    fn read<T: DeserializeOwned>(&self, tree: Value, kind: &'static str) -> Result<T, Error> {
        sliplib_yaml::from_value_at(tree).map_err(|refused| {
            let later: Vec<&Value> = self.later.iter().map(|layer| &layer.body.value).collect();
            let path = refused.path();
            let (layer, depth) = sliplib_yaml::written_by(&self.first.body.value, &later, &path);
            let layer = match layer {
                0 => self.first,
                later => &self.later[later - 1],
            };
            let line = layer.body.line_at(&path[..depth]);
            Error::Laid {
                kind,
                at: Start {
                    file: layer.start.file.clone(),
                    line: line.unwrap_or(layer.start.line),
                },
                source: refused,
            }
        })
    }
}

/// Those a node may hold its own of.
const NODE_LISTS: [&str; 3] = ["direct_interfaces", "learn", "announce"];

/// `network` holds no `null`. Without values of its own, the network's.
fn lay(network: &Value, own: Option<&Value>) -> Value {
    own.map_or_else(|| network.clone(), |own| merge(network.clone(), own))
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
        body(&self.body, &self.aim.place)
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
        body(&self.body, &self.place)
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

/// Every document's block, after the version and revision of every `network` document.
fn blocks(sources: &Sources) -> Result<Vec<Read>, Error> {
    let raw: Vec<Raw> = sources
        .files
        .iter()
        .enumerate()
        .flat_map(|(file, source)| {
            source.yaml.documents().iter().map(move |document| Raw {
                id: DocumentId {
                    file,
                    document: document.index,
                },
                file: &source.path,
                document,
            })
        })
        .collect();
    generation(&raw)?;
    raw.iter().map(Read::block).collect()
}

/// What a file of the format is refused for alone, with nothing laid: a block that does not read,
/// another version or revision.
pub fn check_blocks(sources: &Sources) -> Result<(), Error> {
    blocks(sources).map(|_| ())
}

pub struct File {
    network: Network,
    laid: Value,
    network_documents: Vec<DocumentId>,
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
        Self::read(&Sources::parse(raw)?)
    }

    /// As [`Sources::read`] reads them.
    pub fn open<P: AsRef<Path>>(inputs: &[P]) -> Result<Self, Error> {
        Self::read(&Sources::read(inputs, None)?)
    }

    /// A refusal about no document in particular names the files read.
    pub fn read(sources: &Sources) -> Result<Self, Error> {
        Self::read_documents(sources).map_err(|refusal| {
            let paths: Vec<PathBuf> = sources
                .files
                .iter()
                .filter_map(|file| Some(file.path.as_deref()?.to_owned()))
                .collect();
            match refusal {
                Error::NoNetwork | Error::Invalid(_) | Error::Groups(_) if !paths.is_empty() => {
                    Error::Files {
                        paths,
                        source: Box::new(refusal),
                    }
                }
                refusal => refusal,
            }
        })
    }

    fn read_documents(sources: &Sources) -> Result<Self, Error> {
        let mut documents = blocks(sources)?;
        // Stable: documents of one `order` keep the order they were read in.
        documents.sort_by_key(|document| document.block.order());

        let mut networks = Vec::new();
        let mut groups = Vec::new();
        let mut platforms: Vec<(Platform, PlatformDocument)> = Vec::new();
        let mut aimed_documents = Vec::new();
        for Read { block, document } in documents {
            match block {
                Block::Network(_) => networks.push(document),
                Block::Groups(_) => groups.push(document),
                Block::Platform(PlatformBlock { platform, .. }) => {
                    let place = document.place(PLATFORM);
                    if let Some((_, first)) = platforms.iter().find(|(of, _)| *of == platform) {
                        return Err(Error::PlatformTwice {
                            platform,
                            first: first.place.start.clone(),
                            second: place.start,
                        });
                    }
                    let body = document.body;
                    platforms.push((platform, PlatformDocument { place, body }));
                }
                Block::Template(aimed) => {
                    aimed_documents.push((document.place(TEMPLATE), aimed, document.body));
                }
                Block::Patch(aimed) => {
                    aimed_documents.push((document.place(PATCH), aimed, document.body));
                }
            }
        }

        let Some((first, later)) = networks.split_first() else {
            return Err(Error::NoNetwork);
        };
        let layers = Layers { first, later };
        let written = layers.laid(&keeps_null);
        // Read as laid first: a refusal of what is laid under a node or a link would name no
        // document that wrote it.
        layers.read::<Network>(merge(Value::Null, &written), NETWORK)?;
        let network: Network = layers.read(in_force(&written), NETWORK)?;
        network.check()?;

        let nodes: Vec<Name> = network.nodes.keys().cloned().collect();
        let groups = match groups.split_first() {
            None => Groups::default(),
            Some((first, later)) => {
                let layers = Layers { first, later };
                layers.read::<Groups>(layers.laid(&|_| false), GROUPS)?
            }
        }
        .resolve(&nodes)?;

        let mut templates = Vec::new();
        let mut patches = Vec::new();
        for (place, aimed, body) in aimed_documents {
            let kind = place.kind;
            let aim = aimed.aim(place, &network, &groups)?;
            let document = AimedDocument { aim, body };
            if kind == TEMPLATE {
                templates.push(document);
            } else {
                patches.push(document);
            }
        }
        Ok(Self {
            network,
            laid: written,
            network_documents: networks.iter().map(|layer| layer.id).collect(),
            platforms,
            templates,
            patches,
        })
    }

    /// What is in force under each node's `routing` and in each link's block.
    pub fn network(&self) -> &Network {
        &self.network
    }

    /// The `network` documents laid over each other, a `null` kept inside a node's `routing` and a
    /// link's block, for a write to go by.
    pub fn laid(&self) -> &Value {
        &self.laid
    }

    /// In the order they are laid, for a write.
    pub fn network_documents(&self) -> &[DocumentId] {
        &self.network_documents
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
    use crate::router::InterfacePattern;

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

    /// A `network` document holding `overlay` alone, to be laid over others.
    fn layer(overlay: &str) -> String {
        layer_of(yaml(overlay))
    }

    fn layer_of(overlay: Value) -> String {
        let mut document = Mapping::new();
        document.insert(META_KEY.into(), yaml(BLOCK));
        document.extend(overlay.as_mapping().unwrap().clone());
        sliplib_yaml::to_string(&Value::Mapping(document)).unwrap()
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

    fn patterns(list: &[InterfacePattern]) -> Vec<&str> {
        list.iter().map(InterfacePattern::as_str).collect()
    }

    fn body_names(documents: &[&AimedDocument]) -> Vec<String> {
        documents
            .iter()
            .map(|document| document.value()["name"].as_str().unwrap().to_owned())
            .collect()
    }

    #[test]
    fn a_file_opens_as_it_parses_and_a_refusal_names_it_once() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("slipmesh.yaml");
        std::fs::write(&path, network("{}")).unwrap();
        assert_eq!(
            File::open(&[&path]).unwrap().network(),
            file(&[&network("{}")]).unwrap().network()
        );
        let named_once = |err: Error| {
            let mut text = err.to_string();
            let mut source = std::error::Error::source(&err);
            while let Some(cause) = source {
                text += &format!(": {cause}");
                source = cause.source();
            }
            assert_eq!(
                text.matches(&path.display().to_string()).count(),
                1,
                "{text}"
            );
            err
        };
        std::fs::write(&path, "slipmesh: {kind: patch, platform: talos}\n").unwrap();
        let err = named_once(File::open(&[&path]).unwrap_err());
        assert!(
            matches!(&err, Error::Files { paths, source } if *paths == [path.clone()] && matches!(**source, Error::NoNetwork)),
            "{err:?}"
        );
        std::fs::write(&path, format!("{}---\nmetrics: {{}}\n", network("{}"))).unwrap();
        let err = named_once(File::open(&[&path]).unwrap_err());
        assert!(
            matches!(&err, Error::Block { start, source: BlockError::Missing } if start.file() == Some(path.as_path())),
            "{err:?}"
        );
        std::fs::remove_file(&path).unwrap();
        named_once(File::open(&[&path]).unwrap_err());
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
        assert_eq!(patterns(&node.direct_interfaces), ["lan0"]);
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
        assert_eq!(patterns(&node.direct_interfaces), ["lan1", "lan2"]);
        assert_eq!(node.learn, file.network().routing.learn);
        // The example's own: a node that takes neither range.
        let router = &file.network().nodes["router1"].routing;
        assert_eq!(patterns(&router.direct_interfaces), ["bridge-lan"]);
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
        assert_eq!(patterns(&node.direct_interfaces), ["lan0"]);
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
        let documents = [none.as_str()];
        let at = file_line(&documents, "a-b:");
        let refusal = refusal(&documents);
        assert!(
            matches!(&refusal, Error::Laid { kind: NETWORK, at: key, .. } if key.file().is_none() && key.line() == at),
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
        assert_eq!(
            file.network_documents(),
            [DocumentId {
                file: 0,
                document: 2
            }]
        );
    }

    #[test]
    fn a_document_without_its_block_is_refused_by_its_line() {
        let documents = [network("{}"), "kind: Example\n".to_owned()];
        let documents: Vec<&str> = documents.iter().map(String::as_str).collect();
        let line = line(&documents, 1);
        let refusal = refusal(&documents);
        assert!(
            matches!(&refusal, Error::Block { start, source: BlockError::Missing } if start.line() == line),
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
                matches!(&refusal, Error::Block { start, source: BlockError::Malformed(_) } if start.line() == line),
                "{block}: {refusal:?}"
            );
        }
    }

    #[test]
    fn an_empty_block_is_refused_as_malformed_and_a_key_written_twice_as_not_yaml() {
        let network = network("{}");
        for block in ["slipmesh:\n", "slipmesh: null\n"] {
            let refusal = refusal(&[&network, block]);
            assert!(
                matches!(
                    refusal,
                    Error::Block {
                        source: BlockError::Malformed(_),
                        ..
                    }
                ),
                "{block}: {refusal:?}"
            );
        }
        let twice = "slipmesh: {kind: patch, platform: talos, include: [a], include: [b]}\n";
        let refusal = refusal(&[&network, twice]);
        assert!(matches!(refusal, Error::Yaml(_)), "{refusal:?}");
    }

    #[test]
    fn a_tagged_kind_or_platform_is_refused_by_the_rule_on_tags() {
        let newer = network_edited("{kind: !network, version: 3, revision: 1}", |_| ());
        let network = network("{}");
        for (documents, tagged) in [
            (vec![newer.as_str()], "slipmesh.kind"),
            (
                vec![
                    network.as_str(),
                    "slipmesh:\n  kind: platform\n  platform: !talos\n",
                ],
                "slipmesh.platform",
            ),
        ] {
            let refusal = refusal(&documents);
            let Error::Yaml(source) = &refusal else {
                panic!("{refusal:?}");
            };
            assert!(
                matches!(
                    source.downcast_ref::<sliplib_yaml::Error>(),
                    Some(sliplib_yaml::Error::InDocument { source, .. })
                        if matches!(&**source, sliplib_yaml::Error::Tagged { key, .. } if key == tagged)
                ),
                "{refusal:?}"
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
    fn a_layer_alone_need_not_be_valid_and_layers_are_laid_by_order_then_place() {
        let without_routing = network_edited(BLOCK, |tree| {
            tree.as_mapping_mut().unwrap().shift_remove("routing");
        });
        let mut routing = yaml("{mesh: {mtu: 1300}}");
        routing["routing"] = example()["routing"].clone();
        let routing = layer_of(routing);
        let first = network_edited(
            "{kind: network, version: 2, revision: 1, order: 10}",
            |tree| *tree = yaml("{mesh: {mtu: 1200}}"),
        );
        let file = file(&[&without_routing, &routing, &first]).unwrap();
        assert_eq!(
            file.network().routing,
            super::tests::file(&[&network("{}")])
                .unwrap()
                .network()
                .routing
        );
        assert_eq!(file.network().mesh.mtu, Some(1300));
    }

    #[cfg(unix)]
    #[test]
    fn files_are_read_once_each_in_the_byte_order_of_their_canonical_paths() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir(root.join("a")).unwrap();
        std::fs::write(root.join("0-mesh.yaml"), network("{}")).unwrap();
        // `a-b` before `a/x` byte by byte, after it component by component.
        std::fs::write(root.join("a-b.yaml"), layer("{mesh: {mtu: 1300}}")).unwrap();
        std::fs::write(root.join("a/x.yaml"), layer("{mesh: {mtu: 1200}}")).unwrap();
        let settings = "slipmesh: {kind: platform, platform: talos, order: 0}\nmetrics: {}\n";
        std::fs::write(root.join("talos.yaml"), settings).unwrap();
        std::os::unix::fs::symlink(root.join("talos.yaml"), root.join("link.yaml")).unwrap();
        let inputs = [
            root.join("a/x.yaml"),
            root.join("a-b.yaml"),
            root.join("talos.yaml"),
            root.join("a/../talos.yaml"),
            root.join("link.yaml"),
            root.join("0-mesh.yaml"),
        ];
        assert_eq!(Sources::read(&inputs, None).unwrap().files().len(), 4);
        let file = File::open(&inputs).unwrap();
        assert_eq!(file.network().mesh.mtu, Some(1200));
        let talos = file.platform(Platform::Talos).unwrap().place().start();
        assert_eq!(talos.file(), Some(root.join("talos.yaml").as_path()));
    }

    #[test]
    fn an_edit_file_is_read_once_among_the_others_and_one_not_there_yet_as_new() {
        let dir = tempfile::tempdir().unwrap();
        let (a, c) = (dir.path().join("a.yaml"), dir.path().join("c.yaml"));
        std::fs::write(&a, network("{}")).unwrap();
        std::fs::write(&c, layer("{}")).unwrap();
        let new = dir.path().join("./b.yaml");
        let sources = Sources::read(&[&a, &c], Some(&new)).unwrap();
        assert_eq!(sources.edit(), Some(1));
        let edit = &sources.files()[1];
        assert!(edit.is_new() && edit.yaml().documents().is_empty());
        assert_eq!(edit.path(), Some(new.as_path()));
        assert!(!sources.files()[0].is_new());
        let given = dir.path().join("x/../c.yaml");
        std::fs::create_dir(dir.path().join("x")).unwrap();
        let sources = Sources::read(&[&a, &c], Some(&given)).unwrap();
        assert_eq!((sources.files().len(), sources.edit()), (2, Some(1)));
        assert!(!sources.files()[1].is_new());
        let named_twice = Sources::read(&[&a, &dir.path().join("b.yaml"), &c], Some(&new)).unwrap();
        assert_eq!(
            (named_twice.files().len(), named_twice.edit()),
            (3, Some(1))
        );
        assert!(named_twice.files()[1].is_new());
        let other = dir.path().join("d.yaml");
        assert!(matches!(
            Sources::read(&[&a, &other], Some(&new)),
            Err(Error::Reading { path, .. }) if path == other
        ));
        assert!(matches!(
            Sources::read(&[&a], Some(Path::new(""))),
            Err(Error::NoFileName { .. })
        ));
        assert_eq!(Sources::read(&[&c], Some(&a)).unwrap().edit(), Some(0));
        assert_eq!(Sources::read(&[&a], None).unwrap().edit(), None);
        // Under a directory that is not there, or under a file.
        for nowhere in [dir.path().join("no/such.yaml"), a.join("such.yaml")] {
            assert!(
                matches!(
                    Sources::read(&[&a], Some(&nowhere)),
                    Err(Error::Reading { ref path, .. }) if *path == nowhere
                ),
                "{nowhere:?}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn an_edit_file_that_is_a_link_to_nothing_is_refused_naming_both() {
        let dir = tempfile::tempdir().unwrap();
        let (link, target) = (
            dir.path().join("secrets.yaml"),
            dir.path().join("gone.yaml"),
        );
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(matches!(
            Sources::read::<&Path>(&[], Some(&link)),
            Err(Error::Dangling { path, target: to }) if path == link && to == target
        ));
    }

    #[test]
    fn the_documents_of_a_file_are_checked_alone_with_nothing_laid() {
        let secrets = Sources::parse(&layer("{nodes: {node-a: {wireguard: {}}}}")).unwrap();
        check_blocks(&secrets).unwrap();
        assert!(File::read(&secrets).is_err());
        let version_one = Sources::parse("slipmesh: {kind: network}\n").unwrap();
        assert!(matches!(
            check_blocks(&version_one),
            Err(Error::VersionOne { .. })
        ));
        let blockless = Sources::parse("nodes: {}\n").unwrap();
        assert!(matches!(
            check_blocks(&blockless),
            Err(Error::Block {
                source: BlockError::Missing,
                ..
            })
        ));
    }

    #[test]
    fn the_laid_network_keeps_a_null_inside_a_block_for_a_write() {
        let layers = [
            network("{}"),
            layer("{nodes: {node-b: {routing: {learn: null}}}}"),
        ];
        let layers: Vec<&str> = layers.iter().map(String::as_str).collect();
        let file = file(&layers).unwrap();
        assert_eq!(
            file.laid()["nodes"]["node-b"]["routing"].get("learn"),
            Some(&Value::Null)
        );
    }

    #[test]
    fn what_is_edited_in_the_sources_is_what_is_read() {
        let mut sources = Sources::parse(&network("{}")).unwrap();
        sources.files_mut()[0].yaml_mut().document_mut(0).unwrap()["mesh"]["mtu"] = 1300.into();
        assert_eq!(File::read(&sources).unwrap().network().mesh.mtu, Some(1300));
    }

    #[test]
    fn a_key_a_layer_takes_out_and_a_later_writes_again_stands_last() {
        let mut again = yaml("{nodes: {}}");
        again["nodes"]["node-a"] = example()["nodes"]["node-a"].clone();
        let again = layer_of(again);
        let documents = [network("{}"), layer("{nodes: {node-a: null}}"), again];
        let documents: Vec<&str> = documents.iter().map(String::as_str).collect();
        let file = file(&documents).unwrap();
        let nodes: Vec<&str> = file.network().nodes.keys().map(Name::as_str).collect();
        assert_eq!(nodes, ["node-b", "node-c", "router1", "node-a"]);
    }

    #[test]
    fn a_null_inside_a_block_stays_until_a_later_layer_takes_the_block_out() {
        let learns_nothing = |file: &File| file.network().nodes["node-b"].routing.learn.is_empty();
        let keeps_no_alive = |file: &File| amneziawg(file, "a-b").persistent_keepalive.is_none();
        let rows = [
            (
                network("{}"),
                "{nodes: {node-b: {routing: {learn: null}}}}",
                "{nodes: {node-b: {platform: talos}}}",
                &["{nodes: {node-b: {routing: null}}}"][..],
                learns_nothing as fn(&File) -> bool,
            ),
            (
                network("{mesh: {amneziawg: {persistent_keepalive: 21-29}}}"),
                "{mesh: {links: {a-b: {amneziawg: {persistent_keepalive: null}}}}}",
                "{mesh: {links: {a-b: {type: amneziawg}}}}",
                &[
                    "{mesh: {links: {a-b: {amneziawg: null}}}}",
                    "{mesh: {links: {a-b: {amneziawg: {port: 51830}}}}}",
                ],
                keeps_no_alive,
            ),
        ];
        for (base, null, beside, block_out, taken_away) in rows {
            let mut documents = vec![base, layer(null), layer(beside)];
            let laid = |documents: &[String]| {
                let documents: Vec<&str> = documents.iter().map(String::as_str).collect();
                file(&documents).unwrap()
            };
            assert!(taken_away(&laid(&documents)), "{null}");
            documents.extend(block_out.iter().map(|overlay| layer(overlay)));
            assert!(!taken_away(&laid(&documents)), "{null}");
        }
    }

    #[test]
    fn a_key_written_null_outside_those_blocks_is_the_key_left_out_in_any_layer() {
        for path in [
            &["routing", "bfd"][..],
            &["routing", "bfd", "min_rx_ms"],
            &["routing", "direct_interfaces"],
            &["routing", "learn"],
            &["routing", "announce"],
            &["routing", "classes"],
            &["mesh", "links"],
            &["nodes", "router1", "routing"],
        ] {
            let edited = |null: bool| {
                network_edited(BLOCK, |tree| {
                    let (key, above) = path.split_last().unwrap();
                    let parent = above.iter().fold(tree, |value, key| &mut value[*key]);
                    let parent = parent.as_mapping_mut().unwrap();
                    if null {
                        parent.insert((*key).into(), Value::Null);
                    } else {
                        parent.shift_remove(*key).unwrap();
                    }
                })
            };
            let left_out = file(&[&edited(false)]).unwrap();
            let written_null = file(&[&edited(true)]).unwrap();
            assert_eq!(written_null.network(), left_out.network(), "{path:?}");
            let overlay = path.iter().rev().fold("null".to_owned(), |inner, key| {
                format!("{{{key}: {inner}}}")
            });
            let laid_null = file(&[&network("{}"), &layer(&overlay)]).unwrap();
            assert_eq!(laid_null.network(), left_out.network(), "{path:?} laid");
        }
    }

    #[test]
    fn a_refusal_of_what_is_laid_names_the_file_and_line_of_the_layer_that_wrote_the_key() {
        let node = "{platform: talos, node_id: 0.0.0.9, underlay: {wan: {}}}";
        let network_block = format!("slipmesh: {BLOCK}\n");
        let base = network("{}");
        // Written by the first file, with the later one writing beside it.
        let first = |written: &str, key: &str| {
            let text = network(written);
            let line = file_line(&[&text], key);
            (text, line)
        };
        let (unknown, unknown_at) = first(
            "{mesh: {links: {a-c: {amneziawg: {obfuscation: {colour: 1}}}}}}",
            "colour: 1",
        );
        let (wrong, wrong_at) = first("{mesh: {mtu: many}}", "mtu: many");
        let one = network(&format!("{{nodes: {{1.0: {node}}}}}"));
        let other = layer(&format!("{{nodes: {{1: {node}}}}}"));
        let other_at = file_line(&[&other], "nodes:");
        for (first, later, in_first, line) in [
            // Read by serde's derive.
            (
                base.clone(),
                format!("{network_block}mesh:\n  colour: red\n"),
                false,
                3,
            ),
            // A range.
            (
                base.clone(),
                format!("{network_block}mesh:\n  amneziawg:\n    persistent_keepalive: 9-1\n"),
                false,
                4,
            ),
            // A name, as a key.
            (
                base.clone(),
                format!("{network_block}nodes:\n  node-b: {node}\n  node x: {node}\n"),
                false,
                4,
            ),
            // Two keys that read as one: the path stops at their mapping, in one layer or two.
            (
                base.clone(),
                format!("{network_block}nodes:\n  1.0: {node}\n  1: {node}\n"),
                false,
                2,
            ),
            (one, other, false, other_at),
            // A link read in two steps: the path stops at the link.
            (
                base.clone(),
                format!(
                    "{network_block}mesh:\n  links:\n    a-b:\n      ends:\n        node-b: null\n"
                ),
                false,
                4,
            ),
            // An obfuscation that refuses the keys its reader passes over.
            (
                base.clone(),
                format!(
                    "{network_block}mesh:\n  links:\n    a-b:\n      amneziawg:\n        obfuscation:\n          colour: 1\n"
                ),
                false,
                7,
            ),
            (
                unknown,
                layer("{mesh: {links: {a-c: {amneziawg: {obfuscation: {jc: 5}}}}}}"),
                true,
                unknown_at,
            ),
            (
                wrong,
                layer("{mesh: {interface_prefix: wg-}}"),
                true,
                wrong_at,
            ),
            // A required key missing or written `null`: the path stops at what lacks it.
            (
                base.clone(),
                format!("{network_block}nodes:\n  node-x:\n    node_id: 0.0.0.9\n"),
                false,
                3,
            ),
            (
                base.clone(),
                format!("{network_block}nodes:\n  node-a:\n    platform: null\n"),
                false,
                3,
            ),
            // A group, read as a list or a mapping.
            (
                base.clone(),
                "slipmesh: {kind: groups}\ncore:\n- node-a\n- node a\n".to_owned(),
                false,
                4,
            ),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let paths = [dir.path().join("0.yaml"), dir.path().join("1.yaml")];
            std::fs::write(&paths[0], &first).unwrap();
            std::fs::write(&paths[1], &later).unwrap();
            let holder = &paths[usize::from(!in_first)];
            let refusal = File::open(&paths).unwrap_err();
            assert!(
                matches!(&refusal, Error::Laid { at, .. }
                    if at.file() == Some(holder.as_path()) && at.line() == line),
                "{later}: {refusal:?}"
            );
            assert!(
                refusal.to_string().contains(&holder.display().to_string()),
                "{refusal}"
            );
        }
    }

    #[test]
    fn version_and_revision_are_checked_across_files_every_version_first() {
        let dir = tempfile::tempdir().unwrap();
        let (older, newer) = (dir.path().join("a.yaml"), dir.path().join("b.yaml"));
        std::fs::write(
            &older,
            network_edited("{kind: network, version: 2, revision: 0}", |_| {}),
        )
        .unwrap();
        std::fs::write(
            &newer,
            network_edited("{kind: network, version: 3, revision: 1}", |_| {}),
        )
        .unwrap();
        let refusal = File::open(&[&older, &newer]).unwrap_err();
        assert!(
            matches!(&refusal, Error::NewerVersion { place, version: 3 } if place.start().file() == Some(newer.as_path())),
            "{refusal:?}"
        );
        assert!(
            refusal.to_string().contains(&newer.display().to_string()),
            "{refusal}"
        );
        std::fs::write(&newer, layer("{}")).unwrap();
        let refusal = File::open(&[&older, &newer]).unwrap_err();
        assert!(
            matches!(&refusal, Error::OlderRevision { place, revision: 0 } if place.start().file() == Some(older.as_path())),
            "{refusal:?}"
        );
    }

    #[test]
    fn the_example_reads_from_its_three_files_as_they_are() {
        let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs/example");
        let file =
            File::open(&["talos.yaml", "secrets.yaml", "mesh.yaml"].map(|name| example.join(name)))
                .unwrap();
        let network = file.network();
        assert!(
            network.nodes["node-a"]
                .wireguard
                .as_ref()
                .unwrap()
                .private_key
                .is_some()
        );
        assert_eq!(network.mesh.links["a-b"].kind.obfuscation().jc, Some(4));
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
                &refusal,
                Error::Block {
                    start,
                    source: BlockError::Malformed(_)
                } if start.line() == 1
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
    fn a_key_the_network_does_not_have_is_refused_by_its_line() {
        let documents = [network("{mesh: {colour: red}}")];
        let documents: Vec<&str> = documents.iter().map(String::as_str).collect();
        let at = file_line(&documents, "colour: red");
        let refusal = refusal(&documents);
        assert!(
            matches!(&refusal, Error::Laid { kind: NETWORK, at: key, .. } if key.file().is_none() && key.line() == at),
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
        let at = file_line(&documents, "core: {include: [node-a], only: [node-b]}");
        let refusal = refusal(&documents);
        assert!(
            matches!(&refusal, Error::Laid { kind: GROUPS, at: key, .. } if key.line() == at),
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
    fn a_second_platform_document_for_one_platform_is_refused_naming_both_in_their_order() {
        let late = "slipmesh: {kind: platform, platform: talos}\nmetrics: {}\n";
        let early = "slipmesh: {kind: platform, platform: talos, order: 10}\nmetrics: {}\n";
        let documents = [network("{}").as_str(), late, early].map(str::to_owned);
        let documents: Vec<&str> = documents.iter().map(String::as_str).collect();
        let places = (
            place(PLATFORM, &documents, 2),
            place(PLATFORM, &documents, 1),
        );
        let refusal = refusal(&documents);
        assert!(
            matches!(&refusal, Error::PlatformTwice { platform: Platform::Talos, first, second }
                if (first, second) == (places.0.start(), places.1.start())),
            "{refusal:?}"
        );
    }
}
