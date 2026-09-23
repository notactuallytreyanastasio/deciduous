//! MCP (Model Context Protocol) server for deciduous.
//!
//! Architecture: **functional core, imperative shell**.
//!
//! - `protocol` — Pure JSON-RPC 2.0 and MCP message types (no IO)
//! - `tools` — Tool registry and definitions (no IO)
//! - `handlers` — Tool handler functions (Database → Result<Value>)
//! - `query` — Graph analysis and reporting (pure functions on DecisionGraph)
//! - This module (`mod.rs`) — The imperative shell: stdin/stdout IO loop

pub mod handlers;
pub mod protocol;
pub mod query;
pub mod tools;

use crate::db::Database;
use protocol::{
    build_initialize_result, error_response, parse_request, success_response, tool_result_error,
    JsonRpcErrorResponse, ToolCallParams, ToolListResult, METHOD_NOT_FOUND,
};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

/// Name of the active session file, next to the database (persisted across
/// server restarts).
const SESSION_FILE: &str = "active_session";

/// MCP server state — holds database connection and active session.
///
/// One server instance per conversation. Each conversation gets its own
/// session (decision tree) that nodes are automatically associated with.
///
/// Session ID is persisted to `.deciduous/active_session` so it survives
/// server restarts (MCP clients may kill and re-spawn the server process).
pub struct McpServer {
    db: Database,
    active_session_id: Option<i32>,
    /// Resolved once, from the database's location, never from the cwd.
    session_file: std::path::PathBuf,
}

impl McpServer {
    pub fn new(db: Database) -> Self {
        let session_file = db.data_dir().join(SESSION_FILE);
        // Try to resume from persisted session file
        let active_session_id = load_session_from_disk(&db, &session_file);
        if let Some(id) = active_session_id {
            eprintln!("deciduous-mcp: resumed active session #{id}");
        }
        Self {
            db,
            active_session_id,
            session_file,
        }
    }

    /// Handle a single JSON-RPC message. Returns None for notifications.
    pub fn handle_message(&mut self, raw: &str) -> Option<Value> {
        let request = match parse_request(raw) {
            Ok(req) => req,
            Err(e) => {
                // Answer the request that caused it whenever its id can be
                // read: a reply with "id": null matches nothing, and the
                // client waits for its answer forever.
                return Some(error_to_value(JsonRpcErrorResponse {
                    jsonrpc: "2.0".to_string(),
                    id: protocol::request_id(raw),
                    error: e,
                }));
            }
        };

        // Notifications (no id member at all) don't get responses
        let id = match request.id {
            Some(Value::Null) => {
                return Some(error_to_value(error_response(
                    Value::Null,
                    protocol::INVALID_REQUEST,
                    "id must be a string or a number, not null",
                )));
            }
            Some(id) => id,
            None => {
                handle_notification(&request.method);
                return None;
            }
        };

        let result = match request.method.as_str() {
            "initialize" => handle_initialize(request.params),
            "tools/list" => handle_tools_list(),
            "tools/call" => self.handle_tools_call(&id, request.params),
            "ping" => Ok(json!({})),
            method => Err(error_to_value(error_response(
                id.clone(),
                METHOD_NOT_FOUND,
                format!("Unknown method: {method}"),
            ))),
        };

        Some(match result {
            Ok(value) => serde_json::to_value(success_response(id, value)).unwrap_or(json!(null)),
            Err(err_value) => err_value,
        })
    }

    fn handle_tools_call(&mut self, id: &Value, params: Option<Value>) -> Result<Value, Value> {
        let invalid = |message: String| {
            error_to_value(error_response(
                id.clone(),
                protocol::INVALID_PARAMS,
                message,
            ))
        };
        let params = params.ok_or_else(|| invalid("Missing params".to_string()))?;

        let call: ToolCallParams =
            serde_json::from_value(params).map_err(|e| invalid(format!("Invalid params: {e}")))?;

        if !tools::is_valid_tool(&call.name) {
            let result = tool_result_error(format!(
                "Unknown tool: {}. Use tools/list to see available tools.",
                call.name
            ));
            return serde_json::to_value(result).map_err(|e| json!({"error": e.to_string()}));
        }

        let args = call.arguments.unwrap_or(json!({}));
        if let Err(msg) = tools::validate_tool_args(&call.name, &args) {
            let result = tool_result_error(msg);
            return serde_json::to_value(result).map_err(|e| json!({"error": e.to_string()}));
        }

        // Session lifecycle tools need mutable access to server state
        let result = match call.name.as_str() {
            "start_session" => self.handle_start_session(&args),
            "end_session" => self.handle_end_session(&args),
            "resume_session" => self.handle_resume_session(&args),
            "get_session" => self.handle_get_session(&args),
            "list_sessions" => self.handle_list_sessions(&args),
            _ => {
                let result = handlers::dispatch(&self.db, &call.name, args);

                // Auto-associate newly created nodes with active session
                if let Some(session_id) = self.active_session_id {
                    if call.name == "add_node" {
                        // Extract node_id from the result
                        if result.is_error.is_none() {
                            if let Some(text) = result.content.first().map(|c| &c.text) {
                                if let Ok(val) = serde_json::from_str::<Value>(text) {
                                    if let Ok(Some(node_id)) = handlers::get_id(&val, "node_id") {
                                        let _ = self.db.add_node_to_session(session_id, node_id);
                                    }
                                }
                            }
                        }
                    }
                }

                result
            }
        };
        let mut result = result;
        attach_notices(&mut result);

        serde_json::to_value(result).map_err(|e| json!({"error": e.to_string()}))
    }

    fn handle_start_session(&mut self, args: &Value) -> protocol::ToolCallResult {
        let name = args
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("unnamed session");
        // Checked before the root goal is created, so a refused start leaves
        // nothing behind.
        if name.trim().is_empty() {
            return protocol::tool_result_error("a session name, when given, must not be empty");
        }
        let goal_title = args
            .get("goal_title")
            .and_then(Value::as_str)
            .unwrap_or("Session goal");
        let goal_prompt = args.get("goal_prompt").and_then(Value::as_str);

        // Create the root goal node
        let branch = crate::db::get_current_git_branch();
        let node_id = match self.db.create_node_full(
            "goal",
            goal_title,
            None,
            Some(90),
            None,
            goal_prompt,
            None,
            branch.as_deref(),
            None,
        ) {
            Ok(id) => id,
            Err(e) => {
                return protocol::tool_result_error(format!("Failed to create root goal: {e}"))
            }
        };

        // Create the session
        let session_id = match self.db.create_session(Some(name), Some(node_id)) {
            Ok(id) => id,
            Err(e) => return protocol::tool_result_error(format!("Failed to create session: {e}")),
        };

        // Associate root node with session
        let _ = self.db.add_node_to_session(session_id, node_id);

        // One file per project, so two servers in one project share it. Say
        // when this start displaces a live session: a restart will now
        // resume this one instead. That includes the session this server is
        // in: a server adopts whatever the file names when it starts, so
        // "my own session" is often another server's, and leaving it out
        // (as this did) hid exactly the common case.
        let replaced = load_session_from_disk(&self.db, &self.session_file);
        self.active_session_id = Some(session_id);
        if let Err(e) = save_session_to_disk(&self.session_file, session_id) {
            return protocol::tool_result_error(format!(
                "Session #{session_id} started (root goal #{node_id}) but could not be saved to {}: {e}. \
                 It will not survive a server restart.",
                self.session_file.display()
            ));
        }

        eprintln!(
            "deciduous-mcp: started session #{session_id} '{}' (root goal #{})",
            name, node_id
        );

        let mut message = format!(
            "Started session #{} '{}' with root goal #{}",
            session_id, name, node_id
        );
        if let Some(other) = replaced {
            message.push_str(&format!(
                ". Session #{other} is still open, and another deciduous server in this \
                 project may be using it, but it is no longer the one {} resumes after a restart",
                self.session_file.display()
            ));
        }
        protocol::tool_result_json(&json!({
            "session_id": session_id,
            "root_node_id": node_id,
            "name": name,
            "replaced_session_id": replaced,
            "message": message
        }))
    }

    fn handle_end_session(&mut self, args: &Value) -> protocol::ToolCallResult {
        let session_id = match self.active_session_id {
            Some(id) => id,
            None => return protocol::tool_result_error("No active session to end"),
        };

        let summary = args.get("summary").and_then(Value::as_str);

        if let Err(e) = self.db.end_session(session_id, summary) {
            return protocol::tool_result_error(format!("Failed to end session: {e}"));
        }

        // Get node count for the summary
        let node_count = self
            .db
            .get_session_nodes(session_id)
            .map(|n| n.len())
            .unwrap_or(0);

        eprintln!("deciduous-mcp: ended session #{session_id} ({node_count} nodes)");

        self.active_session_id = None;
        // Only if the file still names this session. Another server may have
        // started its own since, and deleting the file then dropped that
        // live session from every restart, with no warning to anyone.
        if let Err(e) = clear_session_from_disk(&self.session_file, session_id) {
            return protocol::tool_result_error(format!(
                "Ended session #{session_id}, but {} could not be removed: {e}. \
                 A restarted server would resume the ended session.",
                self.session_file.display()
            ));
        }

        protocol::tool_result_json(&json!({
            "session_id": session_id,
            "node_count": node_count,
            "message": format!("Ended session #{} ({} nodes)", session_id, node_count)
        }))
    }

    fn handle_resume_session(&mut self, args: &Value) -> protocol::ToolCallResult {
        let session_id = match handlers::get_id(args, "session_id") {
            Ok(Some(id)) => id,
            Ok(None) => {
                return protocol::tool_result_error("Missing required parameter: session_id")
            }
            Err(e) => return protocol::tool_result_error(e.message),
        };

        // Verify session exists
        let session = match self.db.get_session(session_id) {
            Ok(Some(s)) => s,
            Ok(None) => {
                return protocol::tool_result_error(format!("Session {session_id} not found"))
            }
            Err(e) => return protocol::tool_result_error(format!("Error: {e}")),
        };

        // Reopen if it was ended. This used to call end_session(id, None),
        // which stamped a new ended_at and wiped the summary while the reply
        // said "Resumed", and the file check at the next start then threw the
        // ended session away.
        let reopened = session.ended_at.is_some();
        if reopened {
            if let Err(e) = self.db.reopen_session(session_id) {
                return protocol::tool_result_error(format!("Failed to reopen session: {e}"));
            }
        }

        self.active_session_id = Some(session_id);
        if let Err(e) = save_session_to_disk(&self.session_file, session_id) {
            return protocol::tool_result_error(format!(
                "Resumed session #{session_id} but could not save it to {}: {e}. \
                 It will not survive a server restart.",
                self.session_file.display()
            ));
        }

        let node_count = self
            .db
            .get_session_nodes(session_id)
            .map(|n| n.len())
            .unwrap_or(0);

        eprintln!("deciduous-mcp: resumed session #{session_id} ({node_count} nodes)");

        protocol::tool_result_json(&json!({
            "session_id": session_id,
            "name": session.name,
            "root_node_id": session.root_node_id,
            "node_count": node_count,
            "reopened": reopened,
            "message": format!(
                "Resumed session #{} ({} nodes){}",
                session_id,
                node_count,
                if reopened { "; it had been ended and is open again" } else { "" }
            )
        }))
    }

    fn handle_get_session(&self, args: &Value) -> protocol::ToolCallResult {
        let session_id = match handlers::get_id(args, "session_id") {
            Ok(id) => id.or(self.active_session_id),
            Err(e) => return protocol::tool_result_error(e.message),
        };

        let session_id = match session_id {
            Some(id) => id,
            None => {
                return protocol::tool_result_error("No session ID provided and no active session")
            }
        };

        let session = match self.db.get_session(session_id) {
            Ok(Some(s)) => s,
            Ok(None) => {
                return protocol::tool_result_error(format!("Session {session_id} not found"))
            }
            Err(e) => return protocol::tool_result_error(format!("Error: {e}")),
        };

        let nodes = self.db.get_session_nodes(session_id).unwrap_or_default();
        let node_summaries: Vec<Value> = nodes
            .iter()
            .map(|n| {
                json!({
                    "id": n.id,
                    "node_type": n.node_type,
                    "title": n.title,
                    "status": n.status,
                    "created_at": n.created_at,
                })
            })
            .collect();

        protocol::tool_result_json(&json!({
            "session_id": session.id,
            "name": session.name,
            "started_at": session.started_at,
            "ended_at": session.ended_at,
            "root_node_id": session.root_node_id,
            "summary": session.summary,
            "is_active": session.ended_at.is_none(),
            "node_count": nodes.len(),
            "nodes": node_summaries,
        }))
    }

    fn handle_list_sessions(&self, args: &Value) -> protocol::ToolCallResult {
        let active_only = args
            .get("active_only")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        let sessions = match self.db.get_sessions(active_only) {
            Ok(s) => s,
            Err(e) => return protocol::tool_result_error(format!("Error: {e}")),
        };

        let session_list: Vec<Value> = sessions
            .iter()
            .map(|s| {
                json!({
                    "session_id": s.id,
                    "name": s.name,
                    "started_at": s.started_at,
                    "ended_at": s.ended_at,
                    "root_node_id": s.root_node_id,
                    "is_active": s.ended_at.is_none(),
                })
            })
            .collect();

        protocol::tool_result_json(&json!({
            "count": sessions.len(),
            "active_session_id": self.active_session_id,
            "sessions": session_list,
        }))
    }
}

// ---------------------------------------------------------------------------
// Session persistence (disk)
// ---------------------------------------------------------------------------

/// Load active session ID from disk. Returns None if no file or session ended.
fn load_session_from_disk(db: &Database, file: &std::path::Path) -> Option<i32> {
    let content = std::fs::read_to_string(file).ok()?;
    let session_id: i32 = content.trim().parse().ok()?;
    // Verify the session exists and is still active
    match db.get_session(session_id) {
        Ok(Some(s)) if s.ended_at.is_none() => Some(session_id),
        _ => {
            // Stale file — clean it up
            let _ = std::fs::remove_file(file);
            None
        }
    }
}

/// Persist active session ID to disk.
fn save_session_to_disk(file: &std::path::Path, session_id: i32) -> io::Result<()> {
    std::fs::write(file, session_id.to_string())
}

/// Remove the session file if it names `session_id`. Already gone, or
/// naming another session, is fine.
fn clear_session_from_disk(file: &std::path::Path, session_id: i32) -> io::Result<()> {
    match std::fs::read_to_string(file) {
        Ok(content) if content.trim() != session_id.to_string() => return Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        _ => {}
    }
    match std::fs::remove_file(file) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// Puts what the server log has to say into a tool result, which the agent
/// reads; stderr, where it used to go only, the agent never sees (RUST-N7).
///
/// A write this call made that was not queued makes the call an error: its
/// text says the local write happened, so it is not retried. A refusal
/// found by the replay thread is about an earlier write, and is added to
/// whichever call comes next without changing that call's own outcome.
fn attach_notices(result: &mut protocol::ToolCallResult) {
    for n in crate::oplog::take_notices() {
        if n.kind == crate::oplog::NoticeKind::Unqueued {
            result.is_error = Some(true);
        }
        result.content.push(protocol::ToolResultContent {
            content_type: "text".to_string(),
            text: n.text,
        });
    }
}

/// Run the MCP server on stdin/stdout.
pub fn run_server() -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut stdout = stdout.lock();

    let db_path = Database::db_path();
    let db = match Database::open() {
        Ok(db) => db,
        Err(e) => {
            eprintln!(
                "deciduous-mcp: Failed to open database {}: {e}",
                db_path.display()
            );
            // Only a missing project is fixed by changing directory; a locked
            // or unreadable database is not, and saying so sent people
            // looking for a .deciduous/ that was right there.
            if !db_path.parent().is_some_and(|d| d.is_dir()) {
                eprintln!("deciduous-mcp: Make sure you're in a directory with .deciduous/ or set DECIDUOUS_DB_PATH");
            }
            return Err(io::Error::other(e.to_string()));
        }
    };

    let mut server = McpServer::new(db);
    let replayer = Replayer::spawn();

    eprintln!(
        "deciduous-mcp: server started (v{})",
        env!("CARGO_PKG_VERSION")
    );

    // Read bytes, not `lines()`: one invalid UTF-8 byte made `lines()`
    // return an error, which ended the loop and the server with it, and the
    // client lost every tool for the rest of the session.
    let mut stdin = stdin.lock();
    let mut buf = Vec::new();
    loop {
        buf.clear();
        if stdin.read_until(b'\n', &mut buf)? == 0 {
            break;
        }
        let mut raw_id = None;
        let response = match std::str::from_utf8(&buf) {
            Ok(line) => {
                // RFC 8259 lets a parser ignore a leading byte order mark,
                // and `trim` does not count U+FEFF as whitespace: the
                // message was answered as a parse error with id null.
                let trimmed = line.trim().trim_start_matches('\u{feff}').trim_start();
                if trimmed.is_empty() {
                    continue;
                }
                raw_id = protocol::undecodable_raw_id(trimmed);
                server.handle_message(trimmed)
            }
            Err(e) => {
                let lossy = String::from_utf8_lossy(&buf);
                Some(error_to_value(error_response(
                    protocol::request_id(lossy.trim()),
                    protocol::PARSE_ERROR,
                    format!("Parse error: the message is not valid UTF-8 ({e})"),
                )))
            }
        };

        if let Some(resp) = response {
            let spliced = raw_id.and_then(|raw| protocol::splice_raw_id(&resp, raw));
            let serialized = spliced
                .map(Ok)
                .unwrap_or_else(|| serde_json::to_string(&resp));
            let serialized = serialized.unwrap_or_else(|_| {
                r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32603,"message":"Serialization error"}}"#.to_string()
            });
            writeln!(stdout, "{serialized}")?;
            stdout.flush()?;
        }

        // Handed to the replay thread, so no answer waits on the network.
        // Warnings go to stderr; stdout is the protocol.
        if let Some(log) = crate::oplog::take_appended() {
            replayer.send(log);
        }
    }

    eprintln!("deciduous-mcp: stdin closed, shutting down");
    // What is still waiting gets one last try, bounded by the replay's own
    // timeout, so closing the session does not strand the last writes.
    replayer.finish();
    Ok(())
}

/// Sends the log to the server on its own thread.
///
/// The loop used to replay after each answer, on the loop's thread. Against a
/// server that accepts connections and never answers, every request after a
/// write, reads and pings included, then waited out the replay: about 135 s,
/// and again after every write (RUST-N2), well past an MCP client's
/// timeout. A write's answer does not depend on the server; the local write
/// and its op are already on disk when it is sent.
///
/// Requests that arrive while a replay is running are coalesced: one replay
/// sends everything pending, so a burst of writes costs one more replay, not
/// one each.
struct Replayer {
    tx: Option<std::sync::mpsc::Sender<crate::oplog::OpLog>>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Replayer {
    fn spawn() -> Self {
        let (tx, rx) = std::sync::mpsc::channel::<crate::oplog::OpLog>();
        let handle = std::thread::Builder::new()
            .name("deciduous-replay".into())
            .spawn(move || {
                let mut last = None;
                while let Ok(mut log) = rx.recv() {
                    while let Ok(newer) = rx.try_recv() {
                        log = newer;
                    }
                    crate::remote::replay_after_write_quietly(&log, &mut last);
                }
            })
            .ok();
        if handle.is_none() {
            eprintln!(
                "deciduous-mcp: could not start the replay thread; writes are sent on this thread instead"
            );
        }
        Replayer {
            tx: handle.as_ref().map(|_| tx),
            handle,
        }
    }

    fn send(&self, log: crate::oplog::OpLog) {
        match &self.tx {
            Some(tx) => {
                if let Err(e) = tx.send(log) {
                    crate::remote::replay_after_write(&e.0);
                }
            }
            None => crate::remote::replay_after_write(&log),
        }
    }

    fn finish(mut self) {
        drop(self.tx.take());
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn handle_notification(method: &str) {
    match method {
        "notifications/initialized" => {
            eprintln!("deciduous-mcp: client initialized");
        }
        "notifications/cancelled" => {
            eprintln!("deciduous-mcp: request cancelled");
        }
        _ => {
            eprintln!("deciduous-mcp: unknown notification: {method}");
        }
    }
}

fn handle_initialize(params: Option<Value>) -> Result<Value, Value> {
    if let Some(ref p) = params {
        if let Some(info) = p.get("clientInfo") {
            let name = info
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            eprintln!("deciduous-mcp: client connected: {name}");
        }
    }

    let result = build_initialize_result();
    serde_json::to_value(result).map_err(|e| json!({"error": e.to_string()}))
}

fn handle_tools_list() -> Result<Value, Value> {
    let tools = tools::all_tool_definitions();
    let result = ToolListResult { tools };
    serde_json::to_value(result).map_err(|e| json!({"error": e.to_string()}))
}

fn error_to_value(err: JsonRpcErrorResponse) -> Value {
    serde_json::to_value(err).unwrap_or(
        json!({"jsonrpc":"2.0","id":null,"error":{"code":-32603,"message":"Internal error"}}),
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_server() -> McpServer {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let db = Database::new(path.to_str().unwrap()).unwrap();
        std::mem::forget(dir);
        McpServer::new(db)
    }

    #[test]
    fn test_handle_initialize() {
        let mut server = test_server();
        let msg = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"clientInfo":{"name":"test-client","version":"1.0"}}}"#;
        let resp = server.handle_message(msg).unwrap();
        assert_eq!(resp["id"], 1);
        assert_eq!(
            resp["result"]["serverInfo"]["name"].as_str().unwrap(),
            "deciduous"
        );
    }

    #[test]
    fn test_handle_tools_list() {
        let mut server = test_server();
        let msg = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#;
        let resp = server.handle_message(msg).unwrap();
        let tools = resp["result"]["tools"].as_array().unwrap();
        assert!(!tools.is_empty());
    }

    #[test]
    fn test_handle_tools_call_add_node() {
        let mut server = test_server();
        let msg = r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"add_node","arguments":{"node_type":"goal","title":"Test MCP goal","confidence":90}}}"#;
        let resp = server.handle_message(msg).unwrap();
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("node_id"));
    }

    #[test]
    fn test_handle_tools_call_unknown_tool() {
        let mut server = test_server();
        let msg = r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"nonexistent_tool","arguments":{}}}"#;
        let resp = server.handle_message(msg).unwrap();
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("Unknown tool"));
    }

    #[test]
    fn test_handle_unknown_method() {
        let mut server = test_server();
        let msg = r#"{"jsonrpc":"2.0","id":6,"method":"nonexistent/method"}"#;
        let resp = server.handle_message(msg).unwrap();
        assert_eq!(resp["error"]["code"].as_i64().unwrap(), METHOD_NOT_FOUND);
    }

    #[test]
    fn test_handle_notification_no_response() {
        let mut server = test_server();
        let msg = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
        let resp = server.handle_message(msg);
        assert!(resp.is_none());
    }

    #[test]
    fn test_handle_parse_error() {
        let mut server = test_server();
        let resp = server.handle_message("not json at all");
        assert!(resp.is_some());
        let resp = resp.unwrap();
        assert_eq!(
            resp["error"]["code"].as_i64().unwrap(),
            protocol::PARSE_ERROR
        );
    }

    #[test]
    fn test_handle_ping() {
        let mut server = test_server();
        let msg = r#"{"jsonrpc":"2.0","id":7,"method":"ping"}"#;
        let resp = server.handle_message(msg).unwrap();
        assert_eq!(resp["id"], 7);
    }

    #[test]
    fn test_full_roundtrip_add_then_list() {
        let mut server = test_server();

        let add_msg = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"add_node","arguments":{"node_type":"goal","title":"Roundtrip test"}}}"#;
        server.handle_message(add_msg);

        let list_msg = r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"list_nodes","arguments":{}}}"#;
        let resp = server.handle_message(list_msg).unwrap();
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("Roundtrip test"));
    }

    #[test]
    fn test_full_roundtrip_add_link_trace() {
        let mut server = test_server();

        server.handle_message(r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"add_node","arguments":{"node_type":"goal","title":"Auth"}}}"#);
        server.handle_message(r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"add_node","arguments":{"node_type":"action","title":"Implement"}}}"#);
        server.handle_message(r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"link_nodes","arguments":{"from_id":1,"to_id":2}}}"#);

        let resp = server.handle_message(r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"trace_chain","arguments":{"node_id":1}}}"#).unwrap();
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("\"node_count\": 2"));
    }

    // -- Session tests --

    #[test]
    fn test_start_session() {
        let mut server = test_server();

        let msg = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"start_session","arguments":{"name":"test session","goal_title":"Test goal"}}}"#;
        let resp = server.handle_message(msg).unwrap();
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("session_id"));
        assert!(text.contains("root_node_id"));
        assert!(server.active_session_id.is_some());
    }

    #[test]
    fn test_session_auto_associates_nodes() {
        let mut server = test_server();

        // Start session
        server.handle_message(r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"start_session","arguments":{"name":"auto-assoc test","goal_title":"Session root"}}}"#);
        let session_id = server.active_session_id.unwrap();

        // Add a node — should auto-associate
        server.handle_message(r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"add_node","arguments":{"node_type":"action","title":"Session action"}}}"#);

        // Check session has 2 nodes (root goal + the action)
        let nodes = server.db.get_session_nodes(session_id).unwrap();
        assert_eq!(nodes.len(), 2, "Session should have root goal + action");
    }

    #[test]
    fn test_end_session() {
        let mut server = test_server();

        server.handle_message(r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"start_session","arguments":{"name":"ending test","goal_title":"Will end"}}}"#);
        assert!(server.active_session_id.is_some());

        let resp = server.handle_message(r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"end_session","arguments":{"summary":"Done testing"}}}"#).unwrap();
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("Ended session"));
        assert!(server.active_session_id.is_none());
    }

    #[test]
    fn test_end_session_no_active() {
        let mut server = test_server();

        let resp = server.handle_message(r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"end_session","arguments":{}}}"#).unwrap();
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("No active session"));
    }

    #[test]
    fn test_get_session() {
        let mut server = test_server();

        server.handle_message(r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"start_session","arguments":{"name":"view test","goal_title":"View this"}}}"#);

        let resp = server.handle_message(r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"get_session","arguments":{}}}"#).unwrap();
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("view test"));
        assert!(text.contains("\"is_active\": true"));
        assert!(text.contains("nodes"));
    }

    #[test]
    fn test_list_sessions() {
        let mut server = test_server();

        // Create two sessions
        server.handle_message(r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"start_session","arguments":{"name":"session A","goal_title":"Goal A"}}}"#);
        server.handle_message(r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"end_session","arguments":{"summary":"Done A"}}}"#);
        server.handle_message(r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"start_session","arguments":{"name":"session B","goal_title":"Goal B"}}}"#);

        // List all
        let resp = server.handle_message(r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"list_sessions","arguments":{}}}"#).unwrap();
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("\"count\": 2"));

        // List active only
        let resp = server.handle_message(r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"list_sessions","arguments":{"active_only":true}}}"#).unwrap();
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("\"count\": 1"));
        assert!(text.contains("session B"));
    }

    #[test]
    fn test_resume_session() {
        let mut server = test_server();

        // Start and end a session
        server.handle_message(r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"start_session","arguments":{"name":"resumable","goal_title":"Long running"}}}"#);
        let session_id = server.active_session_id.unwrap();
        server.handle_message(r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"end_session","arguments":{"summary":"Pausing"}}}"#);
        assert!(server.active_session_id.is_none());

        // Resume it
        let msg = format!(
            r#"{{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{{"name":"resume_session","arguments":{{"session_id":{session_id}}}}}}}"#
        );
        let resp = server.handle_message(&msg).unwrap();
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("Resumed session"));
        assert_eq!(server.active_session_id, Some(session_id));

        // New nodes should associate with the resumed session
        server.handle_message(r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"add_node","arguments":{"node_type":"action","title":"After resume"}}}"#);
        let nodes = server.db.get_session_nodes(session_id).unwrap();
        assert!(nodes.iter().any(|n| n.title == "After resume"));
    }

    #[test]
    fn test_resume_nonexistent_session() {
        let mut server = test_server();

        let resp = server.handle_message(r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"resume_session","arguments":{"session_id":999}}}"#).unwrap();
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("not found"));
    }

    #[test]
    fn test_nodes_without_session_not_associated() {
        let mut server = test_server();

        // Add node WITHOUT a session
        server.handle_message(r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"add_node","arguments":{"node_type":"goal","title":"No session"}}}"#);

        // Start session, add node WITH session
        server.handle_message(r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"start_session","arguments":{"name":"test","goal_title":"With session"}}}"#);
        server.handle_message(r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"add_node","arguments":{"node_type":"action","title":"In session"}}}"#);

        let session_id = server.active_session_id.unwrap();
        let nodes = server.db.get_session_nodes(session_id).unwrap();
        // Should have root goal + action, NOT the pre-session node
        assert_eq!(nodes.len(), 2);
        assert!(nodes.iter().all(|n| n.title != "No session"));
    }
}
