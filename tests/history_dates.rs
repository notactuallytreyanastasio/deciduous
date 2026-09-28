//! A node's created_at is when the decision was made. Every store deciduous
//! has had (the JSONL log and its checkpoint, the 0.17 record directory,
//! graph.json, the server) is a copy of that date, and every migration
//! between them was a chance to write the time of the migration instead.
//! A customer case study found about 38% of one graph's dates were import
//! times: 1,303 nodes dated within one minute, 1,113 within one hour.
//!
//! These drive the real binary through the whole chain: a legacy log and
//! checkpoint -> `sync` -> graph.json -> the database -> `remote push
//! --seed` to a stub server that records what it was sent. The server's
//! half (that /import and /ops store what arrives) is tested against
//! Postgres in deciduous_mcp/test/deciduous_mcp/web/history_dates_test.exs.

use deciduous::records::parse_ts;
use serde_json::{json, Value};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Output};
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef";

const A: &str = "aaaa1111-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
const C: &str = "cccc3333-cccc-4ccc-8ccc-cccccccccccc";
const N: &str = "dddd4444-dddd-4ddd-8ddd-dddddddddddd";

/// A's date as the old database wrote it: nanoseconds and an offset. It
/// must come out of every step byte for byte.
const A_MADE: &str = "2019-03-04T05:06:07.123456789-05:00";
/// What the checkpoint says of C: the moment of a compaction, not of C.
const C_RESTAMPED: &str = "2025-01-01T00:00:00+00:00";
/// When the log says C was added, before the checkpoint folded it in.
const C_ADDED_MS: i64 = 1_483_228_800_000; // 2017-01-01T00:00:00Z
const C_MADE: &str = "2017-01-01T00:00:00+00:00";
/// `add --date` stored this as typed before 1.0.
const N_MADE: &str = "2018-06-01 12:00:00";
const EDGE_MADE: &str = "2019-03-05T00:00:00-05:00";

const CHECKPOINT_MS: i64 = 1_735_689_600_000; // 2025-01-01
const REEMIT_MS: i64 = 1_767_225_600_000; // 2026-01-01

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

fn ok(dir: &Path, args: &[&str]) -> String {
    let out = run(dir, args);
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.status.success(), "{args:?} failed:\n{text}");
    text
}

fn doc(dir: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(dir.join(".deciduous/graph.json")).unwrap()).unwrap()
}

fn db_graph(dir: &Path) -> Value {
    serde_json::from_slice(&run(dir, &["graph"]).stdout).unwrap()
}

fn row<'a>(rows: &'a Value, cid: &str) -> &'a Value {
    rows.as_array()
        .unwrap()
        .iter()
        .find(|n| n["change_id"] == cid)
        .unwrap_or_else(|| panic!("no row {cid} in {rows}"))
}

/// A project on a pre-0.17 layout: a checkpoint and a log after it.
fn legacy_project() -> TempDir {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();
    ok(dir, &["init"]);
    let legacy = dir.join(".deciduous/sync");
    fs::create_dir_all(legacy.join("events")).unwrap();

    let node = |cid: &str, title: &str, made: &str| {
        json!({"change_id": cid, "node_type": "goal", "title": title, "description": null,
               "status": "pending", "metadata_json": null,
               "created_at": made, "updated_at": made})
    };
    let checkpoint = json!({
        "created_at": CHECKPOINT_MS,
        "version": "1.0",
        "nodes": [node(A, "A", A_MADE), node(C, "C", C_RESTAMPED), node(N, "N", N_MADE)],
        "edges": [{"edge_id": "e1", "from_change_id": A, "to_change_id": C,
                   "edge_type": "leads_to", "rationale": "r", "created_at": EDGE_MADE}]
    });
    fs::write(legacy.join("checkpoint.json"), checkpoint.to_string()).unwrap();

    let add = |cid: &str, title: &str, ms: i64| {
        json!({"op": "add_node", "change_id": cid, "node_type": "goal", "title": title,
               "description": null, "status": "pending", "metadata_json": null,
               "timestamp": ms, "author": "Alice"})
        .to_string()
    };
    let log = [
        // Folded into the checkpoint already, but the only record of when C
        // was really added.
        add(C, "C", C_ADDED_MS),
        // `events emit` of nodes the checkpoint already has: stamped with
        // the time of the emit, which used to become their created_at.
        add(A, "A", REEMIT_MS),
        json!({"op": "add_edge", "edge_id": "e1", "from_change_id": A, "to_change_id": C,
               "edge_type": "leads_to", "rationale": "r", "timestamp": REEMIT_MS,
               "author": "Alice"})
        .to_string(),
        json!({"op": "update_node", "change_id": A, "title": null, "description": null,
               "status": "completed", "metadata_json": null, "timestamp": REEMIT_MS + 1000,
               "author": "Alice"})
        .to_string(),
    ];
    fs::write(legacy.join("events/Alice.jsonl"), log.join("\n") + "\n").unwrap();
    tmp
}

#[test]
fn a_legacy_log_and_checkpoint_keep_their_dates_through_sync_into_graph_json_and_the_database() {
    let tmp = legacy_project();
    let dir = tmp.path();
    let out = ok(dir, &["sync", "--no-pages"]);
    assert!(out.contains("Imported legacy event log"), "{out}");

    let doc = doc(dir);
    let nodes = &doc["nodes"];
    assert_eq!(
        nodes[A]["created_at"], A_MADE,
        "A was re-dated to its re-emit"
    );
    assert_eq!(
        nodes[A]["status"], "completed",
        "the log's edit still applies"
    );
    assert_eq!(
        nodes[C]["created_at"], C_MADE,
        "C keeps the compaction's date though the log says when it was added"
    );
    assert_eq!(
        nodes[N]["created_at"], N_MADE,
        "a naive date is kept as written"
    );
    let edges: Vec<&Value> = doc["edges"].as_object().unwrap().values().collect();
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0]["created_at"], EDGE_MADE, "the edge was re-dated");

    let g = db_graph(dir);
    assert_eq!(row(&g["nodes"], A)["created_at"], A_MADE);
    assert_eq!(row(&g["nodes"], C)["created_at"], C_MADE);
    assert_eq!(row(&g["nodes"], N)["created_at"], N_MADE);
    assert_eq!(g["edges"][0]["created_at"], EDGE_MADE);

    // Again: nothing moves on a second pass.
    let before = fs::read_to_string(dir.join(".deciduous/graph.json")).unwrap();
    ok(dir, &["sync", "--no-pages"]);
    assert_eq!(
        before,
        fs::read_to_string(dir.join(".deciduous/graph.json")).unwrap()
    );
}

#[test]
fn seed_sends_every_date_as_the_database_holds_it() {
    let tmp = legacy_project();
    let dir = tmp.path();
    ok(dir, &["sync", "--no-pages"]);

    let server = stub();
    ok(dir, &["remote", "init", &server.url]);
    ok(dir, &["remote", "push", "--seed"]);

    let sent = server.imports.lock().unwrap().clone();
    assert!(!sent.is_empty(), "nothing was imported");
    let nodes: Vec<Value> = sent
        .iter()
        .flat_map(|p| p["graph"]["nodes"].as_array().cloned().unwrap_or_default())
        .collect();
    let edges: Vec<Value> = sent
        .iter()
        .flat_map(|p| p["graph"]["edges"].as_array().cloned().unwrap_or_default())
        .collect();
    let nodes = Value::Array(nodes);

    assert_eq!(row(&nodes, A)["created_at"], A_MADE);
    assert_eq!(row(&nodes, C)["created_at"], C_MADE);
    // Naive goes up with this machine's offset, as the CLI has always read
    // it: the same instant, spelled so the server cannot misread it as UTC.
    let n = row(&nodes, N)["created_at"].as_str().unwrap().to_string();
    assert!(
        chrono::DateTime::parse_from_rfc3339(&n).is_ok(),
        "naive date sent as is: {n}"
    );
    assert_eq!(parse_ts(&n), parse_ts(N_MADE));
    // `remote init` seeds, and the stub's export stays empty, so the push
    // sends everything again: every copy must carry the same dates.
    assert!(!edges.is_empty());
    for e in &edges {
        assert_eq!(e["created_at"], EDGE_MADE, "{e}");
    }
    for n in nodes.as_array().unwrap() {
        if n["change_id"] == A {
            assert_eq!(n["created_at"], A_MADE, "{n}");
        }
    }
}

#[test]
fn a_re_stamped_copy_in_graph_json_does_not_move_a_date_forward() {
    let tmp = legacy_project();
    let dir = tmp.path();
    ok(dir, &["sync", "--no-pages"]);

    // A teammate's copy of A, carried through a migration that stamped it
    // with its own time, and edited after: newer, so its title wins, and
    // later-dated, so its created_at must not.
    let path = dir.join(".deciduous/graph.json");
    let mut d = doc(dir);
    d["nodes"][A]["created_at"] = json!("2026-09-28T09:00:00+00:00");
    d["nodes"][A]["updated_at"] = json!("2026-09-28T09:00:01+00:00");
    d["nodes"][A]["title"] = json!("A, retitled");
    fs::write(&path, serde_json::to_string_pretty(&d).unwrap() + "\n").unwrap();

    ok(dir, &["sync", "--no-pages"]);
    let g = db_graph(dir);
    assert_eq!(row(&g["nodes"], A)["title"], "A, retitled");
    assert_eq!(row(&g["nodes"], A)["created_at"], A_MADE);
    assert_eq!(doc(dir)["nodes"][A]["created_at"], A_MADE);

    // The other way: graph.json carries an earlier date than the database
    // (the database was rebuilt from a re-stamped copy). The database
    // takes it, and says so.
    let mut d = doc(dir);
    d["nodes"][C]["created_at"] = json!("2016-06-06T06:06:06+00:00");
    fs::write(&path, serde_json::to_string_pretty(&d).unwrap() + "\n").unwrap();
    let out = ok(dir, &["sync", "--no-pages"]);
    assert!(out.contains("earlier created_at"), "{out}");
    assert_eq!(
        row(&db_graph(dir)["nodes"], C)["created_at"],
        "2016-06-06T06:06:06+00:00"
    );
}

struct Stub {
    url: String,
    imports: Arc<Mutex<Vec<Value>>>,
}

/// /health, an empty /export, and an /import that records its payload.
fn stub() -> Stub {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let imports = Arc::new(Mutex::new(Vec::new()));
    let im = imports.clone();
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
                json!({"nodes": [], "edges": [], "documents": []})
            } else if path.starts_with("/import") {
                let payload: Value = serde_json::from_slice(&body).unwrap();
                let n = payload["graph"]["nodes"].as_array().map_or(0, Vec::len);
                let e = payload["graph"]["edges"].as_array().map_or(0, Vec::len);
                im.lock().unwrap().push(payload);
                json!({"nodes": {"received": n, "upserted": n},
                       "edges": {"received": e, "upserted": e, "unresolved": 0},
                       "documents": {"received": 0, "upserted": 0}})
            } else if path.starts_with("/ops") {
                let payload: Value = serde_json::from_slice(&body).unwrap();
                let results: Vec<Value> = payload["ops"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|o| json!({"op_id": o["op_id"], "result": "applied"}))
                    .collect();
                json!({ "results": results })
            } else if path.starts_with("/claim") {
                json!({"claim": "unchecked"})
            } else if path.starts_with("/locate") {
                json!({"workspaces": []})
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
    Stub { url, imports }
}
