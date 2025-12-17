//! HTTP server for decision graph viewer
//!
//! `deciduous serve` → starts server, opens browser, shows graph

use crate::db::{Database, DecisionGraph, RoadmapItem};
use serde::Serialize;
use tiny_http::{Header, Method, Request, Response, Server};

#[derive(Serialize)]
struct ApiResponse<T> {
    ok: bool,
    data: Option<T>,
    error: Option<String>,
}

impl<T: Serialize> ApiResponse<T> {
    fn success(data: T) -> Self {
        Self {
            ok: true,
            data: Some(data),
            error: None,
        }
    }
}

// Embedded React graph viewer (built with bun from web/ directory)
// To rebuild: cd web && ./build-embed.sh
const GRAPH_VIEWER_HTML: &str = include_str!("viewer.html");

/// Start the decision graph viewer server
pub fn start_graph_server(port: u16) -> std::io::Result<()> {
    let addr = format!("127.0.0.1:{}", port);
    let server = Server::http(&addr)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;

    let url = format!("http://localhost:{}", port);

    eprintln!("\n\x1b[1;32m🌳 Deciduous\x1b[0m");
    eprintln!("   Graph viewer: {}", url);
    eprintln!("   Press Ctrl+C to stop\n");

    // Handle requests
    for request in server.incoming_requests() {
        if let Err(e) = handle_request(request) {
            eprintln!("Error: {}", e);
        }
    }

    Ok(())
}

fn handle_request(request: Request) -> std::io::Result<()> {
    let url = request.url().to_string();
    let path = url.split('?').next().unwrap_or("/");
    let method = request.method().clone();

    match (&method, path) {
        // Serve graph viewer UI
        (&Method::Get, "/") | (&Method::Get, "/graph") => {
            let response = Response::from_string(GRAPH_VIEWER_HTML)
                .with_header(Header::from_bytes(&b"Content-Type"[..], &b"text/html"[..]).unwrap());
            request.respond(response)
        }

        // API: Get decision graph
        (&Method::Get, "/api/graph") => {
            let graph = get_decision_graph();
            let json = serde_json::to_string(&ApiResponse::success(graph))?;

            let response = Response::from_string(json).with_header(
                Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
            );
            request.respond(response)
        }

        // API: Get command log
        (&Method::Get, "/api/commands") => {
            let commands = get_command_log();
            let json = serde_json::to_string(&ApiResponse::success(commands))?;

            let response = Response::from_string(json).with_header(
                Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
            );
            request.respond(response)
        }

        // API: Get roadmap items
        (&Method::Get, "/api/roadmap") => {
            let items = get_roadmap_items();
            let json = serde_json::to_string(&ApiResponse::success(items))?;

            let response = Response::from_string(json).with_header(
                Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
            );
            request.respond(response)
        }

        // API: Toggle roadmap item checkbox (POST /api/roadmap/checkbox)
        (&Method::Post, "/api/roadmap/checkbox") => handle_toggle_checkbox(request),

        // API: Get traces linked to a node
        (&Method::Get, p) if p.starts_with("/api/nodes/") && p.ends_with("/traces") => {
            // Parse /api/nodes/{node_id}/traces
            let path_without_traces = p.strip_suffix("/traces").unwrap_or("");
            let node_id_str = path_without_traces.strip_prefix("/api/nodes/").unwrap_or("");
            if let Ok(node_id) = node_id_str.parse::<i32>() {
                let trace_info = get_node_trace_info(node_id);
                let json = serde_json::to_string(&ApiResponse::success(trace_info))?;

                let response = Response::from_string(json).with_header(
                    Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
                );
                return request.respond(response);
            }
            let response = Response::from_string("Invalid node ID").with_status_code(400);
            request.respond(response)
        }

        // API: Get trace sessions
        (&Method::Get, "/api/traces") => {
            let sessions = get_trace_sessions();
            let json = serde_json::to_string(&ApiResponse::success(sessions))?;

            let response = Response::from_string(json).with_header(
                Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
            );
            request.respond(response)
        }

        // API: Get trace spans for a session
        (&Method::Get, p) if p.starts_with("/api/traces/") && !p.contains("/spans/") => {
            let session_id = p.strip_prefix("/api/traces/").unwrap_or("");
            let spans = get_trace_spans(session_id);
            let json = serde_json::to_string(&ApiResponse::success(spans))?;

            let response = Response::from_string(json).with_header(
                Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
            );
            request.respond(response)
        }

        // API: Get nodes for a span
        (&Method::Get, p) if p.starts_with("/api/traces/spans/") && p.ends_with("/nodes") => {
            // Parse /api/traces/spans/{span_id}/nodes
            let path_without_nodes = p.strip_suffix("/nodes").unwrap_or("");
            let span_id_str = path_without_nodes.strip_prefix("/api/traces/spans/").unwrap_or("");
            if let Ok(span_id) = span_id_str.parse::<i32>() {
                let nodes = get_span_nodes(span_id);
                let json = serde_json::to_string(&ApiResponse::success(nodes))?;

                let response = Response::from_string(json).with_header(
                    Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
                );
                return request.respond(response);
            }
            let response = Response::from_string("Invalid span ID").with_status_code(400);
            request.respond(response)
        }

        // API: Get trace content for a span
        (&Method::Get, p) if p.starts_with("/api/traces/") && p.contains("/spans/") => {
            // Parse /api/traces/{session_id}/spans/{span_id}
            // URL split: ["", "api", "traces", "session_id", "spans", "span_id"]
            // Index:       0     1       2          3           4        5
            let parts: Vec<&str> = p.split('/').collect();
            if parts.len() >= 6 {
                if let Ok(span_id) = parts[5].parse::<i32>() {
                    let content = get_trace_content(span_id);
                    let json = serde_json::to_string(&ApiResponse::success(content))?;

                    let response = Response::from_string(json).with_header(
                        Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
                    );
                    return request.respond(response);
                }
            }
            let response = Response::from_string("Invalid span ID").with_status_code(400);
            request.respond(response)
        }

        // 404
        _ => {
            let response = Response::from_string("Not found").with_status_code(404);
            request.respond(response)
        }
    }
}

fn get_decision_graph() -> DecisionGraph {
    // Load config for external repo support
    let config = crate::config::Config::load();
    let include_config = config.github.commit_repo.is_some();
    let config_opt = if include_config { Some(config) } else { None };

    match Database::open() {
        Ok(db) => db
            .get_graph_with_config(config_opt.clone())
            .unwrap_or_else(|_| DecisionGraph {
                nodes: vec![],
                edges: vec![],
                config: config_opt.clone(),
            }),
        Err(_) => DecisionGraph {
            nodes: vec![],
            edges: vec![],
            config: config_opt,
        },
    }
}

fn get_command_log() -> Vec<crate::db::CommandLog> {
    match Database::open() {
        Ok(db) => db.get_recent_commands(100).unwrap_or_default(),
        Err(_) => vec![],
    }
}

fn get_roadmap_items() -> Vec<RoadmapItem> {
    match Database::open() {
        Ok(db) => db.get_all_roadmap_items().unwrap_or_default(),
        Err(_) => vec![],
    }
}

/// Session with display name for API response
#[derive(serde::Serialize)]
struct SessionWithSummary {
    #[serde(flatten)]
    session: crate::db::TraceSession,
    /// Display name: linked node title, or first user prompt, or session ID
    display_name: Option<String>,
    /// Linked node title if session is linked
    linked_node_title: Option<String>,
}

fn get_trace_sessions() -> Vec<SessionWithSummary> {
    match Database::open() {
        Ok(db) => {
            let sessions = db.get_trace_sessions(100).unwrap_or_default();
            if sessions.is_empty() {
                return vec![];
            }

            // Collect session IDs for batch query
            let session_ids: Vec<String> = sessions.iter().map(|s| s.session_id.clone()).collect();

            // Get first prompts for all sessions
            let first_prompts = db.get_session_first_prompts(&session_ids).unwrap_or_default();

            // Get linked node titles
            let linked_node_ids: Vec<i32> = sessions
                .iter()
                .filter_map(|s| s.linked_node_id)
                .collect();

            let mut node_titles: std::collections::HashMap<i32, String> = std::collections::HashMap::new();
            for node_id in linked_node_ids {
                if let Ok(Some(node)) = db.get_node_by_id(node_id) {
                    node_titles.insert(node_id, node.title);
                }
            }

            // Build enriched sessions
            sessions
                .into_iter()
                .map(|session| {
                    let linked_node_title = session
                        .linked_node_id
                        .and_then(|id| node_titles.get(&id).cloned());

                    let display_name = linked_node_title
                        .clone()
                        .or_else(|| first_prompts.get(&session.session_id).cloned());

                    SessionWithSummary {
                        session,
                        display_name,
                        linked_node_title,
                    }
                })
                .collect()
        }
        Err(_) => vec![],
    }
}

/// Span with node count for API response
#[derive(serde::Serialize)]
struct SpanWithNodeCount {
    #[serde(flatten)]
    span: crate::db::TraceSpan,
    node_count: i64,
}

fn get_trace_spans(session_id: &str) -> Vec<SpanWithNodeCount> {
    match Database::open() {
        Ok(db) => {
            let spans = db.get_trace_spans(session_id).unwrap_or_default();
            let span_ids: Vec<i32> = spans.iter().map(|s| s.id).collect();
            let node_counts = db.get_node_counts_for_spans(&span_ids).unwrap_or_default();

            spans
                .into_iter()
                .map(|span| {
                    let count = node_counts.get(&span.id).copied().unwrap_or(0);
                    SpanWithNodeCount {
                        span,
                        node_count: count,
                    }
                })
                .collect()
        }
        Err(_) => vec![],
    }
}

fn get_trace_content(span_id: i32) -> Vec<crate::db::TraceContent> {
    match Database::open() {
        Ok(db) => db.get_trace_content(span_id).unwrap_or_default(),
        Err(_) => vec![],
    }
}

fn get_span_nodes(span_id: i32) -> Vec<crate::db::DecisionNode> {
    match Database::open() {
        Ok(db) => db.get_nodes_for_span(span_id).unwrap_or_default(),
        Err(_) => vec![],
    }
}

/// Trace info for a node - includes span and session details with content previews
#[derive(serde::Serialize)]
struct NodeTraceInfo {
    spans: Vec<SpanWithSession>,
}

#[derive(serde::Serialize)]
struct SpanWithSession {
    span_id: i32,
    sequence_num: i32,
    session_id: String,
    model: Option<String>,
    duration_ms: Option<i32>,
    started_at: String,
    // Content previews for inline display
    thinking_preview: Option<String>,
    response_preview: Option<String>,
    tool_names: Option<String>,
    user_preview: Option<String>,
}

fn get_node_trace_info(node_id: i32) -> NodeTraceInfo {
    match Database::open() {
        Ok(db) => {
            let spans = db.get_spans_for_node(node_id).unwrap_or_default();
            let spans_with_session: Vec<SpanWithSession> = spans
                .into_iter()
                .map(|s| SpanWithSession {
                    span_id: s.id,
                    sequence_num: s.sequence_num,
                    session_id: s.session_id,
                    model: s.model,
                    duration_ms: s.duration_ms,
                    started_at: s.started_at,
                    thinking_preview: s.thinking_preview,
                    response_preview: s.response_preview,
                    tool_names: s.tool_names,
                    user_preview: s.user_preview,
                })
                .collect();
            NodeTraceInfo { spans: spans_with_session }
        }
        Err(_) => NodeTraceInfo { spans: vec![] },
    }
}

#[derive(serde::Deserialize)]
struct ToggleCheckboxRequest {
    item_id: i32,
    checkbox_state: String,
}

fn handle_toggle_checkbox(mut request: Request) -> std::io::Result<()> {
    // Read request body
    let mut body = String::new();
    if let Err(e) = request.as_reader().read_to_string(&mut body) {
        let json = serde_json::to_string(&ApiResponse::<()> {
            ok: false,
            data: None,
            error: Some(format!("Failed to read body: {}", e)),
        })?;
        let response = Response::from_string(json)
            .with_status_code(400)
            .with_header(
                Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
            );
        return request.respond(response);
    }

    // Parse JSON body
    let req: ToggleCheckboxRequest = match serde_json::from_str(&body) {
        Ok(r) => r,
        Err(e) => {
            let json = serde_json::to_string(&ApiResponse::<()> {
                ok: false,
                data: None,
                error: Some(format!("Invalid JSON: {}", e)),
            })?;
            let response = Response::from_string(json)
                .with_status_code(400)
                .with_header(
                    Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
                );
            return request.respond(response);
        }
    };

    // Update database
    let result = match Database::open() {
        Ok(db) => db.update_roadmap_item_checkbox(req.item_id, &req.checkbox_state),
        Err(e) => Err(e),
    };

    let (json, status) = match result {
        Ok(()) => (serde_json::to_string(&ApiResponse::success(true))?, 200),
        Err(e) => (
            serde_json::to_string(&ApiResponse::<bool> {
                ok: false,
                data: None,
                error: Some(format!("Database error: {}", e)),
            })?,
            500,
        ),
    };

    let response = Response::from_string(json)
        .with_status_code(status)
        .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap());
    request.respond(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    // === ApiResponse Tests ===

    #[test]
    fn test_api_response_success() {
        let response: ApiResponse<String> = ApiResponse::success("hello".to_string());
        assert!(response.ok);
        assert_eq!(response.data, Some("hello".to_string()));
        assert!(response.error.is_none());
    }

    #[test]
    fn test_api_response_success_with_vec() {
        let data = vec![1, 2, 3];
        let response: ApiResponse<Vec<i32>> = ApiResponse::success(data.clone());
        assert!(response.ok);
        assert_eq!(response.data, Some(data));
    }

    #[test]
    fn test_api_response_serializes_to_json() {
        let response: ApiResponse<String> = ApiResponse::success("test".to_string());
        let json = serde_json::to_string(&response).unwrap();

        assert!(json.contains("\"ok\":true"));
        assert!(json.contains("\"data\":\"test\""));
        assert!(json.contains("\"error\":null"));
    }

    #[test]
    fn test_api_response_with_complex_data() {
        #[derive(Serialize, PartialEq, Debug)]
        struct TestData {
            name: String,
            count: u32,
        }

        let data = TestData {
            name: "test".to_string(),
            count: 42,
        };
        let response = ApiResponse::success(data);

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"name\":\"test\""));
        assert!(json.contains("\"count\":42"));
    }

    // === Graph Viewer HTML Tests ===

    #[test]
    fn test_viewer_html_is_valid() {
        // The embedded viewer should be valid HTML
        assert!(
            GRAPH_VIEWER_HTML.contains("<!DOCTYPE html>") || GRAPH_VIEWER_HTML.contains("<html")
        );
        assert!(GRAPH_VIEWER_HTML.contains("</html>"));
    }

    #[test]
    fn test_viewer_html_has_react() {
        // The embedded viewer should have React components
        assert!(
            GRAPH_VIEWER_HTML.contains("React") || GRAPH_VIEWER_HTML.contains("react"),
            "Viewer should include React"
        );
    }
}
