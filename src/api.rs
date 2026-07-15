//! Multi-graph HTTP API daemon (`deciduous serve --api`).
//!
//! Serves many independent decision graphs to remote clients that have no
//! local `.deciduous/` — one SQLite file per graph under
//! `<data_dir>/graphs/<graph_id>/deciduous.db`. Write/read operations are
//! the same tool set the MCP server exposes: requests are routed through
//! [`crate::mcp::handlers::dispatch`], so CLI, MCP (stdio), and HTTP remain
//! one implementation with three transports.
//!
//! Every request must carry `Authorization: Bearer <token>`.
//!
//! Routes (all under `/api/v1`):
//! - `GET  /graphs`                      → list graph ids
//! - `PUT  /graphs/{id}`                 → create (idempotent)
//! - `POST /graphs/{id}/tools/{tool}`    → dispatch an MCP tool; body = args
//! - `POST /graphs/{id}/query`           → read-only SQL: `{"sql": "SELECT …", "limit": 500}`
//!
//! Remote clients cannot rely on the server's git checkout for attribution,
//! so tools that support it should be called with an explicit `branch` arg;
//! the server never injects one.

use std::collections::HashMap;
use std::io::Read;
use std::net::ToSocketAddrs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use tiny_http::{Header, Method, Request, Response, Server};

use crate::db::Database;
use crate::mcp::handlers;
use crate::mcp::protocol::ToolCallResult;

const MAX_BODY_BYTES: usize = 1_048_576;
const DEFAULT_QUERY_ROWS: usize = 500;
const MAX_QUERY_ROWS: usize = 5_000;

pub struct ApiConfig {
    pub bind: String,
    pub port: u16,
    pub data_dir: PathBuf,
    pub token: String,
}

/// A running API server (owned by tests or by the CLI loop).
pub struct ApiServer {
    server: Arc<Server>,
    registry: Arc<Registry>,
    token: String,
}

impl ApiServer {
    pub fn bind(config: ApiConfig) -> std::io::Result<Self> {
        std::fs::create_dir_all(config.data_dir.join("graphs"))?;
        let addr = (config.bind.as_str(), config.port)
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::Other, "could not resolve bind address")
            })?;
        let server =
            Server::http(addr).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        Ok(Self {
            server: Arc::new(server),
            registry: Arc::new(Registry::new(config.data_dir)),
            token: config.token,
        })
    }

    /// The actual port bound (useful when configured with port 0).
    pub fn port(&self) -> u16 {
        self.server
            .server_addr()
            .to_ip()
            .map(|a| a.port())
            .unwrap_or(0)
    }

    /// Serve forever on the current thread.
    pub fn run(&self) {
        for request in self.server.incoming_requests() {
            let registry = Arc::clone(&self.registry);
            let token = self.token.clone();
            // one thread per request is plenty for a graph API
            std::thread::spawn(move || {
                let _ = handle(request, &registry, &token);
            });
        }
    }
}

// ── Graph registry (file-per-graph tenancy) ──────────────────────────────

struct Registry {
    data_dir: PathBuf,
    open: Mutex<HashMap<String, Arc<Database>>>,
}

impl Registry {
    fn new(data_dir: PathBuf) -> Self {
        Self {
            data_dir,
            open: Mutex::new(HashMap::new()),
        }
    }

    fn graph_dir(&self, graph_id: &str) -> PathBuf {
        self.data_dir.join("graphs").join(graph_id)
    }

    fn db_path(&self, graph_id: &str) -> PathBuf {
        self.graph_dir(graph_id).join("deciduous.db")
    }

    fn list(&self) -> Vec<String> {
        let mut ids: Vec<String> = std::fs::read_dir(self.data_dir.join("graphs"))
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .filter(|e| e.path().join("deciduous.db").exists())
                    .filter_map(|e| e.file_name().into_string().ok())
                    .collect()
            })
            .unwrap_or_default();
        ids.sort();
        ids
    }

    fn exists(&self, graph_id: &str) -> bool {
        self.db_path(graph_id).exists()
    }

    /// Open (and cache) a graph database; `create` controls whether a
    /// missing graph is initialized or reported as an error.
    fn database(&self, graph_id: &str, create: bool) -> Result<Arc<Database>, ApiError> {
        if !valid_graph_id(graph_id) {
            return Err(ApiError::bad_request(
                "graph id must be 1-64 chars of [a-z0-9_-], starting alphanumeric",
            ));
        }
        if !create && !self.exists(graph_id) {
            return Err(ApiError::not_found(&format!("no such graph: {graph_id}")));
        }

        let mut open = self.open.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(db) = open.get(graph_id) {
            return Ok(Arc::clone(db));
        }

        std::fs::create_dir_all(self.graph_dir(graph_id))
            .map_err(|e| ApiError::internal(&format!("create graph dir: {e}")))?;
        let db = Database::open_at(self.db_path(graph_id))
            .map_err(|e| ApiError::internal(&format!("open graph db: {e}")))?;
        let db = Arc::new(db);
        open.insert(graph_id.to_string(), Arc::clone(&db));
        Ok(db)
    }
}

fn valid_graph_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

// ── Request handling ──────────────────────────────────────────────────────

struct ApiError {
    status: u16,
    message: String,
}

impl ApiError {
    fn bad_request(msg: &str) -> Self {
        Self {
            status: 400,
            message: msg.to_string(),
        }
    }
    fn unauthorized() -> Self {
        Self {
            status: 401,
            message: "missing or invalid bearer token".to_string(),
        }
    }
    fn not_found(msg: &str) -> Self {
        Self {
            status: 404,
            message: msg.to_string(),
        }
    }
    fn forbidden(msg: &str) -> Self {
        Self {
            status: 403,
            message: msg.to_string(),
        }
    }
    fn internal(msg: &str) -> Self {
        Self {
            status: 500,
            message: msg.to_string(),
        }
    }
}

fn handle(mut request: Request, registry: &Registry, token: &str) -> std::io::Result<()> {
    let outcome = route(&mut request, registry, token);
    let (status, body) = match outcome {
        Ok((status, data)) => (status, json!({"ok": true, "data": data})),
        Err(e) => (e.status, json!({"ok": false, "error": e.message})),
    };
    respond_json(request, status, &body)
}

fn route(
    request: &mut Request,
    registry: &Registry,
    token: &str,
) -> Result<(u16, Value), ApiError> {
    authorize(request, token)?;

    let url = request.url().to_string();
    let path = url.split('?').next().unwrap_or("");
    let segments: Vec<&str> = path.trim_matches('/').split('/').collect();

    match (request.method().clone(), segments.as_slice()) {
        (Method::Get, ["api", "v1", "graphs"]) => Ok((200, json!({"graphs": registry.list()}))),

        (Method::Put, ["api", "v1", "graphs", graph_id]) => {
            let existed = registry.exists(graph_id);
            registry.database(graph_id, true)?;
            let status = if existed { 200 } else { 201 };
            Ok((status, json!({"graph_id": graph_id, "created": !existed})))
        }

        (Method::Post, ["api", "v1", "graphs", graph_id, "tools", tool_name]) => {
            let db = registry.database(graph_id, false)?;
            let args = read_json_body(request)?;
            let result = handlers::dispatch(&db, tool_name, args);
            Ok((200, tool_result_to_json(result)))
        }

        (Method::Post, ["api", "v1", "graphs", graph_id, "query"]) => {
            let db_path = {
                registry.database(graph_id, false)?; // validates id + existence
                registry.db_path(graph_id)
            };
            let body = read_json_body(request)?;
            let sql = body
                .get("sql")
                .and_then(Value::as_str)
                .ok_or_else(|| ApiError::bad_request("body must be {\"sql\": \"SELECT …\"}"))?;
            let limit = body
                .get("limit")
                .and_then(Value::as_u64)
                .map(|l| l as usize)
                .unwrap_or(DEFAULT_QUERY_ROWS)
                .min(MAX_QUERY_ROWS);
            let result = run_readonly_query(&db_path, sql, limit)?;
            Ok((200, result))
        }

        _ => Err(ApiError::not_found("unknown route")),
    }
}

fn authorize(request: &Request, token: &str) -> Result<(), ApiError> {
    let provided = request
        .headers()
        .iter()
        .find(|h| h.field.equiv("Authorization"))
        .map(|h| h.value.as_str().trim().to_string())
        .unwrap_or_default();

    let expected = format!("Bearer {token}");
    if constant_time_eq(provided.as_bytes(), expected.as_bytes()) {
        Ok(())
    } else {
        Err(ApiError::unauthorized())
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn read_json_body(request: &mut Request) -> Result<Value, ApiError> {
    let mut body = String::new();
    request
        .as_reader()
        .take(MAX_BODY_BYTES as u64 + 1)
        .read_to_string(&mut body)
        .map_err(|e| ApiError::bad_request(&format!("unreadable body: {e}")))?;
    if body.len() > MAX_BODY_BYTES {
        return Err(ApiError::bad_request("body too large (max 1 MiB)"));
    }
    if body.trim().is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_str(&body).map_err(|e| ApiError::bad_request(&format!("invalid JSON: {e}")))
}

/// Convert an MCP `ToolCallResult` into the API envelope's data value. Tool
/// handlers return their payload as a JSON string in the first text content;
/// parse it back out so HTTP clients get structured data, not nested JSON.
fn tool_result_to_json(result: ToolCallResult) -> Value {
    let text = result
        .content
        .first()
        .map(|c| c.text.clone())
        .unwrap_or_default();
    let payload = serde_json::from_str::<Value>(&text).unwrap_or(Value::String(text));
    json!({
        "is_error": result.is_error.unwrap_or(false),
        "result": payload,
    })
}

// ── Read-only SQL over a graph ────────────────────────────────────────────

fn run_readonly_query(db_path: &Path, sql: &str, limit: usize) -> Result<Value, ApiError> {
    let conn = rusqlite::Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| ApiError::internal(&format!("open read-only: {e}")))?;

    conn.pragma_update(None, "query_only", "ON")
        .map_err(|e| ApiError::internal(&format!("query_only pragma: {e}")))?;
    conn.busy_timeout(std::time::Duration::from_millis(2_000))
        .map_err(|e| ApiError::internal(&format!("busy timeout: {e}")))?;

    let mut stmt = conn
        .prepare(sql)
        .map_err(|e| ApiError::bad_request(&format!("SQL error: {e}")))?;
    if !stmt.readonly() {
        return Err(ApiError::forbidden(
            "only read-only SELECT statements are allowed",
        ));
    }

    let columns: Vec<String> = stmt.column_names().iter().map(|c| c.to_string()).collect();
    let n_cols = columns.len();

    let mut rows_out: Vec<Vec<Value>> = Vec::new();
    let mut truncated = false;
    let mut rows = stmt
        .query([])
        .map_err(|e| ApiError::bad_request(&format!("SQL error: {e}")))?;
    while let Some(row) = rows
        .next()
        .map_err(|e| ApiError::bad_request(&format!("SQL error: {e}")))?
    {
        if rows_out.len() >= limit {
            truncated = true;
            break;
        }
        let mut out = Vec::with_capacity(n_cols);
        for i in 0..n_cols {
            out.push(sqlite_value_to_json(row.get_ref(i).map_err(|e| {
                ApiError::internal(&format!("read column {i}: {e}"))
            })?));
        }
        rows_out.push(out);
    }

    Ok(json!({
        "columns": columns,
        "rows": rows_out,
        "row_count": rows_out.len(),
        "truncated": truncated,
    }))
}

fn sqlite_value_to_json(value: rusqlite::types::ValueRef<'_>) -> Value {
    use rusqlite::types::ValueRef;
    match value {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(i) => json!(i),
        ValueRef::Real(f) => json!(f),
        ValueRef::Text(t) => Value::String(String::from_utf8_lossy(t).into_owned()),
        ValueRef::Blob(b) => json!({"blob_len": b.len()}),
    }
}

// ── Plumbing ──────────────────────────────────────────────────────────────

fn respond_json(request: Request, status: u16, body: &Value) -> std::io::Result<()> {
    let header = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
        .expect("static header is valid");
    let response = Response::from_string(body.to_string())
        .with_status_code(status)
        .with_header(header);
    request.respond(response)
}
