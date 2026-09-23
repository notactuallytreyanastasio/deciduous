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
}

impl Project {
    fn new() -> Self {
        let p = Self {
            dir: TempDir::new().unwrap(),
            home: TempDir::new().unwrap(),
        };
        std::fs::create_dir_all(p.dir.path().join(".deciduous")).unwrap();
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
        self.root().join(".deciduous/deciduous.db")
    }

    fn graph_path(&self) -> PathBuf {
        self.root().join(".deciduous/graph.json")
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
