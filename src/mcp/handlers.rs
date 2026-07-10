//! MCP tool handlers — bridge between MCP tool calls and the Database API.
//!
//! Each handler function takes a `&Database` and `serde_json::Value` arguments,
//! validates the arguments, calls the appropriate Database method, and returns
//! a `serde_json::Value` result. Handlers are the business logic layer between
//! the MCP protocol and the database.

use crate::db::{self, Database};
use crate::mcp::protocol::{tool_result_error, tool_result_json, tool_result_text, ToolCallResult};
use crate::mcp::query;
use serde_json::{json, Value};

/// Handler error type — always convertible to a ToolCallResult.
#[derive(Debug)]
pub struct HandlerError {
    pub message: String,
}

impl std::fmt::Display for HandlerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl From<db::DbError> for HandlerError {
    fn from(e: db::DbError) -> Self {
        Self {
            message: e.to_string(),
        }
    }
}

impl From<String> for HandlerError {
    fn from(s: String) -> Self {
        Self { message: s }
    }
}

impl From<&str> for HandlerError {
    fn from(s: &str) -> Self {
        Self {
            message: s.to_string(),
        }
    }
}

pub type HandlerResult = std::result::Result<ToolCallResult, HandlerError>;

/// Dispatch a tool call to the appropriate handler.
pub fn dispatch(db: &Database, tool_name: &str, args: Value) -> ToolCallResult {
    let result = match tool_name {
        // CRUD
        "add_node" => handle_add_node(db, &args),
        "link_nodes" => handle_link_nodes(db, &args),
        "unlink_nodes" => handle_unlink_nodes(db, &args),
        "delete_node" => handle_delete_node(db, &args),
        "update_status" => handle_update_status(db, &args),
        "update_prompt" => handle_update_prompt(db, &args),
        // Querying
        "list_nodes" => handle_list_nodes(db, &args),
        "list_edges" => handle_list_edges(db),
        "show_node" => handle_show_node(db, &args),
        "get_graph" => handle_get_graph(db),
        "search_nodes" => handle_search_nodes(db, &args),
        // Documents
        "attach_document" => handle_attach_document(db, &args),
        "list_documents" => handle_list_documents(db, &args),
        // Themes
        "list_themes" => handle_list_themes(db),
        "create_theme" => handle_create_theme(db, &args),
        "tag_node" => handle_tag_node(db, &args),
        "untag_node" => handle_untag_node(db, &args),
        // Analysis
        "trace_chain" => handle_trace_chain(db, &args),
        "get_node_context" => handle_get_node_context(db, &args),
        "get_timeline" => handle_get_timeline(db, &args),
        "get_pulse" => handle_get_pulse(db, &args),
        "find_orphans" => handle_find_orphans(db),
        "get_branch_summary" => handle_get_branch_summary(db, &args),
        // Export
        "export_dot" => handle_export_dot(db, &args),
        "generate_writeup" => handle_generate_writeup(db, &args),
        "events_status" => handle_events_status(),
        _ => Err(HandlerError {
            message: format!("Unknown tool: {tool_name}"),
        }),
    };

    match result {
        Ok(r) => r,
        Err(e) => tool_result_error(e.message),
    }
}

// ---------------------------------------------------------------------------
// Argument extraction helpers (pure functions)
// ---------------------------------------------------------------------------

fn get_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str)
}

fn get_i32(args: &Value, key: &str) -> Option<i32> {
    args.get(key).and_then(Value::as_i64).map(|v| v as i32)
}

fn get_bool(args: &Value, key: &str) -> Option<bool> {
    args.get(key).and_then(Value::as_bool)
}

fn get_u8(args: &Value, key: &str) -> Option<u8> {
    args.get(key)
        .and_then(Value::as_u64)
        .map(|v| v.min(100) as u8)
}

fn require_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, HandlerError> {
    get_str(args, key)
        .ok_or_else(|| HandlerError::from(format!("Missing required parameter: {key}")))
}

fn require_i32(args: &Value, key: &str) -> Result<i32, HandlerError> {
    get_i32(args, key)
        .ok_or_else(|| HandlerError::from(format!("Missing required parameter: {key}")))
}

// ---------------------------------------------------------------------------
// Node serialization (pure function)
// ---------------------------------------------------------------------------

fn node_to_json(node: &crate::db::DecisionNode) -> Value {
    let mut obj = json!({
        "id": node.id,
        "change_id": node.change_id,
        "node_type": node.node_type,
        "title": node.title,
        "status": node.status,
        "created_at": node.created_at,
        "updated_at": node.updated_at,
    });

    if let Some(ref desc) = node.description {
        obj["description"] = json!(desc);
    }

    // Unpack metadata_json into top-level fields for easier consumption
    if let Some(meta) = node.metadata() {
        if let Some(obj_mut) = obj.as_object_mut() {
            if let Some(m) = meta.as_object() {
                for (k, v) in m {
                    obj_mut.insert(k.clone(), v.clone());
                }
            }
        }
    }

    obj
}

fn edge_to_json(edge: &crate::db::DecisionEdge) -> Value {
    let mut obj = json!({
        "id": edge.id,
        "from_node_id": edge.from_node_id,
        "to_node_id": edge.to_node_id,
        "edge_type": edge.edge_type,
        "created_at": edge.created_at,
    });

    if let Some(ref r) = edge.rationale {
        obj["rationale"] = json!(r);
    }
    if let Some(w) = edge.weight {
        obj["weight"] = json!(w);
    }

    obj
}

// ---------------------------------------------------------------------------
// CRUD handlers
// ---------------------------------------------------------------------------

fn handle_add_node(db: &Database, args: &Value) -> HandlerResult {
    let node_type = require_str(args, "node_type")?;
    let title = require_str(args, "title")?;
    let description = get_str(args, "description");
    let confidence = get_u8(args, "confidence");
    let prompt = get_str(args, "prompt");
    let files = get_str(args, "files");
    let branch = get_str(args, "branch");
    let commit = get_str(args, "commit");

    // Resolve HEAD to actual commit hash
    let resolved_commit = match commit {
        Some("HEAD") => db::get_current_git_commit(),
        Some(c) => Some(c.to_string()),
        None => None,
    };

    // Auto-detect branch if not specified
    let resolved_branch = match branch {
        Some(b) => Some(b.to_string()),
        None => db::get_current_git_branch(),
    };

    let node_id = db.create_node_full(
        node_type,
        title,
        description,
        confidence,
        resolved_commit.as_deref(),
        prompt,
        files,
        resolved_branch.as_deref(),
        None, // created_at
    )?;

    Ok(tool_result_json(&json!({
        "node_id": node_id,
        "node_type": node_type,
        "title": title,
        "message": format!("Created {} node #{}", node_type, node_id)
    })))
}

fn handle_link_nodes(db: &Database, args: &Value) -> HandlerResult {
    let from_id = require_i32(args, "from_id")?;
    let to_id = require_i32(args, "to_id")?;
    let rationale = get_str(args, "rationale");
    let edge_type = get_str(args, "edge_type").unwrap_or("leads_to");

    let edge_id = db.create_edge(from_id, to_id, edge_type, rationale)?;

    Ok(tool_result_json(&json!({
        "edge_id": edge_id,
        "from_id": from_id,
        "to_id": to_id,
        "edge_type": edge_type,
        "message": format!("Created edge #{} ({} -> {} via {})", edge_id, from_id, to_id, edge_type)
    })))
}

fn handle_unlink_nodes(db: &Database, args: &Value) -> HandlerResult {
    let from_id = require_i32(args, "from_id")?;
    let to_id = require_i32(args, "to_id")?;

    db.delete_edge(from_id, to_id)?;

    Ok(tool_result_json(&json!({
        "from_id": from_id,
        "to_id": to_id,
        "message": format!("Removed edge {} -> {}", from_id, to_id)
    })))
}

fn handle_delete_node(db: &Database, args: &Value) -> HandlerResult {
    let node_id = require_i32(args, "node_id")?;
    let dry_run = get_bool(args, "dry_run").unwrap_or(false);

    let summary = db.delete_node(node_id, dry_run)?;

    Ok(tool_result_json(&json!({
        "node_id": node_id,
        "node_title": summary.node_title,
        "edges_deleted": summary.edges_deleted,
        "dry_run": dry_run,
        "message": if dry_run {
            format!("Would delete node #{} '{}' and {} edges", node_id, summary.node_title, summary.edges_deleted)
        } else {
            format!("Deleted node #{} '{}' and {} edges", node_id, summary.node_title, summary.edges_deleted)
        }
    })))
}

fn handle_update_status(db: &Database, args: &Value) -> HandlerResult {
    let node_id = require_i32(args, "node_id")?;
    let status = require_str(args, "status")?;

    db.update_node_status(node_id, status)?;

    Ok(tool_result_json(&json!({
        "node_id": node_id,
        "status": status,
        "message": format!("Updated node #{} status to '{}'", node_id, status)
    })))
}

fn handle_update_prompt(db: &Database, args: &Value) -> HandlerResult {
    let node_id = require_i32(args, "node_id")?;
    let prompt = require_str(args, "prompt")?;

    db.update_node_prompt(node_id, prompt)?;

    Ok(tool_result_json(&json!({
        "node_id": node_id,
        "message": format!("Updated prompt on node #{}", node_id)
    })))
}

// ---------------------------------------------------------------------------
// Query handlers
// ---------------------------------------------------------------------------

fn handle_list_nodes(db: &Database, args: &Value) -> HandlerResult {
    let nodes = db.get_all_nodes()?;
    let branch_filter = get_str(args, "branch");
    let type_filter = get_str(args, "node_type");
    let status_filter = get_str(args, "status");
    let theme_filter = get_str(args, "theme");

    let filtered: Vec<Value> = nodes
        .iter()
        .filter(|n| {
            // Branch filter: check metadata_json for branch field
            if let Some(branch) = branch_filter {
                if n.branch().as_deref() != Some(branch) {
                    return false;
                }
            }
            // Type filter
            if let Some(t) = type_filter {
                if n.node_type != t {
                    return false;
                }
            }
            // Status filter
            if let Some(s) = status_filter {
                if n.status != s {
                    return false;
                }
            }
            true
        })
        .map(node_to_json)
        .collect();

    // Apply theme filter if specified (requires DB lookup)
    let result = if let Some(theme_name) = theme_filter {
        let theme_nodes = db.get_nodes_by_theme(theme_name).unwrap_or_default();
        let theme_node_ids: std::collections::HashSet<i32> =
            theme_nodes.iter().map(|n| n.id).collect();
        filtered
            .into_iter()
            .filter(|v| {
                v.get("id")
                    .and_then(Value::as_i64)
                    .map(|id| theme_node_ids.contains(&(id as i32)))
                    .unwrap_or(false)
            })
            .collect()
    } else {
        filtered
    };

    Ok(tool_result_json(&json!({
        "count": result.len(),
        "nodes": result
    })))
}

fn handle_list_edges(db: &Database) -> HandlerResult {
    let edges = db.get_all_edges()?;
    let result: Vec<Value> = edges.iter().map(edge_to_json).collect();

    Ok(tool_result_json(&json!({
        "count": result.len(),
        "edges": result
    })))
}

fn handle_show_node(db: &Database, args: &Value) -> HandlerResult {
    let node_id = require_i32(args, "node_id")?;

    let node = db
        .get_node(node_id)?
        .ok_or_else(|| HandlerError::from(format!("Node {node_id} not found")))?;

    let children = db.get_node_children(node_id).unwrap_or_default();
    let parents = db.get_node_parents(node_id).unwrap_or_default();
    let themes = db.get_node_themes(node_id).unwrap_or_default();
    let documents = db
        .get_node_documents(Some(node_id), false)
        .unwrap_or_default();

    let mut result = node_to_json(&node);
    if let Some(obj) = result.as_object_mut() {
        obj.insert(
            "children".to_string(),
            json!(children.iter().map(|n| json!({"id": n.id, "node_type": n.node_type, "title": n.title, "status": n.status})).collect::<Vec<_>>()),
        );
        obj.insert(
            "parents".to_string(),
            json!(parents.iter().map(|n| json!({"id": n.id, "node_type": n.node_type, "title": n.title, "status": n.status})).collect::<Vec<_>>()),
        );
        obj.insert(
            "themes".to_string(),
            json!(themes
                .iter()
                .map(|t| json!({"name": t.name, "color": t.color}))
                .collect::<Vec<_>>()),
        );
        obj.insert(
            "documents".to_string(),
            json!(documents
                .iter()
                .map(|d| json!({
                    "id": d.id,
                    "filename": d.original_filename,
                    "description": d.description,
                }))
                .collect::<Vec<_>>()),
        );
    }

    Ok(tool_result_json(&result))
}

fn handle_get_graph(db: &Database) -> HandlerResult {
    let graph = db.get_graph()?;

    Ok(tool_result_json(&json!({
        "node_count": graph.nodes.len(),
        "edge_count": graph.edges.len(),
        "nodes": graph.nodes.iter().map(node_to_json).collect::<Vec<_>>(),
        "edges": graph.edges.iter().map(edge_to_json).collect::<Vec<_>>(),
    })))
}

fn handle_search_nodes(db: &Database, args: &Value) -> HandlerResult {
    let query = require_str(args, "query")?;
    let type_filter = get_str(args, "node_type");
    let branch_filter = get_str(args, "branch");

    let query_lower = query.to_lowercase();
    let nodes = db.get_all_nodes()?;

    let results: Vec<Value> = nodes
        .iter()
        .filter(|n| {
            // Text match against title, description, and prompt in metadata
            let title_match = n.title.to_lowercase().contains(&query_lower);
            let desc_match = n
                .description
                .as_ref()
                .map(|d| d.to_lowercase().contains(&query_lower))
                .unwrap_or(false);
            let prompt_match = n
                .prompt()
                .is_some_and(|p| p.to_lowercase().contains(&query_lower));

            let text_match = title_match || desc_match || prompt_match;
            if !text_match {
                return false;
            }

            // Optional type filter
            if let Some(t) = type_filter {
                if n.node_type != t {
                    return false;
                }
            }

            // Optional branch filter
            if let Some(branch) = branch_filter {
                if n.branch().as_deref() != Some(branch) {
                    return false;
                }
            }

            true
        })
        .map(node_to_json)
        .collect();

    Ok(tool_result_json(&json!({
        "query": query,
        "count": results.len(),
        "nodes": results
    })))
}

// ---------------------------------------------------------------------------
// Document handlers
// ---------------------------------------------------------------------------

fn handle_attach_document(db: &Database, args: &Value) -> HandlerResult {
    use sha2::{Digest, Sha256};

    let node_id = require_i32(args, "node_id")?;
    let file_path = require_str(args, "file_path")?;
    let description = get_str(args, "description");

    let path = std::path::Path::new(file_path);
    if !path.exists() {
        return Err(HandlerError::from(format!("File not found: {file_path}")));
    }

    let original_filename = path
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let file_bytes =
        std::fs::read(path).map_err(|e| HandlerError::from(format!("Failed to read file: {e}")))?;

    let hash = format!("{:x}", Sha256::digest(&file_bytes));
    let hash_prefix = hash.get(..8).unwrap_or(hash.as_str());
    let storage_filename = format!("{original_filename}.{hash_prefix}");
    let file_size = file_bytes.len() as i32;

    // Simple MIME detection by extension
    let mime_type = match path.extension().and_then(|e| e.to_str()) {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("pdf") => "application/pdf",
        Some("md") => "text/markdown",
        Some("txt") => "text/plain",
        Some("json") => "application/json",
        Some("html") => "text/html",
        _ => "application/octet-stream",
    };

    // Store file in .deciduous/documents/
    let docs_dir = std::path::PathBuf::from(".deciduous/documents");
    std::fs::create_dir_all(&docs_dir)
        .map_err(|e| HandlerError::from(format!("Failed to create documents dir: {e}")))?;

    let dest = docs_dir.join(&storage_filename);
    if !dest.exists() {
        std::fs::write(&dest, &file_bytes)
            .map_err(|e| HandlerError::from(format!("Failed to write document: {e}")))?;
    }

    let desc_source = if description.is_some() {
        "manual"
    } else {
        "none"
    };
    let doc_id = db.attach_document(
        node_id,
        &hash,
        &original_filename,
        &storage_filename,
        mime_type,
        file_size,
        description,
        desc_source,
        None,
    )?;

    Ok(tool_result_json(&json!({
        "doc_id": doc_id,
        "node_id": node_id,
        "file_path": file_path,
        "message": format!("Attached document #{} to node #{}", doc_id, node_id)
    })))
}

fn handle_list_documents(db: &Database, args: &Value) -> HandlerResult {
    let node_id = get_i32(args, "node_id");
    let docs = db.get_node_documents(node_id, false).unwrap_or_default();

    let result: Vec<Value> = docs
        .iter()
        .map(|d| {
            json!({
                "id": d.id,
                "node_id": d.node_id,
                "filename": d.original_filename,
                "mime_type": d.mime_type,
                "description": d.description,
                "attached_at": d.attached_at,
            })
        })
        .collect();

    Ok(tool_result_json(&json!({
        "count": result.len(),
        "documents": result
    })))
}

// ---------------------------------------------------------------------------
// Theme handlers
// ---------------------------------------------------------------------------

fn handle_list_themes(db: &Database) -> HandlerResult {
    let themes = db.get_all_themes().unwrap_or_default();

    let result: Vec<Value> = themes
        .iter()
        .map(|t| {
            json!({
                "name": t.name,
                "color": t.color,
                "description": t.description,
            })
        })
        .collect();

    Ok(tool_result_json(&json!({
        "count": result.len(),
        "themes": result
    })))
}

fn handle_create_theme(db: &Database, args: &Value) -> HandlerResult {
    let name = require_str(args, "name")?;
    let color = get_str(args, "color").unwrap_or("#6b7280");
    let description = get_str(args, "description");

    let theme_id = db.create_theme(name, color, description)?;

    Ok(tool_result_json(&json!({
        "theme_id": theme_id,
        "name": name,
        "color": color,
        "message": format!("Created theme '{}' (#{}) ", name, theme_id)
    })))
}

fn handle_tag_node(db: &Database, args: &Value) -> HandlerResult {
    let node_id = require_i32(args, "node_id")?;
    let theme = require_str(args, "theme")?;

    db.tag_node(node_id, theme, "mcp")?;

    Ok(tool_result_json(&json!({
        "node_id": node_id,
        "theme": theme,
        "message": format!("Tagged node #{} with theme '{}'", node_id, theme)
    })))
}

fn handle_untag_node(db: &Database, args: &Value) -> HandlerResult {
    let node_id = require_i32(args, "node_id")?;
    let theme = require_str(args, "theme")?;

    let removed = db.untag_node(node_id, theme)?;

    Ok(tool_result_json(&json!({
        "node_id": node_id,
        "theme": theme,
        "removed": removed,
        "message": if removed {
            format!("Removed theme '{}' from node #{}", theme, node_id)
        } else {
            format!("Node #{} was not tagged with '{}'", node_id, theme)
        }
    })))
}

// ---------------------------------------------------------------------------
// Analysis handlers (delegates to query.rs pure functions)
// ---------------------------------------------------------------------------

fn handle_trace_chain(db: &Database, args: &Value) -> HandlerResult {
    let node_id = require_i32(args, "node_id")?;
    let max_depth = get_i32(args, "max_depth").unwrap_or(0) as usize;
    let direction = get_str(args, "direction")
        .map(query::TraceDirection::parse)
        .unwrap_or(query::TraceDirection::Both);

    let graph = db.get_graph()?;
    let result = query::trace_chain(&graph, node_id, max_depth, &direction);

    Ok(tool_result_json(&query::trace_result_to_json(&result)))
}

fn handle_get_node_context(db: &Database, args: &Value) -> HandlerResult {
    let node_id = require_i32(args, "node_id")?;

    let graph = db.get_graph()?;
    let ctx = query::get_node_context(&graph, node_id);

    if ctx.node.is_none() {
        return Err(HandlerError::from(format!("Node {node_id} not found")));
    }

    Ok(tool_result_json(&query::node_context_to_json(&ctx)))
}

fn handle_get_timeline(db: &Database, args: &Value) -> HandlerResult {
    let limit = get_i32(args, "limit").unwrap_or(50) as usize;
    let node_type = get_str(args, "node_type");
    let branch = get_str(args, "branch");
    let since = get_str(args, "since");

    let graph = db.get_graph()?;
    let nodes = query::get_timeline(&graph, limit, node_type, branch, since);

    Ok(tool_result_json(&query::timeline_to_json(&nodes)))
}

fn handle_get_pulse(db: &Database, args: &Value) -> HandlerResult {
    let branch = get_str(args, "branch");

    let graph = db.get_graph()?;
    let report = query::get_pulse(&graph, branch, 10);

    Ok(tool_result_json(&query::pulse_report_to_json(&report)))
}

fn handle_find_orphans(db: &Database) -> HandlerResult {
    let graph = db.get_graph()?;
    let orphans = query::find_orphans(&graph);

    Ok(tool_result_json(&query::orphans_to_json(&orphans)))
}

fn handle_get_branch_summary(db: &Database, args: &Value) -> HandlerResult {
    let branch = require_str(args, "branch")?;

    let graph = db.get_graph()?;
    let summary = query::get_branch_summary(&graph, branch);

    Ok(tool_result_json(&query::branch_summary_to_json(&summary)))
}

// ---------------------------------------------------------------------------
// Export handlers
// ---------------------------------------------------------------------------

fn handle_export_dot(db: &Database, args: &Value) -> HandlerResult {
    let graph = db.get_graph()?;
    let title = get_str(args, "title");
    let rankdir = get_str(args, "rankdir").unwrap_or("TB");
    let roots_str = get_str(args, "roots");
    let nodes_str = get_str(args, "nodes");

    // Filter graph if roots or nodes specified
    let filtered_graph = if let Some(nodes_spec) = nodes_str {
        let node_ids = crate::export::parse_node_range(nodes_spec);
        crate::export::filter_graph_by_ids(&graph, &node_ids)
    } else if let Some(roots_spec) = roots_str {
        let root_ids: Vec<i32> = roots_spec
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        crate::export::filter_graph_from_roots(&graph, &root_ids)
    } else {
        graph
    };

    let config = crate::export::DotConfig {
        title: title.map(|s| s.to_string()),
        rankdir: rankdir.to_string(),
        show_rationale: true,
        show_confidence: true,
        show_ids: true,
    };

    let dot = crate::export::graph_to_dot(&filtered_graph, &config);

    Ok(tool_result_text(dot))
}

fn handle_generate_writeup(db: &Database, args: &Value) -> HandlerResult {
    let graph = db.get_graph()?;
    let title = get_str(args, "title");
    let roots_str = get_str(args, "roots");
    let nodes_str = get_str(args, "nodes");
    let no_dot = get_bool(args, "no_dot").unwrap_or(false);
    let no_test_plan = get_bool(args, "no_test_plan").unwrap_or(false);

    // Filter graph if specified
    let filtered_graph = if let Some(nodes_spec) = nodes_str {
        let node_ids = crate::export::parse_node_range(nodes_spec);
        crate::export::filter_graph_by_ids(&graph, &node_ids)
    } else if let Some(roots_spec) = roots_str {
        let root_ids: Vec<i32> = roots_spec
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        crate::export::filter_graph_from_roots(&graph, &root_ids)
    } else {
        graph
    };

    let config = crate::export::WriteupConfig {
        title: title
            .map(|s| s.to_string())
            .unwrap_or_else(|| "Decision Graph Writeup".to_string()),
        root_ids: vec![],
        include_dot: !no_dot,
        include_test_plan: !no_test_plan,
        png_filename: None,
        github_repo: None,
        git_branch: None,
    };

    let writeup = crate::export::generate_pr_writeup(&filtered_graph, &config);

    Ok(tool_result_text(writeup))
}

fn handle_events_status() -> HandlerResult {
    // Events status is file-system based, provide a summary
    let sync_dir = std::path::Path::new(".deciduous/sync");
    if !sync_dir.exists() {
        return Ok(tool_result_json(&json!({
            "initialized": false,
            "message": "Event sync not initialized. Run 'deciduous events init' to set up."
        })));
    }

    Ok(tool_result_json(&json!({
        "initialized": true,
        "sync_dir": sync_dir.display().to_string(),
        "message": "Event sync is initialized. Use 'deciduous events status' CLI for full details."
    })))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // Helper: create a temp DB for testing
    fn test_db() -> Database {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        // Keep dir alive by leaking it (test-only)
        let db = Database::new(path.to_str().unwrap()).unwrap();
        std::mem::forget(dir);
        db
    }

    #[test]
    fn test_node_to_json_basic() {
        let node = crate::db::DecisionNode {
            id: 1,
            change_id: "uuid-123".to_string(),
            node_type: "goal".to_string(),
            title: "Test goal".to_string(),
            description: Some("A test".to_string()),
            status: "pending".to_string(),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
            metadata_json: Some(r#"{"confidence":90,"branch":"main"}"#.to_string()),
        };

        let j = node_to_json(&node);
        assert_eq!(j["id"], 1);
        assert_eq!(j["title"], "Test goal");
        assert_eq!(j["description"], "A test");
        // Metadata unpacked to top level
        assert_eq!(j["confidence"], 90);
        assert_eq!(j["branch"], "main");
    }

    #[test]
    fn test_node_to_json_no_metadata() {
        let node = crate::db::DecisionNode {
            id: 2,
            change_id: "uuid-456".to_string(),
            node_type: "action".to_string(),
            title: "Do thing".to_string(),
            description: None,
            status: "active".to_string(),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
            metadata_json: None,
        };

        let j = node_to_json(&node);
        assert_eq!(j["id"], 2);
        assert!(j.get("description").is_none());
        assert!(j.get("confidence").is_none());
    }

    #[test]
    fn test_edge_to_json() {
        let edge = crate::db::DecisionEdge {
            id: 10,
            from_node_id: 1,
            to_node_id: 2,
            from_change_id: Some("a".to_string()),
            to_change_id: Some("b".to_string()),
            edge_type: "leads_to".to_string(),
            weight: Some(1.0),
            rationale: Some("because".to_string()),
            created_at: "2024-01-01T00:00:00Z".to_string(),
        };

        let j = edge_to_json(&edge);
        assert_eq!(j["from_node_id"], 1);
        assert_eq!(j["to_node_id"], 2);
        assert_eq!(j["rationale"], "because");
    }

    #[test]
    fn test_get_str_helpers() {
        let args = json!({"name": "hello", "count": 42, "flag": true});
        assert_eq!(get_str(&args, "name"), Some("hello"));
        assert_eq!(get_str(&args, "missing"), None);
        assert_eq!(get_i32(&args, "count"), Some(42));
        assert_eq!(get_bool(&args, "flag"), Some(true));
    }

    #[test]
    fn test_require_str_present() {
        let args = json!({"title": "foo"});
        assert_eq!(require_str(&args, "title").unwrap(), "foo");
    }

    #[test]
    fn test_require_str_missing() {
        let args = json!({});
        let err = require_str(&args, "title").unwrap_err();
        assert!(err.message.contains("title"));
    }

    #[test]
    fn test_dispatch_add_node() {
        let db = test_db();
        let args = json!({"node_type": "goal", "title": "My goal", "confidence": 85});
        let result = dispatch(&db, "add_node", args);

        assert!(result.is_error.is_none());
        assert!(result.content[0].text.contains("node_id"));
        assert!(result.content[0].text.contains("My goal"));
    }

    #[test]
    fn test_dispatch_link_nodes() {
        let db = test_db();

        // Create two nodes first
        let n1 = db.create_node("goal", "Goal 1", None, None, None).unwrap();
        let n2 = db
            .create_node("action", "Action 1", None, None, None)
            .unwrap();

        let args = json!({"from_id": n1, "to_id": n2, "rationale": "test link"});
        let result = dispatch(&db, "link_nodes", args);

        assert!(result.is_error.is_none());
        assert!(result.content[0].text.contains("edge_id"));
    }

    #[test]
    fn test_dispatch_link_nonexistent_node() {
        let db = test_db();
        let args = json!({"from_id": 999, "to_id": 998});
        let result = dispatch(&db, "link_nodes", args);

        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("do not exist"));
    }

    #[test]
    fn test_dispatch_list_nodes_empty() {
        let db = test_db();
        let result = dispatch(&db, "list_nodes", json!({}));

        assert!(result.is_error.is_none());
        assert!(result.content[0].text.contains("\"count\": 0"));
    }

    #[test]
    fn test_dispatch_list_nodes_with_data() {
        let db = test_db();
        db.create_node("goal", "Goal A", None, None, None).unwrap();
        db.create_node("action", "Action B", None, None, None)
            .unwrap();

        let result = dispatch(&db, "list_nodes", json!({}));
        assert!(result.content[0].text.contains("\"count\": 2"));
    }

    #[test]
    fn test_dispatch_list_nodes_type_filter() {
        let db = test_db();
        db.create_node("goal", "Goal A", None, None, None).unwrap();
        db.create_node("action", "Action B", None, None, None)
            .unwrap();

        let result = dispatch(&db, "list_nodes", json!({"node_type": "goal"}));
        assert!(result.content[0].text.contains("\"count\": 1"));
        assert!(result.content[0].text.contains("Goal A"));
    }

    #[test]
    fn test_dispatch_show_node() {
        let db = test_db();
        let id = db
            .create_node("goal", "Test Goal", Some("Details here"), Some(90), None)
            .unwrap();

        let result = dispatch(&db, "show_node", json!({"node_id": id}));
        assert!(result.is_error.is_none());
        assert!(result.content[0].text.contains("Test Goal"));
        assert!(result.content[0].text.contains("children"));
        assert!(result.content[0].text.contains("parents"));
    }

    #[test]
    fn test_dispatch_show_node_not_found() {
        let db = test_db();
        let result = dispatch(&db, "show_node", json!({"node_id": 999}));
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("not found"));
    }

    #[test]
    fn test_dispatch_update_status() {
        let db = test_db();
        let id = db.create_node("goal", "Goal", None, None, None).unwrap();

        let result = dispatch(
            &db,
            "update_status",
            json!({"node_id": id, "status": "completed"}),
        );
        assert!(result.is_error.is_none());

        let node = db.get_node(id).unwrap().unwrap();
        assert_eq!(node.status, "completed");
    }

    #[test]
    fn test_dispatch_delete_node_dry_run() {
        let db = test_db();
        let id = db
            .create_node("goal", "To delete", None, None, None)
            .unwrap();

        let result = dispatch(&db, "delete_node", json!({"node_id": id, "dry_run": true}));
        assert!(result.is_error.is_none());
        assert!(result.content[0].text.contains("Would delete"));

        // Node should still exist
        assert!(db.get_node(id).unwrap().is_some());
    }

    #[test]
    fn test_dispatch_delete_node_actual() {
        let db = test_db();
        let id = db
            .create_node("goal", "To delete", None, None, None)
            .unwrap();

        let result = dispatch(&db, "delete_node", json!({"node_id": id}));
        assert!(result.is_error.is_none());
        assert!(result.content[0].text.contains("Deleted"));

        // Node should be gone
        assert!(db.get_node(id).unwrap().is_none());
    }

    #[test]
    fn test_dispatch_search_nodes() {
        let db = test_db();
        db.create_node("goal", "Authentication feature", None, None, None)
            .unwrap();
        db.create_node("action", "Implement JWT tokens", None, None, None)
            .unwrap();
        db.create_node("goal", "UI redesign", None, None, None)
            .unwrap();

        let result = dispatch(&db, "search_nodes", json!({"query": "auth"}));
        assert!(result.content[0].text.contains("\"count\": 1"));
        assert!(result.content[0].text.contains("Authentication"));
    }

    #[test]
    fn test_dispatch_unknown_tool() {
        let db = test_db();
        let result = dispatch(&db, "nonexistent", json!({}));
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("Unknown tool"));
    }

    #[test]
    fn test_dispatch_get_graph_empty() {
        let db = test_db();
        let result = dispatch(&db, "get_graph", json!({}));
        assert!(result.is_error.is_none());
        assert!(result.content[0].text.contains("\"node_count\": 0"));
    }

    #[test]
    fn test_dispatch_list_edges() {
        let db = test_db();
        let n1 = db.create_node("goal", "G", None, None, None).unwrap();
        let n2 = db.create_node("action", "A", None, None, None).unwrap();
        db.create_edge(n1, n2, "leads_to", Some("test")).unwrap();

        let result = dispatch(&db, "list_edges", json!({}));
        assert!(result.content[0].text.contains("\"count\": 1"));
    }

    #[test]
    fn test_dispatch_unlink_nodes() {
        let db = test_db();
        let n1 = db.create_node("goal", "G", None, None, None).unwrap();
        let n2 = db.create_node("action", "A", None, None, None).unwrap();
        db.create_edge(n1, n2, "leads_to", None).unwrap();

        let result = dispatch(&db, "unlink_nodes", json!({"from_id": n1, "to_id": n2}));
        assert!(result.is_error.is_none());

        // Edge should be gone
        let edges = db.get_all_edges().unwrap();
        assert!(edges.is_empty());
    }

    #[test]
    fn test_dispatch_create_and_list_themes() {
        let db = test_db();

        let result = dispatch(
            &db,
            "create_theme",
            json!({"name": "auth", "color": "#ff0000"}),
        );
        assert!(result.is_error.is_none());

        let result = dispatch(&db, "list_themes", json!({}));
        assert!(result.content[0].text.contains("auth"));
        assert!(result.content[0].text.contains("\"count\": 1"));
    }

    #[test]
    fn test_dispatch_tag_and_untag() {
        let db = test_db();
        let id = db.create_node("goal", "G", None, None, None).unwrap();
        db.create_theme("perf", "#00ff00", None).unwrap();

        let result = dispatch(&db, "tag_node", json!({"node_id": id, "theme": "perf"}));
        assert!(result.is_error.is_none());

        let result = dispatch(&db, "untag_node", json!({"node_id": id, "theme": "perf"}));
        assert!(result.is_error.is_none());
        assert!(result.content[0].text.contains("Removed"));
    }
}
