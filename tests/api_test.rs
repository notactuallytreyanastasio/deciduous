//! Integration tests for the multi-graph HTTP API daemon (`serve --api`).
//!
//! Uses a raw TcpStream HTTP/1.0 client so the test suite gains no new
//! dependencies; the server binds port 0 and each test talks to the real
//! socket.

use std::io::{Read, Write};
use std::net::TcpStream;

use deciduous::api::{ApiConfig, ApiServer};
use serde_json::{json, Value};

const TOKEN: &str = "test-token-123";

fn start_server() -> (u16, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let server = ApiServer::bind(ApiConfig {
        bind: "127.0.0.1".to_string(),
        port: 0,
        data_dir: dir.path().to_path_buf(),
        token: TOKEN.to_string(),
    })
    .expect("bind api server");
    let port = server.port();
    std::thread::spawn(move || server.run());
    (port, dir)
}

fn request(
    port: u16,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: Option<&Value>,
) -> (u16, Value) {
    let payload = body.map(|b| b.to_string()).unwrap_or_default();
    let auth = token
        .map(|t| format!("Authorization: Bearer {t}\r\n"))
        .unwrap_or_default();
    let raw = format!(
        "{method} {path} HTTP/1.0\r\nHost: localhost\r\n{auth}Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{payload}",
        payload.len()
    );

    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream.write_all(raw.as_bytes()).expect("write");
    let mut response = String::new();
    stream.read_to_string(&mut response).expect("read");

    let status: u16 = response
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .expect("status line");
    let body_start = response.find("\r\n\r\n").expect("header terminator") + 4;
    let body: Value = serde_json::from_str(&response[body_start..]).expect("json body");
    (status, body)
}

fn tool(port: u16, graph: &str, name: &str, args: Value) -> (u16, Value) {
    request(
        port,
        "POST",
        &format!("/api/v1/graphs/{graph}/tools/{name}"),
        Some(TOKEN),
        Some(&args),
    )
}

#[test]
fn rejects_missing_or_wrong_token() {
    let (port, _dir) = start_server();

    let (status, body) = request(port, "GET", "/api/v1/graphs", None, None);
    assert_eq!(status, 401);
    assert_eq!(body["ok"], json!(false));

    let (status, _) = request(port, "GET", "/api/v1/graphs", Some("nope"), None);
    assert_eq!(status, 401);
}

#[test]
fn creates_graphs_and_lists_them() {
    let (port, _dir) = start_server();

    let (status, body) = request(port, "PUT", "/api/v1/graphs/bot-otis", Some(TOKEN), None);
    assert_eq!(status, 201, "{body}");
    assert_eq!(body["data"]["created"], json!(true));

    // idempotent re-create
    let (status, body) = request(port, "PUT", "/api/v1/graphs/bot-otis", Some(TOKEN), None);
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["data"]["created"], json!(false));

    let (_, body) = request(port, "GET", "/api/v1/graphs", Some(TOKEN), None);
    assert_eq!(body["data"]["graphs"], json!(["bot-otis"]));
}

#[test]
fn rejects_bad_graph_ids() {
    let (port, _dir) = start_server();
    for bad in ["..", "UPPER", "has space", "-leading", &"x".repeat(65)] {
        let (status, _) = request(
            port,
            "PUT",
            &format!("/api/v1/graphs/{}", bad.replace(' ', "%20")),
            Some(TOKEN),
            None,
        );
        assert_eq!(status, 400, "graph id {bad:?} should be rejected");
    }
}

#[test]
fn dispatches_tools_and_isolates_graphs() {
    let (port, _dir) = start_server();
    request(port, "PUT", "/api/v1/graphs/bot-a", Some(TOKEN), None);
    request(port, "PUT", "/api/v1/graphs/bot-b", Some(TOKEN), None);

    let (status, body) = tool(
        port,
        "bot-a",
        "add_node",
        json!({
            "node_type": "observation",
            "title": "met bobdawg, talked raccoons",
            "branch": "room-default"
        }),
    );
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["data"]["is_error"], json!(false));
    let node_id = body["data"]["result"]["node_id"].as_i64().expect("node id");
    assert!(node_id > 0);

    // visible in bot-a
    let (_, body) = tool(port, "bot-a", "list_nodes", json!({}));
    let listing = body["data"]["result"].to_string();
    assert!(listing.contains("met bobdawg"), "{listing}");

    // invisible in bot-b — tenancy is real
    let (_, body) = tool(port, "bot-b", "list_nodes", json!({}));
    let listing = body["data"]["result"].to_string();
    assert!(!listing.contains("met bobdawg"), "{listing}");

    // tools on a nonexistent graph 404 rather than auto-create
    let (status, _) = tool(port, "ghost", "list_nodes", json!({}));
    assert_eq!(status, 404);
}

#[test]
fn linked_nodes_round_trip() {
    let (port, _dir) = start_server();
    request(port, "PUT", "/api/v1/graphs/mem", Some(TOKEN), None);

    let (_, goal) = tool(
        port,
        "mem",
        "add_node",
        json!({"node_type": "goal", "title": "remember friends"}),
    );
    let (_, obs) = tool(
        port,
        "mem",
        "add_node",
        json!({"node_type": "observation", "title": "bobdawg likes coyotes"}),
    );
    let goal_id = goal["data"]["result"]["node_id"].as_i64().unwrap();
    let obs_id = obs["data"]["result"]["node_id"].as_i64().unwrap();

    let (status, body) = tool(
        port,
        "mem",
        "link_nodes",
        json!({"from_id": goal_id, "to_id": obs_id, "rationale": "episodic memory"}),
    );
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["data"]["is_error"], json!(false));
}

#[test]
fn sql_query_selects_but_never_writes() {
    let (port, _dir) = start_server();
    request(port, "PUT", "/api/v1/graphs/q", Some(TOKEN), None);
    tool(
        port,
        "q",
        "add_node",
        json!({"node_type": "observation", "title": "the moon was in last quarter"}),
    );

    // SELECT works and returns typed rows
    let (status, body) = request(
        port,
        "POST",
        "/api/v1/graphs/q/query",
        Some(TOKEN),
        Some(&json!({"sql": "SELECT title, node_type FROM decision_nodes ORDER BY id"})),
    );
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["data"]["columns"], json!(["title", "node_type"]));
    assert_eq!(
        body["data"]["rows"][0][0],
        json!("the moon was in last quarter")
    );

    // writes are refused
    for evil in [
        "INSERT INTO decision_nodes (node_type, title) VALUES ('goal', 'pwned')",
        "UPDATE decision_nodes SET title = 'pwned'",
        "DELETE FROM decision_nodes",
        "DROP TABLE decision_nodes",
        "CREATE TABLE pwned (id int)",
    ] {
        let (status, body) = request(
            port,
            "POST",
            "/api/v1/graphs/q/query",
            Some(TOKEN),
            Some(&json!({"sql": evil})),
        );
        assert!(
            status == 403 || status == 400,
            "write statement must be refused, got {status}: {body}"
        );
    }

    // nothing was written
    let (_, body) = request(
        port,
        "POST",
        "/api/v1/graphs/q/query",
        Some(TOKEN),
        Some(&json!({"sql": "SELECT count(*) AS n FROM decision_nodes"})),
    );
    assert_eq!(body["data"]["rows"][0][0], json!(1));
}

#[test]
fn query_respects_row_limit() {
    let (port, _dir) = start_server();
    request(port, "PUT", "/api/v1/graphs/lim", Some(TOKEN), None);
    for i in 0..5 {
        tool(
            port,
            "lim",
            "add_node",
            json!({"node_type": "observation", "title": format!("obs {i}")}),
        );
    }

    let (_, body) = request(
        port,
        "POST",
        "/api/v1/graphs/lim/query",
        Some(TOKEN),
        Some(&json!({"sql": "SELECT id FROM decision_nodes", "limit": 2})),
    );
    assert_eq!(body["data"]["row_count"], json!(2));
    assert_eq!(body["data"]["truncated"], json!(true));
}
