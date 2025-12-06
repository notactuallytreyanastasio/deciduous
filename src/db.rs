//! SQLite database with Diesel ORM
//!
//! Stores analysis results, decision graphs, and command logs.
//! Uses embedded migrations for schema management.

use crate::analyzer::{AnalysisResult, Verdict};
use crate::schema::*;
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool, PooledConnection};
use diesel::sqlite::SqliteConnection;
use std::path::Path;

const DEFAULT_DB_PATH: &str = "losselot.db";

/// Current analysis schema version
pub const CURRENT_SCHEMA: AnalysisSchema = AnalysisSchema {
    major: 1,
    minor: 1,
    patch: 0,
    name: "lofi-detection",
    features: &[
        "binary_analysis",
        "spectral_analysis",
        "lowpass_detection",
        "encoder_chain_detection",
        "lofi_detection",
        "cutoff_variance",
        "rolloff_slope",
        "transition_width",
        "natural_rolloff",
    ],
};

/// Describes the version and capabilities of an analysis
#[derive(Debug, Clone)]
pub struct AnalysisSchema {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub name: &'static str,
    pub features: &'static [&'static str],
}

impl AnalysisSchema {
    pub fn version_string(&self) -> String {
        format!("{}.{}.{}", self.major, self.minor, self.patch)
    }

    pub fn is_compatible_with(&self, other: &AnalysisSchema) -> bool {
        self.major == other.major
    }

    pub fn is_newer_than(&self, other: &AnalysisSchema) -> bool {
        (self.major, self.minor, self.patch) > (other.major, other.minor, other.patch)
    }

    pub fn has_feature(&self, feature: &str) -> bool {
        self.features.contains(&feature)
    }
}

impl std::fmt::Display for AnalysisSchema {
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

/// Insertable analysis result
#[derive(Insertable)]
#[diesel(table_name = analysis_results)]
pub struct NewAnalysisResult<'a> {
    pub file_path: &'a str,
    pub file_name: &'a str,
    pub analyzed_at: &'a str,
    pub schema_version: &'a str,
    pub verdict: &'a str,
    pub combined_score: i32,
    pub spectral_score: i32,
    pub binary_score: i32,
    pub bitrate: i32,
    pub sample_rate: i32,
    pub duration_secs: Option<f64>,
    pub encoder: Option<&'a str>,
    pub lowpass: Option<i32>,
    pub rms_full: Option<f64>,
    pub rms_mid_high: Option<f64>,
    pub rms_high: Option<f64>,
    pub rms_upper: Option<f64>,
    pub rms_19_20k: Option<f64>,
    pub rms_ultrasonic: Option<f64>,
    pub high_drop: Option<f64>,
    pub upper_drop: Option<f64>,
    pub ultrasonic_drop: Option<f64>,
    pub ultrasonic_flatness: Option<f64>,
    pub cutoff_variance: Option<f64>,
    pub avg_cutoff_freq: Option<f64>,
    pub rolloff_slope: Option<f64>,
    pub transition_width: Option<f64>,
    pub natural_rolloff: Option<i32>,
    pub binary_details_json: Option<String>,
    pub flags: Option<String>,
    pub error: Option<&'a str>,
}

/// Queryable analysis result (database record)
#[derive(Queryable, Selectable, Debug, Clone, serde::Serialize)]
#[diesel(table_name = analysis_results)]
pub struct DbRecord {
    pub id: i32,
    pub file_path: String,
    pub file_name: String,
    pub analyzed_at: String,
    pub schema_version: String,
    pub verdict: String,
    pub combined_score: i32,
    pub spectral_score: i32,
    pub binary_score: i32,
    pub bitrate: i32,
    pub sample_rate: i32,
    pub duration_secs: Option<f64>,
    pub encoder: Option<String>,
    pub lowpass: Option<i32>,
    pub rms_full: Option<f64>,
    pub rms_mid_high: Option<f64>,
    pub rms_high: Option<f64>,
    pub rms_upper: Option<f64>,
    pub rms_19_20k: Option<f64>,
    pub rms_ultrasonic: Option<f64>,
    pub high_drop: Option<f64>,
    pub upper_drop: Option<f64>,
    pub ultrasonic_drop: Option<f64>,
    pub ultrasonic_flatness: Option<f64>,
    pub cutoff_variance: Option<f64>,
    pub avg_cutoff_freq: Option<f64>,
    pub rolloff_slope: Option<f64>,
    pub transition_width: Option<f64>,
    pub natural_rolloff: Option<i32>,
    pub binary_details_json: Option<String>,
    pub flags: Option<String>,
    pub error: Option<String>,
    pub file_hash: Option<String>,
}

// ============================================================================
// Decision Graph Models
// ============================================================================

/// Insertable decision node
#[derive(Insertable)]
#[diesel(table_name = decision_nodes)]
pub struct NewDecisionNode<'a> {
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
#[diesel(table_name = decision_nodes)]
pub struct DecisionNode {
    pub id: i32,
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
    pub edge_type: &'a str,
    pub weight: Option<f64>,
    pub rationale: Option<&'a str>,
    pub created_at: &'a str,
}

/// Queryable decision edge
#[derive(Queryable, Selectable, Debug, Clone, serde::Serialize)]
#[diesel(table_name = decision_edges)]
pub struct DecisionEdge {
    pub id: i32,
    pub from_node_id: i32,
    pub to_node_id: i32,
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
}

impl std::fmt::Display for DbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DbError::Connection(msg) => write!(f, "Connection error: {}", msg),
            DbError::Query(e) => write!(f, "Query error: {}", e),
            DbError::Pool(e) => write!(f, "Pool error: {}", e),
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

/// Helper for raw SQL avg query
#[derive(QueryableByName)]
struct AvgResult {
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Double>)]
    avg: Option<f64>,
}

impl Database {
    /// Get the default database path
    pub fn db_path() -> std::path::PathBuf {
        std::path::PathBuf::from(DEFAULT_DB_PATH)
    }

    /// Open database at default path
    pub fn open() -> Result<Self> {
        Self::open_at(DEFAULT_DB_PATH)
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
        db.init_schema()?;
        Ok(db)
    }

    fn get_conn(&self) -> Result<DbConn> {
        self.pool.get().map_err(|e| DbError::Connection(e.to_string()))
    }

    fn init_schema(&self) -> Result<()> {
        let mut conn = self.get_conn()?;

        // Run raw SQL to create tables if they don't exist
        // This is simpler than embedded migrations for now
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
            CREATE TABLE IF NOT EXISTS analysis_results (
                id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
                file_path TEXT NOT NULL,
                file_name TEXT NOT NULL,
                analyzed_at TEXT NOT NULL,
                schema_version TEXT NOT NULL,
                verdict TEXT NOT NULL,
                combined_score INTEGER NOT NULL,
                spectral_score INTEGER NOT NULL,
                binary_score INTEGER NOT NULL,
                bitrate INTEGER NOT NULL,
                sample_rate INTEGER NOT NULL,
                duration_secs REAL,
                encoder TEXT,
                lowpass INTEGER,
                rms_full REAL,
                rms_mid_high REAL,
                rms_high REAL,
                rms_upper REAL,
                rms_19_20k REAL,
                rms_ultrasonic REAL,
                high_drop REAL,
                upper_drop REAL,
                ultrasonic_drop REAL,
                ultrasonic_flatness REAL,
                cutoff_variance REAL,
                avg_cutoff_freq REAL,
                rolloff_slope REAL,
                transition_width REAL,
                natural_rolloff INTEGER,
                binary_details_json TEXT,
                flags TEXT,
                error TEXT,
                file_hash TEXT,
                UNIQUE(file_path, analyzed_at)
            )
        "#).execute(&mut conn)?;

        diesel::sql_query(r#"
            CREATE TABLE IF NOT EXISTS decision_nodes (
                id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
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

        // Create indexes
        diesel::sql_query("CREATE INDEX IF NOT EXISTS idx_file_path ON analysis_results(file_path)").execute(&mut conn)?;
        diesel::sql_query("CREATE INDEX IF NOT EXISTS idx_verdict ON analysis_results(verdict)").execute(&mut conn)?;
        diesel::sql_query("CREATE INDEX IF NOT EXISTS idx_analyzed_at ON analysis_results(analyzed_at)").execute(&mut conn)?;
        diesel::sql_query("CREATE INDEX IF NOT EXISTS idx_nodes_type ON decision_nodes(node_type)").execute(&mut conn)?;
        diesel::sql_query("CREATE INDEX IF NOT EXISTS idx_nodes_status ON decision_nodes(status)").execute(&mut conn)?;
        diesel::sql_query("CREATE INDEX IF NOT EXISTS idx_edges_from ON decision_edges(from_node_id)").execute(&mut conn)?;
        diesel::sql_query("CREATE INDEX IF NOT EXISTS idx_edges_to ON decision_edges(to_node_id)").execute(&mut conn)?;
        diesel::sql_query("CREATE INDEX IF NOT EXISTS idx_command_started_at ON command_log(started_at)").execute(&mut conn)?;

        // Register current schema
        self.register_schema(&CURRENT_SCHEMA)?;
        Ok(())
    }

    fn register_schema(&self, schema: &AnalysisSchema) -> Result<()> {
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

    // ========================================================================
    // Analysis Results
    // ========================================================================

    /// Store an analysis result
    pub fn insert_result(&self, result: &AnalysisResult) -> Result<i64> {
        let mut conn = self.get_conn()?;
        let now = chrono::Local::now().to_rfc3339();
        let verdict_str = match result.verdict {
            Verdict::Ok => "OK",
            Verdict::Suspect => "SUSPECT",
            Verdict::Transcode => "TRANSCODE",
            Verdict::Error => "ERROR",
        };

        let flags_str = result.flags.join(",");
        let encoder_str: Option<&str> = if result.encoder.is_empty() { None } else { Some(&result.encoder) };
        let error_str = result.error.as_deref();

        let (
            rms_full, rms_mid_high, rms_high, rms_upper, rms_19_20k, rms_ultrasonic,
            high_drop, upper_drop, ultrasonic_drop, ultrasonic_flatness,
            cutoff_variance, avg_cutoff_freq, rolloff_slope, transition_width, natural_rolloff,
        ) = if let Some(ref s) = result.spectral_details {
            (
                Some(s.rms_full), Some(s.rms_mid_high), Some(s.rms_high), Some(s.rms_upper),
                Some(s.rms_19_20k), Some(s.rms_ultrasonic), Some(s.high_drop), Some(s.upper_drop),
                Some(s.ultrasonic_drop), Some(s.ultrasonic_flatness), Some(s.cutoff_variance),
                Some(s.avg_cutoff_freq), Some(s.rolloff_slope), Some(s.transition_width),
                Some(if s.natural_rolloff { 1 } else { 0 }),
            )
        } else {
            (None, None, None, None, None, None, None, None, None, None, None, None, None, None, None)
        };

        let binary_json = result.binary_details.as_ref()
            .map(|b| serde_json::to_string(b).unwrap_or_default());

        let new_result = NewAnalysisResult {
            file_path: &result.file_path,
            file_name: &result.file_name,
            analyzed_at: &now,
            schema_version: &CURRENT_SCHEMA.version_string(),
            verdict: verdict_str,
            combined_score: result.combined_score as i32,
            spectral_score: result.spectral_score as i32,
            binary_score: result.binary_score as i32,
            bitrate: result.bitrate as i32,
            sample_rate: result.sample_rate as i32,
            duration_secs: Some(result.duration_secs),
            encoder: encoder_str,
            lowpass: result.lowpass.map(|v| v as i32),
            rms_full,
            rms_mid_high,
            rms_high,
            rms_upper,
            rms_19_20k,
            rms_ultrasonic,
            high_drop,
            upper_drop,
            ultrasonic_drop,
            ultrasonic_flatness,
            cutoff_variance,
            avg_cutoff_freq,
            rolloff_slope,
            transition_width,
            natural_rolloff,
            binary_details_json: binary_json,
            flags: Some(flags_str),
            error: error_str,
        };

        diesel::insert_into(analysis_results::table)
            .values(&new_result)
            .execute(&mut conn)?;

        // Get last insert ID
        let id: i32 = diesel::select(diesel::dsl::sql::<diesel::sql_types::Integer>("last_insert_rowid()"))
            .first(&mut conn)?;

        Ok(id as i64)
    }

    /// Get all results, optionally filtered by verdict
    pub fn get_results(&self, verdict_filter: Option<&str>) -> Result<Vec<DbRecord>> {
        let mut conn = self.get_conn()?;

        let results = match verdict_filter {
            Some(v) => {
                analysis_results::table
                    .filter(analysis_results::verdict.eq(v))
                    .order(analysis_results::analyzed_at.desc())
                    .load::<DbRecord>(&mut conn)?
            }
            None => {
                analysis_results::table
                    .order(analysis_results::analyzed_at.desc())
                    .load::<DbRecord>(&mut conn)?
            }
        };

        Ok(results)
    }

    /// Get the most recent analysis for a specific file
    pub fn get_latest_for_file(&self, file_path: &str) -> Result<Option<DbRecord>> {
        let mut conn = self.get_conn()?;

        let result = analysis_results::table
            .filter(analysis_results::file_path.eq(file_path))
            .order(analysis_results::analyzed_at.desc())
            .first::<DbRecord>(&mut conn)
            .optional()?;

        Ok(result)
    }

    /// Get summary statistics
    pub fn get_summary(&self) -> Result<DbSummary> {
        let mut conn = self.get_conn()?;

        let total: i64 = analysis_results::table
            .count()
            .get_result(&mut conn)?;

        let ok_count: i64 = analysis_results::table
            .filter(analysis_results::verdict.eq("OK"))
            .count()
            .get_result(&mut conn)?;

        let suspect_count: i64 = analysis_results::table
            .filter(analysis_results::verdict.eq("SUSPECT"))
            .count()
            .get_result(&mut conn)?;

        let transcode_count: i64 = analysis_results::table
            .filter(analysis_results::verdict.eq("TRANSCODE"))
            .count()
            .get_result(&mut conn)?;

        let error_count: i64 = analysis_results::table
            .filter(analysis_results::verdict.eq("ERROR"))
            .count()
            .get_result(&mut conn)?;

        // Use raw SQL for avg since Diesel's avg returns Numeric type
        let avg_score: Option<f64> = diesel::sql_query("SELECT AVG(combined_score) as avg FROM analysis_results")
            .get_result::<AvgResult>(&mut conn)
            .ok()
            .map(|r| r.avg)
            .flatten();

        Ok(DbSummary {
            total: total as i32,
            ok_count: ok_count as i32,
            suspect_count: suspect_count as i32,
            transcode_count: transcode_count as i32,
            error_count: error_count as i32,
            avg_score,
        })
    }

    /// Clear all analysis records
    pub fn clear(&self) -> Result<usize> {
        let mut conn = self.get_conn()?;
        let count = diesel::delete(analysis_results::table).execute(&mut conn)?;
        Ok(count)
    }

    // ========================================================================
    // Decision Graph Operations
    // ========================================================================

    /// Create a new decision node
    pub fn create_node(&self, node_type: &str, title: &str, description: Option<&str>, confidence: Option<u8>) -> Result<i32> {
        let mut conn = self.get_conn()?;
        let now = chrono::Local::now().to_rfc3339();

        // Build metadata JSON with confidence if provided
        let metadata = confidence.map(|c| format!(r#"{{"confidence":{}}}"#, c.min(100)));

        let new_node = NewDecisionNode {
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
        let now = chrono::Local::now().to_rfc3339();

        let new_edge = NewDecisionEdge {
            from_node_id: from_id,
            to_node_id: to_id,
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
}

// ============================================================================
// Additional Types
// ============================================================================

/// Summary statistics from the database
#[derive(Debug, Clone, serde::Serialize)]
pub struct DbSummary {
    pub total: i32,
    pub ok_count: i32,
    pub suspect_count: i32,
    pub transcode_count: i32,
    pub error_count: i32,
    pub avg_score: Option<f64>,
}

/// Full decision graph for serialization
#[derive(Debug, Clone, serde::Serialize)]
pub struct DecisionGraph {
    pub nodes: Vec<DecisionNode>,
    pub edges: Vec<DecisionEdge>,
}
