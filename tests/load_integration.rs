//! The local SQLite side under load, through the real process boundaries:
//! several `deciduous mcp` stdio servers, the CLI and the `serve --api`
//! daemon writing one project at once.
//!
//! Every test here spawns the built binary. Nothing is mocked: a failure
//! here is a failure a user with two agents open would see.

#![allow(dead_code)]

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};
use tempfile::TempDir;

const BIN: &str = env!("CARGO_BIN_EXE_deciduous");

/// A project directory with `.deciduous/`, isolated from the user's HOME so
/// no stored token or remote config can leak in.
struct Project {
    dir: TempDir,
    home: TempDir,
    /// When set, every process gets `DECIDUOUS_DB_PATH` pointing here
    /// instead of finding `.deciduous/` from its working directory.
    db_override: Option<PathBuf>,
}

impl Project {
    fn new() -> Self {
        let p = Self {
            dir: TempDir::new().unwrap(),
            home: TempDir::new().unwrap(),
            db_override: None,
        };
        std::fs::create_dir_all(p.dir.path().join(".deciduous")).unwrap();
        p
    }

    /// A graph laid out the way the API daemon keeps it,
    /// `<data>/graphs/<id>/deciduous.db`, with every CLI and MCP process
    /// pointed at the same file, so all three transports write one graph.
    fn api_shared(graph_id: &str) -> Self {
        let mut p = Self::new();
        let dir = p.root().join("data/graphs").join(graph_id);
        std::fs::create_dir_all(&dir).unwrap();
        p.db_override = Some(dir.join("deciduous.db"));
        let out = p.cli(&["sync", "--no-pages"]);
        assert!(out.status.success(), "sync failed: {}", text(&out.stderr));
        assert!(p.graph_path().is_file(), "{}", text(&out.stdout));
        p
    }

    /// With `.deciduous/graph.json`, so every write mirrors into it.
    fn with_graph() -> Self {
        let p = Self::new();
        let out = p.cli(&["sync", "--no-pages"]);
        assert!(out.status.success(), "sync failed: {}", text(&out.stderr));
        assert!(p.graph_path().is_file());
        p
    }

    fn root(&self) -> &Path {
        self.dir.path()
    }

    fn db_path(&self) -> PathBuf {
        self.db_override
            .clone()
            .unwrap_or_else(|| self.root().join(".deciduous/deciduous.db"))
    }

    fn graph_path(&self) -> PathBuf {
        self.db_path().with_file_name("graph.json")
    }

    fn graph_doc(&self) -> Value {
        serde_json::from_str(&std::fs::read_to_string(self.graph_path()).unwrap()).unwrap()
    }

    fn command(&self, cwd: &Path) -> Command {
        let mut c = Command::new(BIN);
        c.current_dir(cwd)
            .env("HOME", self.home.path())
            .env("XDG_CONFIG_HOME", self.home.path().join(".config"))
            .env_remove("DECIDUOUS_DB_PATH")
            .env_remove("DECIDUOUS_MCP_TOKEN")
            .env_remove("DECIDUOUS_API_TOKEN")
            .env_remove("DECIDUOUS_API_DATA_DIR")
            .env("GIT_CONFIG_NOSYSTEM", "1");
        if let Some(db) = &self.db_override {
            c.env("DECIDUOUS_DB_PATH", db);
        }
        c
    }

    fn cli(&self, args: &[&str]) -> std::process::Output {
        self.cli_in(self.root(), args)
    }

    fn cli_in(&self, cwd: &Path, args: &[&str]) -> std::process::Output {
        self.command(cwd)
            .args(args)
            .output()
            .expect("run deciduous")
    }

    fn mcp(&self) -> Mcp {
        Mcp::spawn(self.command(self.root()))
    }

    fn mcp_in(&self, cwd: &Path) -> Mcp {
        Mcp::spawn(self.command(cwd))
    }

    fn sql(&self, q: &str) -> i64 {
        rusqlite_like_count(&self.db_path(), q)
    }
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).to_string()
}

/// `sqlite3 db "<q>"` for a single integer, without linking a SQLite into
/// the test: ask the binary under test for its graph instead would hide the
/// very bugs these tests look for.
fn rusqlite_like_count(db: &Path, q: &str) -> i64 {
    let out = Command::new("sqlite3")
        .arg(db)
        .arg(q)
        .output()
        .expect("sqlite3 on PATH");
    assert!(out.status.success(), "sqlite3: {}", text(&out.stderr));
    text(&out.stdout).trim().parse().unwrap_or_else(|_| {
        panic!("sqlite3 answered {:?} to {q}", text(&out.stdout));
    })
}

/// A live `deciduous mcp` stdio server.
struct Mcp {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<String>,
    stderr: std::sync::Arc<std::sync::Mutex<String>>,
    next_id: i64,
}

impl Mcp {
    fn spawn(mut cmd: Command) -> Self {
        let mut child = cmd
            .arg("mcp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn deciduous mcp");
        let stdout = child.stdout.take().unwrap();
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                match line {
                    Ok(l) => {
                        if tx.send(l).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });
        let stderr = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let sink = stderr.clone();
        let mut err = child.stderr.take().unwrap();
        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            while let Ok(n) = err.read(&mut buf) {
                if n == 0 {
                    break;
                }
                sink.lock()
                    .unwrap()
                    .push_str(&String::from_utf8_lossy(&buf[..n]));
            }
        });
        let stdin = child.stdin.take();
        let mut m = Self {
            child,
            stdin,
            lines: rx,
            stderr,
            next_id: 0,
        };
        let init = m.request(
            "initialize",
            json!({"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"load-test","version":"0"}}),
        );
        assert!(init.get("result").is_some(), "initialize failed: {init}");
        m.send_raw(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n");
        m
    }

    fn send_raw(&mut self, bytes: &[u8]) {
        let stdin = self.stdin.as_mut().expect("stdin open");
        stdin.write_all(bytes).expect("write to mcp");
        stdin.flush().unwrap();
    }

    fn read_line(&mut self, timeout: Duration) -> Option<String> {
        self.lines.recv_timeout(timeout).ok()
    }

    fn line(&mut self) -> Value {
        let l = self.read_line(Duration::from_secs(60)).unwrap_or_else(|| {
            panic!("no reply from mcp; stderr: {}", self.stderr.lock().unwrap())
        });
        serde_json::from_str(&l).unwrap_or_else(|e| panic!("not JSON ({e}): {l}"))
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let msg = json!({"jsonrpc":"2.0","id":self.next_id,"method":method,"params":params});
        self.send_raw(format!("{msg}\n").as_bytes());
        self.line()
    }

    /// Call a tool. `Ok(payload)` or `Err(message)`.
    fn call(&mut self, name: &str, args: Value) -> Result<Value, String> {
        let r = self.request("tools/call", json!({"name": name, "arguments": args}));
        let result = r
            .get("result")
            .unwrap_or_else(|| panic!("{name}: JSON-RPC error {r}"));
        let body = result["content"][0]["text"].as_str().unwrap_or_default();
        let parsed = serde_json::from_str(body).unwrap_or(Value::String(body.to_string()));
        if result["isError"].as_bool().unwrap_or(false) {
            Err(body.to_string())
        } else {
            Ok(parsed)
        }
    }

    fn close(mut self) -> (Option<i32>, String) {
        drop(self.stdin.take());
        let deadline = Instant::now() + Duration::from_secs(10);
        let status = loop {
            if let Some(s) = self.child.try_wait().unwrap() {
                break s.code();
            }
            if Instant::now() > deadline {
                let _ = self.child.kill();
                break None;
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        std::thread::sleep(Duration::from_millis(50));
        let err = self.stderr.lock().unwrap().clone();
        (status, err)
    }
}

impl Drop for Mcp {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// ============================================================================
// R3: SQLite waits for a lock instead of failing on it
// ============================================================================

/// Hold SQLite's write lock from another process (the `sqlite3` shell) for
/// `secs`, the way a long import or another writer would.
fn hold_write_lock(db: &Path, secs: f32) -> Child {
    let mut c = Command::new("sqlite3")
        .arg(db)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("sqlite3 on PATH");
    let script = format!("BEGIN EXCLUSIVE;\n.shell sleep {secs}\nCOMMIT;\n");
    c.stdin
        .as_mut()
        .unwrap()
        .write_all(script.as_bytes())
        .unwrap();
    drop(c.stdin.take());
    // Give it time to take the lock before the caller races it.
    std::thread::sleep(Duration::from_millis(300));
    c
}

#[test]
fn mcp_started_while_another_process_writes_waits_instead_of_exiting() {
    let p = Project::new();
    assert!(p.cli(&["add", "goal", "seed"]).status.success());

    let mut locker = hold_write_lock(&p.db_path(), 2.0);
    let mut m = p.mcp();
    let added = m.call(
        "add_node",
        json!({"node_type":"action","title":"during lock","branch":"b"}),
    );
    locker.wait().unwrap();
    assert!(added.is_ok(), "add_node under a held lock: {added:?}");
    assert_eq!(p.sql("select count(*) from decision_nodes"), 2);
    m.close();
}

#[test]
fn cli_write_while_another_process_holds_the_lock_waits() {
    let p = Project::new();
    assert!(p.cli(&["add", "goal", "seed"]).status.success());
    let mut locker = hold_write_lock(&p.db_path(), 2.0);
    let out = p.cli(&["add", "action", "during lock", "-b", "b"]);
    locker.wait().unwrap();
    assert!(
        out.status.success(),
        "cli add under a held lock: {}",
        text(&out.stderr)
    );
}

#[test]
fn four_mcp_writers_at_once_all_succeed() {
    let p = Project::new();
    assert!(p.cli(&["add", "goal", "seed"]).status.success());
    let per = 40;
    let servers: Vec<Mcp> = (0..4).map(|_| p.mcp()).collect();
    let handles: Vec<_> = servers
        .into_iter()
        .enumerate()
        .map(|(k, mut m)| {
            std::thread::spawn(move || {
                let mut failures = Vec::new();
                for i in 0..per {
                    if let Err(e) = m.call(
                        "add_node",
                        json!({"node_type":"action","title":format!("w{k}-{i}"),"branch":"b"}),
                    ) {
                        failures.push(e);
                    }
                }
                m.close();
                failures
            })
        })
        .collect();
    let failures: Vec<String> = handles
        .into_iter()
        .flat_map(|h| h.join().unwrap())
        .collect();
    assert!(
        failures.is_empty(),
        "{} of {} writes failed, first: {}",
        failures.len(),
        4 * per,
        failures[0]
    );
    assert_eq!(
        p.sql("select count(*) from decision_nodes"),
        1 + 4 * per as i64
    );
    assert_eq!(
        text(
            &Command::new("sqlite3")
                .arg(p.db_path())
                .arg("PRAGMA journal_mode")
                .output()
                .unwrap()
                .stdout
        )
        .trim(),
        "wal"
    );
}

fn http(port: u16, method: &str, path: &str, token: &str, body: &Value) -> (u16, Value) {
    let payload = body.to_string();
    let raw = format!(
        "{method} {path} HTTP/1.0\r\nHost: localhost\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{payload}",
        payload.len()
    );
    let mut s = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    s.set_read_timeout(Some(Duration::from_secs(60))).unwrap();
    s.write_all(raw.as_bytes()).unwrap();
    let mut resp = String::new();
    s.read_to_string(&mut resp).unwrap();
    let status = resp
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap();
    let start = resp.find("\r\n\r\n").unwrap() + 4;
    (
        status,
        serde_json::from_str(&resp[start..]).unwrap_or(Value::String(resp[start..].to_string())),
    )
}

/// With WAL, recent writes live in `deciduous.db-wal` until a checkpoint, and
/// a server that stays open (an MCP session) never forces one for small
/// writes. A backup that copies `deciduous.db` alone silently drops them.
#[test]
fn backup_includes_writes_still_in_the_wal() {
    let p = Project::new();
    assert!(p.cli(&["add", "goal", "seed"]).status.success());
    let mut m = p.mcp();
    for i in 0..5 {
        m.call(
            "add_node",
            json!({"node_type":"action","title":format!("in wal {i}"),"branch":"b"}),
        )
        .unwrap();
    }
    let backup = p.root().join("backup.db");
    let out = p.cli(&["backup", "-o", backup.to_str().unwrap()]);
    assert!(out.status.success(), "backup: {}", text(&out.stderr));
    m.close();
    assert_eq!(
        rusqlite_like_count(&backup, "select count(*) from decision_nodes"),
        6,
        "the backup lost the writes that were still in the WAL"
    );
}

// ============================================================================
// The API daemon, as a real process
// ============================================================================

const API_TOKEN: &str = "load-test-token";

/// `deciduous serve --api` on a fixed port (4825-4829 are reserved for this
/// suite), killed on drop.
struct Daemon {
    child: Child,
    port: u16,
}

impl Daemon {
    fn start(p: &Project, port: u16, data_dir: &Path) -> Self {
        let child = p
            .command(p.root())
            .args(["serve", "--api", "--port", &port.to_string(), "--data-dir"])
            .arg(data_dir)
            .env("DECIDUOUS_API_TOKEN", API_TOKEN)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn serve --api");
        let d = Self { child, port };
        let deadline = Instant::now() + Duration::from_secs(20);
        while TcpStream::connect(("127.0.0.1", port)).is_err() {
            assert!(Instant::now() < deadline, "daemon never listened on {port}");
            std::thread::sleep(Duration::from_millis(50));
        }
        d
    }

    fn tool(&self, graph: &str, tool: &str, args: Value) -> (u16, Value) {
        api_tool(self.port, graph, tool, args)
    }
}

fn api_tool(port: u16, graph: &str, tool: &str, args: Value) -> (u16, Value) {
    http(
        port,
        "POST",
        &format!("/api/v1/graphs/{graph}/tools/{tool}"),
        API_TOKEN,
        &args,
    )
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// ============================================================================
// R1: concurrent writers never drop graph.json records
// ============================================================================

/// Titles of live node records in graph.json starting with `prefix`.
fn graph_titles(p: &Project, prefix: &str) -> std::collections::BTreeSet<String> {
    p.graph_doc()["nodes"]
        .as_object()
        .unwrap()
        .values()
        .filter(|r| r["deleted_at"].is_null())
        .filter_map(|r| r["title"].as_str())
        .filter(|t| t.starts_with(prefix))
        .map(str::to_string)
        .collect()
}

#[test]
fn mcp_cli_and_api_writing_at_once_lose_nothing_from_graph_json() {
    let p = Project::api_shared("shared");
    let data = p.root().join("data");
    let daemon = Daemon::start(&p, 4825, &data);
    let per = 30;

    let mut threads = Vec::new();
    for k in 0..2 {
        let mut m = p.mcp();
        threads.push(std::thread::spawn(move || {
            for i in 0..per {
                m.call(
                    "add_node",
                    json!({"node_type":"action","title":format!("c-mcp{k}-{i}"),"branch":"b"}),
                )
                .expect("mcp add_node");
            }
            m.close();
        }));
    }
    let port = daemon.port;
    for k in 0..2 {
        threads.push(std::thread::spawn(move || {
            for i in 0..per / 2 {
                let (status, body) = api_tool(
                    port,
                    "shared",
                    "add_node",
                    json!({"node_type":"action","title":format!("c-api{k}-{i}"),"branch":"b"}),
                );
                assert_eq!(status, 200, "{body}");
                assert_eq!(body["data"]["is_error"], false, "{body}");
            }
        }));
    }
    let cli_cmds: Vec<Command> = (0..per)
        .map(|i| {
            let mut c = p.command(p.root());
            c.args(["add", "action", &format!("c-cli-{i}"), "-b", "b"]);
            c
        })
        .collect();
    threads.push(std::thread::spawn(move || {
        for mut c in cli_cmds {
            let out = c.output().unwrap();
            assert!(out.status.success(), "cli add: {}", text(&out.stderr));
        }
    }));
    for t in threads {
        t.join().unwrap();
    }

    let expected = 2 * per + 2 * (per / 2) + per;
    assert_eq!(
        p.sql("select count(*) from decision_nodes where title like 'c-%'"),
        expected as i64
    );
    let in_file = graph_titles(&p, "c-");
    assert_eq!(
        in_file.len(),
        expected,
        "{} of {expected} committed nodes are missing from graph.json",
        expected - in_file.len()
    );
    drop(daemon);
}

#[test]
fn a_delete_racing_other_writers_is_not_resurrected_by_sync() {
    let p = Project::with_graph();
    let mut deleter = p.mcp();
    let mut ids = Vec::new();
    for i in 0..30 {
        let r = deleter
            .call(
                "add_node",
                json!({"node_type":"action","title":format!("doomed-{i}"),"branch":"b"}),
            )
            .unwrap();
        ids.push(r["node_id"].as_i64().unwrap());
    }
    let mut adder = p.mcp();
    let adding = std::thread::spawn(move || {
        for i in 0..150 {
            adder
                .call(
                    "add_node",
                    json!({"node_type":"action","title":format!("kept-{i}"),"branch":"b"}),
                )
                .unwrap();
        }
        adder.close();
    });
    for id in &ids {
        deleter.call("delete_node", json!({"node_id": id})).unwrap();
    }
    deleter.close();
    adding.join().unwrap();

    let alive = graph_titles(&p, "doomed-");
    assert!(
        alive.is_empty(),
        "{} deleted nodes are live again in graph.json: {:?}",
        alive.len(),
        alive
    );
    assert_eq!(graph_titles(&p, "kept-").len(), 150);
    let out = p.cli(&["sync", "--no-pages"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(
        p.sql("select count(*) from decision_nodes where title like 'doomed-%'"),
        0,
        "sync brought deleted nodes back: {}",
        text(&out.stdout)
    );
}

// ============================================================================
// R2: an id that does not fit is refused, never truncated to another node
// ============================================================================

#[test]
fn out_of_range_ids_are_refused_not_wrapped_onto_another_node() {
    let p = Project::new();
    let mut m = p.mcp();
    for t in ["n1", "n2"] {
        m.call(
            "add_node",
            json!({"node_type":"goal","title":t,"branch":"b"}),
        )
        .unwrap();
    }
    // 2^32 + 2 wraps to 2 under `as i32`.
    let e = m
        .call("delete_node", json!({"node_id": 4294967298u64}))
        .expect_err("a 33-bit id must be refused");
    assert!(e.contains("4294967298"), "{e}");
    for bad in [json!(4294967297u64), json!(-4294967295i64), json!(i64::MAX)] {
        let e = m
            .call("show_node", json!({"node_id": bad}))
            .expect_err("out of range id must be refused");
        assert!(e.contains("out of range"), "{bad}: {e}");
    }
    let e = m
        .call("link_nodes", json!({"from_id": 4294967297u64, "to_id": 2}))
        .expect_err("link with a wrapped id");
    assert!(e.contains("out of range"), "{e}");
    let e = m
        .call("trace_chain", json!({"node_id": 1, "max_depth": -1}))
        .expect_err("negative depth");
    assert!(e.contains("max_depth"), "{e}");

    // Sessions: start one, end it, then try to reach it through a wrapped id.
    let s = m
        .call("start_session", json!({"name":"s","goal_title":"g"}))
        .unwrap();
    let sid = s["session_id"].as_i64().unwrap();
    m.call("end_session", json!({})).unwrap();
    let e = m
        .call("resume_session", json!({"session_id": (1i64 << 32) + sid}))
        .expect_err("resume through a wrapped id");
    assert!(e.contains("out of range"), "{e}");
    m.close();

    assert_eq!(p.sql("select count(*) from decision_nodes"), 3);
}

#[test]
fn out_of_range_ids_are_refused_over_the_api() {
    let p = Project::api_shared("ids");
    let daemon = Daemon::start(&p, 4826, &p.root().join("data"));
    for t in ["n1", "n2"] {
        let (s, b) = daemon.tool(
            "ids",
            "add_node",
            json!({"node_type":"goal","title":t,"branch":"b"}),
        );
        assert_eq!(s, 200, "{b}");
    }
    let (_, b) = daemon.tool(
        "ids",
        "link_nodes",
        json!({"from_id": 4294967297u64, "to_id": 2}),
    );
    assert_eq!(b["data"]["is_error"], true, "{b}");
    assert_eq!(p.sql("select count(*) from decision_edges"), 0);
}

// ============================================================================
// R5: an MCP server started in a subdirectory uses the project's .deciduous/
// ============================================================================

#[test]
fn mcp_in_a_subdirectory_keeps_documents_and_sessions_in_the_project() {
    let p = Project::new();
    assert!(p.cli(&["add", "goal", "root goal"]).status.success());
    let src = p.root().join("src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("notes.md"), "# notes\n").unwrap();

    let mut m = p.mcp_in(&src);
    let r = m
        .call(
            "attach_document",
            json!({"node_id": 1, "file_path": "notes.md"}),
        )
        .unwrap();
    assert!(r["doc_id"].is_number(), "{r}");
    let s = m
        .call(
            "start_session",
            json!({"name":"sub","goal_title":"from src"}),
        )
        .unwrap();
    let sid = s["session_id"].as_i64().unwrap();
    m.close();

    assert!(
        !src.join(".deciduous").exists(),
        "a second .deciduous/ appeared in src/, splitting the graph"
    );
    let docs: Vec<_> = std::fs::read_dir(p.root().join(".deciduous/documents"))
        .expect("documents stored in the project")
        .collect();
    assert_eq!(docs.len(), 1);
    assert_eq!(
        std::fs::read_to_string(p.root().join(".deciduous/active_session"))
            .expect("session file in the project")
            .trim(),
        sid.to_string()
    );

    // A restarted server, and the CLI, both from src/, see the same graph
    // and the same session.
    let mut m = p.mcp_in(&src);
    let got = m.call("get_session", json!({})).unwrap();
    assert_eq!(got["session_id"], sid, "{got}");
    m.close();
    let out = p.cli_in(&src, &["nodes"]);
    assert!(
        text(&out.stdout).contains("root goal"),
        "{}",
        text(&out.stdout)
    );
}

// ============================================================================
// R4: attach_document only reads regular files inside the project
// ============================================================================

#[cfg(unix)]
#[test]
fn attach_document_refuses_anything_but_a_regular_file_in_the_project() {
    let p = Project::new();
    assert!(p.cli(&["add", "goal", "g"]).status.success());
    let outside = p.home.path().join("id_rsa");
    std::fs::write(&outside, "PRIVATE KEY").unwrap();
    std::os::unix::fs::symlink(&outside, p.root().join("innocent.png")).unwrap();
    let fifo = p.root().join("pipe");
    assert!(Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .unwrap()
        .success());
    std::fs::create_dir(p.root().join("adir")).unwrap();
    let big = std::fs::File::create(p.root().join("big.bin")).unwrap();
    big.set_len(200 * 1024 * 1024).unwrap();

    let mut m = p.mcp();
    let rel_outside = format!(
        "../{}/id_rsa",
        p.home.path().file_name().unwrap().to_str().unwrap()
    );
    let cases = [
        ("/etc/hosts", "outside"),
        (outside.to_str().unwrap(), "outside"),
        (rel_outside.as_str(), "outside"),
        ("innocent.png", "outside"),
        ("pipe", "regular file"),
        ("/dev/zero", "outside"),
        ("adir", "regular file"),
        ("big.bin", "larger than"),
    ];
    for (path, why) in cases {
        let started = Instant::now();
        let e = m
            .call("attach_document", json!({"node_id": 1, "file_path": path}))
            .expect_err(path);
        assert!(e.contains(why), "{path}: expected '{why}' in: {e}");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "{path} took {:?}",
            started.elapsed()
        );
    }
    m.close();
    assert_eq!(p.sql("select count(*) from node_documents"), 0);
    assert!(
        !p.root().join(".deciduous/documents").exists()
            || std::fs::read_dir(p.root().join(".deciduous/documents"))
                .unwrap()
                .next()
                .is_none()
    );
}

// ============================================================================
// R6 / R7: /query is bounded in time and size and cannot reach other files
// ============================================================================

fn query(port: u16, graph: &str, sql: &str) -> (u16, Value, Duration) {
    let t = Instant::now();
    let (s, b) = http(
        port,
        "POST",
        &format!("/api/v1/graphs/{graph}/query"),
        API_TOKEN,
        &json!({"sql": sql}),
    );
    (s, b, t.elapsed())
}

#[test]
fn api_query_is_bounded_and_confined_to_its_graph() {
    let p = Project::api_shared("q");
    let daemon = Daemon::start(&p, 4827, &p.root().join("data"));
    let (s, b) = daemon.tool(
        "q",
        "add_node",
        json!({"node_type":"goal","title":"g","branch":"b"}),
    );
    assert_eq!(s, 200, "{b}");

    let (s, b, _) = query(4827, "q", "SELECT count(*) FROM decision_nodes");
    assert_eq!(s, 200, "{b}");
    assert_eq!(b["data"]["rows"][0][0], 1);

    // Never terminates on its own: it has to be stopped.
    let (s, b, took) = query(
        4827,
        "q",
        "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM c) SELECT x FROM c WHERE x<0",
    );
    assert!(took < Duration::from_secs(15), "runaway query ran {took:?}");
    assert_eq!(s, 400, "{b}");
    assert!(b["error"].as_str().unwrap().contains("time limit"), "{b}");

    // SQLite's printf gives NULL instead of a value over the length limit;
    // the default limit is 1 GB, so this used to build all 200 MB.
    let (s, b, _) = query(4827, "q", "SELECT length(printf('%.*c', 200000000, 'x'))");
    assert_eq!(s, 200, "{b}");
    assert_eq!(b["data"]["rows"][0][0], Value::Null, "{b}");
    let (s, b, _) = query(4827, "q", "SELECT length(zeroblob(200000000))");
    assert_eq!(s, 400, "{b}");
    assert!(b["error"].as_str().unwrap().contains("too big"), "{b}");

    // ATTACH must not answer differently for a file that exists and one
    // that does not.
    let (s1, b1, _) = query(4827, "q", "ATTACH '/etc/hosts' AS h");
    let (s2, b2, _) = query(4827, "q", "ATTACH '/nonexistent/nope.db' AS h");
    assert_eq!(s1, 403, "{b1}");
    assert_eq!((s1, &b1), (s2, &b2));

    for sql in [
        "SELECT file FROM pragma_database_list",
        "PRAGMA database_list",
        "SELECT * FROM pragma_table_info('decision_nodes') JOIN pragma_database_list",
    ] {
        let (s, b, _) = query(4827, "q", sql);
        assert_eq!(s, 403, "{sql}: {b}");
        assert!(
            !b.to_string().contains(p.root().to_str().unwrap()),
            "{sql} revealed the data directory: {b}"
        );
    }
    // Harmless pragmas that describe the schema are still readable.
    let (s, b, _) = query(
        4827,
        "q",
        "SELECT name FROM pragma_table_info('decision_nodes')",
    );
    assert_eq!(s, 200, "{b}");
}

// ============================================================================
// R8: the API daemon never attributes a node to its own git checkout
// ============================================================================

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {}", text(&out.stderr));
}

#[test]
fn api_add_node_without_branch_gets_no_branch_from_the_daemon() {
    let p = Project::api_shared("attr");
    git(p.root(), &["init", "-q", "-b", "daemon-branch"]);
    git(p.root(), &["config", "user.name", "Daemon Owner"]);
    std::fs::write(p.root().join("f"), "x").unwrap();
    git(p.root(), &["add", "f"]);
    git(p.root(), &["commit", "-q", "-m", "c"]);
    let daemon = Daemon::start(&p, 4828, &p.root().join("data"));

    let (s, b) = daemon.tool(
        "attr",
        "add_node",
        json!({"node_type":"goal","title":"remote"}),
    );
    assert_eq!(s, 200, "{b}");
    let id = b["data"]["result"]["node_id"].clone();
    let (_, shown) = daemon.tool("attr", "show_node", json!({"node_id": id}));
    let node = &shown["data"]["result"];
    assert!(
        node.get("branch").is_none(),
        "daemon injected a branch: {node}"
    );
    assert!(
        node.get("commit").is_none(),
        "daemon injected a commit: {node}"
    );
    let change_id = node["change_id"].as_str().unwrap();
    let rec = &p.graph_doc()["nodes"][change_id];
    assert_eq!(rec["author"], "deciduous-api", "graph.json author: {rec}");

    let (_, b) = daemon.tool(
        "attr",
        "add_node",
        json!({"node_type":"goal","title":"h","commit":"HEAD"}),
    );
    assert_eq!(
        b["data"]["is_error"], true,
        "HEAD resolved against the daemon's checkout: {b}"
    );

    let (_, b) = daemon.tool(
        "attr",
        "add_node",
        json!({"node_type":"goal","title":"x","branch":"client-branch"}),
    );
    let id = b["data"]["result"]["node_id"].clone();
    let (_, shown) = daemon.tool("attr", "show_node", json!({"node_id": id}));
    assert_eq!(shown["data"]["result"]["branch"], "client-branch");
}

// ============================================================================
// R9 / R10: every request gets an answer carrying its id; bad bytes are an
// error reply, not a crash
// ============================================================================

impl Mcp {
    fn raw_reply(&mut self, line: &[u8]) -> Value {
        self.send_raw(line);
        let l = self
            .read_line(Duration::from_secs(10))
            .unwrap_or_else(|| panic!("no reply to {:?}", String::from_utf8_lossy(line)));
        serde_json::from_str(&l).unwrap()
    }
}

#[test]
fn malformed_requests_are_answered_with_their_id() {
    let p = Project::new();
    let mut m = p.mcp();

    let r = m.raw_reply(b"{\"jsonrpc\":\"2.0\",\"id\":41,\"method\":\"tools/call\"}\n");
    assert_eq!(r["id"], 41, "missing params: {r}");
    assert!(r["error"].is_object(), "{r}");

    let r = m.raw_reply(b"{\"jsonrpc\":\"2.0\",\"id\":42,\"method\":\"tools/call\",\"params\":{\"arguments\":{}}}\n");
    assert_eq!(r["id"], 42, "invalid params: {r}");

    let r = m.raw_reply(b"{\"jsonrpc\":\"1.0\",\"id\":43,\"method\":\"ping\"}\n");
    assert_eq!(r["id"], 43, "wrong version: {r}");

    // A lone surrogate is legal JSON syntax; strict parsers reject it. The
    // request still deserves an answer to its own id.
    let r = m.raw_reply(
        b"{\"jsonrpc\":\"2.0\",\"id\":44,\"method\":\"tools/call\",\"params\":{\"name\":\"add_node\",\"arguments\":{\"node_type\":\"goal\",\"title\":\"x\\ud800y\",\"branch\":\"b\"}}}\n",
    );
    assert_eq!(r["id"], 44, "lone surrogate: {r}");

    // id: null is present, so it is a request, not a notification.
    let r = m.raw_reply(b"{\"jsonrpc\":\"2.0\",\"id\":null,\"method\":\"ping\"}\n");
    assert!(r.get("error").is_some() || r.get("result").is_some(), "{r}");

    // Invalid UTF-8 on the wire: answered, and the server lives on.
    let r =
        m.raw_reply(b"{\"jsonrpc\":\"2.0\",\"id\":45,\"method\":\"ping\",\"x\":\"\xff\xfe\"}\n");
    assert_eq!(r["id"], 45, "invalid utf-8: {r}");
    assert!(
        r["error"]["message"].as_str().unwrap().contains("UTF-8"),
        "{r}"
    );
    let r = m.raw_reply(b"\xff\xfe\n");
    assert_eq!(r["error"]["code"], -32700, "{r}");

    let ok = m
        .call("list_nodes", json!({}))
        .expect("server still serving");
    assert!(ok["count"].is_number());
    let (code, stderr) = m.close();
    assert_eq!(code, Some(0), "{stderr}");
}

// ============================================================================
// R11: resume_session reopens what it says it resumed
// ============================================================================

#[test]
fn resuming_an_ended_session_reopens_it_and_survives_a_restart() {
    let p = Project::new();
    let mut m = p.mcp();
    let s = m
        .call("start_session", json!({"name":"long","goal_title":"g"}))
        .unwrap();
    let sid = s["session_id"].as_i64().unwrap();
    m.call("end_session", json!({"summary":"paused for lunch"}))
        .unwrap();
    let r = m
        .call("resume_session", json!({"session_id": sid}))
        .unwrap();
    assert_eq!(r["reopened"], true, "{r}");
    let got = m.call("get_session", json!({})).unwrap();
    assert_eq!(
        got["is_active"], true,
        "resumed session is still ended: {got}"
    );
    assert_eq!(got["ended_at"], Value::Null, "{got}");
    assert_eq!(
        got["summary"], "paused for lunch",
        "resume wiped the summary: {got}"
    );
    m.close();

    let mut m = p.mcp();
    let got = m.call("get_session", json!({})).unwrap();
    assert_eq!(got["session_id"], sid, "not resumed after restart: {got}");
    assert_eq!(got["is_active"], true, "{got}");
    m.close();
}

#[test]
fn a_second_server_taking_over_the_session_file_says_so() {
    let p = Project::new();
    let mut a = p.mcp();
    let mut b = p.mcp();
    let sa = a
        .call("start_session", json!({"name":"a","goal_title":"ga"}))
        .unwrap();
    let sb = b
        .call("start_session", json!({"name":"b","goal_title":"gb"}))
        .unwrap();
    assert_eq!(
        sb["replaced_session_id"], sa["session_id"],
        "b silently took over a's session file: {sb}"
    );
    a.close();
    b.close();
}

// ============================================================================
// R12: invalid writes are refused, not stored or reported as done
// ============================================================================

#[test]
fn invalid_writes_are_refused_through_mcp_and_cli() {
    let p = Project::new();
    let mut m = p.mcp();
    m.call(
        "add_node",
        json!({"node_type":"goal","title":"real","branch":"b"}),
    )
    .unwrap();

    let refused = [
        (
            "add_node",
            json!({"node_type":"banana","title":"x","branch":"b"}),
            "banana",
        ),
        (
            "add_node",
            json!({"node_type":"goal","title":"","branch":"b"}),
            "title",
        ),
        (
            "add_node",
            json!({"node_type":"goal","title":"   ","branch":"b"}),
            "title",
        ),
        (
            "add_node",
            json!({"node_type":"goal","title":"c","confidence":-5,"branch":"b"}),
            "confidence",
        ),
        (
            "add_node",
            json!({"node_type":"goal","title":"c","confidence":"90","branch":"b"}),
            "confidence",
        ),
        (
            "add_node",
            json!({"node_type":"goal","title":"c","confidence":150,"branch":"b"}),
            "confidence",
        ),
        ("link_nodes", json!({"from_id":1,"to_id":1}), "itself"),
        (
            "link_nodes",
            json!({"from_id":1,"to_id":1,"edge_type":"likes"}),
            "likes",
        ),
        (
            "update_status",
            json!({"node_id":99999,"status":"active"}),
            "99999",
        ),
        (
            "update_status",
            json!({"node_id":1,"status":"frobbed"}),
            "frobbed",
        ),
        (
            "update_prompt",
            json!({"node_id":99999,"prompt":"p"}),
            "99999",
        ),
    ];
    for (tool, args, why) in refused {
        let e = m
            .call(tool, args.clone())
            .expect_err(&format!("{tool} {args} was accepted"));
        assert!(e.contains(why), "{tool} {args}: expected '{why}' in: {e}");
    }
    // Valid values still go through.
    m.call(
        "add_node",
        json!({"node_type":"revisit","title":"ok","confidence":0,"branch":"b"}),
    )
    .unwrap();
    m.call("update_status", json!({"node_id":1,"status":"superseded"}))
        .unwrap();
    m.close();
    assert_eq!(p.sql("select count(*) from decision_nodes"), 2);
    assert_eq!(p.sql("select count(*) from decision_edges"), 0);

    for args in [
        &["add", "banana", "x"][..],
        &["add", "goal", ""][..],
        &["link", "1", "1"][..],
        &["status", "1", "frobbed"][..],
        &["status", "99", "active"][..],
    ] {
        let out = p.cli(args);
        assert!(
            !out.status.success(),
            "cli {args:?} succeeded: {}",
            text(&out.stdout)
        );
    }
    assert_eq!(p.sql("select count(*) from decision_nodes"), 2);
}

// ============================================================================
// R13: export_dot and generate_writeup cannot be steered by their inputs
// ============================================================================

#[test]
fn export_dot_rankdir_and_writeup_titles_cannot_inject() {
    let p = Project::new();
    let mut m = p.mcp();
    m.call(
        "add_node",
        json!({"node_type":"goal","title":"G\n# Injected heading\n- [x] fake check","branch":"b"}),
    )
    .unwrap();
    m.call(
        "add_node",
        json!({"node_type":"outcome","title":"done\n\n## Also injected","branch":"b"}),
    )
    .unwrap();

    let e = m
        .call(
            "export_dot",
            json!({"rankdir": "LR; injected_node [label=\"INJECTED\"]; 1 -> injected_node"}),
        )
        .expect_err("rankdir injection accepted");
    assert!(e.contains("rankdir"), "{e}");
    let dot = m.call("export_dot", json!({"rankdir":"LR"})).unwrap();
    assert!(dot.as_str().unwrap().contains("rankdir=LR"));
    let e = m
        .call("export_dot", json!({"roots":"1,banana"}))
        .expect_err("unparseable root silently dropped");
    assert!(e.contains("banana"), "{e}");

    let md = m.call("generate_writeup", json!({"no_dot": true})).unwrap();
    let md = md.as_str().unwrap();
    for line in md.lines() {
        assert!(
            !line.starts_with("# Injected") && !line.starts_with("## Also injected"),
            "a title broke out of its line:\n{md}"
        );
    }
    assert!(md.contains("Injected heading"), "{md}");
    m.close();

    let out = p.cli(&["dot", "--rankdir", "LR; x [label=y]"]);
    assert!(
        !out.status.success(),
        "cli dot accepted an injected rankdir"
    );
}

// ============================================================================
// R14: the smaller confusions
// ============================================================================

#[test]
fn a_corrupt_graph_file_is_not_answered_with_run_deciduous_sync() {
    let p = Project::with_graph();
    std::fs::write(p.graph_path(), "{ this is not json").unwrap();
    let out = p.cli(&["sync", "--no-pages"]);
    assert!(!out.status.success());
    let all = format!("{}{}", text(&out.stdout), text(&out.stderr));
    assert!(all.contains("graph.json"), "{all}");
    assert!(
        !all.contains("run `deciduous sync`"),
        "sync told the user to run sync: {all}"
    );
    let mut m = p.mcp();
    let e = m
        .call("sync", json!({}))
        .expect_err("sync tool on a corrupt file");
    assert!(!e.contains("run `deciduous sync`"), "{e}");
    m.close();
}

#[test]
fn api_daemon_startup_is_strict_about_tokens_and_touches_no_project() {
    let p = Project::new();
    let cwd = TempDir::new().unwrap();
    let data = TempDir::new().unwrap();
    let run = |args: &[&str], env_token: Option<&str>| {
        let mut c = p.command(cwd.path());
        c.args(["serve", "--api", "--port", "4829", "--data-dir"])
            .arg(data.path())
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(t) = env_token {
            c.env("DECIDUOUS_API_TOKEN", t);
        }
        c.spawn().unwrap()
    };
    let wait_listen = || {
        let deadline = Instant::now() + Duration::from_secs(20);
        while TcpStream::connect(("127.0.0.1", 4829)).is_err() {
            assert!(Instant::now() < deadline, "daemon never listened");
            std::thread::sleep(Duration::from_millis(50));
        }
    };
    let stop = |mut c: Child| {
        let _ = c.kill();
        c.wait_with_output().unwrap()
    };

    // A token that can never match the trimmed Authorization header.
    for bad in ["abc ", " ", "\tabc"] {
        let mut c = run(&["--token", bad], None);
        let deadline = Instant::now() + Duration::from_secs(10);
        let status = loop {
            if let Some(s) = c.try_wait().unwrap() {
                break Some(s);
            }
            if Instant::now() > deadline {
                break None;
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        let out = stop(c);
        let err = text(&out.stderr);
        assert!(
            status.is_some_and(|s| !s.success()),
            "started with token {bad:?}, which no request can ever match"
        );
        assert!(err.contains("whitespace") || err.contains("token"), "{err}");
    }

    // --token "" falls back to the environment instead of failing.
    let c = run(&["--token", ""], Some(API_TOKEN));
    wait_listen();
    let (s, b) = http(4829, "GET", "/api/v1/graphs", API_TOKEN, &json!({}));
    assert_eq!(s, 200, "{b}");

    // Racing PUTs of one new graph: exactly one of them created it.
    let handles: Vec<_> = (0..30)
        .map(|_| {
            std::thread::spawn(|| http(4829, "PUT", "/api/v1/graphs/race", API_TOKEN, &json!({})))
        })
        .collect();
    let created = handles
        .into_iter()
        .map(|h| h.join().unwrap())
        .filter(|(s, b)| *s == 201 && b["data"]["created"] == true)
        .count();
    assert_eq!(created, 1, "{created} PUTs claimed to create the graph");
    let out = stop(c);
    assert!(
        !cwd.path().join(".deciduous").exists(),
        "serve --api created a project database in its cwd"
    );
    let _ = out;

    // A token on the command line is visible in `ps`: say so.
    let c = run(&["--token", API_TOKEN], None);
    wait_listen();
    let out = stop(c);
    assert!(
        text(&out.stderr).contains("DECIDUOUS_API_TOKEN"),
        "no warning about --token: {}",
        text(&out.stderr)
    );
}

// ============================================================================
// Round two: what the verifiers found after the first round of fixes
// ============================================================================

/// Wait for a child to exit on its own; kill it and report `None` if it is
/// still running after `secs`.
fn exits_within(c: &mut Child, secs: u64) -> Option<std::process::ExitStatus> {
    let deadline = Instant::now() + Duration::from_secs(secs);
    loop {
        if let Some(s) = c.try_wait().unwrap() {
            return Some(s);
        }
        if Instant::now() > deadline {
            let _ = c.kill();
            let _ = c.wait();
            return None;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// `serve --api` with no data directory used `./.deciduous/api-data`. Run
/// from a project's subdirectory that made a second `.deciduous/` there, and
/// every later CLI call in that directory found the new, empty one first.
#[test]
fn api_daemon_without_a_data_dir_refuses_to_start_and_splits_no_project() {
    let p = Project::new();
    assert!(p.cli(&["add", "goal", "the project goal"]).status.success());
    let src = p.root().join("src");
    std::fs::create_dir_all(&src).unwrap();

    let mut c = p
        .command(&src)
        .args(["serve", "--api", "--port", "4831"])
        .env("DECIDUOUS_API_TOKEN", API_TOKEN)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let status = exits_within(&mut c, 10);
    let out = c.wait_with_output().unwrap();
    assert!(
        status.is_some_and(|s| !s.success()),
        "serve --api started with no data directory"
    );
    assert!(
        text(&out.stderr).contains("--data-dir"),
        "{}",
        text(&out.stderr)
    );
    assert!(
        !src.join(".deciduous").exists(),
        "serve --api created a second .deciduous/ inside the project"
    );
    let out = p.cli_in(&src, &["nodes"]);
    assert!(
        text(&out.stdout).contains("the project goal"),
        "the project's graph is no longer found from src/: {}",
        text(&out.stdout)
    );

    // The environment variable still names one.
    let data = TempDir::new().unwrap();
    let mut c = p
        .command(&src)
        .args(["serve", "--api", "--port", "4831"])
        .env("DECIDUOUS_API_TOKEN", API_TOKEN)
        .env("DECIDUOUS_API_DATA_DIR", data.path())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    while TcpStream::connect(("127.0.0.1", 4831)).is_err() {
        assert!(Instant::now() < deadline, "daemon never listened");
        assert!(c.try_wait().unwrap().is_none(), "daemon exited");
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = c.kill();
    let _ = c.wait();
    assert!(data.path().join("graphs").is_dir());
    assert!(!src.join(".deciduous").exists());
}

/// Same split, through `narratives init`, whose default output path was
/// `./.deciduous/narratives.md` relative to the cwd.
#[test]
fn narratives_init_in_a_subdirectory_writes_into_the_project() {
    let p = Project::new();
    assert!(p.cli(&["add", "goal", "narrated goal"]).status.success());
    let src = p.root().join("src");
    std::fs::create_dir_all(&src).unwrap();
    let out = p.cli_in(&src, &["narratives", "init"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(
        !src.join(".deciduous").exists(),
        "narratives init created a second .deciduous/ in src/"
    );
    let written = std::fs::read_to_string(p.root().join(".deciduous/narratives.md"))
        .expect("narratives.md in the project's .deciduous/");
    assert!(written.contains("narrated goal"), "{written}");
    let out = p.cli_in(&src, &["narratives", "show"]);
    assert!(
        text(&out.stdout).contains("narrated goal"),
        "{}",
        text(&out.stderr)
    );
}

/// A token no HTTP client can send: header values are visible ASCII.
#[test]
fn api_token_that_no_client_can_send_is_refused_at_startup() {
    let p = Project::new();
    let data = TempDir::new().unwrap();
    for bad in ["tökén", "abc\ndef", "abc\u{7f}"] {
        let mut c = p
            .command(p.root())
            .args(["serve", "--api", "--port", "4832", "--data-dir"])
            .arg(data.path())
            .env("DECIDUOUS_API_TOKEN", bad)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let status = exits_within(&mut c, 10);
        let out = c.wait_with_output().unwrap();
        assert!(
            status.is_some_and(|s| !s.success()),
            "started with token {bad:?}, which no HTTP client can send"
        );
        assert!(text(&out.stderr).contains("token"), "{}", text(&out.stderr));
    }
}

fn rss_kb(pid: u32) -> u64 {
    let out = Command::new("ps")
        .args(["-o", "rss=", "-p", &pid.to_string()])
        .output()
        .unwrap();
    text(&out.stdout).trim().parse().unwrap_or(0)
}

impl Mcp {
    fn ping_alive(&mut self) {
        self.next_id += 1;
        let id = self.next_id;
        self.send_raw(format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"method\":\"ping\"}}\n").as_bytes());
        let l = self.read_line(Duration::from_secs(10)).unwrap_or_else(|| {
            panic!(
                "server stopped answering; stderr: {}",
                self.stderr.lock().unwrap()
            )
        });
        let v: Value = serde_json::from_str(&l).unwrap();
        assert_eq!(v["id"], id, "{v}");
    }

    /// A tool call that must answer within `timeout`; `None` if it did not.
    fn call_within(
        &mut self,
        name: &str,
        args: Value,
        timeout: Duration,
    ) -> Option<Result<Value, String>> {
        self.next_id += 1;
        let msg = json!({"jsonrpc":"2.0","id":self.next_id,"method":"tools/call",
                         "params":{"name":name,"arguments":args}});
        self.send_raw(format!("{msg}\n").as_bytes());
        let l = self.read_line(timeout)?;
        let r: Value = serde_json::from_str(&l).unwrap();
        let result = &r["result"];
        let body = result["content"][0]["text"].as_str().unwrap_or_default();
        Some(if result["isError"].as_bool().unwrap_or(false) {
            Err(body.to_string())
        } else {
            Ok(serde_json::from_str(body).unwrap_or(Value::String(body.to_string())))
        })
    }
}

// ============================================================================
// R4, again: the file checked is the file read
// ============================================================================
