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

        // 404
        _ => {
            let response = Response::from_string("Not found").with_status_code(404);
            request.respond(response)
        }
    }
}

fn get_decision_graph() -> DecisionGraph {
    match Database::open() {
        Ok(db) => db.get_graph().unwrap_or_else(|_| DecisionGraph {
            nodes: vec![],
            edges: vec![],
        }),
        Err(_) => DecisionGraph {
            nodes: vec![],
            edges: vec![],
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
