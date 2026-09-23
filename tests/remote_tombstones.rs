//! The CLI against a server that has deleted a node this machine still has.
//!
//! Found with the real binary against a real server (c4.sh): before a pull,
//! `remote status` said "Drift: local holds more than the server. `deciduous
//! remote push` to send it up.", and `remote push` printed "edges 1 of 1"
//! every time and changed nothing. And `deciduous status 2 completed` on the
//! deleted node auto-pushed without a word; the next pull deleted the node
//! and the edit with it.
//!
//! The server here is a small HTTP stub on a real socket that answers
//! /health, /export and /import in the shape the Elixir server does after
//! this chapter: tombstones in /export carrying change_id and deleted_at
//! only, and /import refusing a deleted change_id with `refused_deleted`.
//! The CLI is the real binary. The server half is tested against Postgres in
//! deciduous_mcp/test/deciduous_mcp/web/import_tombstones_test.exs.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Output};
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef";
const DELETED_AT: &str = "2026-09-23T17:48:53.371218Z";

struct Stub {
    url: String,
    export: Arc<Mutex<Value>>,
    imports: Arc<Mutex<Vec<Value>>>,
}

fn stub() -> Stub {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let export = Arc::new(Mutex::new(json!({"nodes": [], "edges": [], "documents": []})));
    let imports = Arc::new(Mutex::new(Vec::new()));
    let (ex, im) = (export.clone(), imports.clone());

    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let mut len = 0usize;
            loop {
                let mut h = String::new();
                reader.read_line(&mut h).unwrap();
                if h == "\r\n" || h.is_empty() {
                    break;
                }
                if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:") {
                    len = v.trim().parse().unwrap();
                }
            }
            let mut body = vec![0u8; len];
            reader.read_exact(&mut body).unwrap();

            let path = line.split_whitespace().nth(1).unwrap_or("").to_string();
            let reply = if path.starts_with("/health") {
                json!("ok")
            } else if path.starts_with("/export") {
                ex.lock().unwrap().clone()
            } else if path.starts_with("/import") {
                let payload: Value = serde_json::from_slice(&body).unwrap();
                let report = import_report(&ex.lock().unwrap(), &payload["graph"]);
                im.lock().unwrap().push(payload);
                report
            } else {
                json!({"error": "not found"})
            };
            let text = reply.to_string();
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                text.len(),
                text
            );
        }
    });

    Stub {
        url,
        export,
        imports,
    }
}

/// What the server's /import answers: a node or edge touching a tombstone
/// is refused and named, the rest counted as written.
fn import_report(server: &Value, graph: &Value) -> Value {
    let dead: Vec<&str> = server["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|n| !n["deleted_at"].is_null())
        .map(|n| n["change_id"].as_str().unwrap())
        .collect();
    let nodes = graph["nodes"].as_array().cloned().unwrap_or_default();
    let edges = graph["edges"].as_array().cloned().unwrap_or_default();
    let refused: Vec<Value> = nodes
        .iter()
        .filter(|n| dead.contains(&n["change_id"].as_str().unwrap_or("")))
        .map(|n| json!({"change_id": n["change_id"], "deleted_at": DELETED_AT}))
        .collect();
    let dead_edges = edges
        .iter()
        .filter(|e| {
            dead.contains(&e["from_change_id"].as_str().unwrap_or(""))
                || dead.contains(&e["to_change_id"].as_str().unwrap_or(""))
        })
        .count();
    json!({
        "nodes": {"received": nodes.len(), "upserted": nodes.len() - refused.len(),
                  "refused_deleted": refused.len(), "refused_deleted_examples": refused},
        "edges": {"received": edges.len(), "upserted": edges.len() - dead_edges,
                  "unresolved": 0, "refused_deleted": dead_edges},
        "documents": {"received": 0, "upserted": 0}
    })
}

fn run(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_deciduous"))
        .args(args)
        .current_dir(dir)
        .env("HOME", dir)
        .env("DECIDUOUS_MCP_TOKEN", TOKEN)
        .env("DECIDUOUS_NO_SERVER", "1")
        .env_remove("DECIDUOUS_DB_PATH")
        .output()
        .expect("run deciduous")
}

fn text(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

/// A project holding goal -> doomed, a server holding goal live and doomed
/// as a tombstone (and so no edge), and nothing pulled yet.
fn project_after_server_delete() -> (TempDir, Stub, String) {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();
    assert!(run(dir, &["init"]).status.success());
    run(dir, &["add", "goal", "root goal", "-c", "90"]);
    run(dir, &["add", "action", "doomed", "-c", "80", "-p", "my password is hunter2"]);
    run(dir, &["link", "1", "2", "-r", "r"]);

    let graph: Value = serde_json::from_slice(&run(dir, &["graph"]).stdout).unwrap();
    let cid = |title: &str| {
        graph["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["title"] == title)
            .unwrap()["change_id"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let (goal, doomed) = (cid("root goal"), cid("doomed"));

    let server = stub();
    *server.export.lock().unwrap() = json!({
        "nodes": [
            {"id": "u1", "change_id": goal, "node_type": "goal", "title": "root goal",
             "description": null, "status": "pending", "metadata": {},
             "created_at": "2026-09-23T17:00:00Z", "updated_at": "2026-09-23T17:00:00Z",
             "deleted_at": null},
            {"id": "u2", "change_id": doomed, "node_type": "action", "title": "",
             "description": null, "status": "pending", "metadata": null,
             "created_at": "2026-09-23T17:00:00Z", "updated_at": DELETED_AT,
             "deleted_at": DELETED_AT}
        ],
        "edges": [],
        "documents": []
    });
    let init = run(dir, &["remote", "init", &server.url]);
    assert!(init.status.success(), "{}", text(&init));
    (tmp, server, doomed)
}

#[test]
fn status_sends_the_user_to_pull_when_the_server_deleted_a_local_node() {
    let (tmp, _server, _) = project_after_server_delete();
    let out = text(&run(tmp.path(), &["remote", "status"]));

    assert!(
        !out.contains("remote push` to send it up"),
        "status advised a push that cannot help:\n{out}"
    );
    assert!(out.contains("deleted"), "{out}");
    assert!(out.contains("deciduous remote pull"), "{out}");
}

#[test]
fn push_does_not_resend_an_edge_into_a_server_deleted_node_and_says_to_pull() {
    let (tmp, server, doomed) = project_after_server_delete();
    let before = server.imports.lock().unwrap().len();
    let out = text(&run(tmp.path(), &["remote", "push"]));

    let sent: Vec<Value> = server.imports.lock().unwrap()[before..].to_vec();
    for payload in &sent {
        for e in payload["graph"]["edges"].as_array().unwrap() {
            assert!(
                e["to_change_id"] != doomed.as_str() && e["from_change_id"] != doomed.as_str(),
                "push re-sent an edge touching the deleted node: {e}"
            );
        }
    }
    assert!(!out.contains("edges 1 of 1"), "{out}");
    assert!(out.contains("deciduous remote pull"), "{out}");
}

#[test]
fn an_edit_to_a_node_the_server_deleted_is_not_sent_and_the_user_is_told() {
    let (tmp, server, doomed) = project_after_server_delete();
    let before = server.imports.lock().unwrap().len();
    let out = run(tmp.path(), &["status", "2", "completed"]);
    assert!(out.status.success(), "{}", text(&out));
    let out = text(&out);

    for payload in &server.imports.lock().unwrap()[before..] {
        assert!(
            !payload["graph"]["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .any(|n| n["change_id"] == doomed.as_str()),
            "the edit to the deleted node was pushed: {payload}"
        );
    }
    assert!(out.contains("deleted on the server"), "{out}");
    assert!(out.contains("doomed"), "{out}");
    assert!(out.contains("deciduous remote pull"), "{out}");
}

/// The server refuses writes to a deleted node, so a local edit made after
/// the delete can never reach it. reconcile's rule for teammates' records
/// ("edited locally after someone deleted it: resurrect") kept the node
/// live here, and every later `remote status` reported the same deletion
/// while `remote pull` did nothing about it. A server tombstone wins.
#[test]
fn pull_applies_a_server_delete_even_over_a_later_local_edit() {
    let (tmp, _server, _) = project_after_server_delete();
    let dir = tmp.path();
    run(dir, &["status", "2", "completed"]);

    let out = text(&run(dir, &["remote", "pull"]));
    assert!(out.contains("doomed"), "pull did not say what it deleted:\n{out}");

    let nodes = text(&run(dir, &["nodes"]));
    assert!(!nodes.contains("doomed"), "the deleted node survived the pull:\n{nodes}");

    let status = text(&run(dir, &["remote", "status"]));
    assert!(status.contains("counts match"), "{status}");
    assert!(!status.contains("deleted 1 node"), "{status}");

    // The delete is how a pasted secret leaves the graph; the local
    // tombstone keeps no more of the node than the server's does.
    let file = std::fs::read_to_string(dir.join(".deciduous/graph.json")).unwrap();
    assert!(!file.contains("hunter2"), "graph.json kept the deleted prompt");
}

#[test]
fn a_confidence_over_100_is_refused_by_the_cli() {
    let tmp = TempDir::new().unwrap();
    assert!(run(tmp.path(), &["init"]).status.success());
    let out = run(tmp.path(), &["add", "goal", "g", "-c", "150"]);
    assert!(!out.status.success(), "{}", text(&out));
    assert!(text(&out).contains("150"), "{}", text(&out));
}
