//! SQLite database with Diesel ORM
//!
//! Stores decision graphs and command logs for AI-assisted development.
//! Uses embedded migrations for schema management.

use crate::schema::*;
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool, PooledConnection};
use diesel::sqlite::SqliteConnection;
use serde_json::json;
use std::path::Path;
use uuid::Uuid;
#[cfg(feature = "ts-rs")]
use ts_rs::TS;

/// Build metadata JSON from optional fields (confidence, commit, prompt, files, branch)
pub fn build_metadata_json(
    confidence: Option<u8>,
    commit: Option<&str>,
    prompt: Option<&str>,
    files: Option<&str>,
    branch: Option<&str>,
) -> Option<String> {
    // Only create JSON if at least one field is present
    if confidence.is_none() && commit.is_none() && prompt.is_none() && files.is_none() && branch.is_none() {
        return None;
    }

    let mut obj = serde_json::Map::new();

    if let Some(c) = confidence {
        obj.insert("confidence".to_string(), json!(c.min(100)));
    }
    if let Some(h) = commit {
        obj.insert("commit".to_string(), json!(h));
    }
    if let Some(p) = prompt {
        obj.insert("prompt".to_string(), json!(p));
    }
    if let Some(f) = files {
        // Split comma-separated files into array
        let file_list: Vec<&str> = f.split(',').map(|s| s.trim()).collect();
        obj.insert("files".to_string(), json!(file_list));
    }
    if let Some(b) = branch {
        obj.insert("branch".to_string(), json!(b));
    }

    Some(serde_json::Value::Object(obj).to_string())
}

/// Get current git branch name
pub fn get_current_git_branch() -> Option<String> {
    std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                String::from_utf8(output.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty() && s != "HEAD")
            } else {
                None
            }
        })
}

/// Get the current HEAD commit hash (short form, 7 chars)
pub fn get_current_git_commit() -> Option<String> {
    std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                String::from_utf8(output.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
            } else {
                None
            }
        })
}

/// Walk up directory tree to find .deciduous folder (like git finds .git)
/// Can be overridden with DECIDUOUS_DB_PATH env var
fn get_db_path() -> std::path::PathBuf {
    // Check env var first - always takes priority
    if let Ok(path) = std::env::var("DECIDUOUS_DB_PATH") {
        return std::path::PathBuf::from(path);
    }

    // Walk up directory tree to find .deciduous folder
    if let Ok(current_dir) = std::env::current_dir() {
        let mut dir = current_dir.as_path();
        loop {
            let deciduous_dir = dir.join(".deciduous");
            if deciduous_dir.exists() && deciduous_dir.is_dir() {
                return deciduous_dir.join("deciduous.db");
            }
            // Move to parent directory
            match dir.parent() {
                Some(parent) => dir = parent,
                None => break, // Reached filesystem root
            }
        }
    }

    // No .deciduous found - default to current directory
    // (deciduous init will create it here)
    std::path::PathBuf::from(".deciduous/deciduous.db")
}

/// Current schema version for deciduous
pub const CURRENT_SCHEMA: DecisionSchema = DecisionSchema {
    major: 1,
    minor: 0,
    patch: 0,
    name: "decision-graph",
    features: &[
        "decision_nodes",
        "decision_edges",
        "decision_context",
        "decision_sessions",
        "command_log",
    ],
};

/// Describes the version and capabilities of the schema
#[derive(Debug, Clone)]
pub struct DecisionSchema {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub name: &'static str,
    pub features: &'static [&'static str],
}

impl DecisionSchema {
    pub fn version_string(&self) -> String {
        format!("{}.{}.{}", self.major, self.minor, self.patch)
    }

    pub fn is_compatible_with(&self, other: &DecisionSchema) -> bool {
        self.major == other.major
    }

    pub fn is_newer_than(&self, other: &DecisionSchema) -> bool {
        (self.major, self.minor, self.patch) > (other.major, other.minor, other.patch)
    }

    pub fn has_feature(&self, feature: &str) -> bool {
        self.features.contains(&feature)
    }
}

impl std::fmt::Display for DecisionSchema {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "v{} ({})", self.version_string(), self.name)
    }
}

// ============================================================================
// Diesel Models
// ============================================================================

/// Insertable schema version
#[derive(Insertable)]
#[diesel(table_name = schema_versions)]
pub struct NewSchemaVersion<'a> {
    pub version: &'a str,
    pub name: &'a str,
    pub features: &'a str,
    pub introduced_at: &'a str,
}

/// Queryable schema version
#[derive(Queryable, Selectable, Debug, Clone, serde::Serialize)]
#[diesel(table_name = schema_versions)]
pub struct StoredSchema {
    pub id: i32,
    pub version: String,
    pub name: String,
    pub features: String,
    pub introduced_at: String,
}

// ============================================================================
// Decision Graph Models
// ============================================================================

/// Insertable decision node
#[derive(Insertable)]
#[diesel(table_name = decision_nodes)]
pub struct NewDecisionNode<'a> {
    pub change_id: &'a str,
    pub node_type: &'a str,
    pub title: &'a str,
    pub description: Option<&'a str>,
    pub status: &'a str,
    pub created_at: &'a str,
    pub updated_at: &'a str,
    pub metadata_json: Option<&'a str>,
}

/// Queryable decision node
#[derive(Queryable, Selectable, Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "ts-rs", derive(TS))]
#[cfg_attr(feature = "ts-rs", ts(export))]
#[diesel(table_name = decision_nodes)]
pub struct DecisionNode {
    pub id: i32,
    pub change_id: String,
    pub node_type: String,
    pub title: String,
    pub description: Option<String>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
    pub metadata_json: Option<String>,
}

/// Insertable decision edge
#[derive(Insertable)]
#[diesel(table_name = decision_edges)]
pub struct NewDecisionEdge<'a> {
    pub from_node_id: i32,
    pub to_node_id: i32,
    pub from_change_id: Option<&'a str>,
    pub to_change_id: Option<&'a str>,
    pub edge_type: &'a str,
    pub weight: Option<f64>,
    pub rationale: Option<&'a str>,
    pub created_at: &'a str,
}

/// Queryable decision edge
#[derive(Queryable, Selectable, Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "ts-rs", derive(TS))]
#[cfg_attr(feature = "ts-rs", ts(export))]
#[diesel(table_name = decision_edges)]
pub struct DecisionEdge {
    pub id: i32,
    pub from_node_id: i32,
    pub to_node_id: i32,
    pub from_change_id: Option<String>,
    pub to_change_id: Option<String>,
    pub edge_type: String,
    pub weight: Option<f64>,
    pub rationale: Option<String>,
    pub created_at: String,
}

/// Insertable decision context
#[derive(Insertable)]
#[diesel(table_name = decision_context)]
pub struct NewDecisionContext<'a> {
    pub node_id: i32,
    pub context_type: &'a str,
    pub content_json: &'a str,
    pub captured_at: &'a str,
}

/// Queryable decision context
#[derive(Queryable, Selectable, Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "ts-rs", derive(TS))]
#[cfg_attr(feature = "ts-rs", ts(export))]
#[diesel(table_name = decision_context)]
pub struct DecisionContext {
    pub id: i32,
    pub node_id: i32,
    pub context_type: String,
    pub content_json: String,
    pub captured_at: String,
}

/// Insertable session
#[derive(Insertable)]
#[diesel(table_name = decision_sessions)]
pub struct NewDecisionSession<'a> {
    pub name: Option<&'a str>,
    pub started_at: &'a str,
    pub ended_at: Option<&'a str>,
    pub root_node_id: Option<i32>,
    pub summary: Option<&'a str>,
}

/// Queryable session
#[derive(Queryable, Selectable, Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "ts-rs", derive(TS))]
#[cfg_attr(feature = "ts-rs", ts(export))]
#[diesel(table_name = decision_sessions)]
pub struct DecisionSession {
    pub id: i32,
    pub name: Option<String>,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub root_node_id: Option<i32>,
    pub summary: Option<String>,
}

// ============================================================================
// Command Log Models
// ============================================================================

/// Insertable command log entry
#[derive(Insertable)]
#[diesel(table_name = command_log)]
pub struct NewCommandLog<'a> {
    pub command: &'a str,
    pub description: Option<&'a str>,
    pub working_dir: Option<&'a str>,
    pub exit_code: Option<i32>,
    pub stdout: Option<&'a str>,
    pub stderr: Option<&'a str>,
    pub started_at: &'a str,
    pub completed_at: Option<&'a str>,
    pub duration_ms: Option<i32>,
    pub decision_node_id: Option<i32>,
}

/// Queryable command log entry
#[derive(Queryable, Selectable, Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "ts-rs", derive(TS))]
#[cfg_attr(feature = "ts-rs", ts(export))]
#[diesel(table_name = command_log)]
pub struct CommandLog {
    pub id: i32,
    pub command: String,
    pub description: Option<String>,
    pub working_dir: Option<String>,
    pub exit_code: Option<i32>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub duration_ms: Option<i32>,
    pub decision_node_id: Option<i32>,
}

// ============================================================================
// Roadmap Board Models
// ============================================================================

/// Checkbox state enum for type safety
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(TS))]
#[cfg_attr(feature = "ts-rs", ts(export))]
pub enum CheckboxState {
    /// Section header or item without checkbox
    None,
    /// Unchecked checkbox: - [ ]
    Unchecked,
    /// Checked checkbox: - [x]
    Checked,
}

impl CheckboxState {
    /// Convert to database string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            CheckboxState::None => "none",
            CheckboxState::Unchecked => "unchecked",
            CheckboxState::Checked => "checked",
        }
    }

    /// Parse from database string representation
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "checked" => CheckboxState::Checked,
            "unchecked" => CheckboxState::Unchecked,
            _ => CheckboxState::None,
        }
    }

    /// Convert from boolean (for checkbox items)
    pub fn from_bool(checked: bool) -> Self {
        if checked {
            CheckboxState::Checked
        } else {
            CheckboxState::Unchecked
        }
    }

    /// Check if this represents a checked state
    pub fn is_checked(&self) -> bool {
        matches!(self, CheckboxState::Checked)
    }
}

impl std::fmt::Display for CheckboxState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Insertable roadmap item
#[derive(Insertable)]
#[diesel(table_name = roadmap_items)]
pub struct NewRoadmapItem<'a> {
    pub change_id: &'a str,
    pub title: &'a str,
    pub description: Option<&'a str>,
    pub section: Option<&'a str>,
    pub parent_id: Option<i32>,
    pub checkbox_state: &'a str,
    pub github_issue_number: Option<i32>,
    pub github_issue_state: Option<&'a str>,
    pub outcome_node_id: Option<i32>,
    pub outcome_change_id: Option<&'a str>,
    pub markdown_line_start: Option<i32>,
    pub markdown_line_end: Option<i32>,
    pub content_hash: Option<&'a str>,
    pub created_at: &'a str,
    pub updated_at: &'a str,
    pub last_synced_at: Option<&'a str>,
}

/// Queryable roadmap item
#[derive(Queryable, Selectable, Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(TS))]
#[cfg_attr(feature = "ts-rs", ts(export))]
#[diesel(table_name = roadmap_items)]
pub struct RoadmapItem {
    pub id: i32,
    pub change_id: String,
    pub title: String,
    pub description: Option<String>,
    pub section: Option<String>,
    pub parent_id: Option<i32>,
    pub checkbox_state: String,
    pub github_issue_number: Option<i32>,
    pub github_issue_state: Option<String>,
    pub outcome_node_id: Option<i32>,
    pub outcome_change_id: Option<String>,
    pub markdown_line_start: Option<i32>,
    pub markdown_line_end: Option<i32>,
    pub content_hash: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub last_synced_at: Option<String>,
}

impl RoadmapItem {
    /// Get the checkbox state as a typed enum
    pub fn checkbox(&self) -> CheckboxState {
        CheckboxState::parse(&self.checkbox_state)
    }

    /// Check if this item is completed (checkbox checked)
    pub fn is_checked(&self) -> bool {
        self.checkbox().is_checked()
    }
}

/// Insertable roadmap sync state
#[derive(Insertable)]
#[diesel(table_name = roadmap_sync_state)]
pub struct NewRoadmapSyncState<'a> {
    pub roadmap_path: &'a str,
    pub roadmap_content_hash: Option<&'a str>,
    pub github_repo: Option<&'a str>,
    pub last_github_sync: Option<&'a str>,
    pub last_markdown_parse: Option<&'a str>,
    pub conflict_count: i32,
}

/// Queryable roadmap sync state
#[derive(Queryable, Selectable, Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "ts-rs", derive(TS))]
#[cfg_attr(feature = "ts-rs", ts(export))]
#[diesel(table_name = roadmap_sync_state)]
pub struct RoadmapSyncState {
    pub id: i32,
    pub roadmap_path: String,
    pub roadmap_content_hash: Option<String>,
    pub github_repo: Option<String>,
    pub last_github_sync: Option<String>,
    pub last_markdown_parse: Option<String>,
    pub conflict_count: i32,
}

/// Insertable roadmap conflict
#[derive(Insertable)]
#[diesel(table_name = roadmap_conflicts)]
pub struct NewRoadmapConflict<'a> {
    pub item_change_id: &'a str,
    pub conflict_type: &'a str,
    pub local_value: Option<&'a str>,
    pub remote_value: Option<&'a str>,
    pub resolution: Option<&'a str>,
    pub detected_at: &'a str,
    pub resolved_at: Option<&'a str>,
}

/// Queryable roadmap conflict
#[derive(Queryable, Selectable, Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "ts-rs", derive(TS))]
#[cfg_attr(feature = "ts-rs", ts(export))]
#[diesel(table_name = roadmap_conflicts)]
pub struct RoadmapConflict {
    pub id: i32,
    pub item_change_id: String,
    pub conflict_type: String,
    pub local_value: Option<String>,
    pub remote_value: Option<String>,
    pub resolution: Option<String>,
    pub detected_at: String,
    pub resolved_at: Option<String>,
}

// ============================================================================
// Helper structs for raw SQL queries
// ============================================================================

/// Helper for PRAGMA table_info queries
#[derive(QueryableByName, Debug)]
struct PragmaTableInfo {
    #[diesel(sql_type = diesel::sql_types::Integer)]
    #[allow(dead_code)]
    cid: i32,
    #[diesel(sql_type = diesel::sql_types::Text)]
    name: String,
    #[diesel(sql_type = diesel::sql_types::Text)]
    #[allow(dead_code)]
    r#type: String,
    #[diesel(sql_type = diesel::sql_types::Integer)]
    #[allow(dead_code)]
    notnull: i32,
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Text>)]
    #[allow(dead_code)]
    dflt_value: Option<String>,
    #[diesel(sql_type = diesel::sql_types::Integer)]
    #[allow(dead_code)]
    pk: i32,
}

/// Helper for node ID queries
#[derive(QueryableByName, Debug)]
struct NodeIdOnly {
    #[diesel(sql_type = diesel::sql_types::Integer)]
    id: i32,
}

/// Helper for sqlite_master table queries
#[derive(QueryableByName, Debug)]
#[allow(dead_code)]
struct TableInfo {
    #[diesel(sql_type = diesel::sql_types::Text)]
    name: String,
}

// ============================================================================
// Database Connection
// ============================================================================

type DbPool = Pool<ConnectionManager<SqliteConnection>>;
type DbConn = PooledConnection<ConnectionManager<SqliteConnection>>;

/// Database connection wrapper with connection pool
pub struct Database {
    pool: DbPool,
}

/// Error type for database operations
#[derive(Debug)]
pub enum DbError {
    Connection(String),
    Query(diesel::result::Error),
    Pool(diesel::r2d2::Error),
    Validation(String),
}

impl std::fmt::Display for DbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DbError::Connection(msg) => write!(f, "Connection error: {}", msg),
            DbError::Query(e) => write!(f, "Query error: {}", e),
            DbError::Pool(e) => write!(f, "Pool error: {}", e),
            DbError::Validation(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for DbError {}

impl From<diesel::result::Error> for DbError {
    fn from(e: diesel::result::Error) -> Self {
        DbError::Query(e)
    }
}

impl From<diesel::r2d2::Error> for DbError {
    fn from(e: diesel::r2d2::Error) -> Self {
        DbError::Pool(e)
    }
}

pub type Result<T> = std::result::Result<T, DbError>;

impl Database {
    /// Get the database path that will be used
    pub fn db_path() -> std::path::PathBuf {
        get_db_path()
    }

    /// Create a new database at a custom path
    pub fn new(path: &str) -> Result<Self> {
        Self::open_at(path)
    }

    /// Open database at default path (respects DECIDUOUS_DB_PATH env var)
    pub fn open() -> Result<Self> {
        let path = get_db_path();
        // Create parent directory if it doesn't exist
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent).ok();
            }
        }
        Self::open_at(&path)
    }

    /// Open database at specified path
    pub fn open_at<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path_str = path.as_ref().to_string_lossy().to_string();
        let manager = ConnectionManager::<SqliteConnection>::new(&path_str);
        let pool = Pool::builder()
            .max_size(5)
            .build(manager)
            .map_err(|e| DbError::Connection(e.to_string()))?;

        let db = Self { pool };
        // Auto-migrate FIRST - add change_id columns to existing databases before init_schema creates new tables
        let _ = db.migrate_add_change_ids_raw();
        db.init_schema()?;
        Ok(db)
    }

    /// Raw SQL migration that runs before Diesel ORM is used
    fn migrate_add_change_ids_raw(&self) -> Result<bool> {
        let mut conn = self.get_conn()?;

        // Check if decision_nodes table exists
        let tables: Vec<TableInfo> = diesel::sql_query(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='decision_nodes'"
        )
        .load::<TableInfo>(&mut conn)
        .unwrap_or_default();

        if tables.is_empty() {
            return Ok(false); // Table doesn't exist yet, init_schema will create it
        }

        // Check if change_id column exists in decision_nodes
        let columns: Vec<PragmaTableInfo> = diesel::sql_query(
            "PRAGMA table_info(decision_nodes)"
        )
        .load(&mut conn)
        .unwrap_or_default();

        let has_change_id = columns.iter().any(|c| c.name == "change_id");

        if !has_change_id {
            // Add change_id column to decision_nodes
            diesel::sql_query("ALTER TABLE decision_nodes ADD COLUMN change_id TEXT")
                .execute(&mut conn)?;
        }

        // Always backfill any NULL change_ids (handles both new columns and stragglers)
        let nodes: Vec<NodeIdOnly> = diesel::sql_query(
            "SELECT id FROM decision_nodes WHERE change_id IS NULL"
        )
        .load(&mut conn)
        .unwrap_or_default();

        if nodes.is_empty() && has_change_id {
            return Ok(false); // Already fully migrated
        }

        for node in nodes {
            let change_id = Uuid::new_v4().to_string();
            diesel::sql_query(format!(
                "UPDATE decision_nodes SET change_id = '{}' WHERE id = {}",
                change_id, node.id
            ))
            .execute(&mut conn)?;
        }

        // Check if edge columns need migration
        let edge_columns: Vec<PragmaTableInfo> = diesel::sql_query(
            "PRAGMA table_info(decision_edges)"
        )
        .load(&mut conn)
        .unwrap_or_default();

        let has_from_change_id = edge_columns.iter().any(|c| c.name == "from_change_id");

        if !has_from_change_id {
            diesel::sql_query("ALTER TABLE decision_edges ADD COLUMN from_change_id TEXT")
                .execute(&mut conn)?;
            diesel::sql_query("ALTER TABLE decision_edges ADD COLUMN to_change_id TEXT")
                .execute(&mut conn)?;

            // Backfill edge change_ids
            diesel::sql_query(
                "UPDATE decision_edges SET
                    from_change_id = (SELECT change_id FROM decision_nodes WHERE id = decision_edges.from_node_id),
                    to_change_id = (SELECT change_id FROM decision_nodes WHERE id = decision_edges.to_node_id)"
            )
            .execute(&mut conn)?;
        }

        Ok(true) // Migration performed
    }

    fn get_conn(&self) -> Result<DbConn> {
        self.pool.get().map_err(|e| DbError::Connection(e.to_string()))
    }

    fn init_schema(&self) -> Result<()> {
        let mut conn = self.get_conn()?;

        // Run raw SQL to create tables if they don't exist
        diesel::sql_query(r#"
            CREATE TABLE IF NOT EXISTS schema_versions (
                id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
                version TEXT NOT NULL UNIQUE,
                name TEXT NOT NULL,
                features TEXT NOT NULL,
                introduced_at TEXT NOT NULL
            )
        "#).execute(&mut conn)?;

        diesel::sql_query(r#"
            CREATE TABLE IF NOT EXISTS decision_nodes (
                id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
                change_id TEXT NOT NULL UNIQUE,
                node_type TEXT NOT NULL,
                title TEXT NOT NULL,
                description TEXT,
                status TEXT NOT NULL DEFAULT 'pending',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                metadata_json TEXT
            )
        "#).execute(&mut conn)?;

        diesel::sql_query(r#"
            CREATE TABLE IF NOT EXISTS decision_edges (
                id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
                from_node_id INTEGER NOT NULL,
                to_node_id INTEGER NOT NULL,
                from_change_id TEXT,
                to_change_id TEXT,
                edge_type TEXT NOT NULL,
                weight REAL DEFAULT 1.0,
                rationale TEXT,
                created_at TEXT NOT NULL,
                FOREIGN KEY (from_node_id) REFERENCES decision_nodes(id),
                FOREIGN KEY (to_node_id) REFERENCES decision_nodes(id),
                UNIQUE(from_node_id, to_node_id, edge_type)
            )
        "#).execute(&mut conn)?;

        diesel::sql_query(r#"
            CREATE TABLE IF NOT EXISTS decision_context (
                id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
                node_id INTEGER NOT NULL,
                context_type TEXT NOT NULL,
                content_json TEXT NOT NULL,
                captured_at TEXT NOT NULL,
                FOREIGN KEY (node_id) REFERENCES decision_nodes(id)
            )
        "#).execute(&mut conn)?;

        diesel::sql_query(r#"
            CREATE TABLE IF NOT EXISTS decision_sessions (
                id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
                name TEXT,
                started_at TEXT NOT NULL,
                ended_at TEXT,
                root_node_id INTEGER,
                summary TEXT,
                FOREIGN KEY (root_node_id) REFERENCES decision_nodes(id)
            )
        "#).execute(&mut conn)?;

        diesel::sql_query(r#"
            CREATE TABLE IF NOT EXISTS session_nodes (
                session_id INTEGER NOT NULL,
                node_id INTEGER NOT NULL,
                added_at TEXT NOT NULL,
                PRIMARY KEY (session_id, node_id),
                FOREIGN KEY (session_id) REFERENCES decision_sessions(id),
                FOREIGN KEY (node_id) REFERENCES decision_nodes(id)
            )
        "#).execute(&mut conn)?;

        diesel::sql_query(r#"
            CREATE TABLE IF NOT EXISTS command_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
                command TEXT NOT NULL,
                description TEXT,
                working_dir TEXT,
                exit_code INTEGER,
                stdout TEXT,
                stderr TEXT,
                started_at TEXT NOT NULL,
                completed_at TEXT,
                duration_ms INTEGER,
                decision_node_id INTEGER,
                FOREIGN KEY (decision_node_id) REFERENCES decision_nodes(id)
            )
        "#).execute(&mut conn)?;

        // Roadmap Board Tables
        diesel::sql_query(r#"
            CREATE TABLE IF NOT EXISTS roadmap_items (
                id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
                change_id TEXT NOT NULL UNIQUE,
                title TEXT NOT NULL,
                description TEXT,
                section TEXT,
                parent_id INTEGER,
                checkbox_state TEXT NOT NULL DEFAULT 'none',
                github_issue_number INTEGER,
                github_issue_state TEXT,
                outcome_node_id INTEGER,
                outcome_change_id TEXT,
                markdown_line_start INTEGER,
                markdown_line_end INTEGER,
                content_hash TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                last_synced_at TEXT,
                FOREIGN KEY (parent_id) REFERENCES roadmap_items(id),
                FOREIGN KEY (outcome_node_id) REFERENCES decision_nodes(id)
            )
        "#).execute(&mut conn)?;

        diesel::sql_query(r#"
            CREATE TABLE IF NOT EXISTS roadmap_sync_state (
                id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
                roadmap_path TEXT NOT NULL,
                roadmap_content_hash TEXT,
                github_repo TEXT,
                last_github_sync TEXT,
                last_markdown_parse TEXT,
                conflict_count INTEGER NOT NULL DEFAULT 0
            )
        "#).execute(&mut conn)?;

        diesel::sql_query(r#"
            CREATE TABLE IF NOT EXISTS roadmap_conflicts (
                id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
                item_change_id TEXT NOT NULL,
                conflict_type TEXT NOT NULL,
                local_value TEXT,
                remote_value TEXT,
                resolution TEXT,
                detected_at TEXT NOT NULL,
                resolved_at TEXT,
                FOREIGN KEY (item_change_id) REFERENCES roadmap_items(change_id)
            )
        "#).execute(&mut conn)?;

        // Create indexes
        diesel::sql_query("CREATE INDEX IF NOT EXISTS idx_nodes_type ON decision_nodes(node_type)").execute(&mut conn)?;
        diesel::sql_query("CREATE INDEX IF NOT EXISTS idx_nodes_status ON decision_nodes(status)").execute(&mut conn)?;
        diesel::sql_query("CREATE INDEX IF NOT EXISTS idx_nodes_change_id ON decision_nodes(change_id)").execute(&mut conn)?;
        diesel::sql_query("CREATE INDEX IF NOT EXISTS idx_edges_from ON decision_edges(from_node_id)").execute(&mut conn)?;
        diesel::sql_query("CREATE INDEX IF NOT EXISTS idx_edges_to ON decision_edges(to_node_id)").execute(&mut conn)?;
        diesel::sql_query("CREATE INDEX IF NOT EXISTS idx_edges_from_change ON decision_edges(from_change_id)").execute(&mut conn)?;
        diesel::sql_query("CREATE INDEX IF NOT EXISTS idx_edges_to_change ON decision_edges(to_change_id)").execute(&mut conn)?;
        diesel::sql_query("CREATE INDEX IF NOT EXISTS idx_command_started_at ON command_log(started_at)").execute(&mut conn)?;

        // Roadmap indexes
        diesel::sql_query("CREATE INDEX IF NOT EXISTS idx_roadmap_items_change_id ON roadmap_items(change_id)").execute(&mut conn)?;
        diesel::sql_query("CREATE INDEX IF NOT EXISTS idx_roadmap_items_section ON roadmap_items(section)").execute(&mut conn)?;
        diesel::sql_query("CREATE INDEX IF NOT EXISTS idx_roadmap_items_github_issue ON roadmap_items(github_issue_number)").execute(&mut conn)?;
        diesel::sql_query("CREATE INDEX IF NOT EXISTS idx_roadmap_items_outcome ON roadmap_items(outcome_change_id)").execute(&mut conn)?;
        diesel::sql_query("CREATE INDEX IF NOT EXISTS idx_roadmap_conflicts_item ON roadmap_conflicts(item_change_id)").execute(&mut conn)?;

        // Register current schema
        self.register_schema(&CURRENT_SCHEMA)?;
        Ok(())
    }

    fn register_schema(&self, schema: &DecisionSchema) -> Result<()> {
        let mut conn = self.get_conn()?;
        let now = chrono::Local::now().to_rfc3339();
        let features_json = serde_json::to_string(&schema.features).unwrap_or_default();

        let new_schema = NewSchemaVersion {
            version: &schema.version_string(),
            name: schema.name,
            features: &features_json,
            introduced_at: &now,
        };

        diesel::insert_or_ignore_into(schema_versions::table)
            .values(&new_schema)
            .execute(&mut conn)?;

        Ok(())
    }

    /// Migrate existing database to add change_id columns if missing
    pub fn migrate_add_change_ids(&self) -> Result<bool> {
        let mut conn = self.get_conn()?;

        // Check if change_id column exists in decision_nodes
        let columns: Vec<(String,)> = diesel::sql_query(
            "PRAGMA table_info(decision_nodes)"
        )
        .load::<PragmaTableInfo>(&mut conn)
        .map(|rows| rows.into_iter().map(|r| (r.name,)).collect())
        .unwrap_or_default();

        let has_change_id = columns.iter().any(|(name,)| name == "change_id");

        if has_change_id {
            return Ok(false); // Already migrated
        }

        // Add change_id column to decision_nodes
        diesel::sql_query("ALTER TABLE decision_nodes ADD COLUMN change_id TEXT")
            .execute(&mut conn)?;

        // Backfill change_id with UUIDs for existing nodes
        let nodes: Vec<(i32,)> = diesel::sql_query(
            "SELECT id FROM decision_nodes WHERE change_id IS NULL"
        )
        .load::<NodeIdOnly>(&mut conn)
        .map(|rows| rows.into_iter().map(|r| (r.id,)).collect())
        .unwrap_or_default();

        for (node_id,) in nodes {
            let change_id = Uuid::new_v4().to_string();
            diesel::sql_query(format!(
                "UPDATE decision_nodes SET change_id = '{}' WHERE id = {}",
                change_id, node_id
            ))
            .execute(&mut conn)?;
        }

        // Create unique index on change_id
        diesel::sql_query("CREATE UNIQUE INDEX IF NOT EXISTS idx_nodes_change_id_unique ON decision_nodes(change_id)")
            .execute(&mut conn)?;

        // Add from_change_id and to_change_id columns to decision_edges
        let edge_columns: Vec<(String,)> = diesel::sql_query(
            "PRAGMA table_info(decision_edges)"
        )
        .load::<PragmaTableInfo>(&mut conn)
        .map(|rows| rows.into_iter().map(|r| (r.name,)).collect())
        .unwrap_or_default();

        let has_from_change_id = edge_columns.iter().any(|(name,)| name == "from_change_id");

        if !has_from_change_id {
            diesel::sql_query("ALTER TABLE decision_edges ADD COLUMN from_change_id TEXT")
                .execute(&mut conn)?;
            diesel::sql_query("ALTER TABLE decision_edges ADD COLUMN to_change_id TEXT")
                .execute(&mut conn)?;

            // Backfill edge change_ids from node change_ids
            diesel::sql_query(
                "UPDATE decision_edges SET
                    from_change_id = (SELECT change_id FROM decision_nodes WHERE id = decision_edges.from_node_id),
                    to_change_id = (SELECT change_id FROM decision_nodes WHERE id = decision_edges.to_node_id)"
            )
            .execute(&mut conn)?;

            // Create indexes
            diesel::sql_query("CREATE INDEX IF NOT EXISTS idx_edges_from_change ON decision_edges(from_change_id)")
                .execute(&mut conn)?;
            diesel::sql_query("CREATE INDEX IF NOT EXISTS idx_edges_to_change ON decision_edges(to_change_id)")
                .execute(&mut conn)?;
        }

        Ok(true) // Migration performed
    }

    // ========================================================================
    // Decision Graph Operations
    // ========================================================================

    /// Create a new decision node
    pub fn create_node(&self, node_type: &str, title: &str, description: Option<&str>, confidence: Option<u8>, commit: Option<&str>) -> Result<i32> {
        self.create_node_full(node_type, title, description, confidence, commit, None, None, None)
    }

    /// Create a decision node with full metadata (including prompt and files)
    pub fn create_node_full(
        &self,
        node_type: &str,
        title: &str,
        description: Option<&str>,
        confidence: Option<u8>,
        commit: Option<&str>,
        prompt: Option<&str>,
        files: Option<&str>,
        branch: Option<&str>,
    ) -> Result<i32> {
        let mut conn = self.get_conn()?;
        let now = chrono::Local::now().to_rfc3339();
        let change_id = Uuid::new_v4().to_string();

        // Build metadata JSON with all optional fields
        let metadata = build_metadata_json(confidence, commit, prompt, files, branch);

        let new_node = NewDecisionNode {
            change_id: &change_id,
            node_type,
            title,
            description,
            status: "pending",
            created_at: &now,
            updated_at: &now,
            metadata_json: metadata.as_deref(),
        };

        diesel::insert_into(decision_nodes::table)
            .values(&new_node)
            .execute(&mut conn)?;

        let id: i32 = diesel::select(diesel::dsl::sql::<diesel::sql_types::Integer>("last_insert_rowid()"))
            .first(&mut conn)?;

        Ok(id)
    }

    /// Add a node (alias for create_node for doc examples)
    pub fn add_node(&self, node_type: &str, title: &str, description: Option<&str>, confidence: Option<u8>, commit: Option<&str>) -> Result<i32> {
        self.create_node(node_type, title, description, confidence, commit)
    }

    /// Create a node with a specific change_id (for patch application)
    pub fn create_node_with_change_id(
        &self,
        change_id: &str,
        node_type: &str,
        title: &str,
        description: Option<&str>,
        confidence: Option<u8>,
        commit: Option<&str>,
        prompt: Option<&str>,
        files: Option<&str>,
        branch: Option<&str>,
    ) -> Result<i32> {
        let mut conn = self.get_conn()?;
        let now = chrono::Local::now().to_rfc3339();

        // Build metadata JSON with all optional fields
        let metadata = build_metadata_json(confidence, commit, prompt, files, branch);

        let new_node = NewDecisionNode {
            change_id,
            node_type,
            title,
            description,
            status: "pending",
            created_at: &now,
            updated_at: &now,
            metadata_json: metadata.as_deref(),
        };

        diesel::insert_into(decision_nodes::table)
            .values(&new_node)
            .execute(&mut conn)?;

        let id: i32 = diesel::select(diesel::dsl::sql::<diesel::sql_types::Integer>("last_insert_rowid()"))
            .first(&mut conn)?;

        Ok(id)
    }

    /// Create an edge between nodes
    pub fn create_edge(&self, from_id: i32, to_id: i32, edge_type: &str, rationale: Option<&str>) -> Result<i32> {
        let mut conn = self.get_conn()?;

        // Validate both nodes exist and get their change_ids
        let from_node = decision_nodes::table
            .filter(decision_nodes::id.eq(from_id))
            .first::<DecisionNode>(&mut conn)
            .ok();
        let to_node = decision_nodes::table
            .filter(decision_nodes::id.eq(to_id))
            .first::<DecisionNode>(&mut conn)
            .ok();

        let from_change_id = from_node.as_ref().map(|n| n.change_id.clone());
        let to_change_id = to_node.as_ref().map(|n| n.change_id.clone());

        if from_node.is_none() && to_node.is_none() {
            return Err(DbError::Validation(format!("Both nodes {} and {} do not exist. Run 'deciduous nodes' to see existing nodes.", from_id, to_id)));
        } else if from_node.is_none() {
            return Err(DbError::Validation(format!("Source node {} does not exist. Run 'deciduous nodes' to see existing nodes.", from_id)));
        } else if to_node.is_none() {
            return Err(DbError::Validation(format!("Target node {} does not exist. Run 'deciduous nodes' to see existing nodes.", to_id)));
        }

        let now = chrono::Local::now().to_rfc3339();

        let new_edge = NewDecisionEdge {
            from_node_id: from_id,
            to_node_id: to_id,
            from_change_id: from_change_id.as_deref(),
            to_change_id: to_change_id.as_deref(),
            edge_type,
            weight: Some(1.0),
            rationale,
            created_at: &now,
        };

        diesel::insert_into(decision_edges::table)
            .values(&new_edge)
            .execute(&mut conn)?;

        let id: i32 = diesel::select(diesel::dsl::sql::<diesel::sql_types::Integer>("last_insert_rowid()"))
            .first(&mut conn)?;

        Ok(id)
    }

    /// Add an edge (alias for create_edge for doc examples)
    pub fn add_edge(&self, from_id: i32, to_id: i32, edge_type: &str, rationale: Option<&str>) -> Result<i32> {
        self.create_edge(from_id, to_id, edge_type, rationale)
    }

    /// Update node status
    pub fn update_node_status(&self, node_id: i32, status: &str) -> Result<()> {
        let mut conn = self.get_conn()?;
        let now = chrono::Local::now().to_rfc3339();

        diesel::update(decision_nodes::table.filter(decision_nodes::id.eq(node_id)))
            .set((
                decision_nodes::status.eq(status),
                decision_nodes::updated_at.eq(&now),
            ))
            .execute(&mut conn)?;

        Ok(())
    }

    /// Update a node's commit hash in metadata_json
    pub fn update_node_commit(&self, node_id: i32, commit_hash: &str) -> Result<()> {
        let mut conn = self.get_conn()?;
        let now = chrono::Local::now().to_rfc3339();

        // Get current metadata
        let current_meta: Option<String> = decision_nodes::table
            .filter(decision_nodes::id.eq(node_id))
            .select(decision_nodes::metadata_json)
            .first(&mut conn)?;

        // Parse existing metadata or create new
        let mut meta: serde_json::Value = current_meta
            .as_ref()
            .and_then(|m| serde_json::from_str(m).ok())
            .unwrap_or_else(|| serde_json::json!({}));

        // Add/update commit field
        if let Some(obj) = meta.as_object_mut() {
            obj.insert("commit".to_string(), serde_json::json!(commit_hash));
        }

        let new_meta = serde_json::to_string(&meta)
            .map_err(|e| DbError::Validation(format!("JSON serialization error: {}", e)))?;

        diesel::update(decision_nodes::table.filter(decision_nodes::id.eq(node_id)))
            .set((
                decision_nodes::metadata_json.eq(Some(new_meta)),
                decision_nodes::updated_at.eq(&now),
            ))
            .execute(&mut conn)?;

        Ok(())
    }

    /// Get all nodes
    pub fn get_all_nodes(&self) -> Result<Vec<DecisionNode>> {
        let mut conn = self.get_conn()?;
        let nodes = decision_nodes::table
            .order(decision_nodes::created_at.asc())
            .load::<DecisionNode>(&mut conn)?;
        Ok(nodes)
    }

    /// Get all edges
    pub fn get_all_edges(&self) -> Result<Vec<DecisionEdge>> {
        let mut conn = self.get_conn()?;
        let edges = decision_edges::table
            .order(decision_edges::created_at.asc())
            .load::<DecisionEdge>(&mut conn)?;
        Ok(edges)
    }

    /// Get children of a node (outgoing edges)
    pub fn get_node_children(&self, node_id: i32) -> Result<Vec<DecisionNode>> {
        let mut conn = self.get_conn()?;

        let child_ids: Vec<i32> = decision_edges::table
            .filter(decision_edges::from_node_id.eq(node_id))
            .select(decision_edges::to_node_id)
            .load(&mut conn)?;

        let children = decision_nodes::table
            .filter(decision_nodes::id.eq_any(child_ids))
            .load::<DecisionNode>(&mut conn)?;

        Ok(children)
    }

    /// Get parents of a node (incoming edges)
    pub fn get_node_parents(&self, node_id: i32) -> Result<Vec<DecisionNode>> {
        let mut conn = self.get_conn()?;

        let parent_ids: Vec<i32> = decision_edges::table
            .filter(decision_edges::to_node_id.eq(node_id))
            .select(decision_edges::from_node_id)
            .load(&mut conn)?;

        let parents = decision_nodes::table
            .filter(decision_nodes::id.eq_any(parent_ids))
            .load::<DecisionNode>(&mut conn)?;

        Ok(parents)
    }

    /// Get full graph as JSON-serializable structure
    pub fn get_graph(&self) -> Result<DecisionGraph> {
        let nodes = self.get_all_nodes()?;
        let edges = self.get_all_edges()?;
        Ok(DecisionGraph { nodes, edges })
    }

    // ========================================================================
    // Command Log Operations
    // ========================================================================

    /// Log a command execution
    pub fn log_command(&self, command: &str, description: Option<&str>, working_dir: Option<&str>) -> Result<i32> {
        let mut conn = self.get_conn()?;
        let now = chrono::Local::now().to_rfc3339();

        let new_log = NewCommandLog {
            command,
            description,
            working_dir,
            exit_code: None,
            stdout: None,
            stderr: None,
            started_at: &now,
            completed_at: None,
            duration_ms: None,
            decision_node_id: None,
        };

        diesel::insert_into(command_log::table)
            .values(&new_log)
            .execute(&mut conn)?;

        let id: i32 = diesel::select(diesel::dsl::sql::<diesel::sql_types::Integer>("last_insert_rowid()"))
            .first(&mut conn)?;

        Ok(id)
    }

    /// Complete a command log entry
    pub fn complete_command(
        &self,
        log_id: i32,
        exit_code: i32,
        stdout: Option<&str>,
        stderr: Option<&str>,
        duration_ms: i32,
    ) -> Result<()> {
        let mut conn = self.get_conn()?;
        let now = chrono::Local::now().to_rfc3339();

        diesel::update(command_log::table.filter(command_log::id.eq(log_id)))
            .set((
                command_log::exit_code.eq(Some(exit_code)),
                command_log::stdout.eq(stdout),
                command_log::stderr.eq(stderr),
                command_log::completed_at.eq(Some(&now)),
                command_log::duration_ms.eq(Some(duration_ms)),
            ))
            .execute(&mut conn)?;

        Ok(())
    }

    /// Get recent commands
    pub fn get_recent_commands(&self, limit: i64) -> Result<Vec<CommandLog>> {
        let mut conn = self.get_conn()?;
        let commands = command_log::table
            .order(command_log::started_at.desc())
            .limit(limit)
            .load::<CommandLog>(&mut conn)?;
        Ok(commands)
    }

    // ========================================================================
    // Roadmap Board Operations
    // ========================================================================

    /// Create a new roadmap item
    pub fn create_roadmap_item(
        &self,
        title: &str,
        description: Option<&str>,
        section: Option<&str>,
        parent_id: Option<i32>,
        checkbox_state: &str,
    ) -> Result<i32> {
        let mut conn = self.get_conn()?;
        let now = chrono::Local::now().to_rfc3339();
        let change_id = Uuid::new_v4().to_string();

        let new_item = NewRoadmapItem {
            change_id: &change_id,
            title,
            description,
            section,
            parent_id,
            checkbox_state,
            github_issue_number: None,
            github_issue_state: None,
            outcome_node_id: None,
            outcome_change_id: None,
            markdown_line_start: None,
            markdown_line_end: None,
            content_hash: None,
            created_at: &now,
            updated_at: &now,
            last_synced_at: None,
        };

        diesel::insert_into(roadmap_items::table)
            .values(&new_item)
            .execute(&mut conn)?;

        let id: i32 = diesel::select(diesel::dsl::sql::<diesel::sql_types::Integer>("last_insert_rowid()"))
            .first(&mut conn)?;

        Ok(id)
    }

    /// Create a roadmap item with full metadata (for sync operations)
    pub fn create_roadmap_item_full(
        &self,
        change_id: &str,
        title: &str,
        description: Option<&str>,
        section: Option<&str>,
        parent_id: Option<i32>,
        checkbox_state: &str,
        github_issue_number: Option<i32>,
        github_issue_state: Option<&str>,
        outcome_node_id: Option<i32>,
        outcome_change_id: Option<&str>,
        markdown_line_start: Option<i32>,
        markdown_line_end: Option<i32>,
        content_hash: Option<&str>,
    ) -> Result<i32> {
        let mut conn = self.get_conn()?;
        let now = chrono::Local::now().to_rfc3339();

        let new_item = NewRoadmapItem {
            change_id,
            title,
            description,
            section,
            parent_id,
            checkbox_state,
            github_issue_number,
            github_issue_state,
            outcome_node_id,
            outcome_change_id,
            markdown_line_start,
            markdown_line_end,
            content_hash,
            created_at: &now,
            updated_at: &now,
            last_synced_at: None,
        };

        diesel::insert_into(roadmap_items::table)
            .values(&new_item)
            .execute(&mut conn)?;

        let id: i32 = diesel::select(diesel::dsl::sql::<diesel::sql_types::Integer>("last_insert_rowid()"))
            .first(&mut conn)?;

        Ok(id)
    }

    /// Get all roadmap items
    pub fn get_all_roadmap_items(&self) -> Result<Vec<RoadmapItem>> {
        let mut conn = self.get_conn()?;
        let items = roadmap_items::table
            .order(roadmap_items::created_at.asc())
            .load::<RoadmapItem>(&mut conn)?;
        Ok(items)
    }

    /// Get roadmap items by section
    pub fn get_roadmap_items_by_section(&self, section: &str) -> Result<Vec<RoadmapItem>> {
        let mut conn = self.get_conn()?;
        let items = roadmap_items::table
            .filter(roadmap_items::section.eq(section))
            .order(roadmap_items::created_at.asc())
            .load::<RoadmapItem>(&mut conn)?;
        Ok(items)
    }

    /// Get a roadmap item by change_id
    pub fn get_roadmap_item_by_change_id(&self, change_id: &str) -> Result<Option<RoadmapItem>> {
        let mut conn = self.get_conn()?;
        let item = roadmap_items::table
            .filter(roadmap_items::change_id.eq(change_id))
            .first::<RoadmapItem>(&mut conn)
            .optional()?;
        Ok(item)
    }

    /// Update a roadmap item's GitHub issue info
    pub fn update_roadmap_item_github(
        &self,
        item_id: i32,
        issue_number: Option<i32>,
        issue_state: Option<&str>,
    ) -> Result<()> {
        let mut conn = self.get_conn()?;
        let now = chrono::Local::now().to_rfc3339();

        diesel::update(roadmap_items::table.filter(roadmap_items::id.eq(item_id)))
            .set((
                roadmap_items::github_issue_number.eq(issue_number),
                roadmap_items::github_issue_state.eq(issue_state),
                roadmap_items::updated_at.eq(&now),
            ))
            .execute(&mut conn)?;

        Ok(())
    }

    /// Link a roadmap item to a decision graph outcome node
    pub fn link_roadmap_to_outcome(
        &self,
        item_id: i32,
        outcome_node_id: i32,
        outcome_change_id: &str,
    ) -> Result<()> {
        let mut conn = self.get_conn()?;
        let now = chrono::Local::now().to_rfc3339();

        diesel::update(roadmap_items::table.filter(roadmap_items::id.eq(item_id)))
            .set((
                roadmap_items::outcome_node_id.eq(Some(outcome_node_id)),
                roadmap_items::outcome_change_id.eq(Some(outcome_change_id)),
                roadmap_items::updated_at.eq(&now),
            ))
            .execute(&mut conn)?;

        Ok(())
    }

    /// Unlink a roadmap item from its outcome node
    pub fn unlink_roadmap_from_outcome(&self, item_id: i32) -> Result<()> {
        let mut conn = self.get_conn()?;
        let now = chrono::Local::now().to_rfc3339();

        diesel::update(roadmap_items::table.filter(roadmap_items::id.eq(item_id)))
            .set((
                roadmap_items::outcome_node_id.eq(None::<i32>),
                roadmap_items::outcome_change_id.eq(None::<String>),
                roadmap_items::updated_at.eq(&now),
            ))
            .execute(&mut conn)?;

        Ok(())
    }

    /// Update a roadmap item's checkbox state
    pub fn update_roadmap_item_checkbox(&self, item_id: i32, checkbox_state: &str) -> Result<()> {
        let mut conn = self.get_conn()?;
        let now = chrono::Local::now().to_rfc3339();

        diesel::update(roadmap_items::table.filter(roadmap_items::id.eq(item_id)))
            .set((
                roadmap_items::checkbox_state.eq(checkbox_state),
                roadmap_items::updated_at.eq(&now),
            ))
            .execute(&mut conn)?;

        Ok(())
    }

    /// Update last synced timestamp for a roadmap item
    pub fn update_roadmap_item_synced(&self, item_id: i32) -> Result<()> {
        let mut conn = self.get_conn()?;
        let now = chrono::Local::now().to_rfc3339();

        diesel::update(roadmap_items::table.filter(roadmap_items::id.eq(item_id)))
            .set((
                roadmap_items::last_synced_at.eq(Some(&now)),
                roadmap_items::updated_at.eq(&now),
            ))
            .execute(&mut conn)?;

        Ok(())
    }

    /// Get or create roadmap sync state
    pub fn get_or_create_sync_state(&self, roadmap_path: &str) -> Result<RoadmapSyncState> {
        let mut conn = self.get_conn()?;

        // Try to find existing state
        let existing = roadmap_sync_state::table
            .filter(roadmap_sync_state::roadmap_path.eq(roadmap_path))
            .first::<RoadmapSyncState>(&mut conn)
            .optional()?;

        if let Some(state) = existing {
            return Ok(state);
        }

        // Create new state
        let new_state = NewRoadmapSyncState {
            roadmap_path,
            roadmap_content_hash: None,
            github_repo: None,
            last_github_sync: None,
            last_markdown_parse: None,
            conflict_count: 0,
        };

        diesel::insert_into(roadmap_sync_state::table)
            .values(&new_state)
            .execute(&mut conn)?;

        roadmap_sync_state::table
            .filter(roadmap_sync_state::roadmap_path.eq(roadmap_path))
            .first::<RoadmapSyncState>(&mut conn)
            .map_err(|e| e.into())
    }

    /// Update sync state after a sync operation
    pub fn update_sync_state(
        &self,
        state_id: i32,
        content_hash: Option<&str>,
        github_repo: Option<&str>,
        github_synced: bool,
        markdown_parsed: bool,
        conflict_count: i32,
    ) -> Result<()> {
        let mut conn = self.get_conn()?;
        let now = chrono::Local::now().to_rfc3339();

        let last_github = if github_synced { Some(now.clone()) } else { None };
        let last_parse = if markdown_parsed { Some(now) } else { None };

        diesel::update(roadmap_sync_state::table.filter(roadmap_sync_state::id.eq(state_id)))
            .set((
                roadmap_sync_state::roadmap_content_hash.eq(content_hash),
                roadmap_sync_state::github_repo.eq(github_repo),
                roadmap_sync_state::last_github_sync.eq(last_github),
                roadmap_sync_state::last_markdown_parse.eq(last_parse),
                roadmap_sync_state::conflict_count.eq(conflict_count),
            ))
            .execute(&mut conn)?;

        Ok(())
    }

    /// Create a conflict record
    pub fn create_roadmap_conflict(
        &self,
        item_change_id: &str,
        conflict_type: &str,
        local_value: Option<&str>,
        remote_value: Option<&str>,
    ) -> Result<i32> {
        let mut conn = self.get_conn()?;
        let now = chrono::Local::now().to_rfc3339();

        let new_conflict = NewRoadmapConflict {
            item_change_id,
            conflict_type,
            local_value,
            remote_value,
            resolution: None,
            detected_at: &now,
            resolved_at: None,
        };

        diesel::insert_into(roadmap_conflicts::table)
            .values(&new_conflict)
            .execute(&mut conn)?;

        let id: i32 = diesel::select(diesel::dsl::sql::<diesel::sql_types::Integer>("last_insert_rowid()"))
            .first(&mut conn)?;

        Ok(id)
    }

    /// Get all unresolved conflicts
    pub fn get_unresolved_conflicts(&self) -> Result<Vec<RoadmapConflict>> {
        let mut conn = self.get_conn()?;
        let conflicts = roadmap_conflicts::table
            .filter(roadmap_conflicts::resolution.is_null())
            .order(roadmap_conflicts::detected_at.desc())
            .load::<RoadmapConflict>(&mut conn)?;
        Ok(conflicts)
    }

    /// Resolve a conflict
    pub fn resolve_roadmap_conflict(&self, conflict_id: i32, resolution: &str) -> Result<()> {
        let mut conn = self.get_conn()?;
        let now = chrono::Local::now().to_rfc3339();

        diesel::update(roadmap_conflicts::table.filter(roadmap_conflicts::id.eq(conflict_id)))
            .set((
                roadmap_conflicts::resolution.eq(Some(resolution)),
                roadmap_conflicts::resolved_at.eq(Some(&now)),
            ))
            .execute(&mut conn)?;

        Ok(())
    }

    /// Delete a roadmap item by ID
    pub fn delete_roadmap_item(&self, item_id: i32) -> Result<()> {
        let mut conn = self.get_conn()?;
        diesel::delete(roadmap_items::table.filter(roadmap_items::id.eq(item_id)))
            .execute(&mut conn)?;
        Ok(())
    }

    /// Check if a roadmap item is complete (has outcome AND issue closed)
    pub fn check_roadmap_item_completion(&self, item_id: i32) -> Result<(bool, bool, bool)> {
        let mut conn = self.get_conn()?;

        let item = roadmap_items::table
            .filter(roadmap_items::id.eq(item_id))
            .first::<RoadmapItem>(&mut conn)?;

        let has_outcome = item.outcome_change_id.is_some();
        let issue_closed = item.github_issue_state.as_deref() == Some("closed");
        let is_complete = has_outcome && issue_closed;

        Ok((is_complete, has_outcome, issue_closed))
    }
}

// ============================================================================
// Additional Types
// ============================================================================

/// Summary statistics from the database (kept for compatibility)
#[derive(Debug, Clone, serde::Serialize)]
pub struct DbSummary {
    pub total_nodes: i32,
    pub total_edges: i32,
}

/// Alias for backwards compatibility
pub type DbRecord = DecisionNode;

/// Full decision graph for serialization
#[derive(Debug, Clone, serde::Serialize)]
pub struct DecisionGraph {
    pub nodes: Vec<DecisionNode>,
    pub edges: Vec<DecisionEdge>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // === build_metadata_json Tests ===

    #[test]
    fn test_build_metadata_empty() {
        let result = build_metadata_json(None, None, None, None, None);
        assert!(result.is_none());
    }

    #[test]
    fn test_build_metadata_confidence_only() {
        let result = build_metadata_json(Some(85), None, None, None, None);
        assert!(result.is_some());
        let json: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(json.get("confidence").unwrap(), 85);
    }

    #[test]
    fn test_build_metadata_confidence_clamped() {
        let result = build_metadata_json(Some(150), None, None, None, None);
        let json: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        // Should be clamped to 100
        assert_eq!(json.get("confidence").unwrap(), 100);
    }

    #[test]
    fn test_build_metadata_commit() {
        let result = build_metadata_json(None, Some("abc123"), None, None, None);
        let json: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(json.get("commit").unwrap(), "abc123");
    }

    #[test]
    fn test_build_metadata_prompt() {
        let result = build_metadata_json(None, None, Some("User asked: do X"), None, None);
        let json: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(json.get("prompt").unwrap(), "User asked: do X");
    }

    #[test]
    fn test_build_metadata_files() {
        let result = build_metadata_json(None, None, None, Some("a.rs, b.rs, c.rs"), None);
        let json: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        let files = json.get("files").unwrap().as_array().unwrap();
        assert_eq!(files.len(), 3);
        assert_eq!(files[0], "a.rs");
        assert_eq!(files[1], "b.rs");
        assert_eq!(files[2], "c.rs");
    }

    #[test]
    fn test_build_metadata_branch() {
        let result = build_metadata_json(None, None, None, None, Some("feature-x"));
        let json: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(json.get("branch").unwrap(), "feature-x");
    }

    #[test]
    fn test_build_metadata_all_fields() {
        let result = build_metadata_json(
            Some(90),
            Some("def456"),
            Some("User prompt"),
            Some("x.rs"),
            Some("main"),
        );
        let json: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();

        assert_eq!(json.get("confidence").unwrap(), 90);
        assert_eq!(json.get("commit").unwrap(), "def456");
        assert_eq!(json.get("prompt").unwrap(), "User prompt");
        assert_eq!(json.get("branch").unwrap(), "main");
        assert!(json.get("files").unwrap().as_array().is_some());
    }

    // === DecisionSchema Tests ===

    #[test]
    fn test_schema_version_string() {
        let schema = DecisionSchema {
            major: 1,
            minor: 2,
            patch: 3,
            name: "test",
            features: &[],
        };
        assert_eq!(schema.version_string(), "1.2.3");
    }

    #[test]
    fn test_schema_compatibility_same_major() {
        let schema1 = DecisionSchema {
            major: 1,
            minor: 0,
            patch: 0,
            name: "test",
            features: &[],
        };
        let schema2 = DecisionSchema {
            major: 1,
            minor: 5,
            patch: 3,
            name: "test",
            features: &[],
        };
        assert!(schema1.is_compatible_with(&schema2));
    }

    #[test]
    fn test_schema_incompatibility_different_major() {
        let schema1 = DecisionSchema {
            major: 1,
            minor: 0,
            patch: 0,
            name: "test",
            features: &[],
        };
        let schema2 = DecisionSchema {
            major: 2,
            minor: 0,
            patch: 0,
            name: "test",
            features: &[],
        };
        assert!(!schema1.is_compatible_with(&schema2));
    }

    #[test]
    fn test_schema_is_newer_than() {
        let old = DecisionSchema {
            major: 1,
            minor: 0,
            patch: 0,
            name: "test",
            features: &[],
        };
        let new = DecisionSchema {
            major: 1,
            minor: 1,
            patch: 0,
            name: "test",
            features: &[],
        };
        assert!(new.is_newer_than(&old));
        assert!(!old.is_newer_than(&new));
        assert!(!old.is_newer_than(&old));
    }

    // === Current Schema Tests ===

    #[test]
    fn test_current_schema() {
        assert_eq!(CURRENT_SCHEMA.major, 1);
        assert_eq!(CURRENT_SCHEMA.name, "decision-graph");
        assert!(CURRENT_SCHEMA.features.contains(&"decision_nodes"));
        assert!(CURRENT_SCHEMA.features.contains(&"decision_edges"));
    }

    // === update_node_commit Tests ===

    #[test]
    fn test_update_node_commit_new_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let db = Database::new(db_path.to_str().unwrap()).unwrap();

        // Create a node without metadata
        let node_id = db.create_node("action", "Test action", None, None, None).unwrap();

        // Update with commit
        db.update_node_commit(node_id, "abc123def456").unwrap();

        // Verify
        let nodes = db.get_all_nodes().unwrap();
        let node = nodes.iter().find(|n| n.id == node_id).unwrap();
        let meta: serde_json::Value = serde_json::from_str(node.metadata_json.as_ref().unwrap()).unwrap();
        assert_eq!(meta.get("commit").unwrap(), "abc123def456");
    }

    #[test]
    fn test_update_node_commit_preserves_existing_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let db = Database::new(db_path.to_str().unwrap()).unwrap();

        // Create a node with existing metadata (confidence and branch)
        let node_id = db.create_node_full(
            "action",
            "Test action",
            None,
            Some(85),
            None,
            None,
            None,
            Some("feature-x"),
        ).unwrap();

        // Update with commit
        db.update_node_commit(node_id, "def789").unwrap();

        // Verify commit was added and other fields preserved
        let nodes = db.get_all_nodes().unwrap();
        let node = nodes.iter().find(|n| n.id == node_id).unwrap();
        let meta: serde_json::Value = serde_json::from_str(node.metadata_json.as_ref().unwrap()).unwrap();

        assert_eq!(meta.get("commit").unwrap(), "def789");
        assert_eq!(meta.get("confidence").unwrap(), 85);
        assert_eq!(meta.get("branch").unwrap(), "feature-x");
    }

    #[test]
    fn test_update_node_commit_overwrites_existing_commit() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let db = Database::new(db_path.to_str().unwrap()).unwrap();

        // Create a node with an existing commit
        let node_id = db.create_node_full(
            "outcome",
            "Test outcome",
            None,
            None,
            Some("old_commit_hash"),
            None,
            None,
            None,
        ).unwrap();

        // Update with new commit
        db.update_node_commit(node_id, "new_commit_hash").unwrap();

        // Verify commit was overwritten
        let nodes = db.get_all_nodes().unwrap();
        let node = nodes.iter().find(|n| n.id == node_id).unwrap();
        let meta: serde_json::Value = serde_json::from_str(node.metadata_json.as_ref().unwrap()).unwrap();

        assert_eq!(meta.get("commit").unwrap(), "new_commit_hash");
    }
}
