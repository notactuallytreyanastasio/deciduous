//! The shared graph file: one JSON document, tracked in git.
//!
//! A project's decision graph lives in a single file committed alongside the
//! code:
//!
//! ```text
//! .deciduous/graph.json
//! {
//!   "version": 1,
//!   "nodes":  { "<change_id>": { ... } },
//!   "edges":  { "<edge_id>":   { ... } },
//!   "themes": { "<change_id>": { ... } },
//!   "tags":   { "<node_change_id>--<theme_change_id>": { ... } }
//! }
//! ```
//!
//! The local SQLite database is a per-machine cache of that file (plus
//! local-only data such as sessions and the command log). Every write that
//! goes through [`crate::db::Database`] is mirrored into the file at once,
//! and `deciduous sync` reconciles the two in both directions.
//!
//! 0.17 kept one small file per record under `.deciduous/sync/`. The idea was
//! that two people adding records never touch the same file, so git merges
//! them with no conflict. It worked, and the cost was worse than the problem:
//! a real graph is thousands of files, so `git status` after a sync is a wall
//! of noise, a PR diff is unreadable, a clone is dominated by directory
//! entries, and a rename or a hand edit can put a record in a file named after
//! a different one. [`RecordStore::import_legacy_record_dir`] folds such a
//! directory into this file and deletes it.
//!
//! One file means every concurrent change collides in git, so the merge
//! driver stops being a nicety and becomes the mechanism. `deciduous
//! merge-record` (registered by `init`/`update`/`sync`) merges the two
//! documents record by record: a record only one side touched is taken as is,
//! and a record both sides changed goes through [`merge_record_values`], which
//! merges field by field and breaks ties on `updated_at`. Adds from both sides
//! both survive. A file left with conflict markers — merged in a clone where
//! the driver is not registered — is repaired the same way by `deciduous
//! sync`.
//!
//! Integer ids are local aliases that differ between machines. Records refer
//! to each other only by `change_id`, and the CLI accepts a `change_id` prefix
//! wherever it accepts an id.
//!
//! Deletion writes a tombstone (the record with `deleted_at` set) rather than
//! dropping the entry, so a deletion propagates to machines that already have
//! the record.

use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::SystemTime;

use crate::db::{Database, DecisionEdge, DecisionNode, NodeTheme, Theme};

/// File name under `.deciduous/` that holds the shared graph.
pub const STORE_FILE_NAME: &str = "graph.json";

/// Directory name under `.deciduous/` that held the 0.17 per-record store.
/// Still read, once, to migrate it.
pub const STORE_DIR_NAME: &str = "sync";

/// Format version written into every document.
pub const DOC_VERSION: u32 = 1;

// ============================================================================
// Record types
// ============================================================================

/// A decision node as stored on disk.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeRecord {
    pub change_id: String,
    pub node_type: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub status: String,
    /// Expanded `metadata_json` (confidence, branch, prompt, files, commit...).
    /// Stored as an object so diffs are readable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
    pub created_at: String,
    pub updated_at: String,
    /// Who last wrote this record (git user.name).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// Set when the node was deleted; the record is then a tombstone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<String>,
    /// Fields this version does not know about (a newer deciduous, another
    /// tool). Carried through every read and write untouched: dropping them
    /// would delete a teammate's data on the next unrelated edit.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl NodeRecord {
    /// Build a record from a database row.
    pub fn from_db(node: &DecisionNode, author: Option<&str>) -> Self {
        Self {
            change_id: node.change_id.clone(),
            node_type: node.node_type.clone(),
            title: node.title.clone(),
            description: node.description.clone(),
            status: node.status.clone(),
            metadata: node.metadata_json.as_deref().map(parse_metadata),
            created_at: node.created_at.clone(),
            updated_at: node.updated_at.clone(),
            author: author.map(str::to_string),
            deleted_at: None,
            extra: BTreeMap::new(),
        }
    }

    /// The compact `metadata_json` string for the database.
    pub fn metadata_json(&self) -> Option<String> {
        self.metadata
            .as_ref()
            .and_then(|m| serde_json::to_string(m).ok())
    }

    pub fn is_tombstone(&self) -> bool {
        self.deleted_at.is_some()
    }

    /// The instant this record's current state was written.
    pub fn effective_ts(&self) -> DateTime<Utc> {
        match &self.deleted_at {
            Some(d) => parse_ts(d).max(parse_ts(&self.updated_at)),
            None => parse_ts(&self.updated_at),
        }
    }
}

/// An edge as stored on disk. Edges are immutable apart from deletion.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EdgeRecord {
    pub edge_id: String,
    pub from_change_id: String,
    pub to_change_id: String,
    pub edge_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<f64>,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<String>,
    /// Fields this version does not know about (a newer deciduous, another
    /// tool). Carried through every read and write untouched: dropping them
    /// would delete a teammate's data on the next unrelated edit.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl EdgeRecord {
    /// Build a record from a database row. Returns `None` for legacy rows
    /// whose endpoints have no `change_id` (pre-migration databases).
    pub fn from_db(edge: &DecisionEdge, author: Option<&str>) -> Option<Self> {
        let from = edge.from_change_id.clone()?;
        let to = edge.to_change_id.clone()?;
        Some(Self {
            edge_id: edge_id(&from, &to, &edge.edge_type),
            from_change_id: from,
            to_change_id: to,
            edge_type: edge.edge_type.clone(),
            rationale: edge.rationale.clone(),
            weight: edge.weight,
            created_at: edge.created_at.clone(),
            author: author.map(str::to_string),
            deleted_at: None,
            extra: BTreeMap::new(),
        })
    }

    pub fn is_tombstone(&self) -> bool {
        self.deleted_at.is_some()
    }
}

/// A theme (tag definition) as stored on disk.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ThemeRecord {
    pub change_id: String,
    pub name: String,
    pub color: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<String>,
    /// Fields this version does not know about (a newer deciduous, another
    /// tool). Carried through every read and write untouched: dropping them
    /// would delete a teammate's data on the next unrelated edit.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl ThemeRecord {
    pub fn from_db(theme: &Theme, author: Option<&str>) -> Self {
        Self {
            change_id: theme.change_id.clone(),
            name: theme.name.clone(),
            color: theme.color.clone(),
            description: theme.description.clone(),
            created_at: theme.created_at.clone(),
            updated_at: theme.updated_at.clone(),
            author: author.map(str::to_string),
            deleted_at: None,
            extra: BTreeMap::new(),
        }
    }

    pub fn is_tombstone(&self) -> bool {
        self.deleted_at.is_some()
    }

    pub fn effective_ts(&self) -> DateTime<Utc> {
        match &self.deleted_at {
            Some(d) => parse_ts(d).max(parse_ts(&self.updated_at)),
            None => parse_ts(&self.updated_at),
        }
    }
}

/// A node/theme association as stored on disk.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TagRecord {
    pub node_change_id: String,
    pub theme_change_id: String,
    pub source: String,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<String>,
    /// Fields this version does not know about (a newer deciduous, another
    /// tool). Carried through every read and write untouched: dropping them
    /// would delete a teammate's data on the next unrelated edit.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl TagRecord {
    pub fn is_tombstone(&self) -> bool {
        self.deleted_at.is_some()
    }
}

/// What a record's file must be named after, so a file whose name and body
/// disagree (a bad rename, a hand edit) is rejected instead of trusted.
pub trait RecordIdentity {
    fn record_stem(&self) -> String;
}
impl RecordIdentity for NodeRecord {
    fn record_stem(&self) -> String {
        self.change_id.clone()
    }
}
impl RecordIdentity for EdgeRecord {
    fn record_stem(&self) -> String {
        self.edge_id.clone()
    }
}
impl RecordIdentity for ThemeRecord {
    fn record_stem(&self) -> String {
        self.change_id.clone()
    }
}
impl RecordIdentity for TagRecord {
    fn record_stem(&self) -> String {
        tag_id(&self.node_change_id, &self.theme_change_id)
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Deterministic edge identity: the same (from, to, type) on any machine
/// produces the same id, so concurrent creation of the same edge converges
/// on one file.
pub fn edge_id(from_change_id: &str, to_change_id: &str, edge_type: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(from_change_id.as_bytes());
    hasher.update([0u8]);
    hasher.update(to_change_id.as_bytes());
    hasher.update([0u8]);
    hasher.update(edge_type.as_bytes());
    let digest = hasher.finalize();
    digest
        .iter()
        .take(10)
        .map(|b| format!("{:02x}", b))
        .collect()
}

/// File stem for a tag record.
pub fn tag_id(node_change_id: &str, theme_change_id: &str) -> String {
    format!("{}--{}", node_change_id, theme_change_id)
}

/// Parse a stored timestamp. Accepts RFC 3339 (what the database writes)
/// and a couple of looser forms produced by `--date`. Unparseable values
/// sort before everything else so a real timestamp always wins.
pub fn parse_ts(s: &str) -> DateTime<Utc> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return dt.with_timezone(&Utc);
    }
    // Naive shapes come from `add --date`, which reads them as local time.
    let local = |naive: chrono::NaiveDateTime| {
        chrono::Local
            .from_local_datetime(&naive)
            .single()
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_else(|| DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc))
    };
    for fmt in ["%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M:%S", "%Y-%m-%d"] {
        if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(s, fmt) {
            return local(naive);
        }
        if let Ok(date) = chrono::NaiveDate::parse_from_str(s, fmt) {
            return local(date.and_hms_opt(0, 0, 0).unwrap_or_default());
        }
    }
    DateTime::<Utc>::UNIX_EPOCH
}

/// Current time in the same format the database uses for timestamps.
pub fn now_ts() -> String {
    chrono::Local::now().to_rfc3339()
}

fn parse_metadata(raw: &str) -> Value {
    serde_json::from_str(raw).unwrap_or_else(|_| Value::String(raw.to_string()))
}

/// Who to attribute local writes to: git `user.name`, else the OS user.
pub fn get_current_author() -> String {
    if let Ok(output) = std::process::Command::new("git")
        .args(["config", "user.name"])
        .output()
    {
        if output.status.success() {
            if let Ok(name) = String::from_utf8(output.stdout) {
                let name = name.trim();
                if !name.is_empty() {
                    return name.to_string();
                }
            }
        }
    }
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "unknown".to_string())
}

/// Stable, pretty JSON with a trailing newline. Keys come out sorted, so
/// two machines writing the same record produce byte-identical files.
fn to_stable_json<T: Serialize>(value: &T) -> io::Result<String> {
    let v = serde_json::to_value(value).map_err(io::Error::other)?;
    let mut s = serde_json::to_string_pretty(&v).map_err(io::Error::other)?;
    s.push('\n');
    Ok(s)
}

/// Write `content` to `path` only if it differs from what is there.
/// Writes go to a temp file first and are renamed into place, so two
/// processes writing at once can never interleave bytes (the failure that
/// corrupted the old JSONL logs). Returns `true` if the file changed.
fn write_if_changed(path: &Path, content: &str) -> io::Result<bool> {
    if let Ok(existing) = fs::read_to_string(path) {
        if existing == content {
            return Ok(false);
        }
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension(format!("json.tmp-{}", std::process::id()));
    fs::write(&tmp, content)?;
    fs::rename(&tmp, path)?;
    Ok(true)
}

/// A record that could not be read out of the graph file.
#[derive(Debug, Clone, Serialize)]
pub struct ReadError {
    /// The graph file the bad record sits in.
    pub path: String,
    /// The key it was filed under, when the problem is with one record
    /// rather than the whole file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub message: String,
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.id {
            Some(id) => write!(f, "{} [{}]: {}", self.path, id, self.message),
            None => write!(f, "{}: {}", self.path, self.message),
        }
    }
}

/// Result of reading one kind of record: everything that parsed, plus every
/// record that did not (a broken record never blocks the others).
#[derive(Debug)]
pub struct StoreRead<T> {
    pub records: Vec<T>,
    pub errors: Vec<ReadError>,
}

impl<T> Default for StoreRead<T> {
    fn default() -> Self {
        Self {
            records: Vec::new(),
            errors: Vec::new(),
        }
    }
}

impl<T> StoreRead<T> {
    /// Keys that failed to read, which must never be overwritten by an
    /// export: whatever is wrong with them is still the user's data.
    pub fn bad_ids(&self) -> HashSet<String> {
        self.errors.iter().filter_map(|e| e.id.clone()).collect()
    }
}

// ============================================================================
// The document
// ============================================================================

/// The whole shared graph, as it sits in `.deciduous/graph.json`.
///
/// `BTreeMap` rather than `HashMap` so serialization is byte-stable: two
/// machines holding the same records write the same file, and a diff shows
/// only what actually changed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GraphDoc {
    pub version: u32,
    #[serde(default)]
    pub nodes: BTreeMap<String, NodeRecord>,
    #[serde(default)]
    pub edges: BTreeMap<String, EdgeRecord>,
    #[serde(default)]
    pub themes: BTreeMap<String, ThemeRecord>,
    #[serde(default)]
    pub tags: BTreeMap<String, TagRecord>,
    /// Top-level sections this version does not know about, kept as is.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl Default for GraphDoc {
    fn default() -> Self {
        Self {
            version: DOC_VERSION,
            nodes: BTreeMap::new(),
            edges: BTreeMap::new(),
            themes: BTreeMap::new(),
            tags: BTreeMap::new(),
            extra: BTreeMap::new(),
        }
    }
}

impl GraphDoc {
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
            && self.edges.is_empty()
            && self.themes.is_empty()
            && self.tags.is_empty()
    }
}

/// Read the graph file. A missing or empty file is an empty graph; anything
/// else that will not parse is an error, never an empty graph, so a corrupt
/// file is reported instead of being overwritten with local rows.
fn load_doc(path: &Path) -> io::Result<GraphDoc> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(GraphDoc::default()),
        Err(e) => return Err(e),
    };
    if text.trim().is_empty() {
        return Ok(GraphDoc::default());
    }
    serde_json::from_str(&text).map_err(|e| {
        let hint = if text.contains("<<<<<<<") {
            "; it still has git conflict markers, run `deciduous sync` to merge it"
        } else {
            "; left untouched, run `deciduous sync`"
        };
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{} is not readable ({}){}", path.display(), e, hint),
        )
    })
}

/// Pull one kind of record out of the document, rejecting any whose key
/// disagrees with the record filed under it (a bad hand edit or merge).
fn read_map<T: Clone + RecordIdentity>(path: &Path, map: &BTreeMap<String, T>) -> StoreRead<T> {
    let mut out = StoreRead::default();
    for (key, rec) in map {
        let expected = rec.record_stem();
        if *key == expected {
            out.records.push(rec.clone());
        } else {
            out.errors.push(ReadError {
                path: path.display().to_string(),
                id: Some(key.clone()),
                message: format!(
                    "key does not match the record filed under it (expected {})",
                    expected
                ),
            });
        }
    }
    out
}

/// Merge a record produced by a local write into the one already in the
/// document. `base` is what the database held before the write, when the
/// caller knows it: with it, a field only the file changed (a teammate's
/// edit that was pulled but not synced yet) survives the write. Without it
/// every differing field is a collision.
///
/// The write is restamped first (see [`restamp_local_write`]), so it wins
/// every collision. Returns the merged record and, when the write's
/// `updated_at` had to move, the new value, which the database must adopt
/// too or the next sync would see the file as newer and re-import it.
fn merged_record<T>(
    existing: Option<&T>,
    incoming: &T,
    base: Option<&T>,
) -> io::Result<(T, Option<String>)>
where
    T: Serialize + for<'de> Deserialize<'de>,
{
    let bad = |e: serde_json::Error| io::Error::other(e);
    let Some(existing) = existing else {
        let rec = serde_json::to_value(incoming)
            .and_then(serde_json::from_value)
            .map_err(bad)?;
        return Ok((rec, None));
    };
    let ours = serde_json::to_value(existing).map_err(bad)?;
    let mut theirs = serde_json::to_value(incoming).map_err(bad)?;
    let base = base.map(serde_json::to_value).transpose().map_err(bad)?;
    let restamped = restamp_local_write(base.as_ref(), &ours, &mut theirs);
    let merged = merge_record_values(base.as_ref(), &ours, &theirs);
    let rec = serde_json::from_value(merged).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("merging a record produced something unreadable: {}", e),
        )
    })?;
    Ok((rec, restamped))
}

/// Fields that say who wrote a record and when, not what it says.
const STAMP_FIELDS: [&str; 3] = ["author", "created_at", "updated_at"];

/// A local write happens after every version of its record that is already
/// in the file, so it must carry a later timestamp than all of them.
///
/// Normally it does: the database stamps it with the current time. It does
/// not when the file holds a version stamped ahead of this clock: a node
/// backdated into the future with `add --date`, or a teammate whose clock
/// runs fast. Last-writer-wins then picks the file's version, the write
/// reports success, and the next `deciduous sync` reverts the database to
/// the file. Every later edit is discarded the same way until the wall
/// clock passes the bad timestamp.
///
/// So a write that changes anything is moved to just after the newest
/// version it replaces (a Lamport clock, in effect). The field moved is the
/// one that dates this kind of write: `deleted_at` for a tombstone,
/// `updated_at` for an edit, `created_at` for an edge or tag (which have
/// nothing else). Returns the new `updated_at` when that is what moved.
fn restamp_local_write(
    base: Option<&Value>,
    existing: &Value,
    incoming: &mut Value,
) -> Option<String> {
    let ex_ts = record_ts(existing);
    let (Some(inc), Some(ex)) = (incoming.as_object(), existing.as_object()) else {
        return None;
    };
    if record_ts(incoming) > ex_ts {
        return None;
    }
    // What does the write change? Against the database's previous state
    // when known; otherwise against the file, where a field the write does
    // not carry is someone else's addition, not a removal.
    let changes = match base.and_then(Value::as_object) {
        Some(b) => b
            .keys()
            .chain(inc.keys())
            .filter(|k| !STAMP_FIELDS.contains(&k.as_str()))
            .any(|k| b.get(k) != inc.get(k)),
        None => inc
            .iter()
            .filter(|(k, _)| !STAMP_FIELDS.contains(&k.as_str()))
            .any(|(k, v)| ex.get(k) != Some(v)),
    };
    if !changes {
        return None;
    }
    let field = ["deleted_at", "updated_at", "created_at"]
        .into_iter()
        .find(|f| inc.contains_key(*f))?;
    let stamp = (ex_ts + chrono::Duration::milliseconds(1))
        .with_timezone(&chrono::Local)
        .to_rfc3339();
    incoming[field] = Value::String(stamp.clone());
    (field == "updated_at").then_some(stamp)
}

/// Insert `rec` under `key`, reporting whether the document changed.
fn put<T: PartialEq>(map: &mut BTreeMap<String, T>, key: String, rec: T) -> bool {
    if map.get(&key) == Some(&rec) {
        return false;
    }
    map.insert(key, rec);
    true
}

// ============================================================================
// The store
// ============================================================================

/// What the graph file looked like when we last read it, so a write by
/// another process is noticed instead of silently clobbered.
type Stamp = Option<(u64, Option<SystemTime>)>;

fn stamp_of(path: &Path) -> Stamp {
    fs::metadata(path)
        .ok()
        .map(|m| (m.len(), m.modified().ok()))
}

/// The document as this process currently holds it.
#[derive(Debug, Default)]
struct Cache {
    doc: Option<GraphDoc>,
    stamp: Stamp,
    /// Depth of open batches. While it is above zero, mutations stay in
    /// memory and the file is written once, on the way out.
    batch_depth: usize,
    /// Mutations not yet on disk.
    dirty: bool,
}

/// Handle on a project's `.deciduous/graph.json`.
///
/// Cheap to clone: clones share one cached document, so `Database` handing
/// a store to each writer does not mean re-reading the file each time.
#[derive(Debug, Clone)]
pub struct RecordStore {
    path: PathBuf,
    cache: Arc<Mutex<Cache>>,
    /// Resolved on first write (asks git), never on open: `serve` opens the
    /// database on every request.
    author: Arc<OnceLock<String>>,
}

impl RecordStore {
    /// Where the graph file lives for a given database file: next to it
    /// (`.deciduous/deciduous.db` -> `.deciduous/graph.json`). A bare file
    /// name has no directory of its own and gets no store, so a scratch
    /// database never attaches to the project's shared graph.
    pub fn path_for_db(db_path: &Path) -> Option<PathBuf> {
        db_path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(|p| p.join(STORE_FILE_NAME))
    }

    /// Open an existing graph file. Returns `None` if it is absent, which is
    /// how a project that has not enabled sync looks.
    pub fn open(path: impl Into<PathBuf>) -> Option<Self> {
        let path = path.into();
        if path.is_file() {
            Some(Self::at(path))
        } else {
            None
        }
    }

    /// Create the graph file (idempotent) and open it.
    pub fn create(path: impl Into<PathBuf>) -> io::Result<Self> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        if !path.is_file() {
            write_if_changed(&path, &to_stable_json(&GraphDoc::default())?)?;
        }
        Ok(Self::at(path))
    }

    fn at(path: PathBuf) -> Self {
        Self {
            path,
            cache: Arc::default(),
            author: Arc::default(),
        }
    }

    /// Use a fixed author instead of asking git (tests, servers).
    pub fn with_author(mut self, author: impl Into<String>) -> Self {
        let cell = OnceLock::new();
        let _ = cell.set(author.into());
        self.author = Arc::new(cell);
        self
    }

    /// The graph file itself.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The directory the graph file sits in (`.deciduous/`).
    pub fn dir(&self) -> &Path {
        self.path.parent().unwrap_or(Path::new("."))
    }

    pub fn author(&self) -> &str {
        self.author.get_or_init(get_current_author).as_str()
    }

    // ---- the cached document -----------------------------------------------

    /// A poisoned lock means another thread panicked mid-mutation. The
    /// document it left behind is still a valid document, so take it rather
    /// than propagating the panic into an unrelated caller.
    fn lock(&self) -> MutexGuard<'_, Cache> {
        self.cache.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Make sure the cache holds the current document. Unsaved local work
    /// wins: it is newer than anything on disk by definition.
    fn refresh(&self, cache: &mut Cache) -> io::Result<()> {
        if cache.doc.is_some() && (cache.batch_depth > 0 || cache.dirty) {
            return Ok(());
        }
        let stamp = stamp_of(&self.path);
        if cache.doc.is_none() || cache.stamp != stamp {
            cache.doc = Some(load_doc(&self.path)?);
            cache.stamp = stamp;
        }
        Ok(())
    }

    fn flush(&self, cache: &mut Cache) -> io::Result<bool> {
        let Some(doc) = cache.doc.as_ref() else {
            return Ok(false);
        };
        let changed = write_if_changed(&self.path, &to_stable_json(doc)?)?;
        cache.dirty = false;
        cache.stamp = stamp_of(&self.path);
        Ok(changed)
    }

    /// Put `doc` in place of whatever is on disk, without reading it first.
    /// The repair path needs this: the file it is replacing is the one that
    /// will not parse.
    fn replace_doc(&self, doc: GraphDoc) -> io::Result<bool> {
        let mut cache = self.lock();
        cache.doc = Some(doc);
        cache.dirty = true;
        if cache.batch_depth == 0 {
            self.flush(&mut cache)
        } else {
            Ok(true)
        }
    }

    fn read_doc<R>(&self, f: impl FnOnce(&GraphDoc) -> R) -> io::Result<R> {
        let mut cache = self.lock();
        self.refresh(&mut cache)?;
        let doc = cache.doc.as_ref().expect("refresh leaves a document");
        Ok(f(doc))
    }

    /// Apply `f` to the document. `f` reports whether it changed anything;
    /// if it did, the file is rewritten unless a batch is open.
    fn mutate(&self, f: impl FnOnce(&mut GraphDoc) -> io::Result<bool>) -> io::Result<bool> {
        let mut cache = self.lock();
        self.refresh(&mut cache)?;
        let doc = cache.doc.as_mut().expect("refresh leaves a document");
        if !f(doc)? {
            return Ok(false);
        }
        cache.dirty = true;
        if cache.batch_depth == 0 {
            self.flush(&mut cache)?;
        }
        Ok(true)
    }

    /// Run `f` with the file written once at the end instead of once per
    /// record. Without this a bulk export rewrites the whole document for
    /// every node in it, which is quadratic and shows up immediately: a
    /// first `deciduous sync` of a real graph exports thousands of records.
    ///
    /// Writes `f` made are flushed even if `f` returns an error — they
    /// happened, and dropping them would leave the database ahead of the
    /// file with no record of it.
    pub fn batch<R>(&self, f: impl FnOnce() -> R) -> io::Result<R> {
        // Deliberately does not read the file here: `reconcile` opens a
        // batch *before* repairing a file left with conflict markers, which
        // is exactly the file that will not parse.
        self.lock().batch_depth += 1;
        let out = f();
        let mut cache = self.lock();
        cache.batch_depth = cache.batch_depth.saturating_sub(1);
        if cache.batch_depth == 0 && cache.dirty {
            self.flush(&mut cache)?;
        }
        Ok(out)
    }

    // ---- writes ------------------------------------------------------------

    /// Write a node record as given, replacing whatever is there.
    pub fn write_node(&self, rec: &NodeRecord) -> io::Result<bool> {
        self.mutate(|doc| Ok(put(&mut doc.nodes, rec.change_id.clone(), rec.clone())))
    }

    pub fn write_edge(&self, rec: &EdgeRecord) -> io::Result<bool> {
        self.mutate(|doc| Ok(put(&mut doc.edges, rec.edge_id.clone(), rec.clone())))
    }

    pub fn write_theme(&self, rec: &ThemeRecord) -> io::Result<bool> {
        self.mutate(|doc| Ok(put(&mut doc.themes, rec.change_id.clone(), rec.clone())))
    }

    pub fn write_tag(&self, rec: &TagRecord) -> io::Result<bool> {
        let key = tag_id(&rec.node_change_id, &rec.theme_change_id);
        self.mutate(|doc| Ok(put(&mut doc.tags, key, rec.clone())))
    }

    /// Write a record produced by a local mutation, merged with whatever is
    /// already in the document. That entry may hold a teammate's version
    /// that was pulled but not yet synced into the database; overwriting it
    /// would lose their fields. Returns whether the file changed and, if the
    /// write had to be restamped past a newer-looking version, its new
    /// `updated_at`.
    fn write_merged<T>(
        &self,
        pick: impl FnOnce(&mut GraphDoc) -> &mut BTreeMap<String, T>,
        key: String,
        rec: &T,
        base: Option<&T>,
    ) -> io::Result<(bool, Option<String>)>
    where
        T: Serialize + for<'de> Deserialize<'de> + PartialEq,
    {
        let mut restamped = None;
        let changed = self.mutate(|doc| {
            let map = pick(doc);
            let (merged, stamp) = merged_record(map.get(&key), rec, base)?;
            restamped = stamp;
            Ok(put(map, key, merged))
        })?;
        Ok((changed, restamped))
    }

    /// Publish a live node from the database.
    pub fn publish_node(&self, node: &DecisionNode) -> io::Result<bool> {
        Ok(self.publish_node_edit(None, node)?.0)
    }

    /// Publish a node the database just changed. `before` is the row as it
    /// was before the change. Returns whether the file changed, and the
    /// `updated_at` the database must take if the write had to be moved
    /// past a version in the file stamped later than this clock.
    pub fn publish_node_edit(
        &self,
        before: Option<&DecisionNode>,
        node: &DecisionNode,
    ) -> io::Result<(bool, Option<String>)> {
        let rec = NodeRecord::from_db(node, Some(self.author()));
        let base = before.map(|b| NodeRecord::from_db(b, None));
        self.write_merged(|d| &mut d.nodes, rec.change_id.clone(), &rec, base.as_ref())
    }

    /// Mark a node deleted. Keeps the last known fields so history stays
    /// readable and a later edit can resurrect it.
    pub fn tombstone_node(&self, node: &DecisionNode) -> io::Result<bool> {
        let mut rec = NodeRecord::from_db(node, Some(self.author()));
        rec.deleted_at = Some(now_ts());
        let base = NodeRecord::from_db(node, None);
        Ok(self
            .write_merged(|d| &mut d.nodes, rec.change_id.clone(), &rec, Some(&base))?
            .0)
    }

    /// Publish a live edge. Legacy edges without change ids are skipped.
    pub fn publish_edge(&self, edge: &DecisionEdge) -> io::Result<bool> {
        match EdgeRecord::from_db(edge, Some(self.author())) {
            Some(rec) => Ok(self
                .write_merged(|d| &mut d.edges, rec.edge_id.clone(), &rec, None)?
                .0),
            None => Ok(false),
        }
    }

    pub fn tombstone_edge(&self, edge: &DecisionEdge) -> io::Result<bool> {
        match EdgeRecord::from_db(edge, Some(self.author())) {
            Some(mut rec) => {
                rec.deleted_at = Some(now_ts());
                Ok(self
                    .write_merged(|d| &mut d.edges, rec.edge_id.clone(), &rec, None)?
                    .0)
            }
            None => Ok(false),
        }
    }

    pub fn publish_theme(&self, theme: &Theme) -> io::Result<bool> {
        Ok(self.publish_theme_edit(theme)?.0)
    }

    /// Like [`Self::publish_node_edit`], for a theme.
    pub fn publish_theme_edit(&self, theme: &Theme) -> io::Result<(bool, Option<String>)> {
        let rec = ThemeRecord::from_db(theme, Some(self.author()));
        self.write_merged(|d| &mut d.themes, rec.change_id.clone(), &rec, None)
    }

    pub fn tombstone_theme(&self, theme: &Theme) -> io::Result<bool> {
        let mut rec = ThemeRecord::from_db(theme, Some(self.author()));
        rec.deleted_at = Some(now_ts());
        Ok(self
            .write_merged(|d| &mut d.themes, rec.change_id.clone(), &rec, None)?
            .0)
    }

    pub fn publish_tag(
        &self,
        node_change_id: &str,
        theme_change_id: &str,
        source: &str,
        created_at: &str,
    ) -> io::Result<bool> {
        let rec = TagRecord {
            node_change_id: node_change_id.to_string(),
            theme_change_id: theme_change_id.to_string(),
            source: source.to_string(),
            created_at: created_at.to_string(),
            author: Some(self.author().to_string()),
            deleted_at: None,
            extra: Default::default(),
        };
        let key = tag_id(node_change_id, theme_change_id);
        Ok(self.write_merged(|d| &mut d.tags, key, &rec, None)?.0)
    }

    /// Two people created a theme with the same name before syncing. The
    /// smaller change_id is canonical everywhere; this retires the other:
    /// its theme record becomes a tombstone and its tag records are rewritten
    /// under the canonical id.
    pub fn retire_theme_id(&self, old: &str, canonical: &str) -> io::Result<usize> {
        let author = self.author().to_string();
        let now = now_ts();
        let mut moved = 0;
        self.mutate(|doc| {
            let mut changed = false;
            if let Some(theme) = doc.themes.get_mut(old) {
                if !theme.is_tombstone() {
                    theme.deleted_at = Some(now.clone());
                    theme.author = Some(author.clone());
                    changed = true;
                }
            }
            let stale: Vec<TagRecord> = doc
                .tags
                .values()
                .filter(|t| t.theme_change_id == old && !t.is_tombstone())
                .cloned()
                .collect();
            for tag in stale {
                let mut dead = tag.clone();
                dead.deleted_at = Some(now.clone());
                changed |= put(
                    &mut doc.tags,
                    tag_id(&dead.node_change_id, &dead.theme_change_id),
                    dead,
                );
                let key = tag_id(&tag.node_change_id, canonical);
                if !doc.tags.contains_key(&key) {
                    let mut alive = tag.clone();
                    alive.theme_change_id = canonical.to_string();
                    alive.author = Some(author.clone());
                    changed |= put(&mut doc.tags, key, alive);
                }
                moved += 1;
            }
            Ok(changed)
        })?;
        Ok(moved)
    }

    pub fn tombstone_tag(&self, node_change_id: &str, theme_change_id: &str) -> io::Result<bool> {
        let author = self.author().to_string();
        let now = now_ts();
        let key = tag_id(node_change_id, theme_change_id);
        self.mutate(|doc| {
            let mut rec = doc.tags.get(&key).cloned().unwrap_or(TagRecord {
                node_change_id: node_change_id.to_string(),
                theme_change_id: theme_change_id.to_string(),
                source: "manual".to_string(),
                created_at: now.clone(),
                author: None,
                deleted_at: None,
                extra: Default::default(),
            });
            // Later than whatever version is there, even one stamped ahead
            // of this clock (see restamp_local_write).
            let prev = serde_json::to_value(&rec)
                .map(|v| record_ts(&v))
                .unwrap_or_default();
            let deleted = if parse_ts(&now) > prev {
                now
            } else {
                (prev + chrono::Duration::milliseconds(1))
                    .with_timezone(&chrono::Local)
                    .to_rfc3339()
            };
            rec.author = Some(author);
            rec.deleted_at = Some(deleted);
            Ok(put(&mut doc.tags, key, rec))
        })
    }

    // ---- reads -------------------------------------------------------------

    pub fn read_node(&self, change_id: &str) -> io::Result<Option<NodeRecord>> {
        self.read_doc(|doc| doc.nodes.get(change_id).cloned())
    }

    pub fn read_edge(&self, edge_id: &str) -> io::Result<Option<EdgeRecord>> {
        self.read_doc(|doc| doc.edges.get(edge_id).cloned())
    }

    pub fn read_tag(
        &self,
        node_change_id: &str,
        theme_change_id: &str,
    ) -> io::Result<Option<TagRecord>> {
        let key = tag_id(node_change_id, theme_change_id);
        self.read_doc(|doc| doc.tags.get(&key).cloned())
    }

    /// The whole document. Used by the merge driver and by tests.
    pub fn read_doc_all(&self) -> io::Result<GraphDoc> {
        self.read_doc(|doc| doc.clone())
    }

    fn read_kind<T: Clone + RecordIdentity>(
        &self,
        pick: impl FnOnce(&GraphDoc) -> &BTreeMap<String, T>,
    ) -> StoreRead<T> {
        match self.read_doc(|doc| read_map(&self.path, pick(doc))) {
            Ok(read) => read,
            Err(e) => StoreRead {
                records: Vec::new(),
                errors: vec![ReadError {
                    path: self.path.display().to_string(),
                    id: None,
                    message: e.to_string(),
                }],
            },
        }
    }

    pub fn read_nodes(&self) -> StoreRead<NodeRecord> {
        self.read_kind(|doc| &doc.nodes)
    }

    pub fn read_edges(&self) -> StoreRead<EdgeRecord> {
        self.read_kind(|doc| &doc.edges)
    }

    pub fn read_themes(&self) -> StoreRead<ThemeRecord> {
        self.read_kind(|doc| &doc.themes)
    }

    pub fn read_tags(&self) -> StoreRead<TagRecord> {
        self.read_kind(|doc| &doc.tags)
    }

    /// Number of records per kind (live and tombstoned alike).
    pub fn counts(&self) -> StoreCounts {
        self.read_doc(|doc| StoreCounts {
            nodes: doc.nodes.len(),
            edges: doc.edges.len(),
            themes: doc.themes.len(),
            tags: doc.tags.len(),
        })
        .unwrap_or_default()
    }

    // ---- the 0.17 per-record directory -------------------------------------

    fn legacy_dir(&self) -> PathBuf {
        self.dir().join(STORE_DIR_NAME)
    }

    /// True if 0.17's `.deciduous/sync/` directory of per-record files is
    /// still present.
    pub fn has_legacy_record_dir(&self) -> bool {
        ["nodes", "edges", "themes", "tags"]
            .iter()
            .any(|sub| self.legacy_dir().join(sub).is_dir())
    }

    /// Fold `.deciduous/sync/<kind>/<id>.json` into the graph file and
    /// remove the directory.
    ///
    /// A record already in the document wins only if it is newer; otherwise
    /// the directory's version is taken, so running this after a pull that
    /// still carried the old layout does not lose the pulled work. The
    /// directory is removed only when every file in it was read, so a file
    /// that will not parse keeps the whole migration around to retry.
    pub fn import_legacy_record_dir(&self) -> io::Result<LegacyImport> {
        let mut report = LegacyImport::default();
        if !self.has_legacy_record_dir() {
            return Ok(report);
        }
        let dir = self.legacy_dir();

        let nodes = read_legacy_dir::<NodeRecord>(&dir.join("nodes"));
        let edges = read_legacy_dir::<EdgeRecord>(&dir.join("edges"));
        let themes = read_legacy_dir::<ThemeRecord>(&dir.join("themes"));
        let tags = read_legacy_dir::<TagRecord>(&dir.join("tags"));
        for read in [&nodes.errors, &edges.errors, &themes.errors, &tags.errors] {
            report.errors.extend(read.iter().map(|e| e.to_string()));
        }

        self.mutate(|doc| {
            let mut changed = false;
            for rec in &nodes.records {
                let keep = match doc.nodes.get(&rec.change_id) {
                    Some(have) => rec.effective_ts() > have.effective_ts(),
                    None => true,
                };
                if keep && put(&mut doc.nodes, rec.change_id.clone(), rec.clone()) {
                    changed = true;
                    report.nodes += 1;
                }
            }
            for rec in &edges.records {
                if !doc.edges.contains_key(&rec.edge_id)
                    && put(&mut doc.edges, rec.edge_id.clone(), rec.clone())
                {
                    changed = true;
                    report.edges += 1;
                }
            }
            for rec in &themes.records {
                let keep = match doc.themes.get(&rec.change_id) {
                    Some(have) => rec.effective_ts() > have.effective_ts(),
                    None => true,
                };
                if keep && put(&mut doc.themes, rec.change_id.clone(), rec.clone()) {
                    changed = true;
                    report.themes += 1;
                }
            }
            for rec in &tags.records {
                let key = tag_id(&rec.node_change_id, &rec.theme_change_id);
                if !doc.tags.contains_key(&key) && put(&mut doc.tags, key, rec.clone()) {
                    changed = true;
                    report.tags += 1;
                }
            }
            Ok(changed)
        })?;

        if report.errors.is_empty() {
            fs::remove_dir_all(&dir)?;
            report.removed = true;
        }
        Ok(report)
    }

    // ---- legacy event logs -------------------------------------------------

    fn legacy_events_dir(&self) -> PathBuf {
        self.legacy_dir().join("events")
    }

    fn legacy_checkpoint_path(&self) -> PathBuf {
        self.legacy_dir().join("checkpoint.json")
    }

    /// True if the pre-0.17 JSONL event log or checkpoint is still present.
    pub fn has_legacy_events(&self) -> bool {
        self.legacy_events_dir().is_dir() || self.legacy_checkpoint_path().is_file()
    }

    /// Convert the old per-author JSONL logs and checkpoint into records.
    ///
    /// Reads tolerantly: lines that hold several concatenated JSON objects
    /// (the old appender could interleave under concurrency) are split and
    /// every object recovered. Existing records are only overwritten when
    /// the event log has a newer version. The legacy files are removed only
    /// if every line parsed, so nothing is ever silently dropped.
    pub fn import_legacy_events(&self) -> io::Result<LegacyImport> {
        use crate::events::{read_checkpoint, read_events_tolerant, MaterializedState};

        let mut report = LegacyImport::default();
        if !self.has_legacy_events() {
            return Ok(report);
        }

        let mut cutoff: Option<DateTime<Utc>> = None;
        let mut state = match read_checkpoint(&self.legacy_checkpoint_path()) {
            Ok(Some(cp)) => {
                report.checkpoint = true;
                cutoff = Some(cp.created_at);
                MaterializedState::from_checkpoint(&cp)
            }
            Ok(None) => MaterializedState::default(),
            Err(e) => {
                report.errors.push(format!("checkpoint.json: {}", e));
                MaterializedState::default()
            }
        };

        let (mut events, errors) = read_events_tolerant(&self.legacy_events_dir());
        // The old rebuild replayed only events newer than the checkpoint;
        // older ones are already folded in and would regress its state.
        if let Some(cutoff) = cutoff {
            events.retain(|e| e.timestamp() > cutoff);
        }
        report.events = events.len();
        report.errors.extend(errors);
        state.replay(&events);

        let into_record =
            |node: &crate::events::MaterializedNode, deleted_at: Option<String>| NodeRecord {
                change_id: node.change_id.clone(),
                node_type: node.node_type.clone(),
                title: node.title.clone(),
                description: node.description.clone(),
                status: node.status.clone(),
                metadata: node.metadata_json.as_deref().map(parse_metadata),
                created_at: node.created_at.to_rfc3339(),
                updated_at: node.updated_at.to_rfc3339(),
                author: node.author.clone(),
                deleted_at,
                extra: Default::default(),
            };

        self.mutate(|doc| {
            let mut changed = false;
            let mut take_node = |rec: NodeRecord| {
                let keep = match doc.nodes.get(&rec.change_id) {
                    Some(have) => rec.effective_ts() > have.effective_ts(),
                    None => true,
                };
                if keep && put(&mut doc.nodes, rec.change_id.clone(), rec) {
                    changed = true;
                    report.nodes += 1;
                }
            };
            for node in state.nodes.values() {
                take_node(into_record(node, None));
            }
            for (node, deleted_at) in state.tombstoned_nodes.values() {
                take_node(into_record(node, Some(deleted_at.to_rfc3339())));
            }

            for edge in state.edges.values() {
                let rec = EdgeRecord {
                    edge_id: edge_id(&edge.from_change_id, &edge.to_change_id, &edge.edge_type),
                    from_change_id: edge.from_change_id.clone(),
                    to_change_id: edge.to_change_id.clone(),
                    edge_type: edge.edge_type.clone(),
                    rationale: edge.rationale.clone(),
                    weight: None,
                    created_at: edge.created_at.to_rfc3339(),
                    author: edge.author.clone(),
                    deleted_at: None,
                    extra: Default::default(),
                };
                if !doc.edges.contains_key(&rec.edge_id)
                    && put(&mut doc.edges, rec.edge_id.clone(), rec)
                {
                    changed = true;
                    report.edges += 1;
                }
            }

            for (edge, deleted_at) in state.tombstoned_edges.values() {
                let rec = EdgeRecord {
                    edge_id: edge_id(&edge.from_change_id, &edge.to_change_id, &edge.edge_type),
                    from_change_id: edge.from_change_id.clone(),
                    to_change_id: edge.to_change_id.clone(),
                    edge_type: edge.edge_type.clone(),
                    rationale: edge.rationale.clone(),
                    weight: None,
                    created_at: edge.created_at.to_rfc3339(),
                    author: edge.author.clone(),
                    deleted_at: Some(deleted_at.to_rfc3339()),
                    extra: Default::default(),
                };
                let keep = match doc.edges.get(&rec.edge_id) {
                    Some(existing) => {
                        !existing.is_tombstone() && parse_ts(&existing.created_at) <= *deleted_at
                    }
                    None => true,
                };
                if keep && put(&mut doc.edges, rec.edge_id.clone(), rec) {
                    changed = true;
                    report.edges += 1;
                }
            }
            Ok(changed)
        })?;

        if report.errors.is_empty() {
            let events_dir = self.legacy_events_dir();
            if events_dir.is_dir() {
                fs::remove_dir_all(&events_dir)?;
            }
            let cp = self.legacy_checkpoint_path();
            if cp.is_file() {
                fs::remove_file(&cp)?;
            }
            report.removed = true;
        }

        Ok(report)
    }
}

/// Read one directory of the 0.17 layout, rejecting any file whose name
/// disagrees with the record inside it.
fn read_legacy_dir<T: for<'de> Deserialize<'de> + RecordIdentity>(dir: &Path) -> StoreRead<T> {
    let mut out = StoreRead::default();
    let Ok(entries) = fs::read_dir(dir) else {
        return out;
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|e| e == "json").unwrap_or(false))
        .collect();
    paths.sort();
    for path in paths {
        let err = |message: String| ReadError {
            path: path.display().to_string(),
            id: None,
            message,
        };
        match fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<T>(&text) {
                Ok(rec) => {
                    let expected = legacy_stem(&rec.record_stem());
                    let actual = path
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default();
                    if actual == expected {
                        out.records.push(rec);
                    } else {
                        out.errors.push(err(format!(
                            "file name does not match the record inside it (expected {}.json)",
                            expected
                        )));
                    }
                }
                Err(e) => out.errors.push(err(e.to_string())),
            },
            Err(e) => out.errors.push(err(e.to_string())),
        }
    }
    out
}

/// How 0.17 turned a record id into a file name.
fn legacy_stem(id: &str) -> String {
    let cleaned: String = id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() || cleaned.starts_with('.') {
        format!("_{}", cleaned)
    } else {
        cleaned
    }
}

/// Record counts by kind.
#[derive(Debug, Clone, Default, Serialize)]
pub struct StoreCounts {
    pub nodes: usize,
    pub edges: usize,
    pub themes: usize,
    pub tags: usize,
}

/// What an import of a superseded layout did.
#[derive(Debug, Clone, Default, Serialize)]
pub struct LegacyImport {
    pub checkpoint: bool,
    pub events: usize,
    pub nodes: usize,
    pub edges: usize,
    pub themes: usize,
    pub tags: usize,
    pub errors: Vec<String>,
    /// Legacy files were deleted (only when everything parsed).
    pub removed: bool,
}

// ============================================================================
// Reconcile: store <-> database
// ============================================================================

/// What `deciduous sync` changed (or, with `dry_run`, would change).
#[derive(Debug, Clone, Default, Serialize)]
pub struct SyncReport {
    pub dry_run: bool,
    pub nodes_imported: usize,
    pub nodes_updated: usize,
    pub nodes_deleted: usize,
    pub nodes_exported: usize,
    pub edges_imported: usize,
    pub edges_deleted: usize,
    pub edges_exported: usize,
    /// Edges whose endpoint is not in the store or the database yet. They
    /// import on a later sync once the node arrives.
    pub edges_pending: usize,
    /// Edges that point at a tombstoned node; skipped.
    pub edges_orphaned: usize,
    pub themes_imported: usize,
    pub themes_updated: usize,
    pub themes_deleted: usize,
    pub themes_exported: usize,
    pub tags_imported: usize,
    pub tags_deleted: usize,
    pub tags_exported: usize,
    /// Files that could not be parsed; everything else still synced.
    pub read_errors: Vec<ReadError>,
    /// One line per pending edge, for the human.
    pub pending_details: Vec<String>,
    /// Same-named themes created on two machines that were folded into one.
    pub themes_merged: usize,
    /// Records the database refused (constraint violations and the like).
    /// The rest of the run still completes.
    pub errors: Vec<String>,
    /// Record files that carried git conflict markers: merged (or not) by
    /// this run, or (dry run) still waiting to be merged.
    pub conflicts: Vec<ConflictRepair>,
}

impl SyncReport {
    /// Records pulled from the store into the database.
    pub fn imported(&self) -> usize {
        self.nodes_imported
            + self.nodes_updated
            + self.nodes_deleted
            + self.edges_imported
            + self.edges_deleted
            + self.themes_imported
            + self.themes_updated
            + self.themes_deleted
            + self.tags_imported
            + self.tags_deleted
            + self.themes_merged
    }

    /// Records pushed from the database into the store.
    pub fn exported(&self) -> usize {
        self.nodes_exported + self.edges_exported + self.themes_exported + self.tags_exported
    }

    /// Nothing moved in either direction.
    pub fn is_clean(&self) -> bool {
        self.imported() == 0 && self.exported() == 0
    }
}

/// Make the database and the store agree.
///
/// For every record the newer side wins, judged by `updated_at` (or
/// `deleted_at` for tombstones). Records only in the store are imported;
/// rows only in the database are exported. Edges import once both endpoints
/// exist locally. With `dry_run`, nothing is written on either side.
pub fn reconcile(
    db: &Database,
    store: &RecordStore,
    dry_run: bool,
) -> std::result::Result<SyncReport, String> {
    // One write at the end, not one per record: a first sync of a real
    // graph exports thousands of them.
    match store.batch(|| reconcile_inner(db, store, dry_run)) {
        Ok(report) => report,
        Err(e) => Err(format!("record store: {}", e)),
    }
}

fn reconcile_inner(
    db: &Database,
    store: &RecordStore,
    dry_run: bool,
) -> std::result::Result<SyncReport, String> {
    let mut report = SyncReport {
        dry_run,
        ..Default::default()
    };
    let io_err = |e: io::Error| format!("record store: {}", e);
    let db_err = |e: crate::db::DbError| format!("database: {}", e);

    // Files merged by git without the merge driver still carry markers.
    if dry_run {
        report.conflicts = store
            .conflicted_files()
            .into_iter()
            .map(|p| ConflictRepair {
                path: p.display().to_string(),
                merged: false,
                message: Some("has conflict markers; `deciduous sync` will merge it".into()),
            })
            .collect();
    } else {
        report.conflicts = store.repair_conflicted_files().map_err(io_err)?;
    }

    // Everything below compares the document with the database and writes
    // the loser. If the document will not parse there is nothing to compare
    // against, and carrying on would export every local row over it.
    if dry_run && !report.conflicts.is_empty() {
        return Ok(report);
    }
    store.read_doc_all().map_err(io_err)?;

    // ---- nodes -------------------------------------------------------------
    let node_read = store.read_nodes();
    // Records that would not read. Never export over one: whatever is wrong
    // with it is still somebody's data, and `publish_*` would replace it.
    let unreadable_nodes = node_read.bad_ids();
    report.read_errors.extend(node_read.errors);
    let store_nodes: HashMap<String, NodeRecord> = node_read
        .records
        .into_iter()
        .map(|r| (r.change_id.clone(), r))
        .collect();

    let db_nodes = db.get_all_nodes().map_err(db_err)?;
    let mut db_by_change: HashMap<String, DecisionNode> = db_nodes
        .into_iter()
        .map(|n| (n.change_id.clone(), n))
        .collect();

    let mut tombstoned_nodes: HashSet<String> = HashSet::new();

    for (change_id, rec) in &store_nodes {
        match db_by_change.get(change_id) {
            None => {
                if rec.is_tombstone() {
                    tombstoned_nodes.insert(change_id.clone());
                } else {
                    if !dry_run {
                        if let Err(e) = db.import_node_record(rec) {
                            report
                                .errors
                                .push(format!("node {}: {}", short(change_id), e));
                            continue;
                        }
                    }
                    report.nodes_imported += 1;
                }
            }
            Some(row) => {
                let row_ts = parse_ts(&row.updated_at);
                if rec.is_tombstone() {
                    if rec.effective_ts() >= row_ts {
                        if !dry_run {
                            db.delete_node_local(row.id).map_err(db_err)?;
                        }
                        tombstoned_nodes.insert(change_id.clone());
                        report.nodes_deleted += 1;
                    } else {
                        // Edited locally after someone deleted it: resurrect.
                        if !dry_run {
                            store.publish_node(row).map_err(io_err)?;
                        }
                        report.nodes_exported += 1;
                    }
                } else {
                    let rec_ts = parse_ts(&rec.updated_at);
                    // On a tie the store wins: a merge driver result keeps
                    // the winning side's updated_at but carries fields from
                    // both sides, and the database has only ours.
                    // created_at is never rewritten by an import and author is
                    // attribution, so neither counts as a content difference.
                    let same_content = {
                        let mine = NodeRecord::from_db(row, None);
                        let mut theirs = rec.clone();
                        theirs.author = None;
                        theirs.created_at = mine.created_at.clone();
                        // Fields the database cannot hold are not a
                        // difference it could ever resolve.
                        theirs.extra.clear();
                        mine == theirs
                    };
                    if rec_ts > row_ts || (rec_ts == row_ts && !same_content) {
                        if !dry_run {
                            db.update_node_record(row.id, rec).map_err(db_err)?;
                        }
                        report.nodes_updated += 1;
                    } else if row_ts > rec_ts {
                        if !dry_run {
                            store.publish_node(row).map_err(io_err)?;
                        }
                        report.nodes_exported += 1;
                    }
                }
            }
        }
    }

    for (change_id, row) in &db_by_change {
        if !store_nodes.contains_key(change_id) && !unreadable_nodes.contains(change_id) {
            if !dry_run {
                store.publish_node(row).map_err(io_err)?;
            }
            report.nodes_exported += 1;
        }
    }

    // Refresh the id map after imports/deletes (real run) or predict it (dry run).
    let local_ids: HashMap<String, i32> = if dry_run {
        db_by_change.retain(|cid, _| !tombstoned_nodes.contains(cid));
        let mut ids: HashMap<String, i32> = db_by_change
            .iter()
            .map(|(cid, n)| (cid.clone(), n.id))
            .collect();
        for (cid, rec) in &store_nodes {
            if !rec.is_tombstone() && !ids.contains_key(cid) {
                ids.insert(cid.clone(), -1);
            }
        }
        ids
    } else {
        db.get_all_nodes()
            .map_err(db_err)?
            .into_iter()
            .map(|n| (n.change_id, n.id))
            .collect()
    };

    // ---- themes ------------------------------------------------------------
    // Theme names are unique per database. Two machines that each created
    // "infra" before syncing hold two change_ids for one name; fold them
    // deterministically (smaller change_id wins) before importing, or the
    // UNIQUE constraint would abort the run every time.
    {
        let pre = store.read_themes();
        let db_by_name: HashMap<String, Theme> = db
            .get_all_themes()
            .map_err(db_err)?
            .into_iter()
            .map(|t| (t.name.clone(), t))
            .collect();
        let known: HashSet<String> = db_by_name.values().map(|t| t.change_id.clone()).collect();
        for rec in pre.records.iter().filter(|r| !r.is_tombstone()) {
            let Some(local) = db_by_name.get(&rec.name) else {
                continue;
            };
            if known.contains(&rec.change_id) || local.change_id == rec.change_id {
                continue;
            }
            if !dry_run {
                if rec.change_id < local.change_id {
                    // Remote id is canonical: our row adopts it, our old
                    // record and tag files retire.
                    db.update_theme_change_id(local.id, &rec.change_id)
                        .map_err(db_err)?;
                    store
                        .retire_theme_id(&local.change_id, &rec.change_id)
                        .map_err(io_err)?;
                } else {
                    store
                        .retire_theme_id(&rec.change_id, &local.change_id)
                        .map_err(io_err)?;
                }
            }
            report.themes_merged += 1;
        }
    }

    let theme_read = store.read_themes();
    let unreadable_themes = theme_read.bad_ids();
    report.read_errors.extend(theme_read.errors);
    let store_themes: HashMap<String, ThemeRecord> = theme_read
        .records
        .into_iter()
        .map(|r| (r.change_id.clone(), r))
        .collect();
    let db_themes = db.get_all_themes().map_err(db_err)?;
    let db_theme_by_change: HashMap<String, Theme> = db_themes
        .into_iter()
        .map(|t| (t.change_id.clone(), t))
        .collect();
    let mut tombstoned_themes: HashSet<String> = HashSet::new();

    for (change_id, rec) in &store_themes {
        match db_theme_by_change.get(change_id) {
            None => {
                if rec.is_tombstone() {
                    tombstoned_themes.insert(change_id.clone());
                } else {
                    if !dry_run {
                        if let Err(e) = db.import_theme_record(rec) {
                            report.errors.push(format!(
                                "theme '{}' ({}): {}",
                                rec.name,
                                short(change_id),
                                e
                            ));
                            continue;
                        }
                    }
                    report.themes_imported += 1;
                }
            }
            Some(row) => {
                let row_ts = parse_ts(&row.updated_at);
                if rec.is_tombstone() {
                    if rec.effective_ts() >= row_ts {
                        if !dry_run {
                            db.delete_theme_local(row.id).map_err(db_err)?;
                        }
                        tombstoned_themes.insert(change_id.clone());
                        report.themes_deleted += 1;
                    } else {
                        if !dry_run {
                            store.publish_theme(row).map_err(io_err)?;
                        }
                        report.themes_exported += 1;
                    }
                } else {
                    let rec_ts = parse_ts(&rec.updated_at);
                    // created_at is never rewritten by an import and author is
                    // attribution, so neither counts as a content difference.
                    let same_content = {
                        let mine = ThemeRecord::from_db(row, None);
                        let mut theirs = rec.clone();
                        theirs.author = None;
                        theirs.created_at = mine.created_at.clone();
                        // Fields the database cannot hold are not a
                        // difference it could ever resolve.
                        theirs.extra.clear();
                        mine == theirs
                    };
                    if rec_ts > row_ts || (rec_ts == row_ts && !same_content) {
                        if !dry_run {
                            db.update_theme_record(row.id, rec).map_err(db_err)?;
                        }
                        report.themes_updated += 1;
                    } else if row_ts > rec_ts {
                        if !dry_run {
                            store.publish_theme(row).map_err(io_err)?;
                        }
                        report.themes_exported += 1;
                    }
                }
            }
        }
    }
    for (change_id, row) in &db_theme_by_change {
        if !store_themes.contains_key(change_id) && !unreadable_themes.contains(change_id) {
            if !dry_run {
                store.publish_theme(row).map_err(io_err)?;
            }
            report.themes_exported += 1;
        }
    }

    let theme_ids: HashMap<String, i32> = if dry_run {
        let mut ids: HashMap<String, i32> = db_theme_by_change
            .iter()
            .filter(|(cid, _)| !tombstoned_themes.contains(*cid))
            .map(|(cid, t)| (cid.clone(), t.id))
            .collect();
        for (cid, rec) in &store_themes {
            if !rec.is_tombstone() && !ids.contains_key(cid) {
                ids.insert(cid.clone(), -1);
            }
        }
        ids
    } else {
        db.get_all_themes()
            .map_err(db_err)?
            .into_iter()
            .map(|t| (t.change_id, t.id))
            .collect()
    };

    // ---- edges -------------------------------------------------------------
    let edge_read = store.read_edges();
    let unreadable_edges = edge_read.bad_ids();
    report.read_errors.extend(edge_read.errors);
    let store_edges: HashMap<String, EdgeRecord> = edge_read
        .records
        .into_iter()
        .map(|r| (r.edge_id.clone(), r))
        .collect();

    let db_edges = db.get_all_edges().map_err(db_err)?;
    let mut db_edge_by_key: HashMap<String, DecisionEdge> = HashMap::new();
    for e in db_edges {
        if let (Some(f), Some(t)) = (&e.from_change_id, &e.to_change_id) {
            db_edge_by_key.insert(edge_id(f, t, &e.edge_type), e);
        }
    }

    for (eid, rec) in &store_edges {
        match db_edge_by_key.get(eid) {
            None => {
                if rec.is_tombstone() {
                    continue;
                }
                let from_dead = tombstoned_nodes.contains(&rec.from_change_id);
                let to_dead = tombstoned_nodes.contains(&rec.to_change_id);
                if from_dead || to_dead {
                    report.edges_orphaned += 1;
                    continue;
                }
                match (
                    local_ids.get(&rec.from_change_id),
                    local_ids.get(&rec.to_change_id),
                ) {
                    (Some(&from_id), Some(&to_id)) => {
                        if !dry_run {
                            if let Err(e) = db.import_edge_record(from_id, to_id, rec) {
                                report.errors.push(format!("edge {}: {}", short(eid), e));
                                continue;
                            }
                        }
                        report.edges_imported += 1;
                    }
                    _ => {
                        report.edges_pending += 1;
                        let missing = if local_ids.contains_key(&rec.from_change_id) {
                            &rec.to_change_id
                        } else {
                            &rec.from_change_id
                        };
                        report.pending_details.push(format!(
                            "edge {} -> {} ({}) waits for node {}",
                            short(&rec.from_change_id),
                            short(&rec.to_change_id),
                            rec.edge_type,
                            short(missing)
                        ));
                    }
                }
            }
            Some(row) => {
                if rec.is_tombstone() {
                    let deleted = rec.deleted_at.as_deref().map(parse_ts).unwrap_or_default();
                    if deleted >= parse_ts(&row.created_at) {
                        if !dry_run {
                            db.delete_edge_local(row.id).map_err(db_err)?;
                        }
                        report.edges_deleted += 1;
                    } else {
                        if !dry_run {
                            store.publish_edge(row).map_err(io_err)?;
                        }
                        report.edges_exported += 1;
                    }
                }
            }
        }
    }
    for (eid, row) in &db_edge_by_key {
        if !store_edges.contains_key(eid) && !unreadable_edges.contains(eid) {
            if !dry_run {
                store.publish_edge(row).map_err(io_err)?;
            }
            report.edges_exported += 1;
        }
    }

    // ---- tags --------------------------------------------------------------
    let tag_read = store.read_tags();
    let unreadable_tags = tag_read.bad_ids();
    report.read_errors.extend(tag_read.errors);
    let store_tags: HashMap<String, TagRecord> = tag_read
        .records
        .into_iter()
        .map(|r| (tag_id(&r.node_change_id, &r.theme_change_id), r))
        .collect();

    let node_change_by_id: HashMap<i32, String> =
        local_ids.iter().map(|(c, i)| (*i, c.clone())).collect();
    let theme_change_by_id: HashMap<i32, String> =
        theme_ids.iter().map(|(c, i)| (*i, c.clone())).collect();

    let db_tags: Vec<NodeTheme> = db.get_all_node_themes().map_err(db_err)?;
    let mut db_tag_by_key: HashMap<String, (NodeTheme, String, String)> = HashMap::new();
    for t in db_tags {
        if let (Some(n), Some(th)) = (
            node_change_by_id.get(&t.node_id),
            theme_change_by_id.get(&t.theme_id),
        ) {
            db_tag_by_key.insert(tag_id(n, th), (t, n.clone(), th.clone()));
        }
    }

    for (key, rec) in &store_tags {
        match db_tag_by_key.get(key) {
            None => {
                if rec.is_tombstone() {
                    continue;
                }
                if let (Some(&node_id), Some(&theme_id)) = (
                    local_ids.get(&rec.node_change_id),
                    theme_ids.get(&rec.theme_change_id),
                ) {
                    if !dry_run {
                        if let Err(e) = db.import_tag_record(node_id, theme_id, rec) {
                            report.errors.push(format!("tag {}: {}", key, e));
                            continue;
                        }
                    }
                    report.tags_imported += 1;
                }
            }
            Some((row, node_cid, theme_cid)) => {
                if rec.is_tombstone() {
                    let deleted = rec.deleted_at.as_deref().map(parse_ts).unwrap_or_default();
                    if deleted >= parse_ts(&row.created_at) {
                        if !dry_run {
                            db.delete_tag_local(row.node_id, row.theme_id)
                                .map_err(db_err)?;
                        }
                        report.tags_deleted += 1;
                    } else {
                        // Tagged again after someone untagged it: the newer
                        // tag wins and goes back out.
                        if !dry_run {
                            store
                                .publish_tag(node_cid, theme_cid, &row.source, &row.created_at)
                                .map_err(io_err)?;
                        }
                        report.tags_exported += 1;
                    }
                }
            }
        }
    }
    for (key, (row, node_cid, theme_cid)) in &db_tag_by_key {
        if !store_tags.contains_key(key) && !unreadable_tags.contains(key) {
            if !dry_run {
                store
                    .publish_tag(node_cid, theme_cid, &row.source, &row.created_at)
                    .map_err(io_err)?;
            }
            report.tags_exported += 1;
        }
    }

    Ok(report)
}

fn short(change_id: &str) -> &str {
    change_id.get(..8).unwrap_or(change_id)
}

// ============================================================================
// Merging concurrent edits of one record
// ============================================================================

/// Record-level timestamp used to break ties: the later of `updated_at` and
/// `deleted_at`.
fn record_ts(v: &Value) -> DateTime<Utc> {
    let get = |k: &str| v.get(k).and_then(Value::as_str).map(parse_ts);
    match (get("updated_at"), get("deleted_at")) {
        (Some(u), Some(d)) => u.max(d),
        (Some(u), None) => u,
        (None, Some(d)) => d,
        // Edges and tags are immutable: created_at is their version.
        (None, None) => get("created_at").unwrap_or(DateTime::<Utc>::UNIX_EPOCH),
    }
}

/// Three-way, field-level merge of two versions of one record.
///
/// `base` is the common ancestor (what both sides started from); `None`
/// means the record was created independently on both sides, or the base is
/// unknown, in which case every differing field is treated as a collision.
///
/// Rules, per field:
/// - unchanged on one side: take the other side's value
/// - `metadata`: merged key by key with the same rules
/// - `updated_at` / `deleted_at`: the later; `created_at`: the earlier
/// - anything else both sides changed differently: the side whose record
///   has the later `updated_at` wins (ours on a tie)
///
/// Then tombstones: if only one side deleted the record and the other side
/// edited it *after* that deletion, the edit wins and the record lives.
pub fn merge_record_values(base: Option<&Value>, ours: &Value, theirs: &Value) -> Value {
    let (Some(o), Some(t)) = (ours.as_object(), theirs.as_object()) else {
        return if record_ts(ours) >= record_ts(theirs) {
            ours.clone()
        } else {
            theirs.clone()
        };
    };
    let b = base.and_then(Value::as_object);
    let ours_newer = record_ts(ours) >= record_ts(theirs);
    let mut out = merge_objects(b, o, t, ours_newer);

    // A one-sided delete versus an edit made after it: the edit wins.
    let o_del = o.get("deleted_at").and_then(Value::as_str);
    let t_del = t.get("deleted_at").and_then(Value::as_str);
    let b_del = b.and_then(|m| m.get("deleted_at")).and_then(Value::as_str);
    if b_del.is_none() {
        let survivor_edit = match (o_del, t_del) {
            (Some(d), None) => Some((parse_ts(d), record_ts(theirs))),
            (None, Some(d)) => Some((parse_ts(d), record_ts(ours))),
            _ => None,
        };
        if let Some((deleted, edited)) = survivor_edit {
            if edited > deleted {
                out.remove("deleted_at");
            }
        }
    }
    Value::Object(out)
}

fn merge_objects(
    base: Option<&serde_json::Map<String, Value>>,
    ours: &serde_json::Map<String, Value>,
    theirs: &serde_json::Map<String, Value>,
    ours_newer: bool,
) -> serde_json::Map<String, Value> {
    let mut keys: Vec<&String> = ours.keys().chain(theirs.keys()).collect();
    if let Some(b) = base {
        keys.extend(b.keys());
    }
    keys.sort();
    keys.dedup();

    let mut out = serde_json::Map::new();
    for key in keys {
        let bv = base.and_then(|m| m.get(key));
        let ov = ours.get(key);
        let tv = theirs.get(key);
        let merged: Option<Value> = if ov == tv {
            ov.cloned()
        } else if base.is_some() && ov == bv {
            tv.cloned()
        } else if base.is_some() && tv == bv {
            ov.cloned()
        } else {
            match (key.as_str(), ov, tv) {
                (_, Some(Value::Object(om)), Some(Value::Object(tm))) => Some(Value::Object(
                    merge_objects(bv.and_then(Value::as_object), om, tm, ours_newer),
                )),
                ("updated_at" | "deleted_at", Some(Value::String(a)), Some(Value::String(c))) => {
                    Some(Value::String(if parse_ts(a) >= parse_ts(c) {
                        a.clone()
                    } else {
                        c.clone()
                    }))
                }
                ("created_at", Some(Value::String(a)), Some(Value::String(c))) => {
                    Some(Value::String(if parse_ts(a) <= parse_ts(c) {
                        a.clone()
                    } else {
                        c.clone()
                    }))
                }
                // Present on one side only and absent from the base: an
                // addition, keep it. (With a base, one side removed it while
                // the other changed it, which is a real collision below.)
                (_, Some(v), None) | (_, None, Some(v)) if bv.is_none() => Some(v.clone()),
                _ => {
                    if ours_newer {
                        ov.cloned()
                    } else {
                        tv.cloned()
                    }
                }
            }
        };
        if let Some(v) = merged {
            out.insert(key.clone(), v);
        }
    }
    out
}

/// The four record maps a graph document is made of.
const RECORD_KINDS: [&str; 4] = ["nodes", "edges", "themes", "tags"];

/// Merge two versions of one record map.
///
/// A record only one side has is the ordinary case — two people each added
/// their own — and is kept. A record both sides changed goes field by field
/// through [`merge_record_values`]. A record one side dropped from the map
/// entirely (not tombstoned: deleted by hand, or rewritten history) is taken
/// as gone only if the other side left it alone.
fn merge_record_maps(
    base: Option<&serde_json::Map<String, Value>>,
    ours: &serde_json::Map<String, Value>,
    theirs: &serde_json::Map<String, Value>,
) -> serde_json::Map<String, Value> {
    let mut keys: Vec<&String> = ours.keys().chain(theirs.keys()).collect();
    keys.sort();
    keys.dedup();

    let mut out = serde_json::Map::new();
    for key in keys {
        let bv = base.and_then(|m| m.get(key));
        let merged = match (ours.get(key), theirs.get(key)) {
            (Some(o), Some(t)) if o == t => Some(o.clone()),
            (Some(o), Some(t)) => Some(merge_record_values(bv, o, t)),
            // Removed on one side. If the other side did not touch it since
            // the ancestor, the removal stands; otherwise the edit wins.
            (Some(o), None) => (bv != Some(o)).then(|| o.clone()),
            (None, Some(t)) => (bv != Some(t)).then(|| t.clone()),
            (None, None) => None,
        };
        if let Some(v) = merged {
            out.insert(key.clone(), v);
        }
    }
    out
}

/// Merge two whole graph documents, record by record.
fn merge_docs(base: Option<&Value>, ours: &Value, theirs: &Value) -> io::Result<Value> {
    let object = |v: &Value, what: &str| -> io::Result<serde_json::Map<String, Value>> {
        v.as_object().cloned().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{} is not a graph document", what),
            )
        })
    };
    let o = object(ours, "ours")?;
    let t = object(theirs, "theirs")?;
    let b = base.and_then(Value::as_object);

    let map_of = |m: &serde_json::Map<String, Value>, kind: &str| {
        m.get(kind)
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default()
    };

    let mut out = serde_json::Map::new();
    let version = [&o, &t]
        .iter()
        .filter_map(|m| m.get("version").and_then(Value::as_u64))
        .max()
        .unwrap_or(DOC_VERSION as u64);
    out.insert("version".into(), Value::from(version));
    for kind in RECORD_KINDS {
        let merged = merge_record_maps(
            b.map(|m| map_of(m, kind)).as_ref(),
            &map_of(&o, kind),
            &map_of(&t, kind),
        );
        out.insert(kind.into(), Value::Object(merged));
    }

    // Sections this version does not know. A section that is a map of
    // records merges like one; anything else takes the side that changed
    // it, ours when both did.
    let mut others: Vec<&String> = o
        .keys()
        .chain(t.keys())
        .filter(|k| k.as_str() != "version" && !RECORD_KINDS.contains(&k.as_str()))
        .collect();
    others.sort();
    others.dedup();
    for key in others {
        let bv = b.and_then(|m| m.get(key));
        let merged = match (o.get(key), t.get(key)) {
            (Some(ov), Some(tv)) if ov == tv => Some(ov.clone()),
            (Some(Value::Object(om)), Some(Value::Object(tm))) => Some(Value::Object(
                merge_record_maps(bv.and_then(Value::as_object), om, tm),
            )),
            (Some(ov), Some(tv)) => Some(if bv == Some(ov) {
                tv.clone()
            } else {
                ov.clone()
            }),
            (Some(v), None) | (None, Some(v)) => (bv != Some(v)).then(|| v.clone()),
            (None, None) => None,
        };
        if let Some(v) = merged {
            out.insert(key.clone(), v);
        }
    }
    Ok(Value::Object(out))
}

/// Merge three graph files the way a git merge driver is called: `base`
/// (may be empty for add/add), `ours`, `theirs`. Returns the merged document
/// as stable JSON text.
pub fn merge_record_files(base: &Path, ours: &Path, theirs: &Path) -> io::Result<String> {
    let read = |p: &Path| -> io::Result<Option<Value>> {
        let text = fs::read_to_string(p)?;
        if text.trim().is_empty() {
            return Ok(None);
        }
        serde_json::from_str(&text).map(Some).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{}: {}", p.display(), e),
            )
        })
    };
    let base_v = read(base)?;
    let ours_v =
        read(ours)?.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "ours is empty"))?;
    let theirs_v = read(theirs)?
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "theirs is empty"))?;
    let merged = merge_docs(base_v.as_ref(), &ours_v, &theirs_v)?;
    to_stable_json(&merged)
}

/// Split a file that contains git conflict markers into (ours, base, theirs).
/// Lines outside the markers belong to all three. `base` is only known for
/// diff3-style markers (`|||||||`). Returns `None` if there are no markers.
pub fn split_conflict_markers(text: &str) -> Option<(String, Option<String>, String)> {
    #[derive(PartialEq)]
    enum Side {
        Common,
        Ours,
        Base,
        Theirs,
    }
    let mut side = Side::Common;
    let mut ours = String::new();
    let mut base = String::new();
    let mut theirs = String::new();
    let mut seen_marker = false;
    let mut seen_base = false;
    for line in text.lines() {
        if line.starts_with("<<<<<<<") {
            side = Side::Ours;
            seen_marker = true;
            continue;
        }
        if line.starts_with("|||||||") && side == Side::Ours {
            side = Side::Base;
            seen_base = true;
            continue;
        }
        if line.starts_with("=======") && (side == Side::Ours || side == Side::Base) {
            side = Side::Theirs;
            continue;
        }
        if line.starts_with(">>>>>>>") && side == Side::Theirs {
            side = Side::Common;
            continue;
        }
        let push = |buf: &mut String| {
            buf.push_str(line);
            buf.push('\n');
        };
        match side {
            Side::Common => {
                push(&mut ours);
                push(&mut base);
                push(&mut theirs);
            }
            Side::Ours => push(&mut ours),
            Side::Base => push(&mut base),
            Side::Theirs => push(&mut theirs),
        }
    }
    if !seen_marker {
        return None;
    }
    Some((ours, if seen_base { Some(base) } else { None }, theirs))
}

/// A conflicted record file that `deciduous sync` merged (or could not).
#[derive(Debug, Clone, Serialize)]
pub struct ConflictRepair {
    pub path: String,
    pub merged: bool,
    pub message: Option<String>,
}

impl RecordStore {
    /// The graph file, if git left conflict markers in it — a merge done in
    /// a clone where `deciduous merge-record` is not registered.
    pub fn conflicted_files(&self) -> Vec<PathBuf> {
        match fs::read_to_string(&self.path) {
            Ok(text) if text.starts_with("<<<<<<<") || text.contains("\n<<<<<<<") => {
                vec![self.path.clone()]
            }
            _ => Vec::new(),
        }
    }

    /// Merge a graph file that still carries conflict markers, using the
    /// same rules as the merge driver. A file whose sides do not parse as
    /// JSON is left untouched and reported.
    pub fn repair_conflicted_files(&self) -> io::Result<Vec<ConflictRepair>> {
        let mut out = Vec::new();
        for path in self.conflicted_files() {
            let text = fs::read_to_string(&path)?;
            let display = path.display().to_string();
            let Some((ours, base, theirs)) = split_conflict_markers(&text) else {
                continue;
            };
            let parse = |s: &str| serde_json::from_str::<Value>(s);
            let (ours_v, theirs_v) = match (parse(&ours), parse(&theirs)) {
                (Ok(o), Ok(t)) => (o, t),
                (Err(e), _) | (_, Err(e)) => {
                    out.push(ConflictRepair {
                        path: display,
                        merged: false,
                        message: Some(format!("a side is not valid JSON: {}", e)),
                    });
                    continue;
                }
            };
            let base_v = base.as_deref().and_then(|b| parse(b).ok());
            let merged = match merge_docs(base_v.as_ref(), &ours_v, &theirs_v).and_then(|v| {
                serde_json::from_value::<GraphDoc>(v)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
            }) {
                Ok(doc) => doc,
                Err(e) => {
                    out.push(ConflictRepair {
                        path: display,
                        merged: false,
                        message: Some(format!("merging the two sides failed: {}", e)),
                    });
                    continue;
                }
            };
            self.replace_doc(merged)?;
            out.push(ConflictRepair {
                path: display,
                merged: true,
                message: None,
            });
        }
        Ok(out)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn store() -> (TempDir, RecordStore) {
        let dir = TempDir::new().unwrap();
        let s = RecordStore::create(dir.path().join(STORE_FILE_NAME))
            .unwrap()
            .with_author("alice");
        (dir, s)
    }

    fn db_in(dir: &Path) -> Database {
        Database::new(dir.join("deciduous.db").to_str().unwrap()).unwrap()
    }

    fn node(change_id: &str, title: &str, updated_at: &str) -> NodeRecord {
        NodeRecord {
            change_id: change_id.into(),
            node_type: "goal".into(),
            title: title.into(),
            description: None,
            status: "pending".into(),
            metadata: Some(serde_json::json!({"confidence": 80, "branch": "main"})),
            created_at: "2026-01-01T00:00:00+00:00".into(),
            updated_at: updated_at.into(),
            author: Some("bob".into()),
            deleted_at: None,
            extra: Default::default(),
        }
    }

    #[test]
    fn path_for_db_ignores_bare_file_names() {
        assert_eq!(
            RecordStore::path_for_db(Path::new("/x/.deciduous/deciduous.db")),
            Some(PathBuf::from("/x/.deciduous/graph.json"))
        );
        assert_eq!(RecordStore::path_for_db(Path::new("scratch.db")), None);
    }

    #[test]
    fn write_through_merges_with_a_pulled_record_instead_of_clobbering_it() {
        let (dir, s) = store();
        let db = db_in(dir.path());
        let id = db.create_node("goal", "G", None, None, None).unwrap();
        let row = db.get_node(id).unwrap().unwrap();
        // Alice's version arrives via git: a description we do not have yet.
        let mut theirs = s.read_node(&row.change_id).unwrap().unwrap();
        theirs.description = Some("from alice".into());
        theirs.updated_at = "2020-01-01T00:00:00+00:00".into();
        s.write_node(&theirs).unwrap();
        // Bob edits locally (later than Alice) before syncing.
        db.update_node_status(id, "active").unwrap();
        let on_disk = s.read_node(&row.change_id).unwrap().unwrap();
        assert_eq!(on_disk.description.as_deref(), Some("from alice"));
        assert_eq!(on_disk.status, "active");
        // A graph file that does not parse is never overwritten.
        fs::write(s.path(), "{broken").unwrap();
        db.update_node_status(id, "completed").unwrap();
        assert_eq!(fs::read_to_string(s.path()).unwrap(), "{broken");
    }

    #[test]
    fn a_local_edit_keeps_a_pulled_change_to_a_field_it_did_not_touch() {
        let (dir, s) = store();
        let db = db_in(dir.path());
        let id = db
            .create_node("goal", "Old title", None, None, None)
            .unwrap();
        let row = db.get_node(id).unwrap().unwrap();
        // Alice renamed it; her record is pulled but not synced yet, and her
        // clock is a day ahead.
        let mut theirs = s.read_node(&row.change_id).unwrap().unwrap();
        theirs.title = "Alice's title".into();
        theirs.updated_at = (Utc::now() + chrono::Duration::days(1)).to_rfc3339();
        s.write_node(&theirs).unwrap();
        // Bob changes the status. The database knows what it held before,
        // so the title is Alice's change, not a collision.
        db.update_node_status(id, "active").unwrap();
        let on_disk = s.read_node(&row.change_id).unwrap().unwrap();
        assert_eq!(on_disk.title, "Alice's title");
        assert_eq!(on_disk.status, "active");
        assert!(parse_ts(&on_disk.updated_at) > parse_ts(&theirs.updated_at));
        // The row took the same stamp, so sync imports only Alice's title.
        let row = db.get_node(id).unwrap().unwrap();
        assert_eq!(row.updated_at, on_disk.updated_at);
        let r = reconcile(&db, &s, false).unwrap();
        assert_eq!(r.nodes_updated, 1, "{r:?}");
        let row = db.get_node(id).unwrap().unwrap();
        assert_eq!(
            (row.title.as_str(), row.status.as_str()),
            ("Alice's title", "active")
        );
        assert!(reconcile(&db, &s, false).unwrap().is_clean());
    }

    #[test]
    fn reconcile_imports_on_updated_at_tie_when_content_differs() {
        let (dir, s) = store();
        let db = db_in(dir.path());
        let id = db.create_node("goal", "G", None, Some(50), None).unwrap();
        let row = db.get_node(id).unwrap().unwrap();
        // The merge driver kept our updated_at but added Bob's field.
        let mut merged = s.read_node(&row.change_id).unwrap().unwrap();
        merged.metadata = Some(serde_json::json!({"confidence": 50, "commit": "abc123"}));
        s.write_node(&merged).unwrap();
        let r = reconcile(&db, &s, false).unwrap();
        assert_eq!(r.nodes_updated, 1);
        let row = db.get_node(id).unwrap().unwrap();
        assert!(row.metadata_json.unwrap().contains("abc123"));
        assert!(reconcile(&db, &s, false).unwrap().is_clean());
    }

    #[test]
    fn reconcile_never_exports_over_an_unreadable_file() {
        let (dir, s) = store();
        let db = db_in(dir.path());
        let id = db.create_node("goal", "G", None, None, None).unwrap();
        let row = db.get_node(id).unwrap().unwrap();
        drop(row);
        fs::write(s.path(), "<<<<<<< not json").unwrap();
        // The conflicted file cannot be merged, so nothing is exported over it.
        let r = reconcile(&db, &s, false);
        assert!(r.is_err() || r.as_ref().unwrap().nodes_exported == 0);
        assert!(fs::read_to_string(s.path()).unwrap().starts_with("<<<<<<<"));
        let _ = id;
    }

    #[test]
    fn reconcile_folds_same_named_themes_from_two_machines() {
        let (dir_a, s) = store();
        let a = db_in(dir_a.path());
        a.set_store(None);
        a.create_theme("infra", "#111111", None).unwrap();
        let dir_b = TempDir::new().unwrap();
        let b = db_in(dir_b.path());
        b.set_store(None);
        b.create_theme("infra", "#222222", None).unwrap();
        let nb = b.create_node("goal", "B's", None, None, None).unwrap();
        b.tag_node(nb, "infra", "manual").unwrap();

        // Both publish, then A syncs against the union.
        reconcile(&b, &s, false).unwrap();
        let r = reconcile(&a, &s, false).unwrap();
        assert_eq!(r.themes_merged, 1, "{r:?}");
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        let themes = a.get_all_themes().unwrap();
        assert_eq!(themes.len(), 1);
        let live: Vec<_> = s
            .read_themes()
            .records
            .into_iter()
            .filter(|t| !t.is_tombstone())
            .collect();
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].change_id, themes[0].change_id);
        // B's tag followed the theme to the canonical id.
        let imported = a
            .get_all_nodes()
            .unwrap()
            .into_iter()
            .find(|n| n.title == "B's")
            .unwrap();
        assert_eq!(a.get_node_themes(imported.id).unwrap().len(), 1);
        // And B converges too.
        let r = reconcile(&b, &s, false).unwrap();
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        assert_eq!(b.get_all_themes().unwrap().len(), 1);
        assert_eq!(
            b.get_all_themes().unwrap()[0].change_id,
            themes[0].change_id
        );
        assert_eq!(b.get_node_themes(nb).unwrap().len(), 1);
        // One more exchange carries the newer color across; then it is quiet.
        reconcile(&a, &s, false).unwrap();
        reconcile(&b, &s, false).unwrap();
        {
            let r = reconcile(&a, &s, false).unwrap();
            assert!(r.is_clean(), "{r:?}");
        }
        assert!(reconcile(&b, &s, false).unwrap().is_clean());
        assert_eq!(
            a.get_all_themes().unwrap()[0].color,
            b.get_all_themes().unwrap()[0].color
        );
    }

    #[test]
    fn retag_after_remote_untag_is_republished() {
        let (dir, s) = store();
        let db = db_in(dir.path());
        db.set_store(Some(s.clone()));
        let n = db.create_node("goal", "G", None, None, None).unwrap();
        db.create_theme("ops", "#333333", None).unwrap();
        let cid = db.get_node(n).unwrap().unwrap().change_id;
        let theme_cid = db.get_all_themes().unwrap()[0].change_id.clone();
        // Someone else untagged it a while ago.
        s.write_tag(&TagRecord {
            node_change_id: cid.clone(),
            theme_change_id: theme_cid.clone(),
            source: "manual".into(),
            created_at: "2020-01-01T00:00:00+00:00".into(),
            author: None,
            deleted_at: Some("2020-01-02T00:00:00+00:00".into()),
            extra: Default::default(),
        })
        .unwrap();
        db.tag_node(n, "ops", "manual").unwrap();
        assert!(!s
            .read_tag(&cid, &theme_cid)
            .unwrap()
            .unwrap()
            .is_tombstone());
        // Even if the tombstone had come back afterwards, reconcile republishes.
        s.write_tag(&TagRecord {
            node_change_id: cid.clone(),
            theme_change_id: theme_cid.clone(),
            source: "manual".into(),
            created_at: "2020-01-01T00:00:00+00:00".into(),
            author: None,
            deleted_at: Some("2020-01-02T00:00:00+00:00".into()),
            extra: Default::default(),
        })
        .unwrap();
        let r = reconcile(&db, &s, false).unwrap();
        assert_eq!(r.tags_exported, 1);
        assert!(!s
            .read_tag(&cid, &theme_cid)
            .unwrap()
            .unwrap()
            .is_tombstone());
    }

    #[test]
    fn delete_node_tombstones_its_tags() {
        let (dir, s) = store();
        let db = db_in(dir.path());
        db.set_store(Some(s.clone()));
        let n = db.create_node("goal", "G", None, None, None).unwrap();
        db.create_theme("ops", "#333333", None).unwrap();
        db.tag_node(n, "ops", "manual").unwrap();
        db.delete_node(n, false).unwrap();
        let tags = s.read_tags().records;
        assert_eq!(tags.len(), 1);
        assert!(tags[0].is_tombstone());
        assert!(db.get_all_node_themes().unwrap().is_empty());
    }

    #[test]
    fn legacy_import_ignores_events_older_than_the_checkpoint() {
        let (_d, s) = store();
        fs::create_dir_all(s.dir().join("sync/events")).unwrap();
        let cp = serde_json::json!({
            "created_at": 2_000_000,
            "version": "1.0",
            "nodes": [{
                "change_id": "n1", "node_type": "goal", "title": "T",
                "description": null, "status": "completed", "metadata_json": null,
                "created_at": "2026-01-01T00:00:00+00:00", "updated_at": "2026-01-02T00:00:00+00:00"
            }],
            "edges": []
        });
        fs::write(s.dir().join("sync/checkpoint.json"), cp.to_string()).unwrap();
        fs::write(
            s.dir().join("sync/events/a.jsonl"),
            r#"{"op":"add_node","change_id":"n1","node_type":"goal","title":"T","description":null,"status":"pending","metadata_json":null,"timestamp":1000000,"author":"a"}
"#,
        )
        .unwrap();
        let report = s.import_legacy_events().unwrap();
        assert_eq!(report.events, 0);
        assert_eq!(s.read_node("n1").unwrap().unwrap().status, "completed");
    }

    #[test]
    fn mismatched_key_is_rejected() {
        let (_d, s) = store();
        s.write_node(&node("real-id", "R", "2026-01-02T00:00:00+00:00"))
            .unwrap();
        // A hand edit files the record under someone else's id.
        let mut doc = s.read_doc_all().unwrap();
        let rec = doc.nodes.get("real-id").unwrap().clone();
        doc.nodes.insert("other-id".into(), rec);
        fs::write(s.path(), to_stable_json(&doc).unwrap()).unwrap();

        let fresh = RecordStore::open(s.path()).unwrap();
        let read = fresh.read_nodes();
        assert_eq!(read.records.len(), 1);
        assert_eq!(read.errors.len(), 1);
        assert_eq!(read.errors[0].id.as_deref(), Some("other-id"));
        assert!(read.errors[0].message.contains("does not match"));
        assert_eq!(read.bad_ids(), HashSet::from(["other-id".to_string()]));
    }

    #[test]
    fn edge_id_is_deterministic_and_distinct() {
        let a = edge_id("n1", "n2", "leads_to");
        assert_eq!(a, edge_id("n1", "n2", "leads_to"));
        assert_ne!(a, edge_id("n2", "n1", "leads_to"));
        assert_ne!(a, edge_id("n1", "n2", "requires"));
        assert_eq!(a.len(), 20);
    }

    #[test]
    fn parse_ts_handles_rfc3339_offsets_and_garbage() {
        let a = parse_ts("2026-01-01T12:00:00-05:00");
        let b = parse_ts("2026-01-01T17:00:00+00:00");
        assert_eq!(a, b);
        assert!(parse_ts("2026-01-02") > a);
        assert_eq!(parse_ts("nonsense"), DateTime::<Utc>::UNIX_EPOCH);
    }

    #[test]
    fn node_roundtrip_is_stable_and_sorted() {
        let (_d, s) = store();
        let rec = node("abc-1", "Hello", "2026-01-02T00:00:00+00:00");
        assert!(s.write_node(&rec).unwrap());
        // Same content: no rewrite.
        assert!(!s.write_node(&rec).unwrap());
        let text = fs::read_to_string(s.path()).unwrap();
        assert!(text.ends_with('\n'));
        let author_pos = text.find("\"author\"").unwrap();
        let updated_pos = text.find("\"updated_at\"").unwrap();
        assert!(
            author_pos < updated_pos,
            "keys must be sorted for stable diffs"
        );
        let back = s.read_node("abc-1").unwrap().unwrap();
        assert_eq!(back, rec);
        assert_eq!(
            back.metadata_json().unwrap(),
            r#"{"branch":"main","confidence":80}"#
        );
    }

    #[test]
    fn a_broken_graph_file_reads_as_an_error_not_an_empty_graph() {
        let (_d, s) = store();
        s.write_node(&node("good", "ok", "2026-01-02T00:00:00+00:00"))
            .unwrap();
        fs::write(s.path(), "{not json").unwrap();
        let fresh = RecordStore::open(s.path()).unwrap();
        let read = fresh.read_nodes();
        assert!(read.records.is_empty());
        assert_eq!(read.errors.len(), 1);
        assert!(read.errors[0].message.contains("not readable"));
        // And it is never written over.
        assert!(fresh
            .write_node(&node("x", "x", "2026-01-02T00:00:00+00:00"))
            .is_err());
        assert_eq!(fs::read_to_string(s.path()).unwrap(), "{not json");
    }

    #[test]
    fn legacy_stem_neutralises_path_tricks() {
        assert_eq!(legacy_stem("../x"), "_.._x");
        assert!(!legacy_stem("..").starts_with('.'));
        assert_eq!(legacy_stem("a/b\\c"), "a_b_c");
    }

    #[test]
    fn write_through_publishes_and_tombstones() {
        let (dir, s) = store();
        let db = db_in(dir.path());
        db.set_store(Some(s.clone()));

        let g = db
            .create_node("goal", "Ship it", None, Some(90), None)
            .unwrap();
        let a = db.create_node("action", "Do it", None, None, None).unwrap();
        db.create_edge(g, a, "leads_to", Some("because")).unwrap();
        let counts = s.counts();
        assert_eq!((counts.nodes, counts.edges), (2, 1));

        let g_row = db.get_node(g).unwrap().unwrap();
        let rec = s.read_node(&g_row.change_id).unwrap().unwrap();
        assert_eq!(rec.title, "Ship it");
        assert_eq!(rec.author.as_deref(), Some("alice"));
        assert_eq!(rec.metadata.unwrap()["confidence"], 90);

        db.update_node_status(g, "active").unwrap();
        assert_eq!(
            s.read_node(&g_row.change_id).unwrap().unwrap().status,
            "active"
        );

        db.delete_node(a, false).unwrap();
        let a_row_rec = s
            .read_nodes()
            .records
            .into_iter()
            .find(|r| r.title == "Do it")
            .unwrap();
        assert!(a_row_rec.is_tombstone());
        let e = s.read_edges().records.pop().unwrap();
        assert!(e.is_tombstone(), "cascaded edge must be tombstoned too");
    }

    #[test]
    fn reconcile_imports_exports_and_is_idempotent() {
        let (dir, s) = store();
        // Bob's database has a node the store does not (created before sync was enabled).
        let db = db_in(dir.path());
        db.set_store(None);
        let local = db
            .create_node("goal", "Local only", None, None, None)
            .unwrap();
        // The store has a node from Alice plus an edge to Bob's node.
        let alice = node("alice-1", "From Alice", "2026-01-02T00:00:00+00:00");
        s.write_node(&alice).unwrap();
        let local_cid = db.get_node(local).unwrap().unwrap().change_id;
        s.write_edge(&EdgeRecord {
            edge_id: edge_id("alice-1", &local_cid, "leads_to"),
            from_change_id: "alice-1".into(),
            to_change_id: local_cid.clone(),
            edge_type: "leads_to".into(),
            rationale: Some("cross-user link".into()),
            weight: None,
            created_at: "2026-01-02T00:00:00+00:00".into(),
            author: Some("alice".into()),
            deleted_at: None,
            extra: Default::default(),
        })
        .unwrap();

        let dry = reconcile(&db, &s, true).unwrap();
        assert_eq!(
            (dry.nodes_imported, dry.nodes_exported, dry.edges_imported),
            (1, 1, 1)
        );
        assert_eq!(
            db.get_all_nodes().unwrap().len(),
            1,
            "dry run must not write"
        );
        assert!(s.read_node(&local_cid).unwrap().is_none());

        let real = reconcile(&db, &s, false).unwrap();
        assert_eq!(
            (
                real.nodes_imported,
                real.nodes_exported,
                real.edges_imported
            ),
            (1, 1, 1)
        );
        assert_eq!(db.get_all_nodes().unwrap().len(), 2);
        assert_eq!(db.get_all_edges().unwrap().len(), 1);
        assert!(s.read_node(&local_cid).unwrap().is_some());

        let again = reconcile(&db, &s, false).unwrap();
        assert!(again.is_clean(), "second sync must be a no-op: {:?}", again);
    }

    #[test]
    fn reconcile_newer_side_wins_each_way() {
        let (dir, s) = store();
        let db = db_in(dir.path());
        let id = db.create_node("goal", "v1", None, None, None).unwrap();
        let row = db.get_node(id).unwrap().unwrap();

        // Store has an older version: database wins, store gets rewritten.
        let mut older = NodeRecord::from_db(&row, Some("bob"));
        older.title = "v0".into();
        older.updated_at = "2000-01-01T00:00:00+00:00".into();
        s.write_node(&older).unwrap();
        let r = reconcile(&db, &s, false).unwrap();
        assert_eq!(r.nodes_exported, 1);
        assert_eq!(s.read_node(&row.change_id).unwrap().unwrap().title, "v1");

        // Store has a newer version: database gets updated.
        let mut newer = NodeRecord::from_db(&row, Some("bob"));
        newer.title = "v2".into();
        newer.status = "active".into();
        newer.updated_at = "2999-01-01T00:00:00+00:00".into();
        s.write_node(&newer).unwrap();
        let r = reconcile(&db, &s, false).unwrap();
        assert_eq!(r.nodes_updated, 1);
        let row = db.get_node(id).unwrap().unwrap();
        assert_eq!((row.title.as_str(), row.status.as_str()), ("v2", "active"));
    }

    #[test]
    fn reconcile_applies_tombstones_and_skips_orphaned_edges() {
        let (dir, s) = store();
        let db = db_in(dir.path());
        let a = db.create_node("goal", "A", None, None, None).unwrap();
        let b = db.create_node("action", "B", None, None, None).unwrap();
        db.create_edge(a, b, "leads_to", None).unwrap();
        reconcile(&db, &s, false).unwrap();

        // Someone else deleted B.
        let b_row = db.get_node(b).unwrap().unwrap();
        let mut tomb = NodeRecord::from_db(&b_row, Some("bob"));
        tomb.deleted_at = Some("2999-01-01T00:00:00+00:00".into());
        s.write_node(&tomb).unwrap();

        let r = reconcile(&db, &s, false).unwrap();
        assert_eq!(r.nodes_deleted, 1);
        assert!(db.get_node(b).unwrap().is_none());
        assert_eq!(
            db.get_all_edges().unwrap().len(),
            0,
            "edges cascade locally"
        );
        // The live edge record now points at a tombstoned node: orphaned, not pending.
        let r = reconcile(&db, &s, false).unwrap();
        assert_eq!(r.edges_orphaned, 1);
        assert_eq!(r.edges_imported, 0);
    }

    #[test]
    fn reconcile_edge_waits_for_missing_node_then_imports() {
        let (dir, s) = store();
        let db = db_in(dir.path());
        s.write_node(&node("x", "X", "2026-01-02T00:00:00+00:00"))
            .unwrap();
        s.write_edge(&EdgeRecord {
            edge_id: edge_id("x", "y", "leads_to"),
            from_change_id: "x".into(),
            to_change_id: "y".into(),
            edge_type: "leads_to".into(),
            rationale: None,
            weight: None,
            created_at: "2026-01-02T00:00:00+00:00".into(),
            author: None,
            deleted_at: None,
            extra: Default::default(),
        })
        .unwrap();
        let r = reconcile(&db, &s, false).unwrap();
        assert_eq!((r.edges_pending, r.edges_imported), (1, 0));
        assert!(r.pending_details[0].contains("waits for node y"));

        s.write_node(&node("y", "Y", "2026-01-03T00:00:00+00:00"))
            .unwrap();
        let r = reconcile(&db, &s, false).unwrap();
        assert_eq!((r.edges_pending, r.edges_imported), (0, 1));
    }

    #[test]
    fn reconcile_syncs_themes_and_tags() {
        let (dir, s) = store();
        let db = db_in(dir.path());
        db.set_store(Some(s.clone()));
        let n = db.create_node("goal", "Tagged", None, None, None).unwrap();
        db.create_theme("Infra", "#123456", Some("infra work"))
            .unwrap();
        db.tag_node(n, "infra", "manual").unwrap();
        assert_eq!(s.counts().themes, 1);
        assert_eq!(s.counts().tags, 1);

        // A fresh database imports everything from the store.
        let other_dir = TempDir::new().unwrap();
        let other = db_in(other_dir.path());
        let r = reconcile(&other, &s, false).unwrap();
        assert_eq!(
            (r.nodes_imported, r.themes_imported, r.tags_imported),
            (1, 1, 1)
        );
        let imported = other.get_all_nodes().unwrap().pop().unwrap();
        assert_eq!(other.get_node_themes(imported.id).unwrap()[0].name, "infra");

        db.untag_node(n, "infra").unwrap();
        let r = reconcile(&other, &s, false).unwrap();
        assert_eq!(r.tags_deleted, 1);
        assert!(other.get_node_themes(imported.id).unwrap().is_empty());
    }

    #[test]
    fn legacy_events_import_recovers_concatenated_lines() {
        let (_d, s) = store();
        let events_dir = s.dir().join("sync/events");
        fs::create_dir_all(&events_dir).unwrap();
        let add = |cid: &str, title: &str, ts: i64| {
            format!(
                r#"{{"op":"add_node","change_id":"{cid}","node_type":"goal","title":"{title}","description":null,"status":"pending","metadata_json":"{{\"confidence\":85}}","timestamp":{ts},"author":"Bobby"}}"#
            )
        };
        let edge = r#"{"op":"add_edge","edge_id":"edge-old","from_change_id":"n1","to_change_id":"n2","edge_type":"leads_to","rationale":"r","timestamp":1784149119470,"author":"Bobby"}"#;
        let update = r#"{"op":"update_node","change_id":"n1","title":null,"description":null,"status":"active","metadata_json":null,"timestamp":1784149119480,"author":"Bobby"}"#;
        let delete =
            r#"{"op":"delete_node","change_id":"n3","timestamp":1784149119490,"author":"Bobby"}"#;
        // Two events glued onto one line, as the old appender could do.
        let content = format!(
            "{}{}\n{}\n{}\n{}\n{}\n",
            add("n1", "One", 1784149119459),
            add("n2", "Two", 1784149119460),
            add("n3", "Three", 1784149119461),
            edge,
            update,
            delete
        );
        fs::write(events_dir.join("Bobby.jsonl"), content).unwrap();

        let report = s.import_legacy_events().unwrap();
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(report.events, 6);
        assert_eq!(report.nodes, 3);
        assert_eq!(report.edges, 1);
        assert!(report.removed);
        assert!(!s.has_legacy_events());

        let n1 = s.read_node("n1").unwrap().unwrap();
        assert_eq!(n1.status, "active");
        assert_eq!(n1.metadata.unwrap()["confidence"], 85);
        assert!(s.read_node("n3").unwrap().unwrap().is_tombstone());
        let e = s
            .read_edge(&edge_id("n1", "n2", "leads_to"))
            .unwrap()
            .unwrap();
        assert_eq!(e.rationale.as_deref(), Some("r"));
    }

    #[test]
    fn legacy_import_keeps_files_when_a_line_is_unreadable() {
        let (_d, s) = store();
        let events_dir = s.dir().join("sync/events");
        fs::create_dir_all(&events_dir).unwrap();
        fs::write(
            events_dir.join("x.jsonl"),
            "{\"op\":\"add_node\",\"change_id\":\"n1\",\"node_type\":\"goal\",\"title\":\"T\",\"description\":null,\"status\":\"pending\",\"metadata_json\":null,\"timestamp\":1,\"author\":\"a\"}\n{garbage\n",
        )
        .unwrap();
        let report = s.import_legacy_events().unwrap();
        assert_eq!(report.nodes, 1);
        assert_eq!(report.errors.len(), 1);
        assert!(!report.removed);
        assert!(s.has_legacy_events());
    }

    // ------------------------------------------------------------------
    // merging
    // ------------------------------------------------------------------

    fn rec(status: &str, updated: &str, meta: serde_json::Value) -> Value {
        serde_json::json!({
            "change_id": "n1",
            "node_type": "goal",
            "title": "Goal",
            "status": status,
            "metadata": meta,
            "created_at": "2026-01-01T00:00:00+00:00",
            "updated_at": updated,
            "author": "base"
        })
    }

    #[test]
    fn merge_takes_one_sided_changes_from_each_side() {
        let base = rec(
            "pending",
            "2026-01-01T00:00:00+00:00",
            serde_json::json!({"confidence": 80}),
        );
        // Alice set the status; Bob linked a commit.
        let mut ours = base.clone();
        ours["status"] = "active".into();
        ours["updated_at"] = "2026-01-02T00:00:00+00:00".into();
        ours["author"] = "alice".into();
        let mut theirs = base.clone();
        theirs["metadata"]["commit"] = "abc123".into();
        theirs["updated_at"] = "2026-01-03T00:00:00+00:00".into();
        theirs["author"] = "bob".into();

        let m = merge_record_values(Some(&base), &ours, &theirs);
        assert_eq!(m["status"], "active");
        assert_eq!(m["metadata"]["commit"], "abc123");
        assert_eq!(m["metadata"]["confidence"], 80);
        assert_eq!(m["updated_at"], "2026-01-03T00:00:00+00:00");
        assert_eq!(m["author"], "bob", "author follows the later write");
        // Symmetric.
        let m2 = merge_record_values(Some(&base), &theirs, &ours);
        assert_eq!(m, m2);
    }

    #[test]
    fn merge_same_field_collision_goes_to_later_updated_at() {
        let base = rec(
            "pending",
            "2026-01-01T00:00:00+00:00",
            serde_json::json!({}),
        );
        let mut ours = base.clone();
        ours["status"] = "completed".into();
        ours["updated_at"] = "2026-01-05T00:00:00+00:00".into();
        let mut theirs = base.clone();
        theirs["status"] = "abandoned".into();
        theirs["updated_at"] = "2026-01-02T00:00:00+00:00".into();
        assert_eq!(
            merge_record_values(Some(&base), &ours, &theirs)["status"],
            "completed"
        );
        assert_eq!(
            merge_record_values(Some(&base), &theirs, &ours)["status"],
            "completed"
        );
    }

    #[test]
    fn merge_without_base_still_unions_metadata_and_picks_later_side() {
        let ours = rec(
            "active",
            "2026-01-05T00:00:00+00:00",
            serde_json::json!({"a": 1}),
        );
        let theirs = rec(
            "pending",
            "2026-01-02T00:00:00+00:00",
            serde_json::json!({"b": 2}),
        );
        let m = merge_record_values(None, &ours, &theirs);
        assert_eq!(m["status"], "active");
        assert_eq!(m["metadata"], serde_json::json!({"a": 1, "b": 2}));
    }

    #[test]
    fn merge_edit_after_delete_resurrects_and_delete_after_edit_sticks() {
        let base = rec(
            "pending",
            "2026-01-01T00:00:00+00:00",
            serde_json::json!({}),
        );
        let mut deleted = base.clone();
        deleted["deleted_at"] = "2026-01-02T00:00:00+00:00".into();
        let mut edited_later = base.clone();
        edited_later["status"] = "active".into();
        edited_later["updated_at"] = "2026-01-03T00:00:00+00:00".into();
        let m = merge_record_values(Some(&base), &deleted, &edited_later);
        assert!(m.get("deleted_at").is_none(), "{m}");
        assert_eq!(m["status"], "active");

        let mut edited_earlier = base.clone();
        edited_earlier["status"] = "active".into();
        edited_earlier["updated_at"] = "2026-01-01T12:00:00+00:00".into();
        let m = merge_record_values(Some(&base), &edited_earlier, &deleted);
        assert_eq!(m["deleted_at"], "2026-01-02T00:00:00+00:00");
        assert_eq!(m["status"], "active", "tombstone keeps the last fields");
    }

    #[test]
    fn split_conflict_markers_handles_plain_and_diff3() {
        let plain =
            "{\n<<<<<<< HEAD\n  \"a\": 1,\n=======\n  \"a\": 2,\n>>>>>>> theirs\n  \"b\": 3\n}\n";
        let (o, b, t) = split_conflict_markers(plain).unwrap();
        assert_eq!(o, "{\n  \"a\": 1,\n  \"b\": 3\n}\n");
        assert_eq!(t, "{\n  \"a\": 2,\n  \"b\": 3\n}\n");
        assert!(b.is_none());

        let diff3 = "{\n<<<<<<< HEAD\n  \"a\": 1\n||||||| base\n  \"a\": 0\n=======\n  \"a\": 2\n>>>>>>> theirs\n}\n";
        let (_, b, _) = split_conflict_markers(diff3).unwrap();
        assert_eq!(b.unwrap(), "{\n  \"a\": 0\n}\n");
        assert!(split_conflict_markers("{\"a\": 1}").is_none());
    }

    #[test]
    fn sync_repairs_a_conflicted_graph_file() {
        let (dir, s) = store();
        let db = db_in(dir.path());
        // What git leaves behind in a clone where the merge driver is not
        // registered: markers inside the one record both sides touched.
        let conflicted = concat!(
            "{\n",
            "  \"nodes\": {\n",
            "    \"n1\": {\n",
            "      \"change_id\": \"n1\",\n",
            "      \"created_at\": \"2026-01-01T00:00:00+00:00\",\n",
            "<<<<<<< HEAD\n",
            "      \"metadata\": {\n        \"confidence\": 90\n      },\n",
            "      \"node_type\": \"goal\",\n",
            "      \"status\": \"active\",\n",
            "      \"title\": \"Goal\",\n",
            "      \"updated_at\": \"2026-01-03T00:00:00+00:00\"\n",
            "||||||| base\n",
            "      \"node_type\": \"goal\",\n",
            "      \"status\": \"pending\",\n",
            "      \"title\": \"Goal\",\n",
            "      \"updated_at\": \"2026-01-01T00:00:00+00:00\"\n",
            "=======\n",
            "      \"node_type\": \"goal\",\n",
            "      \"status\": \"pending\",\n",
            "      \"title\": \"Renamed goal\",\n",
            "      \"updated_at\": \"2026-01-02T00:00:00+00:00\"\n",
            ">>>>>>> theirs\n",
            "    }\n",
            "  },\n",
            "  \"version\": 1\n",
            "}\n"
        );
        fs::write(s.path(), conflicted).unwrap();

        let dry = reconcile(&db, &s, true).unwrap();
        assert_eq!(dry.conflicts.len(), 1);
        assert!(!dry.conflicts[0].merged);
        // A dry run never touches the file.
        assert!(fs::read_to_string(s.path()).unwrap().contains("<<<<<<<"));

        let real = reconcile(&db, &s, false).unwrap();
        assert!(real.conflicts[0].merged);
        assert_eq!(real.nodes_imported, 1);
        let rec = s.read_node("n1").unwrap().unwrap();
        // Ours is newer, so it wins the field both sides changed...
        assert_eq!(rec.status, "active");
        assert_eq!(rec.updated_at, "2026-01-03T00:00:00+00:00");
        // ...but a field only they changed, and one only we added, survive.
        assert_eq!(rec.title, "Renamed goal");
        assert_eq!(rec.metadata.unwrap()["confidence"], 90);
        assert!(s.conflicted_files().is_empty());
    }

    #[test]
    fn merging_two_documents_keeps_both_sides_additions() {
        let dir = TempDir::new().unwrap();
        let base = dir.path().join("base");
        let ours = dir.path().join("ours");
        let theirs = dir.path().join("theirs");

        let doc = |nodes: Value| {
            serde_json::json!({"version": 1, "nodes": nodes, "edges": {}}).to_string()
        };
        let rec = |title: &str, status: &str, updated: &str| {
            serde_json::json!({
                "change_id": "shared", "node_type": "goal", "title": title,
                "status": status, "created_at": "2026-01-01T00:00:00+00:00",
                "updated_at": updated
            })
        };
        fs::write(
            &base,
            doc(serde_json::json!({
                "shared": rec("Goal", "pending", "2026-01-01T00:00:00+00:00")
            })),
        )
        .unwrap();
        fs::write(
            &ours,
            doc(serde_json::json!({
                "shared": rec("Goal", "active", "2026-01-03T00:00:00+00:00"),
                "mine": rec("Mine", "pending", "2026-01-02T00:00:00+00:00")
            })),
        )
        .unwrap();
        fs::write(
            &theirs,
            doc(serde_json::json!({
                "shared": rec("Renamed", "pending", "2026-01-02T00:00:00+00:00"),
                "yours": rec("Yours", "pending", "2026-01-02T00:00:00+00:00")
            })),
        )
        .unwrap();

        let merged = merge_record_files(&base, &ours, &theirs).unwrap();
        let v: Value = serde_json::from_str(&merged).unwrap();
        // Neither side's new node is lost — the whole point of merging the
        // document rather than taking one side of the file.
        assert!(v["nodes"]["mine"].is_object());
        assert!(v["nodes"]["yours"].is_object());
        // The record both sides changed merges field by field.
        assert_eq!(v["nodes"]["shared"]["status"], "active");
        assert_eq!(v["nodes"]["shared"]["title"], "Renamed");
        assert!(merged.ends_with('\n'));
    }

    #[test]
    fn a_record_one_side_deleted_stays_deleted_unless_the_other_edited_it() {
        let dir = TempDir::new().unwrap();
        let write = |name: &str, nodes: Value| {
            let p = dir.path().join(name);
            fs::write(
                &p,
                serde_json::json!({"version": 1, "nodes": nodes}).to_string(),
            )
            .unwrap();
            p
        };
        let rec = |updated: &str| {
            serde_json::json!({
                "change_id": "n", "node_type": "goal", "title": "T", "status": "pending",
                "created_at": "2026-01-01T00:00:00+00:00", "updated_at": updated
            })
        };
        let base = write(
            "base",
            serde_json::json!({"n": rec("2026-01-01T00:00:00+00:00")}),
        );

        // They dropped it, we left it alone: it goes.
        let ours = write(
            "ours",
            serde_json::json!({"n": rec("2026-01-01T00:00:00+00:00")}),
        );
        let theirs = write("theirs", serde_json::json!({}));
        let merged: Value =
            serde_json::from_str(&merge_record_files(&base, &ours, &theirs).unwrap()).unwrap();
        assert!(merged["nodes"].get("n").is_none());

        // They dropped it, we edited it: the edit wins.
        let ours = write(
            "ours2",
            serde_json::json!({"n": rec("2026-01-05T00:00:00+00:00")}),
        );
        let merged: Value =
            serde_json::from_str(&merge_record_files(&base, &ours, &theirs).unwrap()).unwrap();
        assert_eq!(
            merged["nodes"]["n"]["updated_at"],
            "2026-01-05T00:00:00+00:00"
        );
    }

    #[test]
    fn a_batch_writes_the_file_once() {
        let (_d, s) = store();
        let before = fs::metadata(s.path()).unwrap().len();
        s.batch(|| {
            for i in 0..50 {
                s.write_node(&node(&format!("n{i}"), "T", "2026-01-02T00:00:00+00:00"))
                    .unwrap();
            }
            // Nothing has reached disk yet.
            assert_eq!(fs::metadata(s.path()).unwrap().len(), before);
        })
        .unwrap();
        assert_eq!(s.counts().nodes, 50);
        let fresh = RecordStore::open(s.path()).unwrap();
        assert_eq!(fresh.read_nodes().records.len(), 50);
    }

    #[test]
    fn the_0_17_record_directory_is_folded_in_and_removed() {
        let (dir, s) = store();
        let sync = dir.path().join("sync");
        fs::create_dir_all(sync.join("nodes")).unwrap();
        fs::create_dir_all(sync.join("edges")).unwrap();
        // One record the document already has, but older...
        s.write_node(&node("keep", "Newer", "2026-02-01T00:00:00+00:00"))
            .unwrap();
        for (id, title, updated) in [
            ("keep", "Older", "2026-01-01T00:00:00+00:00"),
            ("fresh", "Arrives", "2026-01-01T00:00:00+00:00"),
        ] {
            fs::write(
                sync.join("nodes").join(format!("{id}.json")),
                to_stable_json(&node(id, title, updated)).unwrap(),
            )
            .unwrap();
        }
        let eid = edge_id("keep", "fresh", "leads_to");
        fs::write(
            sync.join("edges").join(format!("{eid}.json")),
            to_stable_json(&EdgeRecord {
                edge_id: eid.clone(),
                from_change_id: "keep".into(),
                to_change_id: "fresh".into(),
                edge_type: "leads_to".into(),
                rationale: None,
                weight: None,
                created_at: "2026-01-01T00:00:00+00:00".into(),
                author: None,
                deleted_at: None,
                extra: Default::default(),
            })
            .unwrap(),
        )
        .unwrap();

        assert!(s.has_legacy_record_dir());
        let report = s.import_legacy_record_dir().unwrap();
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(report.nodes, 1, "only the record we did not already have");
        assert_eq!(report.edges, 1);
        assert!(report.removed);
        assert!(!sync.exists());

        // The newer version in the document was not clobbered by the older
        // file — that is the difference between a migration and a restore.
        assert_eq!(s.read_node("keep").unwrap().unwrap().title, "Newer");
        assert_eq!(s.read_node("fresh").unwrap().unwrap().title, "Arrives");
        assert!(s.read_edge(&eid).unwrap().is_some());
    }

    #[test]
    fn a_bad_file_keeps_the_0_17_directory_around_to_retry() {
        let (dir, s) = store();
        let sync = dir.path().join("sync");
        fs::create_dir_all(sync.join("nodes")).unwrap();
        fs::write(sync.join("nodes/n1.json"), "{not json").unwrap();
        let report = s.import_legacy_record_dir().unwrap();
        assert_eq!(report.errors.len(), 1);
        assert!(!report.removed);
        assert!(sync.exists());
    }
}
