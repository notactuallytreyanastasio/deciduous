//! The cross-surface harness the `e2e_*` integration tests share.
//!
//! Everything here drives a real process across a real boundary: the built
//! `deciduous` binary, `deciduous mcp` over stdio, `deciduous serve --api`
//! over TCP, a shared graph server over HTTP MCP, and git clones of a bare
//! origin. Nothing is mocked, because every finding this battery exists for
//! was invisible from inside one process.
//!
//! Gating, so a plain `cargo test` stays fast and hermetic:
//!
//! * `DECIDUOUS_E2E=1` runs the local battery: CLI, stdio MCP, API daemon,
//!   git clones. No network, no server.
//! * `DECIDUOUS_E2E_SERVER=<url>` plus `DECIDUOUS_E2E_TOKEN=<token>` also
//!   runs everything that needs a shared graph server (and implies the local
//!   battery). Every test writes to its own fresh workspace, so a shared
//!   server is safe to point at, but never point it at production.
//! * `DECIDUOUS_E2E_SEED=<u64>` replays one randomized run exactly;
//!   `DECIDUOUS_E2E_STEPS=<n>` changes how long it is.
//!
//! A gated test that is not enabled prints one `skipped:` line and passes.

#![allow(dead_code)]

use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Output, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tempfile::TempDir;

// ---------------------------------------------------------------- gating

pub fn local_enabled() -> bool {
    std::env::var("DECIDUOUS_E2E").is_ok_and(|v| !v.is_empty() && v != "0") || server().is_some()
}

/// The shared graph server under test, if one was given.
pub fn server() -> Option<Server> {
    let url = std::env::var("DECIDUOUS_E2E_SERVER").ok()?;
    let url = url.trim().trim_end_matches('/').to_string();
    if url.is_empty() {
        return None;
    }
    let token = std::env::var("DECIDUOUS_E2E_TOKEN")
        .unwrap_or_else(|_| panic!("DECIDUOUS_E2E_SERVER is set but DECIDUOUS_E2E_TOKEN is not"));
    Some(Server {
        url,
        token: token.trim().to_string(),
    })
}

/// `let Some(()) = e2e_support::local() else { return };` at the top of a
/// test that needs no server.
pub fn local(test: &str) -> Option<()> {
    if local_enabled() {
        Some(())
    } else {
        eprintln!("skipped: {test} (set DECIDUOUS_E2E=1)");
        None
    }
}

pub fn remote(test: &str) -> Option<Server> {
    let s = server();
    if s.is_none() {
        eprintln!("skipped: {test} (set DECIDUOUS_E2E_SERVER and DECIDUOUS_E2E_TOKEN)");
    }
    s
}

pub fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_deciduous"))
}

/// A name no other test or run has used: workspaces on a shared server
/// outlive the run, so collisions would read one test's graph as another's.
pub fn unique(prefix: &str) -> String {
    // The clock alone is not unique: macOS reports microseconds, and threads
    // spawned together read the same one. s7's 40 writers then shared a
    // branch name, and the server's branch lock refused the ones that
    // collided, correctly. A per-process counter makes every name distinct.
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!(
        "e2e-{prefix}-{}-{:x}-{n}",
        std::process::id(),
        nanos & 0xff_ffff_ffff
    )
    .to_lowercase()
}

// ---------------------------------------------------------------- rng

/// splitmix64: reproducible from one u64, no dependency.
#[derive(Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed)
    }
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    pub fn below(&mut self, n: usize) -> usize {
        assert!(n > 0, "Rng::below(0)");
        (self.next() % n as u64) as usize
    }
    pub fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        if items.is_empty() {
            None
        } else {
            Some(&items[self.below(items.len())])
        }
    }
}

/// The seed for a randomized test: `DECIDUOUS_E2E_SEED` or the clock. The
/// caller prints it before doing anything, so a failure always shows it.
pub fn seed() -> u64 {
    std::env::var("DECIDUOUS_E2E_SEED")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(|s| {
            s.trim()
                .parse()
                .unwrap_or_else(|_| panic!("DECIDUOUS_E2E_SEED must be a u64, got {s:?}"))
        })
        .unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64
        })
}

pub fn steps(default: usize) -> usize {
    std::env::var("DECIDUOUS_E2E_STEPS")
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(default)
}

// ---------------------------------------------------------------- processes

#[derive(Debug, Clone)]
pub struct Out {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl Out {
    pub fn ok(&self) -> bool {
        self.code == Some(0)
    }
    pub fn all(&self) -> String {
        format!("{}{}", self.stdout, self.stderr)
    }
}

impl From<Output> for Out {
    fn from(o: Output) -> Self {
        Out {
            code: o.status.code(),
            stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
        }
    }
}

/// One sandbox: a HOME nobody else uses, a PATH whose `deciduous` is the
/// binary under test (git's merge driver runs whatever `deciduous` PATH
/// names), and a token only when a server is under test.
pub struct Sandbox {
    pub root: TempDir,
    pub home: PathBuf,
    pub path_env: String,
    pub server: Option<Server>,
}

impl Sandbox {
    pub fn new() -> Self {
        let root = tempfile::Builder::new()
            .prefix("dx-e2e-")
            .tempdir()
            .expect("tempdir");
        // Canonical, so paths the binary prints (it canonicalizes) compare
        // equal to ours on macOS, where /var is /private/var.
        let base = root.path().canonicalize().unwrap();
        let home = base.join("home");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(
            home.join(".gitconfig"),
            "[user]\n\tname = e2e\n\temail = e2e@deciduous.test\n[init]\n\tdefaultBranch = main\n[advice]\n\tdetachedHead = false\n",
        )
        .unwrap();
        let bin_dir = bin().parent().unwrap().to_path_buf();
        let system = std::env::var("PATH").unwrap_or_default();
        Sandbox {
            root,
            home,
            path_env: format!("{}:{system}", bin_dir.display()),
            server: server(),
        }
    }

    pub fn with_server(server: Server) -> Self {
        let mut s = Self::new();
        s.server = Some(server);
        s
    }

    pub fn base(&self) -> PathBuf {
        self.root.path().canonicalize().unwrap()
    }

    /// Environment every child gets, overriding whatever the developer's
    /// shell has, so a test can never reach their real graph or token.
    pub fn apply(&self, cmd: &mut Command) {
        cmd.env("HOME", &self.home)
            .env("XDG_CONFIG_HOME", self.home.join(".config"))
            .env("PATH", &self.path_env)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("DECIDUOUS_NO_SERVER", "1")
            .env("NO_COLOR", "1")
            .env("CLICOLOR", "0")
            .env_remove("DECIDUOUS_DB_PATH")
            .env_remove("DECIDUOUS_API_TOKEN")
            .env_remove("CLAUDE_CONFIG_DIR")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE");
        match &self.server {
            Some(s) => cmd.env("DECIDUOUS_MCP_TOKEN", &s.token),
            None => cmd.env_remove("DECIDUOUS_MCP_TOKEN"),
        };
    }

    pub fn cmd(&self, program: impl AsRef<std::ffi::OsStr>, dir: &Path) -> Command {
        let mut c = Command::new(program);
        c.current_dir(dir).stdin(Stdio::null());
        self.apply(&mut c);
        c
    }

    pub fn git(&self, dir: &Path, args: &[&str]) -> Out {
        self.cmd("git", dir)
            .args(args)
            .output()
            .expect("run git")
            .into()
    }

    pub fn git_ok(&self, dir: &Path, args: &[&str]) -> String {
        let o = self.git(dir, args);
        assert!(
            o.ok(),
            "git {args:?} in {} failed:\n{}",
            dir.display(),
            o.all()
        );
        o.stdout
    }

    /// A bare repository standing in for GitHub.
    pub fn origin(&self, name: &str) -> PathBuf {
        let p = self.base().join(format!("{name}.git"));
        self.git_ok(&self.base(), &["init", "-q", "--bare", p.to_str().unwrap()]);
        p
    }

    /// A git repository with `deciduous init` run in it and the tracked
    /// deciduous files committed, pushed to `origin` if one is given.
    pub fn project(&self, name: &str, origin: Option<&Path>) -> Project<'_> {
        let dir = self.base().join(name);
        std::fs::create_dir_all(&dir).unwrap();
        self.git_ok(&dir, &["init", "-q"]);
        self.git_ok(&dir, &["commit", "-q", "--allow-empty", "-m", "init"]);
        let p = Project {
            dir: dir.clone(),
            sb: self,
        };
        let init = p.dx(&["init"]);
        assert!(init.ok(), "deciduous init failed:\n{}", init.all());
        p.commit_graph("deciduous init");
        if let Some(o) = origin {
            self.git_ok(&dir, &["remote", "add", "origin", o.to_str().unwrap()]);
            self.git_ok(&dir, &["push", "-q", "-u", "origin", "HEAD:main"]);
        }
        p
    }

    /// A second machine: `git clone`, then the first `deciduous sync`, which
    /// is what registers the merge driver in a fresh clone.
    pub fn clone_of(&self, origin: &Path, name: &str) -> Project<'_> {
        let dir = self.base().join(name);
        self.git_ok(
            &self.base(),
            &[
                "clone",
                "-q",
                origin.to_str().unwrap(),
                dir.to_str().unwrap(),
            ],
        );
        let p = Project { dir, sb: self };
        p.ok(&["sync"]);
        p
    }
}

impl Default for Sandbox {
    fn default() -> Self {
        Self::new()
    }
}

/// One working copy, i.e. one machine's view of one repository.
pub struct Project<'a> {
    pub dir: PathBuf,
    pub sb: &'a Sandbox,
}

impl Project<'_> {
    pub fn dx_cmd(&self) -> Command {
        self.sb.cmd(bin(), &self.dir)
    }

    pub fn dx(&self, args: &[&str]) -> Out {
        self.dx_cmd()
            .args(args)
            .output()
            .expect("run deciduous")
            .into()
    }

    pub fn dx_in(&self, sub: &Path, args: &[&str]) -> Out {
        self.sb
            .cmd(bin(), sub)
            .args(args)
            .output()
            .expect("run deciduous")
            .into()
    }

    pub fn dx_stdin(&self, args: &[&str], input: &[u8]) -> Out {
        let mut c = self.dx_cmd();
        c.args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = c.spawn().expect("spawn deciduous");
        child.stdin.take().unwrap().write_all(input).unwrap();
        child.wait_with_output().unwrap().into()
    }

    pub fn ok(&self, args: &[&str]) -> String {
        let o = self.dx(args);
        assert!(
            o.ok(),
            "deciduous {args:?} in {} failed ({:?}):\n{}",
            self.dir.display(),
            o.code,
            o.all()
        );
        o.stdout
    }

    pub fn git(&self, args: &[&str]) -> Out {
        self.sb.git(&self.dir, args)
    }

    pub fn git_ok(&self, args: &[&str]) -> String {
        self.sb.git_ok(&self.dir, args)
    }

    pub fn graph_file(&self) -> PathBuf {
        self.dir.join(".deciduous").join("graph.json")
    }

    pub fn graph_doc(&self) -> Value {
        let text = std::fs::read_to_string(self.graph_file()).expect("read graph.json");
        serde_json::from_str(&text)
            .unwrap_or_else(|e| panic!("graph.json does not parse: {e}\n{text}"))
    }

    /// `deciduous graph`: the local database, as the CLI sees it.
    pub fn graph(&self) -> Value {
        let out = self.ok(&["graph"]);
        serde_json::from_str(&out).unwrap_or_else(|e| panic!("graph is not JSON: {e}\n{out}"))
    }

    pub fn view(&self) -> View {
        View::from_local(&self.graph())
    }

    /// Adds a node through the CLI and returns its change_id.
    pub fn add(&self, node_type: &str, title: &str) -> String {
        let out = self.ok(&["add", node_type, title]);
        let id = created_id(&out);
        self.change_id_of(id)
    }

    pub fn change_id_of(&self, local_id: i64) -> String {
        let g = self.graph();
        g["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"].as_i64() == Some(local_id))
            .and_then(|n| n["change_id"].as_str())
            .unwrap_or_else(|| panic!("no local node {local_id} in {g}"))
            .to_string()
    }

    /// Points this project at the server under test, in its own workspace.
    pub fn remote_init(&self, workspace: &str) {
        let url = &self.sb.server.as_ref().expect("no server under test").url;
        self.ok(&["remote", "init", url, "--workspace", workspace]);
    }

    /// Rewrites the configured URL without going through `remote init`
    /// (which checks the server): how a test takes the server away.
    pub fn set_remote_url(&self, url: &str) {
        let path = self.dir.join(".deciduous").join("config.toml");
        let text = std::fs::read_to_string(&path).unwrap();
        let mut doc: toml_edit::DocumentMut = text.parse().unwrap();
        doc["remote"]["url"] = toml_edit::value(url);
        std::fs::write(&path, doc.to_string()).unwrap();
    }

    pub fn commit_graph(&self, msg: &str) {
        let mut paths = vec![".deciduous/config.toml", ".deciduous/graph.json"];
        for extra in [".gitattributes", ".gitignore"] {
            if self.dir.join(extra).exists() {
                paths.push(extra);
            }
        }
        let mut args = vec!["add", "--"];
        args.extend(paths.iter().copied());
        self.git_ok(&args);
        if !self.git(&["diff", "--cached", "--quiet"]).ok() {
            self.git_ok(&["commit", "-q", "-m", msg]);
        }
    }

    /// What a person does at a keyboard: sync, commit, pull (the merge
    /// driver runs here), sync again, push.
    pub fn git_exchange(&self) {
        self.ok(&["sync"]);
        self.commit_graph("graph");
        let pull = self.git(&["pull", "-q", "--no-rebase", "origin", "main"]);
        assert!(
            pull.ok(),
            "git pull failed in {}:\n{}",
            self.dir.display(),
            pull.all()
        );
        self.ok(&["sync"]);
        self.commit_graph("graph after merge");
        self.git_ok(&["push", "-q", "origin", "HEAD:main"]);
    }
}

/// `Created node 12 (...)` -> 12.
pub fn created_id(out: &str) -> i64 {
    let rest = out
        .split("Created node ")
        .nth(1)
        .unwrap_or_else(|| panic!("no 'Created node' in {out:?}"));
    rest.split(|c: char| !c.is_ascii_digit())
        .next()
        .and_then(|d| d.parse().ok())
        .unwrap_or_else(|| panic!("no id after 'Created node' in {out:?}"))
}

// ---------------------------------------------------------------- content views

/// The part of a graph two copies must agree on: live nodes by change_id and
/// edges by their endpoints' change_ids. Local ids, timestamps and server
/// UUIDs are deliberately left out; they differ per copy by design.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct View {
    pub nodes: BTreeMap<String, NodeView>,
    pub edges: BTreeSet<(String, String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeView {
    pub node_type: String,
    pub title: String,
    pub status: String,
    pub prompt: Option<String>,
}

fn prompt_of(meta: &Value) -> Option<String> {
    meta.get("prompt")
        .and_then(Value::as_str)
        .map(str::to_string)
}

impl View {
    pub fn from_local(graph: &Value) -> View {
        let mut v = View::default();
        let mut cid_of = BTreeMap::new();
        for n in graph["nodes"].as_array().expect("nodes array") {
            let cid = n["change_id"].as_str().expect("change_id").to_string();
            cid_of.insert(n["id"].as_i64().unwrap(), cid.clone());
            let meta: Value = n["metadata_json"]
                .as_str()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or(Value::Null);
            v.nodes.insert(
                cid,
                NodeView {
                    node_type: n["node_type"].as_str().unwrap_or("").to_string(),
                    title: n["title"].as_str().unwrap_or("").to_string(),
                    status: n["status"].as_str().unwrap_or("").to_string(),
                    prompt: prompt_of(&meta),
                },
            );
        }
        for e in graph["edges"].as_array().expect("edges array") {
            let from = e["from_change_id"]
                .as_str()
                .map(str::to_string)
                .or_else(|| cid_of.get(&e["from_node_id"].as_i64().unwrap()).cloned())
                .unwrap_or_else(|| format!("<dangling local {}>", e["from_node_id"]));
            let to = e["to_change_id"]
                .as_str()
                .map(str::to_string)
                .or_else(|| cid_of.get(&e["to_node_id"].as_i64().unwrap()).cloned())
                .unwrap_or_else(|| format!("<dangling local {}>", e["to_node_id"]));
            v.edges.insert((
                from,
                to,
                e["edge_type"].as_str().unwrap_or("leads_to").to_string(),
            ));
        }
        v
    }

    pub fn from_server(export: &Value) -> View {
        let mut v = View::default();
        for n in export["nodes"].as_array().expect("nodes array") {
            if !n["deleted_at"].is_null() && n.get("deleted_at").is_some() {
                continue;
            }
            let cid = n["change_id"].as_str().expect("change_id").to_string();
            v.nodes.insert(
                cid,
                NodeView {
                    node_type: n["node_type"].as_str().unwrap_or("").to_string(),
                    title: n["title"].as_str().unwrap_or("").to_string(),
                    status: n["status"].as_str().unwrap_or("").to_string(),
                    prompt: prompt_of(&n["metadata"]),
                },
            );
        }
        for e in export["edges"].as_array().expect("edges array") {
            v.edges.insert((
                e["from_change_id"].as_str().unwrap_or("<none>").to_string(),
                e["to_change_id"].as_str().unwrap_or("<none>").to_string(),
                e["edge_type"].as_str().unwrap_or("leads_to").to_string(),
            ));
        }
        v
    }

    /// Edges whose endpoints are not live nodes of this same view.
    pub fn dangling(&self) -> Vec<(String, String, String)> {
        self.edges
            .iter()
            .filter(|(f, t, _)| !self.nodes.contains_key(f) || !self.nodes.contains_key(t))
            .cloned()
            .collect()
    }

    /// A readable, bounded description of how two views differ.
    pub fn diff(&self, other: &View, a: &str, b: &str) -> String {
        let mut out = Vec::new();
        for (k, n) in &self.nodes {
            match other.nodes.get(k) {
                None => out.push(format!("node {k} {:?} in {a}, missing in {b}", n.title)),
                Some(m) if m != n => out.push(format!("node {k}: {a}={n:?} {b}={m:?}")),
                _ => {}
            }
        }
        for (k, n) in &other.nodes {
            if !self.nodes.contains_key(k) {
                out.push(format!("node {k} {:?} in {b}, missing in {a}", n.title));
            }
        }
        for e in self.edges.difference(&other.edges) {
            out.push(format!("edge {e:?} in {a}, missing in {b}"));
        }
        for e in other.edges.difference(&self.edges) {
            out.push(format!("edge {e:?} in {b}, missing in {a}"));
        }
        let n = out.len();
        out.truncate(40);
        if n > 40 {
            out.push(format!("... and {} more", n - 40));
        }
        out.join("\n")
    }
}

// ---------------------------------------------------------------- stdio MCP

/// `deciduous mcp`, spoken to over its real stdin and stdout.
pub struct StdioMcp {
    pub child: Child,
    stdin: Option<ChildStdin>,
    lines: mpsc::Receiver<String>,
    next_id: i64,
    pub stderr: std::sync::Arc<std::sync::Mutex<String>>,
}

impl StdioMcp {
    pub fn spawn(sb: &Sandbox, dir: &Path) -> Self {
        let mut c = sb.cmd(bin(), dir);
        c.arg("mcp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = c.spawn().expect("spawn deciduous mcp");
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let err_buf = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let eb = err_buf.clone();
        std::thread::spawn(move || {
            let mut r = BufReader::new(stderr);
            let mut s = String::new();
            let _ = r.read_to_string(&mut s);
            eb.lock().unwrap().push_str(&s);
        });
        let mut m = StdioMcp {
            child,
            stdin: None,
            lines: rx,
            next_id: 1,
            stderr: err_buf,
        };
        m.stdin = m.child.stdin.take();
        let init = m.request(
            "initialize",
            json!({"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"e2e","version":"0"}}),
        );
        assert!(init.get("result").is_some(), "initialize failed: {init}");
        m.send_raw(br#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);
        m
    }

    pub fn send_raw(&mut self, bytes: &[u8]) {
        let stdin = self.stdin.as_mut().expect("stdin closed");
        let _ = stdin.write_all(bytes);
        let _ = stdin.write_all(b"\n");
        let _ = stdin.flush();
    }

    pub fn close_stdin(&mut self) {
        self.stdin = None;
    }

    /// The next line the server writes, or None on timeout / exit.
    pub fn recv(&self, timeout: Duration) -> Option<Value> {
        let line = self.lines.recv_timeout(timeout).ok()?;
        Some(
            serde_json::from_str(&line).unwrap_or_else(|e| {
                panic!("stdio MCP wrote a line that is not JSON ({e}): {line:?}")
            }),
        )
    }

    /// Sends one request and waits for the reply carrying its id.
    pub fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let msg = json!({"jsonrpc":"2.0","id":id,"method":method,"params":params});
        self.send_raw(msg.to_string().as_bytes());
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let Some(v) = self.recv(left) else {
                panic!(
                    "stdio MCP gave no reply to {method} id {id} (it exited, or 60s passed); stderr:\n{}",
                    self.stderr.lock().unwrap()
                )
            };
            if v["id"] == json!(id) {
                return v;
            }
        }
    }

    /// A tool call: Ok(parsed JSON text) or Err(error text).
    pub fn call(&mut self, tool: &str, args: Value) -> Result<Value, String> {
        let v = self.request("tools/call", json!({"name":tool,"arguments":args}));
        tool_result(&v)
    }

    pub fn call_ok(&mut self, tool: &str, args: Value) -> Value {
        let a = args.to_string();
        self.call(tool, args)
            .unwrap_or_else(|e| panic!("stdio {tool} {a} failed: {e}"))
    }

    /// Whether the process is still running.
    pub fn alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
}

impl Drop for StdioMcp {
    fn drop(&mut self) {
        self.stdin = None;
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if !matches!(self.child.try_wait(), Ok(None)) {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Unwraps a JSON-RPC tools/call reply into Ok(text as JSON, or as a JSON
/// string) or Err(the error text).
pub fn tool_result(v: &Value) -> Result<Value, String> {
    if let Some(e) = v.get("error") {
        return Err(format!("JSON-RPC error: {e}"));
    }
    let r = &v["result"];
    let text: String = r["content"]
        .as_array()
        .map(|c| {
            c.iter()
                .filter_map(|x| x["text"].as_str())
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default();
    if r["isError"] == json!(true) {
        return Err(text);
    }
    Ok(serde_json::from_str(&text).unwrap_or(Value::String(text)))
}

// ---------------------------------------------------------------- HTTP MCP

#[derive(Debug, Clone)]
pub struct Server {
    pub url: String,
    pub token: String,
}

pub struct HttpResp {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub body: String,
}

impl HttpResp {
    /// The JSON-RPC message in the body, whether sent as JSON or as SSE.
    pub fn rpc(&self) -> Option<Value> {
        let b = self.body.trim();
        let json_text = if b.starts_with("event:") || b.starts_with("data:") {
            b.lines()
                .filter_map(|l| l.strip_prefix("data:"))
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            b.to_string()
        };
        serde_json::from_str(&json_text).ok()
    }
}

impl Server {
    pub fn request(
        &self,
        method: &str,
        path: &str,
        auth: Option<&str>,
        headers: &[(&str, &str)],
        body: Option<&[u8]>,
    ) -> HttpResp {
        self.try_request(method, path, auth, headers, body, Duration::from_secs(120))
            .unwrap_or_else(|e| panic!("{method} {path}: {e}"))
    }

    /// Like `request`, but a transport failure (refused, reset, timed out)
    /// is returned rather than panicking: for tests where that failure is
    /// the finding.
    pub fn try_request(
        &self,
        method: &str,
        path: &str,
        auth: Option<&str>,
        headers: &[(&str, &str)],
        body: Option<&[u8]>,
        timeout: Duration,
    ) -> Result<HttpResp, String> {
        let mut r = ureq::request(method, &format!("{}{}", self.url, path)).timeout(timeout);
        if let Some(a) = auth {
            r = r.set("authorization", a);
        }
        for (k, v) in headers {
            r = r.set(k, v);
        }
        let res = match body {
            Some(b) => r.send_bytes(b),
            None => r.call(),
        };
        let resp = match res {
            Ok(r) => r,
            Err(ureq::Error::Status(_, r)) => r,
            Err(ureq::Error::Transport(t)) => return Err(format!("transport error {t}")),
        };
        let status = resp.status();
        let mut hs = BTreeMap::new();
        for name in resp.headers_names() {
            if let Some(v) = resp.header(&name) {
                hs.insert(name.to_lowercase(), v.to_string());
            }
        }
        let mut raw = Vec::new();
        let _ = resp.into_reader().take(64 << 20).read_to_end(&mut raw);
        Ok(HttpResp {
            status,
            headers: hs,
            body: String::from_utf8_lossy(&raw).into_owned(),
        })
    }

    pub fn bearer(&self) -> String {
        format!("Bearer {}", self.token)
    }

    pub fn export(&self, workspace: &str) -> Value {
        let r = self.request(
            "GET",
            &format!("/export?workspace={}", urlencode(workspace)),
            Some(&self.bearer()),
            &[],
            None,
        );
        assert_eq!(r.status, 200, "export {workspace}: {}", r.body);
        serde_json::from_str(&r.body).expect("export is JSON")
    }

    pub fn view(&self, workspace: &str) -> View {
        View::from_server(&self.export(workspace))
    }

    pub fn session(&self, pin: Option<&str>) -> HttpMcp {
        HttpMcp::connect(self.clone(), pin)
    }
}

pub fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// One MCP session against the shared server, optionally pinned to a
/// workspace by header the way a repository's `.mcp.json` pins it.
pub struct HttpMcp {
    pub server: Server,
    pub pin: Option<String>,
    pub session: Option<String>,
    /// Put into every write that names none. The server locks a branch of a
    /// workspace to the session writing it, so two sessions writing with no
    /// branch contend; tests that are not about that lock should not.
    pub branch: Option<String>,
    next_id: i64,
}

const WRITE_TOOLS: &[&str] = &[
    "add_node",
    "update_node",
    "delete_node",
    "add_edge",
    "delete_edge",
    "close_thread",
    "log_decision",
    "log_observation",
    "capture_conversation_turn",
];

impl HttpMcp {
    pub fn connect(server: Server, pin: Option<&str>) -> Self {
        let mut m = HttpMcp {
            server,
            pin: pin.map(str::to_string),
            session: None,
            branch: Some(unique("agent")),
            next_id: 1,
        };
        let r = m.post(
            json!({"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"e2e","version":"0"}}})
                .to_string()
                .as_bytes(),
        );
        assert_eq!(r.status, 200, "initialize: {}", r.body);
        m.session = r.headers.get("mcp-session-id").cloned();
        m.post(br#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);
        m
    }

    pub fn headers(&self) -> Vec<(String, String)> {
        let mut h = vec![
            ("content-type".to_string(), "application/json".to_string()),
            (
                "accept".to_string(),
                "application/json, text/event-stream".to_string(),
            ),
        ];
        if let Some(p) = &self.pin {
            h.push(("x-deciduous-workspace".to_string(), p.clone()));
        }
        if let Some(s) = &self.session {
            h.push(("mcp-session-id".to_string(), s.clone()));
        }
        h
    }

    pub fn post(&self, body: &[u8]) -> HttpResp {
        let h = self.headers();
        let hr: Vec<(&str, &str)> = h.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
        self.server
            .request("POST", "/mcp", Some(&self.server.bearer()), &hr, Some(body))
    }

    pub fn call(&mut self, tool: &str, mut args: Value) -> Result<Value, String> {
        if let (Some(b), Some(obj)) = (&self.branch, args.as_object_mut()) {
            if WRITE_TOOLS.contains(&tool) && !obj.contains_key("branch") {
                obj.insert("branch".to_string(), Value::String(b.clone()));
            }
        }
        let id = self.next_id;
        self.next_id += 1;
        let body = json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":tool,"arguments":args}});
        let r = self.post(body.to_string().as_bytes());
        let Some(v) = r.rpc() else {
            return Err(format!(
                "HTTP {} with no JSON-RPC body: {}",
                r.status, r.body
            ));
        };
        tool_result(&v)
    }

    pub fn call_ok(&mut self, tool: &str, args: Value) -> Value {
        let a = args.to_string();
        self.call(tool, args)
            .unwrap_or_else(|e| panic!("http {tool} {a} failed: {e}"))
    }

    /// Server UUID of the node with this change_id, from an export.
    pub fn uuid_of(&self, workspace: &str, change_id: &str) -> String {
        let ex = self.server.export(workspace);
        ex["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["change_id"].as_str() == Some(change_id))
            .and_then(|n| n["id"].as_str())
            .unwrap_or_else(|| panic!("server workspace {workspace} has no node {change_id}"))
            .to_string()
    }
}

// ---------------------------------------------------------------- API daemon

/// A port nothing is listening on (bound, read, released). Only for a URL
/// that must refuse connections: a daemon given one races every other
/// socket for it (see [`spawn_listening`]).
pub fn dead_port() -> u16 {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    l.local_addr().unwrap().port()
}

/// Spawns `serve --api --port 0 ...` (as `c` says) and returns it with the
/// port the daemon itself bound, read from its "API daemon on http://..:N"
/// line; stdout is drained after that.
///
/// Why not `dead_port()` and pass it in: macOS hands out ephemeral ports in
/// sequence (59081, 59082, ...), so between the release and the daemon's
/// bind the port is free for any other test's socket. The r14 flake: all 30
/// racing PUTs "Connection reset by peer" in one battery run, and a
/// ulimit-64 daemon that "never answered" in another, both consistent with
/// a test talking to a port some other test's daemon or client held.
pub fn spawn_listening(mut c: Command) -> (Child, u16) {
    c.stdout(Stdio::piped());
    let mut child = c.spawn().expect("spawn serve --api");
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        use std::io::BufRead;
        let mut tx = Some(tx);
        for line in std::io::BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if let Some(tx) = tx.take_if(|_| line.contains("API daemon on http://")) {
                let port = line
                    .split("http://")
                    .nth(1)
                    .and_then(|r| r.split_whitespace().next())
                    .and_then(|hp| hp.rsplit(':').next())
                    .map(|p| p.trim_matches(|c: char| !c.is_ascii_digit()).to_string())
                    .and_then(|p| p.parse::<u16>().ok());
                let _ = tx.send(port.unwrap_or_else(|| panic!("no port in {line:?}")));
            }
        }
    });
    match rx.recv_timeout(Duration::from_secs(10)) {
        Ok(port) => (child, port),
        Err(_) => {
            let _ = child.kill();
            let out = child.wait_with_output().unwrap();
            panic!(
                "serve --api never said where it listens; stderr:\n{}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }
}

/// `deciduous serve --api` as its own process.
pub struct ApiDaemon {
    pub child: Child,
    pub port: u16,
    pub token: String,
    pub data_dir: PathBuf,
}

impl ApiDaemon {
    /// Starts the daemon on a port it chooses (`--port 0`) and learns that
    /// port from the line it prints once it is bound.
    ///
    /// It used to be handed a port found free a moment earlier (dead_port:
    /// bind to 0, read the port, release it) and be taken as listening as
    /// soon as any TCP connect to that port succeeded. Both halves are a
    /// window: the port is free only when it is looked at, and a connect
    /// that succeeds says something is there, not that it is this child.
    /// The r6 flake (a first request "Connection refused", or "reset by
    /// peer" with twelve daemons starting at once) came through it.
    pub fn spawn(sb: &Sandbox, data_dir: &Path, token: &str) -> Self {
        std::fs::create_dir_all(data_dir).unwrap();
        let mut c = sb.cmd(bin(), data_dir);
        c.args([
            "serve",
            "--api",
            "--port",
            "0",
            "--data-dir",
            data_dir.to_str().unwrap(),
        ])
        .env("DECIDUOUS_API_TOKEN", token)
        .env("NO_COLOR", "1")
        .stderr(Stdio::null());
        let (child, port) = spawn_listening(c);
        ApiDaemon {
            child,
            port,
            token: token.to_string(),
            data_dir: data_dir.to_path_buf(),
        }
    }

    pub fn as_server(&self) -> Server {
        Server {
            url: format!("http://127.0.0.1:{}", self.port),
            token: self.token.clone(),
        }
    }

    pub fn tool(&self, graph: &str, tool: &str, args: Value) -> (u16, Value) {
        let s = self.as_server();
        let r = s.request(
            "POST",
            &format!("/api/v1/graphs/{graph}/tools/{tool}"),
            Some(&s.bearer()),
            &[("content-type", "application/json")],
            Some(args.to_string().as_bytes()),
        );
        (
            r.status,
            serde_json::from_str(&r.body).unwrap_or(Value::String(r.body)),
        )
    }
}

impl Drop for ApiDaemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// ---------------------------------------------------------------- misc

/// Polls `f` until it returns Some or the time runs out.
pub fn wait_for<T>(timeout: Duration, mut f: impl FnMut() -> Option<T>) -> Option<T> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(v) = f() {
            return Some(v);
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// What `remote status` must say when the local and server content agree,
/// and must not say when they do not. The load-bearing part is the exit
/// code: status exits 1 whenever anything differs, is waiting or was
/// refused, as `sync --check` does. The words are the operation log's
/// ("In sync: no writes waiting, and every node, edge and document
/// matches"); the count-comparing 1.0.7 said "OK: counts match", which is
/// accepted too so the base run still means something. "Drift" or
/// "Differs" anywhere means not clean.
pub fn status_says_clean(o: &Out) -> bool {
    let all = o.all();
    o.ok()
        && (o.stdout.contains("In sync") || o.stdout.contains("OK"))
        && !all.contains("Drift")
        && !all.contains("Differs")
}
