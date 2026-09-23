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

/// Author on graph-file records written through the API. The daemon has no
/// idea who its caller is, and its own git identity is not the caller's.
pub const API_AUTHOR: &str = "deciduous-api";
const DEFAULT_QUERY_ROWS: usize = 500;
const MAX_QUERY_ROWS: usize = 5_000;

pub struct ApiConfig {
    pub bind: String,
    pub port: u16,
    pub data_dir: PathBuf,
    pub token: String,
    /// The deciduous executable. Each `/query` runs in a child process of
    /// it (`<exe> __api-query`, see [`query_child_main`]), so a query past
    /// its time limit can be killed wherever it is. `serve --api` passes
    /// its own executable.
    pub query_exe: PathBuf,
}

/// A running API server (owned by tests or by the CLI loop).
///
/// The listening socket is owned here, and each tiny_http server accepts on
/// a duplicate of it. tiny_http 0.12 ends its accept thread for good on the
/// first error accept() returns (EMFILE when descriptors run out,
/// ECONNABORTED when a queued client resets), and panics in it when the
/// descriptors run out just after an accept. Either way it drops its copy of
/// the listener. With the only copy, that closed the socket: every
/// connection queued on it was reset ("Connection reset by peer"), `run`
/// returned, and `serve --api` exited 0 without a word. Holding our own
/// copy keeps the socket and its queue open while `run` starts another
/// tiny_http on a new duplicate.
pub struct ApiServer {
    listener: std::net::TcpListener,
    first: Mutex<Option<Server>>,
    registry: Arc<Registry>,
    token: String,
}

/// The executable `/query` children run, set once by [`ApiServer::bind`].
static QUERY_EXE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Set by the panic hook when tiny_http's accept thread panics, which it
/// does without telling `recv`.
static TINY_HTTP_PANICKED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

fn watch_tiny_http_panics() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            // Only the accept thread's panic: RefinedTcpStream::new's
            // try_clone().unwrap() when descriptors run out just after an
            // accept (it is called nowhere else). A panic in a connection's
            // task does not stop accepting, and starting another server for
            // it only added one more to poll.
            if info.location().is_some_and(|l| {
                l.file().contains("tiny_http") && l.file().ends_with("refined_tcp_stream.rs")
            }) {
                TINY_HTTP_PANICKED.store(true, std::sync::atomic::Ordering::SeqCst);
            }
            previous(info);
        }));
    });
}

/// A tiny_http server accepting on a duplicate of `listener`.
fn serve_on(listener: &std::net::TcpListener) -> std::io::Result<Server> {
    Server::from_listener(listener.try_clone()?, None).map_err(std::io::Error::other)
}

/// Raises this process's open-file soft limit to its hard limit. macOS
/// starts processes at 256, and every connection, request thread and SQLite
/// file costs descriptors; running out is what makes accept() fail.
#[cfg(unix)]
fn raise_open_file_limit() {
    // SAFETY: getrlimit/setrlimit on a local struct; no pointers retained.
    unsafe {
        let mut lim = std::mem::zeroed::<libc::rlimit>();
        if libc::getrlimit(libc::RLIMIT_NOFILE, &mut lim) != 0 || lim.rlim_cur >= lim.rlim_max {
            return;
        }
        let wanted = lim.rlim_max;
        lim.rlim_cur = wanted;
        if libc::setrlimit(libc::RLIMIT_NOFILE, &lim) != 0 {
            // macOS refuses more than OPEN_MAX for the soft limit.
            lim.rlim_cur = wanted.min(10_240);
            let _ = libc::setrlimit(libc::RLIMIT_NOFILE, &lim);
        }
    }
}

/// Windows has no RLIMIT_NOFILE; its handle limit is not the constraint here.
#[cfg(not(unix))]
fn raise_open_file_limit() {}

impl ApiServer {
    pub fn bind(config: ApiConfig) -> std::io::Result<Self> {
        std::fs::create_dir_all(config.data_dir.join("graphs"))?;
        let addr = (config.bind.as_str(), config.port)
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| std::io::Error::other("could not resolve bind address"))?;
        raise_open_file_limit();
        watch_tiny_http_panics();
        if !config.query_exe.is_file() {
            return Err(std::io::Error::other(format!(
                "the executable for /query children, {}, is not a file",
                config.query_exe.display()
            )));
        }
        let _ = QUERY_EXE.set(config.query_exe.clone());
        let listener = std::net::TcpListener::bind(addr)?;
        let server = serve_on(&listener)?;
        Ok(Self {
            listener,
            first: Mutex::new(Some(server)),
            registry: Arc::new(Registry::new(config.data_dir)),
            token: config.token,
        })
    }

    /// The actual port bound (useful when configured with port 0).
    pub fn port(&self) -> u16 {
        self.listener.local_addr().map(|a| a.port()).unwrap_or(0)
    }

    /// Serve forever on the current thread.
    ///
    /// Each tiny_http server gets a thread of its own blocked in `recv()`.
    /// One whose accept thread died still owns connections it accepted
    /// before, whose next requests arrive in its queue, so its thread keeps
    /// reading it. This loop only starts a new server when one reports an
    /// accept error or tiny_http's accept thread panicked.
    ///
    /// Why not poll every server from this loop (as this did): each poll
    /// was `recv_timeout(5 ms)`, one after another, so every request waited
    /// about 5 ms more per restart, without bound: median GET latency was
    /// 0.209 s after 25 bursts of idle connections, 0.823 s after 100. A
    /// dead server now costs one parked thread and nothing per request.
    pub fn run(&self) {
        let first = self
            .first
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
            .expect("ApiServer::run called twice");
        let (died_tx, died_rx) = std::sync::mpsc::channel::<()>();
        self.dispatch(first, died_tx.clone());
        let mut backoff = std::time::Duration::from_millis(50);
        let mut last_restart: Option<std::time::Instant> = None;
        loop {
            let reported = match died_rx.recv_timeout(std::time::Duration::from_millis(250)) {
                Ok(()) => true,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => false,
                // We hold a sender, so this cannot happen.
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => false,
            };
            let panicked = TINY_HTTP_PANICKED.swap(false, std::sync::atomic::Ordering::SeqCst);
            if panicked {
                eprintln!(
                    "deciduous api: the HTTP accept thread panicked (see above); accepting again"
                );
            }
            if !reported && !panicked {
                continue;
            }
            // Several dispatchers may report one shortage; one restart
            // answers all of them.
            while died_rx.try_recv().is_ok() {}
            if last_restart.is_some_and(|t| t.elapsed() > std::time::Duration::from_secs(10)) {
                backoff = std::time::Duration::from_millis(50);
            }
            // Descriptors that ran out come back as connections close, so
            // retry with backoff until an accept thread runs again.
            loop {
                std::thread::sleep(backoff);
                backoff = (backoff * 2).min(std::time::Duration::from_secs(2));
                match serve_on(&self.listener) {
                    Ok(server) => {
                        self.dispatch(server, died_tx.clone());
                        last_restart = Some(std::time::Instant::now());
                        break;
                    }
                    Err(e) => eprintln!(
                        "deciduous api: could not accept on the socket yet ({e}); \
                         retrying in {} ms",
                        backoff.as_millis()
                    ),
                }
            }
        }
    }

    /// Read `server`'s requests on a thread of its own, forever, handing each
    /// to a thread of its own. An accept error is reported on `died` and the
    /// thread goes on reading: connections accepted earlier still deliver.
    fn dispatch(&self, server: Server, died: std::sync::mpsc::Sender<()>) {
        let registry = Arc::clone(&self.registry);
        let token = self.token.clone();
        std::thread::spawn(move || loop {
            match server.recv() {
                Ok(request) => {
                    let registry = Arc::clone(&registry);
                    let token = token.clone();
                    // one thread per request is plenty for a graph API
                    std::thread::spawn(move || {
                        let _ = handle(request, &registry, &token);
                    });
                }
                Err(e) => {
                    eprintln!(
                        "deciduous api: accepting a connection failed ({e}); accepting again"
                    );
                    let _ = died.send(());
                }
            }
        });
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
        // Validate before the id ever reaches a path join: `exists` is called
        // from route() (for PUT) ahead of `database`'s own check, so without
        // this an unvalidated id would reach the filesystem. An invalid id
        // can't name a real graph anyway, so "invalid" and "absent" coincide.
        valid_graph_id(graph_id) && self.db_path(graph_id).exists()
    }

    /// Open (and cache) a graph database; `create` controls whether a
    /// missing graph is initialized or reported as an error.
    ///
    /// The filesystem is authoritative: a cached handle is only trusted while
    /// its `deciduous.db` still exists on disk. If the data dir was deleted
    /// under the running daemon (backup rotation, restore, disk cleanup) the
    /// stale handle is evicted, so we re-create the database (`create = true`)
    /// or honestly report 404 (`create = false`) instead of returning a handle
    /// to a file that is gone — the wedge where PUT answered 201 while every
    /// subsequent write 404'd forever.
    fn database(&self, graph_id: &str, create: bool) -> Result<Arc<Database>, ApiError> {
        self.open_graph(graph_id, create).map(|(db, _)| db)
    }

    /// [`database`](Self::database), also saying whether this call created
    /// the graph. Existence is checked under the registry lock: checked
    /// before it, thirty concurrent PUTs of one new graph each saw "absent"
    /// and one to three of them answered 201 created.
    fn open_graph(&self, graph_id: &str, create: bool) -> Result<(Arc<Database>, bool), ApiError> {
        if !valid_graph_id(graph_id) {
            return Err(ApiError::bad_request(
                "graph id must be 1-64 chars of [a-z0-9_-], starting alphanumeric",
            ));
        }

        let mut open = self.open.lock().unwrap_or_else(|e| e.into_inner());
        let on_disk = self.exists(graph_id);
        if !create && !on_disk {
            return Err(ApiError::not_found(&format!("no such graph: {graph_id}")));
        }

        if on_disk {
            // File still present: a cached handle is trustworthy.
            if let Some(db) = open.get(graph_id) {
                return Ok((Arc::clone(db), false));
            }
        } else {
            // File gone but we were asked to create: drop any stale handle so
            // the open below re-creates the database rather than handing back a
            // connection to a vanished file.
            open.remove(graph_id);
        }

        std::fs::create_dir_all(self.graph_dir(graph_id))
            .map_err(|e| ApiError::internal(&format!("create graph dir: {e}")))?;
        let db = Database::open_at(self.db_path(graph_id))
            .map_err(|e| ApiError::internal(&format!("open graph db: {e}")))?;
        // Records written through the API are attributed to the API, never
        // to whoever `git config user.name` names in the daemon's cwd. Also
        // for a graph.json that `deciduous sync` creates after this open.
        db.set_store_author(API_AUTHOR);
        let db = Arc::new(db);
        open.insert(graph_id.to_string(), Arc::clone(&db));
        Ok((db, !on_disk))
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

/// The tools an API caller may invoke: append and read, never destroy.
///
/// The full tool set (see `mcp::handlers::dispatch`) includes `delete_node`,
/// `unlink_nodes`, `update_status`, and `update_prompt`, which are right for a
/// human curating their own graph and wrong for a shared graph reachable by a
/// bearer token — "many authors" must not mean "any token holder can erase
/// what the others wrote". This list is the canonical server-side boundary;
/// clients (e.g. party_line's Memory.Broker) mirror it, but the daemon is
/// authoritative because a leaked token bypasses every client.
fn tool_is_allowed(tool: &str) -> bool {
    matches!(
        tool,
        "add_node"
            | "link_nodes"
            | "list_nodes"
            | "list_edges"
            | "show_node"
            | "get_graph"
            | "get_node_context"
            | "search_nodes"
            | "trace_chain"
            | "list_themes"
    )
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
    fn unavailable(msg: &str) -> Self {
        Self {
            status: 503,
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
    let url = request.url().to_string();
    let path = url.split('?').next().unwrap_or("");
    let segments: Vec<&str> = path.trim_matches('/').split('/').collect();

    // Health is intentionally unauthenticated and side-effect free — a reverse
    // proxy or uptime probe hits it without the bearer token. It opens no
    // database, lists no graphs, and discloses nothing beyond "up".
    if request.method() == &Method::Get
        && matches!(segments.as_slice(), ["health"] | ["api", "v1", "health"])
    {
        return Ok((200, json!({"status": "ok"})));
    }

    authorize(request, token)?;

    match (request.method().clone(), segments.as_slice()) {
        (Method::Get, ["api", "v1", "graphs"]) => Ok((200, json!({"graphs": registry.list()}))),

        (Method::Put, ["api", "v1", "graphs", graph_id]) => {
            let (_, created) = registry.open_graph(graph_id, true)?;
            let status = if created { 201 } else { 200 };
            Ok((status, json!({"graph_id": graph_id, "created": created})))
        }

        (Method::Post, ["api", "v1", "graphs", graph_id, "tools", tool_name]) => {
            // Append and read, never destroy — enforced HERE, at the transport,
            // not only in a well-behaved client. A leaked token, or any
            // misbehaving caller, must not be able to erase or rewrite a
            // shared graph's history by calling delete_node/unlink_nodes
            // directly over HTTP. The daemon is the authoritative boundary; a
            // client-side allowlist alone is bypassed the moment the token
            // leaves the client. Keep this list in sync with the client's.
            if !tool_is_allowed(tool_name) {
                return Err(ApiError::forbidden(&format!(
                    "tool not permitted over the API: {tool_name}"
                )));
            }
            let db = registry.database(graph_id, false)?;
            let args = read_json_body(request)?;
            let result = handlers::dispatch_as(&db, tool_name, args, handlers::Caller::Remote);
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

/// Wall-clock budget for one `/query`. A recursive CTE without a bound
/// never finishes, and the daemon runs one thread per request: four aborted
/// requests pinned it at 400% CPU until it was killed.
///
/// It is enforced by killing the child process the query runs in, not by
/// anything inside SQLite. A progress handler runs every N VM ops and
/// `sqlite3_interrupt` is seen between ops, and a single op can run far past
/// the limit: `instr()` over two values near the length cap is one op of
/// about 4.5 s, and LIKE with a leading % and a 50,000-byte pattern over a
/// 1 MB value one of about 50 s. Four of those, from clients that gave up,
/// held every /query slot and four cores for 50 s after an interrupt. Lower
/// SQLite limits would only bound the functions someone thought of (LIKE,
/// GLOB, instr, replace, two-argument trim are all O(n*m) in one op); a
/// killed process stops whatever it was doing.
const QUERY_TIME_LIMIT: std::time::Duration = std::time::Duration::from_secs(5);

/// Queries that may execute at once. Past this, `/query` answers 503 at
/// once: every query is a process at full CPU for up to the time limit.
const QUERY_MAX_RUNNING: usize = 4;

static QUERIES_RUNNING: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// A place among the `QUERY_MAX_RUNNING`, held until the query's child
/// process has exited (killed, if it ran past the limit) and been reaped.
struct QuerySlot;

impl QuerySlot {
    fn take() -> Option<Self> {
        use std::sync::atomic::Ordering;
        QUERIES_RUNNING
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < QUERY_MAX_RUNNING).then_some(n + 1)
            })
            .ok()
            .map(|_| QuerySlot)
    }
}

impl Drop for QuerySlot {
    fn drop(&mut self) {
        QUERIES_RUNNING.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
    }
}

/// Largest string or blob a query may build (SQLITE_LIMIT_LENGTH). The
/// default is 1 GB; `printf('%.*c', 200000000, 'x')` built 200 MB in 62 ms.
const QUERY_MAX_VALUE_BYTES: i32 = 1_000_000;

/// Most memory SQLite may hold in a /query child (PRAGMA hard_heap_limit);
/// past it the query fails with "out of memory". Room for several values of
/// QUERY_MAX_VALUE_BYTES, and for the page cache of a large graph.
const QUERY_MAX_HEAP_BYTES: i64 = 64 * 1024 * 1024;

/// Largest `/query` result, as serialized JSON. SQLITE_LIMIT_LENGTH caps
/// one value, not the response: 1000 rows of a 999 KB value made a 999 MB
/// body, and the daemon kept 1.6 GB of it after the request ended.
const QUERY_MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

/// Counts what serde_json would write, without keeping it.
struct ByteCount(usize);

impl std::io::Write for ByteCount {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0 += buf.len();
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Table-valued pragmas that only describe this graph's schema. Every other
/// pragma, and `pragma_database_list` in particular (it returns the data
/// directory's absolute path), is refused.
const SCHEMA_PRAGMAS: &[&str] = &[
    "table_info",
    "table_xinfo",
    "table_list",
    "index_list",
    "index_info",
    "index_xinfo",
    "foreign_key_list",
];

/// What a `/query` statement may do: read tables of the one database this
/// connection opened. Enforced by SQLite's authorizer while the statement
/// is prepared, so it covers every spelling (`PRAGMA x`, `pragma_x(...)`,
/// a view, a CTE) rather than whatever a text check thought of.
///
/// ATTACH is the reason this exists. It is read-only by SQLite's own
/// definition, so `stmt.readonly()` let it through, and its error told the
/// caller whether any path on the server existed ("file is not a database"
/// versus "unable to open database file").
fn query_authorizer(ctx: rusqlite::hooks::AuthContext<'_>) -> rusqlite::hooks::Authorization {
    use rusqlite::hooks::{AuthAction, Authorization};
    let schema_pragma = |name: &str| SCHEMA_PRAGMAS.contains(&name.trim_start_matches("pragma_"));
    match ctx.action {
        AuthAction::Select | AuthAction::Recursive | AuthAction::Function { .. } => {
            Authorization::Allow
        }
        AuthAction::Read { table_name, .. } => {
            if table_name.starts_with("pragma_") && !schema_pragma(table_name) {
                Authorization::Deny
            } else {
                Authorization::Allow
            }
        }
        AuthAction::Pragma {
            pragma_name,
            pragma_value: _,
        } if schema_pragma(pragma_name) => Authorization::Allow,
        _ => Authorization::Deny,
    }
}

fn sql_error(e: rusqlite::Error) -> ApiError {
    let msg = e.to_string();
    if msg.contains("not authorized") || msg.contains("is prohibited") {
        ApiError::forbidden(
            "statement refused: /query may only read this graph's tables \
             (no ATTACH, no pragmas beyond table/index/foreign-key info)",
        )
    } else if msg.contains("out of memory") {
        ApiError::bad_request(&format!(
            "query stopped: it needed more than the {} MB of memory a /query may use",
            QUERY_MAX_HEAP_BYTES / (1024 * 1024)
        ))
    } else {
        ApiError::bad_request(&format!("SQL error: {msg}"))
    }
}

fn run_readonly_query(db_path: &Path, sql: &str, limit: usize) -> Result<Value, ApiError> {
    use std::process::{Command, Stdio};
    let _slot = QuerySlot::take().ok_or_else(|| {
        ApiError::unavailable(&format!(
            "{QUERY_MAX_RUNNING} queries are already running on this daemon; \
             try again in a moment (each is stopped at {} s)",
            QUERY_TIME_LIMIT.as_secs()
        ))
    })?;
    let exe = QUERY_EXE
        .get()
        .ok_or_else(|| ApiError::internal("no executable for /query was configured"))?;
    let mut child = Command::new(exe)
        .arg(QUERY_CHILD_ARG)
        .arg(db_path)
        .arg(limit.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .env_remove("DECIDUOUS_API_TOKEN")
        .spawn()
        .map_err(|e| ApiError::internal(&format!("starting the query process: {e}")))?;
    let deadline = std::time::Instant::now() + QUERY_TIME_LIMIT;
    // Stdin and stdout on threads of their own: the SQL can be up to the
    // body limit and the answer up to 8 MiB, more than a pipe holds, so
    // writing or reading on this thread could block past the deadline.
    let mut stdin = child.stdin.take().expect("stdin is piped");
    let sql = sql.to_string();
    std::thread::spawn(move || {
        use std::io::Write;
        let _ = stdin.write_all(sql.as_bytes());
    });
    let mut stdout = child.stdout.take().expect("stdout is piped");
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut out = Vec::new();
        let read = stdout.read_to_end(&mut out).map(|_| out);
        let _ = tx.send(read);
    });
    let output = rx.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()));
    let output = match output {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(ApiError::internal(&format!(
                "reading the query process: {e}"
            )));
        }
        Err(_) => {
            // Past the limit: stop it now, wherever it is, and reap it
            // before the slot is given back.
            let _ = child.kill();
            let _ = child.wait();
            return Err(time_limit_error());
        }
    };
    let status = child
        .wait()
        .map_err(|e| ApiError::internal(&format!("waiting for the query process: {e}")))?;
    let answer: Value = serde_json::from_slice(&output).map_err(|_| {
        ApiError::internal(&format!(
            "the query process ended ({status}) without an answer"
        ))
    })?;
    match (answer.get("ok"), answer.get("error")) {
        (Some(result), _) => Ok(result.clone()),
        (None, Some(err)) => Err(ApiError {
            status: err["status"].as_u64().unwrap_or(500) as u16,
            message: err["message"]
                .as_str()
                .unwrap_or("query failed")
                .to_string(),
        }),
        _ => Err(ApiError::internal(&format!(
            "the query process answered something unreadable ({status})"
        ))),
    }
}

/// The first argument that makes the deciduous executable a `/query` child
/// instead of the CLI. Handled before argument parsing, so it opens nothing
/// the CLI would (the project database, config).
pub const QUERY_CHILD_ARG: &str = "__api-query";

/// A `/query` child: `<exe> __api-query <db path> <row limit>`, SQL on
/// stdin, one JSON object on stdout, `{"ok": result}` or `{"error":
/// {"status", "message"}}`. `args` are the ones after QUERY_CHILD_ARG.
/// Returns the exit code. The parent kills it at the time limit.
pub fn query_child_main(args: &[String]) -> i32 {
    let answer = (|| -> Result<Value, ApiError> {
        let [db_path, limit] = args else {
            return Err(ApiError::internal(&format!(
                "{QUERY_CHILD_ARG} takes a database path and a row limit, got {args:?}"
            )));
        };
        let limit: usize = limit
            .parse()
            .map_err(|_| ApiError::internal(&format!("row limit {limit:?} is not a number")))?;
        let mut sql = String::new();
        std::io::stdin()
            .read_to_string(&mut sql)
            .map_err(|e| ApiError::internal(&format!("reading the SQL: {e}")))?;
        let conn = open_query_connection(Path::new(db_path))?;
        execute_query(&conn, &sql, limit)
    })();
    let out = match answer {
        Ok(result) => json!({ "ok": result }),
        Err(e) => json!({ "error": { "status": e.status, "message": e.message } }),
    };
    use std::io::Write;
    let mut stdout = std::io::stdout().lock();
    let written = serde_json::to_writer(&mut stdout, &out).is_ok() && stdout.flush().is_ok();
    if written {
        0
    } else {
        1
    }
}

fn time_limit_error() -> ApiError {
    ApiError::bad_request(&format!(
        "query stopped: it exceeded the {} s time limit",
        QUERY_TIME_LIMIT.as_secs()
    ))
}

fn open_query_connection(db_path: &Path) -> Result<rusqlite::Connection, ApiError> {
    let conn = rusqlite::Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| ApiError::internal(&format!("open read-only: {e}")))?;

    conn.pragma_update(None, "query_only", "ON")
        .map_err(|e| ApiError::internal(&format!("query_only pragma: {e}")))?;
    conn.busy_timeout(std::time::Duration::from_millis(2_000))
        .map_err(|e| ApiError::internal(&format!("busy timeout: {e}")))?;
    conn.set_limit(
        rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH,
        QUERY_MAX_VALUE_BYTES,
    );
    conn.set_limit(rusqlite::limits::Limit::SQLITE_LIMIT_ATTACHED, 0);
    // SQLITE_LIMIT_LENGTH caps a value once it is finished, not an
    // aggregate (json_group_object, group_concat) while it is built: one
    // such query held 1.5 GB until it was killed at 5 s. The heap limit is
    // process-wide, which is right here: this process runs one query.
    // An aggregate that hits it may ignore the failed append and run on
    // (json_group_array does, until its final step), so it is still stopped
    // at the time limit; what the limit bounds is its memory: 63 MB RSS
    // where it was 1.5 GB.
    conn.query_row(
        &format!("PRAGMA hard_heap_limit = {QUERY_MAX_HEAP_BYTES}"),
        [],
        |_| Ok(()),
    )
    .map_err(|e| ApiError::internal(&format!("hard_heap_limit pragma: {e}")))?;
    conn.authorizer(Some(query_authorizer));
    Ok(conn)
}

fn execute_query(conn: &rusqlite::Connection, sql: &str, limit: usize) -> Result<Value, ApiError> {
    let mut stmt = conn.prepare(sql).map_err(sql_error)?;
    if !stmt.readonly() {
        return Err(ApiError::forbidden(
            "only read-only SELECT statements are allowed",
        ));
    }

    let columns: Vec<String> = stmt.column_names().iter().map(|c| c.to_string()).collect();
    let n_cols = columns.len();

    let mut rows_out: Vec<Vec<Value>> = Vec::new();
    let mut truncated = false;
    let mut size = ByteCount(0);
    let mut rows = stmt.query([]).map_err(sql_error)?;
    while let Some(row) = rows.next().map_err(sql_error)? {
        if rows_out.len() >= limit {
            truncated = true;
            break;
        }
        let mut out = Vec::with_capacity(n_cols);
        for i in 0..n_cols {
            let value = sqlite_value_to_json(
                row.get_ref(i)
                    .map_err(|e| ApiError::internal(&format!("read column {i}: {e}")))?,
            );
            // Counted as it is built, so the limit bounds the memory too,
            // not only the body: checking the finished Vec would already
            // have held all of it.
            let _ = serde_json::to_writer(&mut size, &value);
            if size.0 > QUERY_MAX_RESPONSE_BYTES {
                return Err(ApiError::bad_request(&format!(
                    "query result is larger than {QUERY_MAX_RESPONSE_BYTES} bytes \
                     (reached in row {}); select fewer or shorter columns, or lower the limit",
                    rows_out.len() + 1
                )));
            }
            out.push(value);
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
