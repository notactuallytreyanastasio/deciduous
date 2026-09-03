//! Git-native record store: the one multi-user sync mechanism.
//!
//! The shared source of truth for a decision graph is a directory of small
//! JSON files, one per record, committed alongside the code:
//!
//! ```text
//! .deciduous/sync/
//! ├── nodes/<change_id>.json
//! ├── edges/<edge_id>.json
//! ├── themes/<change_id>.json
//! └── tags/<node_change_id>--<theme_change_id>.json
//! ```
//!
//! The local SQLite database is a per-machine cache of that directory (plus
//! local-only data such as sessions and the command log). Every write that
//! goes through [`crate::db::Database`] is mirrored into the store at once,
//! and `deciduous sync` reconciles the two in both directions.
//!
//! Why one file per record instead of a log:
//!
//! - Two people adding records never touch the same file, so git merges their
//!   branches with no conflicts. Only concurrent edits of the *same* record
//!   conflict, on one small JSON file, which is a real conflict worth a look.
//! - Nothing has to be replayed in order: each record carries its own
//!   `updated_at`, and the newer version wins. No checkpoints, no compaction,
//!   no clock-skew window to fall into.
//! - `git log -- .deciduous/sync/nodes/<change_id>.json` is the history of a
//!   decision, and `git blame` says who changed it.
//!
//! Integer ids are local aliases that differ between machines. Records refer
//! to each other only by `change_id`, and the CLI accepts a `change_id` prefix
//! wherever it accepts an id.
//!
//! Deletion writes a tombstone (the record with `deleted_at` set) rather than
//! removing the file, so a deletion propagates to machines that already have
//! the record, and so `git checkout` of an older branch never looks like a
//! mass deletion.
//!
//! When two people edit the *same* record on different branches, git calls
//! `deciduous merge-record` (a merge driver registered by `init`/`update`/
//! `sync`) with the common ancestor and both sides. [`merge_record_values`]
//! merges field by field: a field only one side touched is taken as is,
//! `metadata` merges key by key, and a field both sides changed goes to the
//! side with the later `updated_at`. A delete loses to an edit made after it.
//! Files that still carry conflict markers (merged without the driver) are
//! repaired the same way by `deciduous sync`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::db::{Database, DecisionEdge, DecisionNode, NodeTheme, Theme};

/// Directory name under `.deciduous/` that holds the record store.
pub const STORE_DIR_NAME: &str = "sync";

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
}

impl TagRecord {
    pub fn is_tombstone(&self) -> bool {
        self.deleted_at.is_some()
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
    for fmt in ["%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M:%S", "%Y-%m-%d"] {
        if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(s, fmt) {
            return DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc);
        }
        if let Ok(date) = chrono::NaiveDate::parse_from_str(s, fmt) {
            return DateTime::<Utc>::from_naive_utc_and_offset(
                date.and_hms_opt(0, 0, 0).unwrap_or_default(),
                Utc,
            );
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

/// Keep file names boring: ids are UUIDs, but never trust that.
fn safe_stem(id: &str) -> String {
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

/// Stable, pretty JSON with a trailing newline. Keys come out sorted, so
/// two machines writing the same record produce byte-identical files.
fn to_stable_json<T: Serialize>(value: &T) -> io::Result<String> {
    let v = serde_json::to_value(value).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    let mut s =
        serde_json::to_string_pretty(&v).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
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

/// A file that could not be read as a record.
#[derive(Debug, Clone, Serialize)]
pub struct ReadError {
    pub path: String,
    pub message: String,
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path, self.message)
    }
}

/// Result of reading one record directory: everything that parsed, plus
/// every file that did not (a broken file never blocks the others).
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

fn read_dir_records<T: for<'de> Deserialize<'de>>(dir: &Path) -> StoreRead<T> {
    let mut out = StoreRead::default();
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return out,
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|e| e == "json").unwrap_or(false))
        .collect();
    paths.sort();
    for path in paths {
        match fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<T>(&text) {
                Ok(rec) => out.records.push(rec),
                Err(e) => out.errors.push(ReadError {
                    path: path.display().to_string(),
                    message: e.to_string(),
                }),
            },
            Err(e) => out.errors.push(ReadError {
                path: path.display().to_string(),
                message: e.to_string(),
            }),
        }
    }
    out
}

// ============================================================================
// The store
// ============================================================================

/// Handle on a `.deciduous/sync/` directory.
#[derive(Debug, Clone)]
pub struct RecordStore {
    root: PathBuf,
    author: String,
}

impl RecordStore {
    /// Where the store lives for a given database file: a `sync/` directory
    /// next to it (`.deciduous/deciduous.db` -> `.deciduous/sync`).
    pub fn dir_for_db(db_path: &Path) -> PathBuf {
        match db_path.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent.join(STORE_DIR_NAME),
            _ => PathBuf::from(".deciduous").join(STORE_DIR_NAME),
        }
    }

    /// Open an existing store. Returns `None` if the directory is absent,
    /// which is how a project that has not enabled sync looks.
    pub fn open(root: impl Into<PathBuf>) -> Option<Self> {
        let root = root.into();
        if root.is_dir() {
            Some(Self {
                root,
                author: get_current_author(),
            })
        } else {
            None
        }
    }

    /// Create the store directories (idempotent) and open it.
    pub fn create(root: impl Into<PathBuf>) -> io::Result<Self> {
        let root = root.into();
        for sub in ["nodes", "edges", "themes", "tags"] {
            fs::create_dir_all(root.join(sub))?;
        }
        Ok(Self {
            root,
            author: get_current_author(),
        })
    }

    /// Use a fixed author instead of asking git (tests, servers).
    pub fn with_author(mut self, author: impl Into<String>) -> Self {
        self.author = author.into();
        self
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn author(&self) -> &str {
        &self.author
    }

    fn nodes_dir(&self) -> PathBuf {
        self.root.join("nodes")
    }
    fn edges_dir(&self) -> PathBuf {
        self.root.join("edges")
    }
    fn themes_dir(&self) -> PathBuf {
        self.root.join("themes")
    }
    fn tags_dir(&self) -> PathBuf {
        self.root.join("tags")
    }

    pub fn node_path(&self, change_id: &str) -> PathBuf {
        self.nodes_dir()
            .join(format!("{}.json", safe_stem(change_id)))
    }
    pub fn edge_path(&self, edge_id: &str) -> PathBuf {
        self.edges_dir()
            .join(format!("{}.json", safe_stem(edge_id)))
    }
    pub fn theme_path(&self, change_id: &str) -> PathBuf {
        self.themes_dir()
            .join(format!("{}.json", safe_stem(change_id)))
    }
    pub fn tag_path(&self, node_change_id: &str, theme_change_id: &str) -> PathBuf {
        self.tags_dir().join(format!(
            "{}.json",
            safe_stem(&tag_id(node_change_id, theme_change_id))
        ))
    }

    // ---- writes ------------------------------------------------------------

    /// Write a node record. Returns `true` if the file changed.
    pub fn write_node(&self, rec: &NodeRecord) -> io::Result<bool> {
        write_if_changed(&self.node_path(&rec.change_id), &to_stable_json(rec)?)
    }

    pub fn write_edge(&self, rec: &EdgeRecord) -> io::Result<bool> {
        write_if_changed(&self.edge_path(&rec.edge_id), &to_stable_json(rec)?)
    }

    pub fn write_theme(&self, rec: &ThemeRecord) -> io::Result<bool> {
        write_if_changed(&self.theme_path(&rec.change_id), &to_stable_json(rec)?)
    }

    pub fn write_tag(&self, rec: &TagRecord) -> io::Result<bool> {
        write_if_changed(
            &self.tag_path(&rec.node_change_id, &rec.theme_change_id),
            &to_stable_json(rec)?,
        )
    }

    /// Publish a live node from the database.
    pub fn publish_node(&self, node: &DecisionNode) -> io::Result<bool> {
        self.write_node(&NodeRecord::from_db(node, Some(&self.author)))
    }

    /// Mark a node deleted. Keeps the last known fields so history stays
    /// readable and a later edit can resurrect it.
    pub fn tombstone_node(&self, node: &DecisionNode) -> io::Result<bool> {
        let mut rec = NodeRecord::from_db(node, Some(&self.author));
        rec.deleted_at = Some(now_ts());
        self.write_node(&rec)
    }

    /// Publish a live edge. Legacy edges without change ids are skipped.
    pub fn publish_edge(&self, edge: &DecisionEdge) -> io::Result<bool> {
        match EdgeRecord::from_db(edge, Some(&self.author)) {
            Some(rec) => self.write_edge(&rec),
            None => Ok(false),
        }
    }

    pub fn tombstone_edge(&self, edge: &DecisionEdge) -> io::Result<bool> {
        match EdgeRecord::from_db(edge, Some(&self.author)) {
            Some(mut rec) => {
                rec.deleted_at = Some(now_ts());
                self.write_edge(&rec)
            }
            None => Ok(false),
        }
    }

    pub fn publish_theme(&self, theme: &Theme) -> io::Result<bool> {
        self.write_theme(&ThemeRecord::from_db(theme, Some(&self.author)))
    }

    pub fn tombstone_theme(&self, theme: &Theme) -> io::Result<bool> {
        let mut rec = ThemeRecord::from_db(theme, Some(&self.author));
        rec.deleted_at = Some(now_ts());
        self.write_theme(&rec)
    }

    pub fn publish_tag(
        &self,
        node_change_id: &str,
        theme_change_id: &str,
        source: &str,
        created_at: &str,
    ) -> io::Result<bool> {
        self.write_tag(&TagRecord {
            node_change_id: node_change_id.to_string(),
            theme_change_id: theme_change_id.to_string(),
            source: source.to_string(),
            created_at: created_at.to_string(),
            author: Some(self.author.clone()),
            deleted_at: None,
        })
    }

    pub fn tombstone_tag(&self, node_change_id: &str, theme_change_id: &str) -> io::Result<bool> {
        let existing = self.read_tag(node_change_id, theme_change_id)?;
        let mut rec = existing.unwrap_or(TagRecord {
            node_change_id: node_change_id.to_string(),
            theme_change_id: theme_change_id.to_string(),
            source: "manual".to_string(),
            created_at: now_ts(),
            author: None,
            deleted_at: None,
        });
        rec.author = Some(self.author.clone());
        rec.deleted_at = Some(now_ts());
        self.write_tag(&rec)
    }

    // ---- reads -------------------------------------------------------------

    pub fn read_node(&self, change_id: &str) -> io::Result<Option<NodeRecord>> {
        read_one(&self.node_path(change_id))
    }

    pub fn read_edge(&self, edge_id: &str) -> io::Result<Option<EdgeRecord>> {
        read_one(&self.edge_path(edge_id))
    }

    pub fn read_tag(
        &self,
        node_change_id: &str,
        theme_change_id: &str,
    ) -> io::Result<Option<TagRecord>> {
        read_one(&self.tag_path(node_change_id, theme_change_id))
    }

    pub fn read_nodes(&self) -> StoreRead<NodeRecord> {
        read_dir_records(&self.nodes_dir())
    }

    pub fn read_edges(&self) -> StoreRead<EdgeRecord> {
        read_dir_records(&self.edges_dir())
    }

    pub fn read_themes(&self) -> StoreRead<ThemeRecord> {
        read_dir_records(&self.themes_dir())
    }

    pub fn read_tags(&self) -> StoreRead<TagRecord> {
        read_dir_records(&self.tags_dir())
    }

    /// Number of record files per kind (live and tombstoned alike).
    pub fn counts(&self) -> StoreCounts {
        let count = |dir: PathBuf| {
            fs::read_dir(dir)
                .map(|entries| {
                    entries
                        .filter_map(|e| e.ok())
                        .filter(|e| e.path().extension().map(|x| x == "json").unwrap_or(false))
                        .count()
                })
                .unwrap_or(0)
        };
        StoreCounts {
            nodes: count(self.nodes_dir()),
            edges: count(self.edges_dir()),
            themes: count(self.themes_dir()),
            tags: count(self.tags_dir()),
        }
    }

    // ---- legacy event logs -------------------------------------------------

    fn legacy_events_dir(&self) -> PathBuf {
        self.root.join("events")
    }

    fn legacy_checkpoint_path(&self) -> PathBuf {
        self.root.join("checkpoint.json")
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

        let mut state = match read_checkpoint(&self.legacy_checkpoint_path()) {
            Ok(Some(cp)) => {
                report.checkpoint = true;
                MaterializedState::from_checkpoint(&cp)
            }
            Ok(None) => MaterializedState::default(),
            Err(e) => {
                report.errors.push(format!("checkpoint.json: {}", e));
                MaterializedState::default()
            }
        };

        let (events, errors) = read_events_tolerant(&self.legacy_events_dir());
        report.events = events.len();
        report.errors.extend(errors);
        state.replay(&events);

        for node in state.nodes.values() {
            let rec = NodeRecord {
                change_id: node.change_id.clone(),
                node_type: node.node_type.clone(),
                title: node.title.clone(),
                description: node.description.clone(),
                status: node.status.clone(),
                metadata: node.metadata_json.as_deref().map(parse_metadata),
                created_at: node.created_at.to_rfc3339(),
                updated_at: node.updated_at.to_rfc3339(),
                author: node.author.clone(),
                deleted_at: None,
            };
            if self.newer_than_existing_node(&rec)? && self.write_node(&rec)? {
                report.nodes += 1;
            }
        }

        for (node, deleted_at) in state.tombstoned_nodes.values() {
            let rec = NodeRecord {
                change_id: node.change_id.clone(),
                node_type: node.node_type.clone(),
                title: node.title.clone(),
                description: node.description.clone(),
                status: node.status.clone(),
                metadata: node.metadata_json.as_deref().map(parse_metadata),
                created_at: node.created_at.to_rfc3339(),
                updated_at: node.updated_at.to_rfc3339(),
                author: node.author.clone(),
                deleted_at: Some(deleted_at.to_rfc3339()),
            };
            if self.newer_than_existing_node(&rec)? && self.write_node(&rec)? {
                report.nodes += 1;
            }
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
            };
            if self.read_edge(&rec.edge_id)?.is_none() && self.write_edge(&rec)? {
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
            };
            let keep = match self.read_edge(&rec.edge_id)? {
                Some(existing) => {
                    !existing.is_tombstone() && parse_ts(&existing.created_at) <= *deleted_at
                }
                None => true,
            };
            if keep && self.write_edge(&rec)? {
                report.edges += 1;
            }
        }

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

    fn newer_than_existing_node(&self, rec: &NodeRecord) -> io::Result<bool> {
        Ok(match self.read_node(&rec.change_id)? {
            Some(existing) => rec.effective_ts() > existing.effective_ts(),
            None => true,
        })
    }
}

fn read_one<T: for<'de> Deserialize<'de>>(path: &Path) -> io::Result<Option<T>> {
    if !path.is_file() {
        return Ok(None);
    }
    let text = fs::read_to_string(path)?;
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
}

/// Record counts by kind.
#[derive(Debug, Clone, Default, Serialize)]
pub struct StoreCounts {
    pub nodes: usize,
    pub edges: usize,
    pub themes: usize,
    pub tags: usize,
}

/// What a legacy event-log import did.
#[derive(Debug, Clone, Default, Serialize)]
pub struct LegacyImport {
    pub checkpoint: bool,
    pub events: usize,
    pub nodes: usize,
    pub edges: usize,
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

    // ---- nodes -------------------------------------------------------------
    let node_read = store.read_nodes();
    report.read_errors.extend(node_read.errors);
    let mut store_nodes: HashMap<String, NodeRecord> = node_read
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
                        db.import_node_record(rec).map_err(db_err)?;
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
                    if rec_ts > row_ts {
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
        if !store_nodes.contains_key(change_id) {
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
    store_nodes.retain(|_, r| !r.is_tombstone());

    // ---- themes ------------------------------------------------------------
    let theme_read = store.read_themes();
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
                        db.import_theme_record(rec).map_err(db_err)?;
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
                    if rec_ts > row_ts {
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
        if !store_themes.contains_key(change_id) {
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
                            db.import_edge_record(from_id, to_id, rec).map_err(db_err)?;
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
        if !store_edges.contains_key(eid) {
            if !dry_run {
                store.publish_edge(row).map_err(io_err)?;
            }
            report.edges_exported += 1;
        }
    }

    // ---- tags --------------------------------------------------------------
    let tag_read = store.read_tags();
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
                        db.import_tag_record(node_id, theme_id, rec)
                            .map_err(db_err)?;
                    }
                    report.tags_imported += 1;
                }
            }
            Some((row, _, _)) => {
                if rec.is_tombstone() {
                    let deleted = rec.deleted_at.as_deref().map(parse_ts).unwrap_or_default();
                    if deleted >= parse_ts(&row.created_at) {
                        if !dry_run {
                            db.delete_tag_local(row.node_id, row.theme_id)
                                .map_err(db_err)?;
                        }
                        report.tags_deleted += 1;
                    }
                }
            }
        }
    }
    for (key, (row, node_cid, theme_cid)) in &db_tag_by_key {
        if !store_tags.contains_key(key) {
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
        (None, None) => DateTime::<Utc>::UNIX_EPOCH,
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

/// Merge three record files the way a git merge driver is called:
/// `base` (may be empty for add/add), `ours`, `theirs`. Returns the merged
/// record as stable JSON text.
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
    let merged = merge_record_values(base_v.as_ref(), &ours_v, &theirs_v);
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
    /// Find record files that still contain git conflict markers.
    pub fn conflicted_files(&self) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for dir in [
            self.nodes_dir(),
            self.edges_dir(),
            self.themes_dir(),
            self.tags_dir(),
        ] {
            let Ok(entries) = fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map(|e| e == "json").unwrap_or(false) {
                    if let Ok(text) = fs::read_to_string(&path) {
                        if text.starts_with("<<<<<<<") || text.contains("\n<<<<<<<") {
                            out.push(path);
                        }
                    }
                }
            }
        }
        out.sort();
        out
    }

    /// Merge every record file that still carries conflict markers, using
    /// the same field-level rules as the merge driver. Files whose sides do
    /// not parse as JSON are left untouched and reported.
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
            let merged = merge_record_values(base_v.as_ref(), &ours_v, &theirs_v);
            write_if_changed(&path, &to_stable_json(&merged)?)?;
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
        let s = RecordStore::create(dir.path().join("sync"))
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
        }
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
        let text = fs::read_to_string(s.node_path("abc-1")).unwrap();
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
    fn broken_file_does_not_block_the_rest() {
        let (_d, s) = store();
        s.write_node(&node("good", "ok", "2026-01-02T00:00:00+00:00"))
            .unwrap();
        fs::write(s.node_path("bad"), "{not json").unwrap();
        let read = s.read_nodes();
        assert_eq!(read.records.len(), 1);
        assert_eq!(read.errors.len(), 1);
        assert!(read.errors[0].path.ends_with("bad.json"));
    }

    #[test]
    fn safe_stem_neutralises_path_tricks() {
        assert_eq!(safe_stem("../x"), "_.._x");
        assert!(!safe_stem("..").starts_with('.'));
        assert_eq!(safe_stem("a/b\\c"), "a_b_c");
    }

    #[test]
    fn write_through_publishes_and_tombstones() {
        let (dir, s) = store();
        let mut db = db_in(dir.path());
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
        let mut db = db_in(dir.path());
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
        let mut db = db_in(dir.path());
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
        let events_dir = s.root().join("events");
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
        let events_dir = s.root().join("events");
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
    fn sync_repairs_a_conflicted_record_file() {
        let (dir, s) = store();
        let db = db_in(dir.path());
        let conflicted = concat!(
            "{\n",
            "  \"change_id\": \"n1\",\n",
            "  \"created_at\": \"2026-01-01T00:00:00+00:00\",\n",
            "<<<<<<< HEAD\n",
            "  \"metadata\": {\n    \"confidence\": 90\n  },\n",
            "  \"node_type\": \"goal\",\n",
            "  \"status\": \"active\",\n",
            "  \"title\": \"Goal\",\n",
            "  \"updated_at\": \"2026-01-03T00:00:00+00:00\"\n",
            "||||||| base\n",
            "  \"node_type\": \"goal\",\n",
            "  \"status\": \"pending\",\n",
            "  \"title\": \"Goal\",\n",
            "  \"updated_at\": \"2026-01-01T00:00:00+00:00\"\n",
            "=======\n",
            "  \"node_type\": \"goal\",\n",
            "  \"status\": \"pending\",\n",
            "  \"title\": \"Renamed goal\",\n",
            "  \"updated_at\": \"2026-01-02T00:00:00+00:00\"\n",
            ">>>>>>> theirs\n",
            "}\n"
        );
        fs::write(s.node_path("n1"), conflicted).unwrap();

        let dry = reconcile(&db, &s, true).unwrap();
        assert_eq!(dry.conflicts.len(), 1);
        assert!(!dry.conflicts[0].merged);

        let real = reconcile(&db, &s, false).unwrap();
        assert!(real.conflicts[0].merged);
        assert_eq!(real.nodes_imported, 1);
        let rec = s.read_node("n1").unwrap().unwrap();
        assert_eq!(rec.status, "active");
        assert_eq!(rec.title, "Renamed goal");
        assert_eq!(rec.metadata.unwrap()["confidence"], 90);
        assert_eq!(rec.updated_at, "2026-01-03T00:00:00+00:00");
        assert!(s.conflicted_files().is_empty());
    }

    #[test]
    fn merge_record_files_matches_git_driver_contract() {
        let dir = TempDir::new().unwrap();
        let base = dir.path().join("base");
        let ours = dir.path().join("ours");
        let theirs = dir.path().join("theirs");
        fs::write(&base, "").unwrap(); // add/add: no common ancestor
        fs::write(&ours, r#"{"change_id":"x","status":"active","updated_at":"2026-01-02T00:00:00+00:00","metadata":{"a":1}}"#).unwrap();
        fs::write(&theirs, r#"{"change_id":"x","status":"pending","updated_at":"2026-01-01T00:00:00+00:00","metadata":{"b":2}}"#).unwrap();
        let merged = merge_record_files(&base, &ours, &theirs).unwrap();
        let v: Value = serde_json::from_str(&merged).unwrap();
        assert_eq!(v["status"], "active");
        assert_eq!(v["metadata"], serde_json::json!({"a": 1, "b": 2}));
        assert!(merged.ends_with('\n'));
    }
}
