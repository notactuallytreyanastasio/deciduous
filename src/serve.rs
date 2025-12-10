//! HTTP server for decision graph viewer
//!
//! `deciduous serve` → starts server, opens browser, shows graph

use crate::db::{Database, DecisionGraph};
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
