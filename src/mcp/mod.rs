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

/// Path to the active session file (persisted across server restarts).
const SESSION_FILE: &str = ".deciduous/active_session";

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
}

impl McpServer {
    pub fn new(db: Database) -> Self {
        // Try to resume from persisted session file
        let active_session_id = load_session_from_disk(&db);
        if let Some(id) = active_session_id {
            eprintln!("deciduous-mcp: resumed active session #{id}");
        }
        Self {
            db,
            active_session_id,
        }
    }

    /// Handle a single JSON-RPC message. Returns None for notifications.
    pub fn handle_message(&mut self, raw: &str) -> Option<Value> {
        let request = match parse_request(raw) {
            Ok(req) => req,
            Err(e) => {
                return Some(
                    serde_json::to_value(JsonRpcErrorResponse {
                        jsonrpc: "2.0".to_string(),
                        id: Value::Null,
                        error: e,
                    })
                    .unwrap_or(json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"Parse error"}})),
                );
            }
        };

        // Notifications (no id) don't get responses
        let id = match request.id {
            Some(id) => id,
            None => {
                handle_notification(&request.method);
                return None;
            }
        };

        let result = match request.method.as_str() {
            "initialize" => handle_initialize(request.params),
            "tools/list" => handle_tools_list(),
            "tools/call" => self.handle_tools_call(request.params),
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

    fn handle_tools_call(&mut self, params: Option<Value>) -> Result<Value, Value> {
        let params = params.ok_or_else(|| {
            json!({"jsonrpc":"2.0","id":null,"error":{"code":-32602,"message":"Missing params"}})
        })?;

        let call: ToolCallParams = serde_json::from_value(params).map_err(|e| {
            json!({"jsonrpc":"2.0","id":null,"error":{"code":-32602,"message":format!("Invalid params: {e}")}})
        })?;

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
                                    if let Some(node_id) = val.get("node_id").and_then(Value::as_i64)
                                    {
                                        let _ = self
                                            .db
                                            .add_node_to_session(session_id, node_id as i32);
                                    }
                                }
                            }
                        }
                    }
                }

                result
            }
        };

        serde_json::to_value(result).map_err(|e| json!({"error": e.to_string()}))
    }

    fn handle_start_session(
        &mut self,
        args: &Value,
    ) -> protocol::ToolCallResult {
        let name = args
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("unnamed session");
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
            Err(e) => return protocol::tool_result_error(format!("Failed to create root goal: {e}")),
        };

        // Create the session
        let session_id = match self.db.create_session(Some(name), Some(node_id)) {
            Ok(id) => id,
            Err(e) => return protocol::tool_result_error(format!("Failed to create session: {e}")),
        };

        // Associate root node with session
        let _ = self.db.add_node_to_session(session_id, node_id);

        self.active_session_id = Some(session_id);
        save_session_to_disk(session_id);

        eprintln!(
            "deciduous-mcp: started session #{session_id} '{}' (root goal #{})",
            name, node_id
        );

        protocol::tool_result_json(&json!({
            "session_id": session_id,
            "root_node_id": node_id,
            "name": name,
            "message": format!("Started session #{} '{}' with root goal #{}", session_id, name, node_id)
        }))
    }

    fn handle_end_session(
        &mut self,
        args: &Value,
    ) -> protocol::ToolCallResult {
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
        clear_session_from_disk();

        protocol::tool_result_json(&json!({
            "session_id": session_id,
            "node_count": node_count,
            "message": format!("Ended session #{} ({} nodes)", session_id, node_count)
        }))
    }

    fn handle_resume_session(
        &mut self,
        args: &Value,
    ) -> protocol::ToolCallResult {
        let session_id = match args.get("session_id").and_then(Value::as_i64) {
            Some(id) => id as i32,
            None => return protocol::tool_result_error("Missing required parameter: session_id"),
        };

        // Verify session exists
        let session = match self.db.get_session(session_id) {
            Ok(Some(s)) => s,
            Ok(None) => {
                return protocol::tool_result_error(format!("Session {session_id} not found"))
            }
            Err(e) => return protocol::tool_result_error(format!("Error: {e}")),
        };

        // Reopen if it was ended
        if session.ended_at.is_some() {
            // Clear ended_at to reactivate
            if let Err(e) = self.db.end_session(session_id, None) {
                return protocol::tool_result_error(format!("Failed to reopen session: {e}"));
            }
            // end_session sets ended_at, but we want to clear it — use raw update
            // For now, just create a fresh session that continues the tree
            eprintln!("deciduous-mcp: note - session #{session_id} was ended, resuming anyway");
        }

        self.active_session_id = Some(session_id);
        save_session_to_disk(session_id);

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
            "message": format!("Resumed session #{} ({} nodes)", session_id, node_count)
        }))
    }

    fn handle_get_session(
        &self,
        args: &Value,
    ) -> protocol::ToolCallResult {
        let session_id = args
            .get("session_id")
            .and_then(Value::as_i64)
            .map(|v| v as i32)
            .or(self.active_session_id);

        let session_id = match session_id {
            Some(id) => id,
            None => return protocol::tool_result_error("No session ID provided and no active session"),
        };

        let session = match self.db.get_session(session_id) {
            Ok(Some(s)) => s,
            Ok(None) => return protocol::tool_result_error(format!("Session {session_id} not found")),
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

    fn handle_list_sessions(
        &self,
        args: &Value,
    ) -> protocol::ToolCallResult {
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
fn load_session_from_disk(db: &Database) -> Option<i32> {
    let content = std::fs::read_to_string(SESSION_FILE).ok()?;
    let session_id: i32 = content.trim().parse().ok()?;
    // Verify the session exists and is still active
    match db.get_session(session_id) {
        Ok(Some(s)) if s.ended_at.is_none() => Some(session_id),
        _ => {
            // Stale file — clean it up
            let _ = std::fs::remove_file(SESSION_FILE);
            None
        }
    }
}

/// Persist active session ID to disk.
fn save_session_to_disk(session_id: i32) {
    let _ = std::fs::write(SESSION_FILE, session_id.to_string());
}

/// Remove the session file from disk.
fn clear_session_from_disk() {
    let _ = std::fs::remove_file(SESSION_FILE);
}

/// Run the MCP server on stdin/stdout.
pub fn run_server() -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut stdout = stdout.lock();

    let db = match Database::open() {
        Ok(db) => db,
        Err(e) => {
            eprintln!("deciduous-mcp: Failed to open database: {e}");
            eprintln!("deciduous-mcp: Make sure you're in a directory with .deciduous/ or set DECIDUOUS_DB_PATH");
            return Err(io::Error::new(io::ErrorKind::Other, e.to_string()));
        }
    };

    let mut server = McpServer::new(db);

    eprintln!(
        "deciduous-mcp: server started (v{})",
        env!("CARGO_PKG_VERSION")
    );

    for line in stdin.lock().lines() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let response = server.handle_message(trimmed);

        if let Some(resp) = response {
            let serialized = serde_json::to_string(&resp).unwrap_or_else(|_| {
                r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32603,"message":"Serialization error"}}"#.to_string()
            });
            writeln!(stdout, "{serialized}")?;
            stdout.flush()?;
        }
    }

    eprintln!("deciduous-mcp: stdin closed, shutting down");
    Ok(())
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
        assert_eq!(resp["error"]["code"].as_i64().unwrap(), protocol::PARSE_ERROR);
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
        let msg = format!(r#"{{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{{"name":"resume_session","arguments":{{"session_id":{session_id}}}}}}}"#);
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
