//! CLI writes reach the shared server through a local operation log.
//!
//! These drive the real `deciduous` binary in real git repositories. The
//! tests marked `#[ignore]` also need a real deciduous server; run them with
//!
//! ```sh
//! DECIDUOUS_TEST_SERVER=http://127.0.0.1:4814 DECIDUOUS_TEST_TOKEN=... \
//!     cargo test --test remote_oplog -- --include-ignored
//! ```
//!
//! They panic, rather than pass, when those variables are missing: a test
//! that quietly does nothing without its server is worse than no test.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

const NEEDS_SERVER: &str =
    "needs a real server: set DECIDUOUS_TEST_SERVER and DECIDUOUS_TEST_TOKEN, run with --include-ignored";

fn server() -> (String, String) {
    let url = std::env::var("DECIDUOUS_TEST_SERVER").unwrap_or_else(|_| panic!("{NEEDS_SERVER}"));
    let token = std::env::var("DECIDUOUS_TEST_TOKEN").unwrap_or_else(|_| panic!("{NEEDS_SERVER}"));
    (url.trim_end_matches('/').to_string(), token)
}

/// A workspace name no other run has used, so reruns against one long-lived
/// server never see each other's rows.
fn unique(prefix: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{prefix}-{}-{nanos}", std::process::id())
}

/// One sandbox per test: its own HOME, so no stored credential or config of
/// the person running the tests is read or written.
struct Sandbox {
    root: TempDir,
    token: String,
}

impl Sandbox {
    fn new(token: &str) -> Self {
        let root = TempDir::new().unwrap();
        std::fs::create_dir_all(root.path().join("home")).unwrap();
        Sandbox {
            root,
            token: token.to_string(),
        }
    }

    fn path(&self) -> &Path {
        self.root.path()
    }

    fn git(&self, dir: &Path, args: &[&str]) -> Output {
        let out = Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("HOME", self.path().join("home"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        out
    }

    fn dx(&self, dir: &Path, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_deciduous"))
            .args(args)
            .current_dir(dir)
            .env("HOME", self.path().join("home"))
            .env("XDG_CONFIG_HOME", self.path().join("home").join(".config"))
            .env("DECIDUOUS_MCP_TOKEN", &self.token)
            .env("DECIDUOUS_NO_SERVER", "1")
            .env_remove("DECIDUOUS_DB_PATH")
            .env("NO_COLOR", "1")
            .output()
            .unwrap()
    }

    fn dx_ok(&self, dir: &Path, args: &[&str]) -> String {
        let out = self.dx(dir, args);
        assert!(
            out.status.success(),
            "deciduous {args:?} failed\nstdout: {}\nstderr: {}",
            text(&out.stdout),
            text(&out.stderr)
        );
        format!("{}{}", text(&out.stdout), text(&out.stderr))
    }

    /// A git repository with one commit and `deciduous init` run in it.
    fn repo(&self, rel: &str) -> PathBuf {
        let dir = self.path().join(rel);
        std::fs::create_dir_all(&dir).unwrap();
        self.git(&dir, &["init", "-q", "-b", "main"]);
        // The directory in the message: two empty commits with the same
        // message in the same second are the same commit, and root commits
        // are what tells repositories apart.
        self.git(
            &dir,
            &[
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                &format!("init {rel}"),
            ],
        );
        self.dx_ok(&dir, &["init"]);
        dir
    }

    /// A repository pointed at the test server under its own workspace.
    fn remote_repo(&self, rel: &str, url: &str, workspace: &str) -> PathBuf {
        let dir = self.repo(rel);
        self.dx_ok(&dir, &["remote", "init", url, "--workspace", workspace]);
        dir
    }
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).to_string()
}

fn export(url: &str, token: &str, workspace: &str) -> Value {
    ureq::get(&format!("{url}/export?workspace={workspace}"))
        .set("authorization", &format!("Bearer {token}"))
        .call()
        .unwrap()
        .into_json()
        .unwrap()
}

fn server_node<'a>(graph: &'a Value, title: &str) -> &'a Value {
    graph["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["title"] == title)
        .unwrap_or_else(|| panic!("no node titled {title:?} on the server: {graph}"))
}

fn local_change_id(sb: &Sandbox, dir: &Path, id: i64) -> String {
    let g: Value = serde_json::from_str(&sb.dx_ok(dir, &["graph"])).unwrap();
    g["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == id)
        .unwrap()["change_id"]
        .as_str()
        .unwrap()
        .to_string()
}

/// MCP tool calls in one session, the way an agent makes them: initialize,
/// then call. One session, because the server's write lock is per session:
/// a second session writing the same branch a moment later is refused.
fn mcp(url: &str, token: &str, workspace: &str, calls: &[(&str, Value)]) {
    let post = |sid: Option<&str>, body: Value| {
        let mut req = ureq::post(&format!("{url}/mcp"))
            .set("authorization", &format!("Bearer {token}"))
            .set("accept", "application/json, text/event-stream")
            .set("x-deciduous-workspace", workspace);
        if let Some(s) = sid {
            req = req.set("mcp-session-id", s);
        }
        req.send_json(body).unwrap()
    };
    let init = post(
        None,
        serde_json::json!({"jsonrpc":"2.0","id":0,"method":"initialize","params":{
            "protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}),
    );
    let sid = init.header("mcp-session-id").unwrap().to_string();
    post(
        Some(&sid),
        serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    );
    for (i, (tool, args)) in calls.iter().enumerate() {
        let resp = post(
            Some(&sid),
            serde_json::json!({"jsonrpc":"2.0","id":i + 1,"method":"tools/call","params":{"name":tool,"arguments":args}}),
        )
        .into_string()
        .unwrap();
        let body = if resp.trim_start().starts_with('{') {
            resp
        } else {
            resp.lines()
                .filter_map(|l| l.strip_prefix("data:"))
                .collect::<Vec<_>>()
                .join("\n")
        };
        let v: Value = serde_json::from_str(&body).unwrap();
        assert!(
            v.get("error").is_none() && v["result"]["isError"] != true,
            "{tool} failed on the server: {v}"
        );
    }
}

fn set_remote_url(dir: &Path, url: &str) {
    let path = dir.join(".deciduous").join("config.toml");
    let text = std::fs::read_to_string(&path).unwrap();
    let mut doc: toml_edit::DocumentMut = text.parse().unwrap();
    doc["remote"]["url"] = toml_edit::value(url);
    std::fs::write(&path, doc.to_string()).unwrap();
}

fn log_lines(dir: &Path) -> Vec<Value> {
    let path = dir.join(".deciduous").join("remote-log.jsonl");
    std::fs::read_to_string(&path)
        .unwrap_or_default()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

/// A port nothing listens on: bound, then released.
fn dead_url() -> String {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    drop(l);
    format!("http://127.0.0.1:{port}")
}

// ---------------------------------------------------------------------------
// C1: a CLI edit changes the fields it names and nothing else.
// ---------------------------------------------------------------------------

#[test]
#[ignore = "needs a real server: set DECIDUOUS_TEST_SERVER and DECIDUOUS_TEST_TOKEN"]
fn status_and_link_do_not_overwrite_an_agents_edit() {
    let (url, token) = server();
    let sb = Sandbox::new(&token);
    let ws = unique("wal-c1");
    let dir = sb.remote_repo("c1", &url, &ws);

    sb.dx_ok(&dir, &["add", "goal", "a1"]);
    sb.dx_ok(&dir, &["add", "goal", "g1"]);

    let g = export(&url, &token, &ws);
    let a1 = server_node(&g, "a1")["id"].as_str().unwrap().to_string();
    let g1 = server_node(&g, "g1")["id"].as_str().unwrap().to_string();

    mcp(
        &url,
        &token,
        &ws,
        &[
            (
                "update_node",
                serde_json::json!({"node_id": a1, "title": "a1 retitled by agent", "description": "agent detail"}),
            ),
            (
                "update_node",
                serde_json::json!({"node_id": g1, "title": "g1 retitled by agent", "status": "completed"}),
            ),
        ],
    );

    sb.dx_ok(&dir, &["status", "1", "completed"]);
    sb.dx_ok(&dir, &["link", "2", "1", "-r", "because"]);

    let g = export(&url, &token, &ws);
    let a1 = server_node(&g, "a1 retitled by agent");
    assert_eq!(a1["status"], "completed", "the CLI's status edit arrived");
    assert_eq!(
        a1["description"], "agent detail",
        "the agent's description survived"
    );
    let g1 = server_node(&g, "g1 retitled by agent");
    assert_eq!(g1["status"], "completed", "link did not resend g1");
    assert_eq!(g.get("edges").unwrap().as_array().unwrap().len(), 1);
}

// ---------------------------------------------------------------------------
// C2: an edit made while the server is down is queued and replayed.
// ---------------------------------------------------------------------------

#[test]
#[ignore = "needs a real server: set DECIDUOUS_TEST_SERVER and DECIDUOUS_TEST_TOKEN"]
fn an_offline_edit_reaches_the_server_on_push() {
    let (url, token) = server();
    let sb = Sandbox::new(&token);
    let ws = unique("wal-c2");
    let dir = sb.remote_repo("c2", &url, &ws);

    sb.dx_ok(&dir, &["add", "goal", "offline goal"]);

    set_remote_url(&dir, &dead_url());
    let out = sb.dx_ok(&dir, &["status", "1", "completed"]);
    assert!(
        out.contains("queued"),
        "the user is told the write waits: {out}"
    );

    set_remote_url(&dir, &url);
    let out = sb.dx_ok(&dir, &["remote", "push"]);
    assert!(
        out.contains("1 applied"),
        "push replays the queued op: {out}"
    );

    let g = export(&url, &token, &ws);
    assert_eq!(server_node(&g, "offline goal")["status"], "completed");

    let out = sb.dx_ok(&dir, &["remote", "push"]);
    assert!(out.contains("Nothing to push"), "{out}");
}

// ---------------------------------------------------------------------------
// C3: archaeology writes go through the same log as every other write.
// ---------------------------------------------------------------------------

#[test]
#[ignore = "needs a real server: set DECIDUOUS_TEST_SERVER and DECIDUOUS_TEST_TOKEN"]
fn archaeology_pivot_and_supersede_reach_the_server() {
    let (url, token) = server();
    let sb = Sandbox::new(&token);
    let ws = unique("wal-c3");
    let dir = sb.remote_repo("c3", &url, &ws);

    sb.dx_ok(&dir, &["add", "decision", "old approach"]);
    sb.dx_ok(&dir, &["add", "decision", "side approach"]);
    sb.dx_ok(
        &dir,
        &["archaeology", "pivot", "1", "it was slow", "new approach"],
    );
    sb.dx_ok(&dir, &["archaeology", "supersede", "2"]);

    let local: Value = serde_json::from_str(&sb.dx_ok(&dir, &["graph"])).unwrap();
    let g = export(&url, &token, &ws);
    assert_eq!(
        g["nodes"].as_array().unwrap().len(),
        local["nodes"].as_array().unwrap().len(),
        "server: {g}"
    );
    assert_eq!(
        g["edges"].as_array().unwrap().len(),
        local["edges"].as_array().unwrap().len()
    );
    assert_eq!(server_node(&g, "old approach")["status"], "superseded");
    assert_eq!(server_node(&g, "side approach")["status"], "superseded");
}

// ---------------------------------------------------------------------------
// Deletes are ops too: removing a node or an edge reaches the server.
// ---------------------------------------------------------------------------

#[test]
#[ignore = "needs a real server: set DECIDUOUS_TEST_SERVER and DECIDUOUS_TEST_TOKEN"]
fn delete_and_unlink_reach_the_server() {
    let (url, token) = server();
    let sb = Sandbox::new(&token);
    let ws = unique("wal-del");
    let dir = sb.remote_repo("del", &url, &ws);

    sb.dx_ok(&dir, &["add", "goal", "keep"]);
    sb.dx_ok(&dir, &["add", "option", "doomed"]);
    sb.dx_ok(&dir, &["add", "option", "unlinked"]);
    sb.dx_ok(&dir, &["link", "1", "2"]);
    sb.dx_ok(&dir, &["link", "1", "3"]);

    let out = sb.dx_ok(&dir, &["unlink", "1", "3"]);
    assert!(!out.contains("removed locally only"), "{out}");
    sb.dx_ok(&dir, &["delete", "2"]);

    let g = export(&url, &token, &ws);
    let live: Vec<&str> = g["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|n| n["deleted_at"].is_null())
        .map(|n| n["title"].as_str().unwrap())
        .collect();
    assert!(
        !live.contains(&"doomed"),
        "server still has the deleted node: {g}"
    );
    assert!(g["edges"].as_array().unwrap().is_empty(), "{g}");
}

// ---------------------------------------------------------------------------
// The log itself, without a server.
// ---------------------------------------------------------------------------

#[test]
fn writes_queue_in_an_ignored_log_while_the_server_is_unreachable() {
    let sb = Sandbox::new("0123456789abcdef0123456789abcdef");
    let dir = sb.repo("offline");
    // Written by hand: `remote init` refuses a server it cannot reach.
    std::fs::write(
        dir.join(".deciduous").join("config.toml"),
        format!(
            "[remote]\nurl = \"{}\"\nworkspace = \"offline\"\n",
            dead_url()
        ),
    )
    .unwrap();

    let out = sb.dx_ok(&dir, &["add", "goal", "on a plane"]);
    assert!(out.contains("queued"), "{out}");
    sb.dx_ok(&dir, &["status", "1", "completed"]);
    sb.dx_ok(&dir, &["prompt", "1", "the exact words"]);

    let lines = log_lines(&dir);
    let kinds: Vec<&str> = lines.iter().filter_map(|l| l["kind"].as_str()).collect();
    assert_eq!(
        kinds,
        ["create_node", "update_node", "update_node"],
        "{lines:?}"
    );
    let cid = local_change_id(&sb, &dir, 1);
    assert!(lines.iter().all(|l| l["change_id"] == cid.as_str()));
    // Field level: the status op carries the status and nothing else.
    assert_eq!(lines[1]["set"], serde_json::json!({"status": "completed"}));
    assert_eq!(
        lines[2]["metadata"],
        serde_json::json!({"prompt": "the exact words"})
    );
    let ids: std::collections::HashSet<&str> =
        lines.iter().map(|l| l["op_id"].as_str().unwrap()).collect();
    assert_eq!(ids.len(), 3, "every op has its own id");

    let ignored = sb.git(&dir, &["check-ignore", ".deciduous/remote-log.jsonl"]);
    assert!(text(&ignored.stdout).contains("remote-log.jsonl"));
}

#[test]
fn a_project_without_a_remote_keeps_no_log() {
    let sb = Sandbox::new("0123456789abcdef0123456789abcdef");
    let dir = sb.repo("local-only");
    sb.dx_ok(&dir, &["add", "goal", "local"]);
    assert!(!dir.join(".deciduous").join("remote-log.jsonl").exists());
}

// ---------------------------------------------------------------------------
// C9: remote status compares content and reports the queue, not counts.
// ---------------------------------------------------------------------------

#[test]
#[ignore = "needs a real server: set DECIDUOUS_TEST_SERVER and DECIDUOUS_TEST_TOKEN"]
fn status_reports_waiting_writes_and_content_differences_when_counts_match() {
    let (url, token) = server();
    let sb = Sandbox::new(&token);
    let ws = unique("wal-c9");
    let dir = sb.remote_repo("c9", &url, &ws);

    sb.dx_ok(&dir, &["add", "goal", "shared goal"]);

    // Bob writes while the server is unreachable; an agent writes a different
    // node and changes the shared one. 2 nodes here, 2 on the server.
    set_remote_url(&dir, &dead_url());
    sb.dx_ok(&dir, &["add", "goal", "bobs unpushed goal"]);
    set_remote_url(&dir, &url);

    let g = export(&url, &token, &ws);
    let shared = server_node(&g, "shared goal")["id"]
        .as_str()
        .unwrap()
        .to_string();
    mcp(
        &url,
        &token,
        &ws,
        &[
            (
                "add_node",
                serde_json::json!({"node_type": "goal", "title": "agents goal"}),
            ),
            (
                "update_node",
                serde_json::json!({"node_id": shared, "status": "completed"}),
            ),
        ],
    );

    let st = sb.dx(&dir, &["remote", "status"]);
    assert_eq!(st.status.code(), Some(1), "drift exits 1");
    let out = text(&st.stdout);
    assert!(!out.contains("OK"), "counts match, content does not: {out}");
    assert!(out.contains("1 write(s) waiting"), "{out}");
    assert!(out.contains("bobs unpushed goal"), "{out}");
    assert!(out.contains("agents goal"), "{out}");
    assert!(
        out.contains("status") && out.contains("completed"),
        "the differing field is named: {out}"
    );

    // Once the queue is sent and the server's side pulled, nothing differs.
    sb.dx_ok(&dir, &["remote", "pull"]);
    let out = sb.dx_ok(&dir, &["remote", "status"]);
    assert!(out.contains("In sync"), "{out}");
}

fn config_workspace(dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(dir.join(".deciduous").join("config.toml")).ok()?;
    let doc: toml::Value = toml::from_str(&text).ok()?;
    doc.get("remote")?
        .get("workspace")?
        .as_str()
        .map(str::to_string)
}

fn live_titles(g: &Value) -> Vec<String> {
    let mut t: Vec<String> = g["nodes"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter(|n| n["deleted_at"].is_null())
                .filter_map(|n| n["title"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    t.sort();
    t
}

// ---------------------------------------------------------------------------
// C6: the workspace is recorded at remote init; a rename or a clone under
// another name keeps writing to the same graph.
// ---------------------------------------------------------------------------

#[test]
#[ignore = "needs a real server: set DECIDUOUS_TEST_SERVER and DECIDUOUS_TEST_TOKEN"]
fn renaming_or_cloning_the_repository_keeps_its_workspace() {
    let (url, token) = server();
    let sb = Sandbox::new(&token);
    let name = unique("wal-c6");
    let dir = sb.repo(&name);
    sb.dx_ok(&dir, &["remote", "init", &url]);
    sb.dx_ok(&dir, &["add", "goal", "before the move"]);

    let moved = sb.path().join(format!("{name}-renamed"));
    std::fs::rename(&dir, &moved).unwrap();
    sb.dx_ok(&moved, &["add", "goal", "after the move"]);

    // The shared config travels with the code, so a clone under another
    // name writes to the same workspace too.
    sb.git(&moved, &["add", ".deciduous/config.toml"]);
    sb.git(&moved, &["commit", "-q", "-m", "deciduous config"]);
    let clone = sb.path().join(format!("{name}-clone"));
    sb.git(
        sb.path(),
        &[
            "clone",
            "-q",
            moved.to_str().unwrap(),
            clone.to_str().unwrap(),
        ],
    );
    sb.dx_ok(&clone, &["add", "goal", "from the clone"]);

    assert_eq!(
        live_titles(&export(&url, &token, &name)),
        ["after the move", "before the move", "from the clone"]
    );
    assert!(live_titles(&export(&url, &token, &format!("{name}-renamed"))).is_empty());
    assert!(live_titles(&export(&url, &token, &format!("{name}-clone"))).is_empty());
    assert_eq!(config_workspace(&clone).as_deref(), Some(name.as_str()));
}

// ---------------------------------------------------------------------------
// C7: a linked worktree resolves to the main repository's workspace, which
// is what the server's instructions tell agents to use.
// ---------------------------------------------------------------------------

#[test]
fn a_worktree_uses_the_main_repositorys_workspace() {
    let sb = Sandbox::new("0123456789abcdef0123456789abcdef");
    let main = sb.repo("wt-main");
    // A 1.0.7 config: URL only, workspace derived on every call. The server
    // is asked which workspace holds this project's nodes (none here) before
    // the derived name is recorded, so it has to answer.
    let url = stub_server(serde_json::json!({"nodes": [], "edges": [], "documents": []}));
    std::fs::write(
        main.join(".deciduous").join("config.toml"),
        format!("[remote]\nurl = \"{url}\"\n"),
    )
    .unwrap();
    sb.git(&main, &["add", ".deciduous/config.toml"]);
    sb.git(&main, &["commit", "-q", "-m", "config"]);
    sb.git(
        &main,
        &[
            "worktree",
            "add",
            "-q",
            "../wt-main-feature",
            "-b",
            "feature",
        ],
    );
    let wt = sb.path().join("wt-main-feature");

    let out = sb.dx_ok(&wt, &["add", "goal", "from the worktree"]);
    assert_eq!(
        config_workspace(&wt).as_deref(),
        Some("wt-main"),
        "worktree resolved to its own directory name: {out}"
    );
    assert!(
        out.contains("wt-main"),
        "the recorded name is announced: {out}"
    );
}

// ---------------------------------------------------------------------------
// C5: two repositories with the same directory name do not share a
// workspace by accident.
// ---------------------------------------------------------------------------

#[test]
#[ignore = "needs a real server: set DECIDUOUS_TEST_SERVER and DECIDUOUS_TEST_TOKEN"]
fn same_named_repositories_do_not_share_a_workspace_unless_named() {
    let (url, token) = server();
    let sb = Sandbox::new(&token);
    let name = unique("wal-c5");

    let a = sb.repo(&format!("a/{name}"));
    sb.dx_ok(&a, &["remote", "init", &url]);
    sb.dx_ok(&a, &["add", "goal", "a's goal"]);

    // Same name, unrelated history: refused, and nothing is written.
    let b = sb.repo(&format!("b/{name}"));
    let out = sb.dx(&b, &["remote", "init", &url]);
    let err = text(&out.stderr);
    assert!(!out.status.success(), "{}{err}", text(&out.stdout));
    assert!(err.contains("another repository"), "{err}");
    assert!(err.contains("--workspace"), "the fix is named: {err}");
    assert_eq!(config_workspace(&b), None);

    // A case-only difference derives the same name and is refused too.
    let c = sb.repo(&format!("c/{}", name.to_uppercase()));
    assert!(!sb.dx(&c, &["remote", "init", &url]).status.success());

    // A 1.0.7 config (URL only) in B: its writes are refused loudly and
    // stay queued; a pull refuses rather than import A's graph.
    std::fs::write(
        b.join(".deciduous").join("config.toml"),
        format!("[remote]\nurl = \"{url}\"\n"),
    )
    .unwrap();
    let out = sb.dx_ok(&b, &["add", "goal", "b's goal"]);
    assert!(out.contains("another repository"), "{out}");
    assert!(out.contains("refused"), "{out}");
    let pull = sb.dx(&b, &["remote", "pull"]);
    assert!(!pull.status.success());
    let local: Value = serde_json::from_str(&sb.dx_ok(&b, &["graph"])).unwrap();
    assert_eq!(live_titles(&local), ["b's goal"]);

    // Naming the workspace is how two repositories share one on purpose.
    let d = sb.repo(&format!("d/{name}"));
    sb.dx_ok(&d, &["remote", "init", &url, "--workspace", &name]);
    sb.dx_ok(&d, &["add", "goal", "d's goal"]);

    assert_eq!(
        live_titles(&export(&url, &token, &name)),
        ["a's goal", "d's goal"]
    );
}

// ---------------------------------------------------------------------------
// C8: a repository whose name is not ASCII can use a remote.
// ---------------------------------------------------------------------------

#[test]
#[ignore = "needs a real server: set DECIDUOUS_TEST_SERVER and DECIDUOUS_TEST_TOKEN"]
fn a_non_ascii_repository_name_reaches_the_server() {
    let (url, token) = server();
    let sb = Sandbox::new(&token);
    let name = format!("{}-café-ünï", unique("wal-c8"));
    let dir = sb.repo(&name);

    let out = sb.dx_ok(&dir, &["remote", "init", &url]);
    assert!(out.contains(&name), "{out}");
    sb.dx_ok(&dir, &["add", "goal", "accented"]);
    let out = sb.dx_ok(&dir, &["remote", "status"]);
    assert!(out.contains("In sync"), "{out}");

    // Encoded here the way a browser would: UTF-8 bytes.
    let encoded: String = name
        .bytes()
        .map(|b| match b {
            b'a'..=b'z' | b'0'..=b'9' | b'-' => (b as char).to_string(),
            b => format!("%{b:02X}"),
        })
        .collect();
    assert_eq!(live_titles(&export(&url, &token, &encoded)), ["accented"]);
}

// ---------------------------------------------------------------------------
// C4 (the pull half): a node an agent deleted on the server leaves the
// local graph on the next pull. The server half, /export carrying the
// tombstone, is chapter 21's; this stands in for it with a stub that sends
// the shape that chapter emits: the row, in `nodes`, with `deleted_at` set.
// ---------------------------------------------------------------------------

/// A minimal HTTP server with the routes pull touches, on a free port.
fn stub_server(export: Value) -> String {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let port = server.server_addr().to_ip().unwrap().port();
    std::thread::spawn(move || {
        for mut req in server.incoming_requests() {
            let mut body = String::new();
            let _ = req.as_reader().read_to_string(&mut body);
            let path = req.url().split('?').next().unwrap_or("").to_string();
            let reply = match path.as_str() {
                "/health" => "ok".to_string(),
                "/claim" => r#"{"workspace":"stub","claim":"unchecked"}"#.to_string(),
                "/locate" => r#"{"workspaces":[]}"#.to_string(),
                "/ops" => {
                    let v: Value = serde_json::from_str(&body).unwrap();
                    let results: Vec<Value> = v["ops"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|op| serde_json::json!({"op_id": op["op_id"], "result": "applied"}))
                        .collect();
                    serde_json::json!({"workspace": "stub", "results": results}).to_string()
                }
                "/export" => export.to_string(),
                _ => {
                    let _ = req.respond(tiny_http::Response::empty(404));
                    continue;
                }
            };
            let _ = req.respond(tiny_http::Response::from_string(reply));
        }
    });
    format!("http://127.0.0.1:{port}")
}

#[test]
fn pull_removes_a_node_the_server_deleted() {
    let sb = Sandbox::new("0123456789abcdef0123456789abcdef");
    let dir = sb.repo("tomb");
    std::fs::write(
        dir.join(".deciduous").join("config.toml"),
        format!("[remote]\nurl = \"{}\"\nworkspace = \"tomb\"\n", dead_url()),
    )
    .unwrap();
    sb.dx_ok(&dir, &["add", "goal", "deleted by an agent"]);
    sb.dx_ok(&dir, &["add", "goal", "still here"]);
    let dead = local_change_id(&sb, &dir, 1);
    let kept = local_change_id(&sb, &dir, 2);

    let node = |cid: &str, title: &str, deleted_at: Value| {
        serde_json::json!({
            "id": format!("srv-{cid}"), "change_id": cid, "node_type": "goal", "title": title,
            "description": null, "status": "pending", "metadata": {"branch": "main"},
            "created_at": "2026-09-23T10:00:00Z", "updated_at": "2026-09-23T10:00:00Z",
            "deleted_at": deleted_at
        })
    };
    // Deleted after the local copy was last written, as it would be.
    let mut tombstone = node(
        &dead,
        "deleted by an agent",
        Value::String("2099-01-01T00:00:00Z".into()),
    );
    tombstone["updated_at"] = Value::String("2099-01-01T00:00:00Z".into());
    let url = stub_server(serde_json::json!({
        "nodes": [
            tombstone,
            node(&kept, "still here", Value::Null),
            node("agent-node-0001", "an agent's goal", Value::Null),
        ],
        "edges": [],
        "documents": []
    }));
    set_remote_url(&dir, &url);

    let out = sb.dx_ok(&dir, &["remote", "pull"]);
    let local: Value = serde_json::from_str(&sb.dx_ok(&dir, &["graph"])).unwrap();
    assert_eq!(
        live_titles(&local),
        ["an agent's goal", "still here"],
        "pull said: {out}"
    );
    assert!(out.contains("removed 1"), "the removal is reported: {out}");
    assert!(out.contains("imported 1"), "{out}");
}

// C4 (the push half): a node an agent deleted is not re-sent by later
// writes, and an edit to it is refused by the server loudly, not applied to
// the tombstone or silently dropped.
#[test]
#[ignore = "needs a real server: set DECIDUOUS_TEST_SERVER and DECIDUOUS_TEST_TOKEN"]
fn a_node_deleted_on_the_server_is_not_resent_and_edits_to_it_are_refused_loudly() {
    let (url, token) = server();
    let sb = Sandbox::new(&token);
    let ws = unique("wal-c4");
    let dir = sb.remote_repo("c4", &url, &ws);

    sb.dx_ok(&dir, &["add", "goal", "doomed by an agent"]);
    let g = export(&url, &token, &ws);
    let id = server_node(&g, "doomed by an agent")["id"]
        .as_str()
        .unwrap()
        .to_string();
    mcp(
        &url,
        &token,
        &ws,
        &[("delete_node", serde_json::json!({"node_id": id}))],
    );

    let out = sb.dx_ok(&dir, &["add", "goal", "unrelated"]);
    assert!(!out.contains("Pushed"), "{out}");
    assert_eq!(live_titles(&export(&url, &token, &ws)), ["unrelated"]);

    let out = sb.dx_ok(&dir, &["status", "1", "completed"]);
    assert!(out.contains("Rejected"), "{out}");
    assert!(out.contains("deleted on the server"), "{out}");
    let out = text(&sb.dx(&dir, &["remote", "status"]).stdout);
    assert!(out.contains("1 rejected"), "{out}");
    sb.dx_ok(&dir, &["remote", "push", "--drop-rejected"]);
    let out = text(&sb.dx(&dir, &["remote", "status"]).stdout);
    assert!(out.contains("0 rejected"), "{out}");
}

// ===========================================================================
// Findings from the verifiers of this chapter. Each test reproduces one.
// ===========================================================================

fn server_id(g: &Value, title: &str) -> String {
    server_node(g, title)["id"].as_str().unwrap().to_string()
}

// A queued op replayed after an agent changed the same field put back the
// older value: update_node applied `set` without looking at the row.
#[test]
#[ignore = "needs a real server: set DECIDUOUS_TEST_SERVER and DECIDUOUS_TEST_TOKEN"]
fn a_queued_edit_does_not_overwrite_a_newer_agent_edit_to_the_same_field() {
    let (url, token) = server();
    let sb = Sandbox::new(&token);
    let ws = unique("wal-stale");
    let dir = sb.remote_repo("stale", &url, &ws);

    sb.dx_ok(&dir, &["add", "goal", "same field"]);
    sb.dx_ok(&dir, &["add", "goal", "other field"]);

    set_remote_url(&dir, &dead_url());
    sb.dx_ok(&dir, &["status", "1", "completed"]);
    sb.dx_ok(&dir, &["status", "2", "completed"]);
    set_remote_url(&dir, &url);

    let g = export(&url, &token, &ws);
    mcp(
        &url,
        &token,
        &ws,
        &[
            (
                "update_node",
                serde_json::json!({"node_id": server_id(&g, "same field"), "status": "rejected"}),
            ),
            (
                "update_node",
                serde_json::json!({"node_id": server_id(&g, "other field"), "title": "other field, retitled"}),
            ),
        ],
    );

    // A push the server refused anything of exits 1 (round-2 BRIDGE-N10):
    // the refused write has not reached it.
    let raw = sb.dx(&dir, &["remote", "push"]);
    let out = format!("{}{}", text(&raw.stdout), text(&raw.stderr));
    assert!(!raw.status.success(), "a refused push exited 0: {out}");
    assert!(out.contains("1 applied"), "{out}");
    assert!(out.contains("Rejected"), "the conflict is reported: {out}");
    assert!(
        out.contains("changed on the server"),
        "the reason names the conflict: {out}"
    );

    let g = export(&url, &token, &ws);
    assert_eq!(
        server_node(&g, "same field")["status"],
        "rejected",
        "the agent's newer status survived"
    );
    assert_eq!(
        server_node(&g, "other field, retitled")["status"],
        "completed",
        "an edit to a different field still lands"
    );
}

fn export_with_tombstones(url: &str, token: &str, workspace: &str) -> Value {
    ureq::get(&format!("{url}/export?workspace={workspace}&tombstones=1"))
        .set("authorization", &format!("Bearer {token}"))
        .call()
        .unwrap()
        .into_json()
        .unwrap()
}

// `remote status` sent a node an agent had deleted to `push --seed`, which
// wrote into the soft-deleted row through /import and linked an edge to it;
// status still said "Only here" afterwards, so the advice looped.
#[test]
#[ignore = "needs a real server: set DECIDUOUS_TEST_SERVER and DECIDUOUS_TEST_TOKEN"]
fn a_node_the_server_deleted_is_not_seeded_back_and_pull_removes_it() {
    let (url, token) = server();
    let sb = Sandbox::new(&token);
    let ws = unique("wal-del3");
    let dir = sb.remote_repo("del3", &url, &ws);

    sb.dx_ok(&dir, &["add", "action", "a1"]);
    let g = export(&url, &token, &ws);
    mcp(
        &url,
        &token,
        &ws,
        &[(
            "delete_node",
            serde_json::json!({"node_id": server_id(&g, "a1")}),
        )],
    );
    sb.dx_ok(&dir, &["add", "goal", "g2"]);
    sb.dx(&dir, &["status", "1", "completed"]);
    sb.dx(&dir, &["link", "2", "1"]);
    sb.dx_ok(&dir, &["remote", "push", "--drop-rejected"]);

    let out = text(&sb.dx(&dir, &["remote", "status"]).stdout);
    assert!(
        !out.contains("push --seed"),
        "status does not send a deleted node to --seed: {out}"
    );
    assert!(out.contains("Deleted on the server"), "{out}");

    let before = export_with_tombstones(&url, &token, &ws);
    let out = sb.dx_ok(&dir, &["remote", "push", "--seed"]);
    let after = export_with_tombstones(&url, &token, &ws);
    let row = |g: &Value| {
        g["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| !n["deleted_at"].is_null())
            .cloned()
            .unwrap_or_else(|| panic!("no tombstone in {g}"))
    };
    assert_eq!(
        row(&before),
        row(&after),
        "seed wrote into the deleted row: {out}"
    );
    assert!(
        after["edges"].as_array().unwrap().is_empty(),
        "an edge to the deleted node was written: {out}"
    );

    let out = sb.dx_ok(&dir, &["remote", "pull"]);
    assert!(out.contains("removed 1"), "{out}");
    let out = sb.dx_ok(&dir, &["remote", "status"]);
    assert!(out.contains("In sync"), "{out}");
}

// The pull report counted a node as "updated" when only the spelling of
// its timestamp differed (-04:00 here, Z on the server), or when the server's
// row was newer with the same content.
#[test]
#[ignore = "needs a real server: set DECIDUOUS_TEST_SERVER and DECIDUOUS_TEST_TOKEN"]
fn a_pull_that_changes_no_content_reports_no_updates() {
    let (url, token) = server();
    let sb = Sandbox::new(&token);
    let ws = unique("wal-pullcount");
    let dir = sb.remote_repo("pullcount", &url, &ws);

    sb.dx_ok(&dir, &["add", "goal", "one"]);
    sb.dx_ok(&dir, &["add", "goal", "two"]);
    sb.dx_ok(&dir, &["status", "2", "completed"]);
    sb.dx_ok(&dir, &["link", "1", "2"]);
    let out = sb.dx_ok(&dir, &["remote", "status"]);
    assert!(out.contains("In sync"), "{out}");

    let out = sb.dx_ok(&dir, &["remote", "pull"]);
    assert!(
        out.contains("imported 0 new node(s), updated 0, removed 0"),
        "{out}"
    );

    // A real change is still counted.
    let g = export(&url, &token, &ws);
    mcp(
        &url,
        &token,
        &ws,
        &[(
            "update_node",
            serde_json::json!({"node_id": server_id(&g, "one"), "title": "one, retitled"}),
        )],
    );
    let out = sb.dx_ok(&dir, &["remote", "pull"]);
    assert!(out.contains("updated 1,"), "{out}");
}

// An edit whose op never reached the log (written before this clone had a
// [remote], or the log could not be written) showed as "Different" in
// status, and neither push, --seed nor pull could send it.
#[test]
#[ignore = "needs a real server: set DECIDUOUS_TEST_SERVER and DECIDUOUS_TEST_TOKEN"]
fn an_edit_the_log_never_got_is_sent_by_push_repair() {
    let (url, token) = server();
    let sb = Sandbox::new(&token);
    let ws = unique("wal-repair");
    let dir = sb.remote_repo("repair", &url, &ws);
    sb.dx_ok(&dir, &["add", "goal", "shared goal"]);
    sb.dx_ok(&dir, &["prompt", "1", "first words"]);

    // The edit happens with no [remote] in the config: no op is logged.
    let cfg = dir.join(".deciduous").join("config.toml");
    let saved = std::fs::read_to_string(&cfg).unwrap();
    std::fs::write(&cfg, "").unwrap();
    sb.dx_ok(&dir, &["status", "1", "completed"]);
    sb.dx_ok(&dir, &["prompt", "1", "second words"]);
    std::fs::write(&cfg, saved).unwrap();

    let st = sb.dx(&dir, &["remote", "status"]);
    let out = text(&st.stdout);
    assert!(out.contains("Different"), "{out}");
    assert!(
        out.contains("push --repair"),
        "status names the command that fixes it: {out}"
    );

    let out = sb.dx_ok(&dir, &["remote", "push", "--repair"]);
    assert!(out.contains("applied"), "{out}");
    let g = export(&url, &token, &ws);
    let n = server_node(&g, "shared goal");
    assert_eq!(n["status"], "completed");
    assert_eq!(n["metadata"]["prompt"], "second words");

    let out = sb.dx_ok(&dir, &["remote", "status"]);
    assert!(out.contains("In sync"), "{out}");
}

// `remote status` exited 0 whatever it found, so a script could not tell
// drift from a clean state.
#[test]
#[ignore = "needs a real server: set DECIDUOUS_TEST_SERVER and DECIDUOUS_TEST_TOKEN"]
fn remote_status_exits_nonzero_when_anything_differs() {
    let (url, token) = server();
    let sb = Sandbox::new(&token);
    let ws = unique("wal-rc");
    let dir = sb.remote_repo("rc", &url, &ws);
    sb.dx_ok(&dir, &["add", "goal", "g"]);
    assert!(sb.dx(&dir, &["remote", "status"]).status.success());

    let g = export(&url, &token, &ws);
    mcp(
        &url,
        &token,
        &ws,
        &[(
            "update_node",
            serde_json::json!({"node_id": server_id(&g, "g"), "status": "completed"}),
        )],
    );
    let st = sb.dx(&dir, &["remote", "status"]);
    assert_eq!(st.status.code(), Some(1), "{}", text(&st.stdout));
}

// A document attached here never reached the server, and status said
// "In sync"; --seed said "Nothing to seed" because it only looked at nodes
// and edges.
#[test]
#[ignore = "needs a real server: set DECIDUOUS_TEST_SERVER and DECIDUOUS_TEST_TOKEN"]
fn an_attached_document_is_reported_and_seeded_with_its_bytes() {
    let (url, token) = server();
    let sb = Sandbox::new(&token);
    let ws = unique("wal-doc");
    let dir = sb.remote_repo("doc", &url, &ws);
    sb.dx_ok(&dir, &["add", "goal", "has a spec"]);
    std::fs::write(dir.join("spec.txt"), "the spec, in full\n").unwrap();
    // An attach is an op now and reaches the server by itself (round-2
    // BRIDGE-N4). One made with no [remote] (before `remote init`, or by
    // 1.0.7) is what status reports and --seed sends.
    let cfg = dir.join(".deciduous").join("config.toml");
    let saved = std::fs::read_to_string(&cfg).unwrap();
    std::fs::write(&cfg, "").unwrap();
    sb.dx_ok(&dir, &["doc", "attach", "1", "spec.txt"]);
    std::fs::write(&cfg, saved).unwrap();

    let out = text(&sb.dx(&dir, &["remote", "status"]).stdout);
    assert!(!out.contains("In sync"), "{out}");
    assert!(out.contains("spec.txt"), "the document is named: {out}");

    let out = sb.dx_ok(&dir, &["remote", "push", "--seed"]);
    assert!(out.contains("documents 1"), "{out}");
    let g = export(&url, &token, &ws);
    let docs = g["documents"].as_array().unwrap();
    assert_eq!(docs.len(), 1, "{g}");
    let id = docs[0]["id"].as_str().unwrap();
    let bytes = ureq::get(&format!("{url}/documents/{id}"))
        .set("authorization", &format!("Bearer {token}"))
        .call()
        .unwrap()
        .into_string()
        .unwrap();
    assert_eq!(bytes, "the spec, in full\n");

    let out = sb.dx_ok(&dir, &["remote", "status"]);
    assert!(out.contains("In sync"), "{out}");
    assert!(
        out.to_lowercase().contains("themes and tags"),
        "what is not compared is said: {out}"
    );
}

// `deciduous mcp`, the stdio server agents use, returned before the replay
// guard existed: its writes were logged and never sent, and nothing said so.
#[test]
#[ignore = "needs a real server: set DECIDUOUS_TEST_SERVER and DECIDUOUS_TEST_TOKEN"]
fn writes_through_the_local_mcp_server_reach_the_server_while_it_runs() {
    use std::io::{BufRead, BufReader, Write};
    let (url, token) = server();
    let sb = Sandbox::new(&token);
    let ws = unique("wal-localmcp");
    let dir = sb.remote_repo("localmcp", &url, &ws);

    let mut child = Command::new(env!("CARGO_BIN_EXE_deciduous"))
        .arg("mcp")
        .current_dir(&dir)
        .env("HOME", sb.path().join("home"))
        .env("XDG_CONFIG_HOME", sb.path().join("home").join(".config"))
        .env("DECIDUOUS_MCP_TOKEN", &token)
        .env("DECIDUOUS_NO_SERVER", "1")
        .env_remove("DECIDUOUS_DB_PATH")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut call = |id: u64, method: &str, params: Value| {
        let msg = serde_json::json!({"jsonrpc":"2.0","id":id,"method":method,"params":params});
        writeln!(stdin, "{msg}").unwrap();
        stdin.flush().unwrap();
        let mut line = String::new();
        stdout.read_line(&mut line).unwrap();
        line
    };
    call(
        1,
        "initialize",
        serde_json::json!({"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"0"}}),
    );
    call(
        2,
        "tools/call",
        serde_json::json!({"name":"add_node","arguments":{"node_type":"goal","title":"via local mcp"}}),
    );
    call(
        3,
        "tools/call",
        serde_json::json!({"name":"update_status","arguments":{"node_id":1,"status":"completed"}}),
    );

    // The process is still running: nothing has exited to trigger a replay.
    // The replay follows each answer, so it is waited for, briefly.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let status = loop {
        let g = export(&url, &token, &ws);
        let status = g["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["title"] == "via local mcp")
            .map(|n| n["status"].clone());
        if status.as_ref().is_some_and(|s| s == "completed") || std::time::Instant::now() > deadline
        {
            break status;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    };
    let _ = child.kill();
    let _ = child.wait();
    assert_eq!(status, Some(Value::String("completed".into())));
}

/// A repository with no commit yet: `git init` and `deciduous init` only.
fn unborn_repo(sb: &Sandbox, rel: &str) -> PathBuf {
    let dir = sb.path().join(rel);
    std::fs::create_dir_all(&dir).unwrap();
    sb.git(&dir, &["init", "-q", "-b", "main"]);
    sb.dx_ok(&dir, &["init"]);
    dir
}

// C5 bypasses: a repository with no commit sent no roots and was let in;
// /import and /export checked no claim at all, so `push --seed` from an
// unrelated repository wrote into another project's workspace. And a
// refusal was reported as the server being unreachable.
#[test]
#[ignore = "needs a real server: set DECIDUOUS_TEST_SERVER and DECIDUOUS_TEST_TOKEN"]
fn a_claimed_workspace_refuses_unborn_and_unrelated_repositories_on_every_path() {
    let (url, token) = server();
    let sb = Sandbox::new(&token);
    let name = unique("wal-c5b");

    let a = sb.repo(&format!("a/{name}"));
    sb.dx_ok(&a, &["remote", "init", &url]);
    sb.dx_ok(&a, &["add", "goal", "A's secret goal"]);

    // B: same name, no commit yet.
    let b = unborn_repo(&sb, &format!("b/{name}"));
    std::fs::write(
        b.join(".deciduous").join("config.toml"),
        format!("[remote]\nurl = \"{url}\"\nworkspace = \"{name}\"\n"),
    )
    .unwrap();
    let out = sb.dx_ok(&b, &["add", "goal", "B goal before first commit"]);
    assert!(out.contains("refused"), "{out}");
    assert!(
        !out.contains("once the server is reachable"),
        "a refusal is not an outage: {out}"
    );
    assert!(!sb.dx(&b, &["remote", "pull"]).status.success());
    let local: Value = serde_json::from_str(&sb.dx_ok(&b, &["graph"])).unwrap();
    assert_eq!(live_titles(&local), ["B goal before first commit"]);

    // C: same name, unrelated history, a 1.0.7 config; everything through
    // /import and /export.
    let c = sb.repo(&format!("c/{name}"));
    std::fs::write(c.join(".deciduous").join("config.toml"), "").unwrap();
    sb.dx_ok(&c, &["add", "goal", "C's private goal"]);
    std::fs::write(
        c.join(".deciduous").join("config.toml"),
        format!("[remote]\nurl = \"{url}\"\n"),
    )
    .unwrap();
    let seed = sb.dx(&c, &["remote", "push", "--seed"]);
    assert!(!seed.status.success(), "{}", text(&seed.stdout));
    assert!(text(&seed.stderr).contains("another repository"));
    let over = sb.dx(&c, &["remote", "push", "--overwrite"]);
    assert!(!over.status.success(), "{}", text(&over.stdout));
    let st = sb.dx(&c, &["remote", "status"]);
    assert!(!st.status.success());
    assert!(
        !text(&st.stdout).contains("A's secret goal"),
        "status showed another repository's graph: {}",
        text(&st.stdout)
    );

    assert_eq!(
        live_titles(&export(&url, &token, &name)),
        ["A's secret goal"]
    );
}

// A shallow clone (CI) answers `rev-list --max-parents=0` with its shallow
// boundary, not the root, so it was refused as "another repository".
#[test]
#[ignore = "needs a real server: set DECIDUOUS_TEST_SERVER and DECIDUOUS_TEST_TOKEN"]
fn a_shallow_clone_writes_to_its_repositorys_workspace() {
    let (url, token) = server();
    let sb = Sandbox::new(&token);
    let name = unique("wal-shal");
    let dir = sb.repo(&name);
    for i in 0..2 {
        sb.git(
            &dir,
            &["commit", "-q", "--allow-empty", "-m", &format!("c{i}")],
        );
    }
    sb.dx_ok(&dir, &["remote", "init", &url]);
    sb.dx_ok(&dir, &["add", "goal", "from the full clone"]);
    sb.git(&dir, &["add", ".deciduous/config.toml"]);
    sb.git(&dir, &["commit", "-q", "-m", "deciduous config"]);

    let ci = sb.path().join("ci").join(&name);
    std::fs::create_dir_all(ci.parent().unwrap()).unwrap();
    sb.git(
        sb.path(),
        &[
            "clone",
            "-q",
            "--depth",
            "1",
            &format!("file://{}", dir.display()),
            ci.to_str().unwrap(),
        ],
    );
    sb.dx_ok(&ci, &["init"]);
    let out = sb.dx_ok(&ci, &["add", "goal", "from ci"]);
    assert!(!out.contains("another repository"), "{out}");
    assert_eq!(
        live_titles(&export(&url, &token, &name)),
        ["from ci", "from the full clone"]
    );
}

// A repository written by 1.0.7 (URL-only config, workspace derived from
// the directory name on every call, pushed through /import with no roots)
// and then renamed: 1.0.8 recorded the new directory name and started a
// second, empty graph.
#[test]
#[ignore = "needs a real server: set DECIDUOUS_TEST_SERVER and DECIDUOUS_TEST_TOKEN"]
fn a_renamed_1_0_7_repository_keeps_writing_to_the_workspace_it_wrote_before() {
    let (url, token) = server();
    let sb = Sandbox::new(&token);
    let name = unique("wal-old");
    let dir = sb.repo(&name);
    std::fs::write(dir.join(".deciduous").join("config.toml"), "").unwrap();
    sb.dx_ok(&dir, &["add", "goal", "old1"]);

    // What 1.0.7's push did: the local graph through /import, no roots.
    let graph: Value = serde_json::from_str(&sb.dx_ok(&dir, &["graph"])).unwrap();
    ureq::post(&format!("{url}/import"))
        .set("authorization", &format!("Bearer {token}"))
        .send_json(serde_json::json!({"workspace": name, "graph": graph}))
        .unwrap();
    std::fs::write(
        dir.join(".deciduous").join("config.toml"),
        format!("[remote]\nurl = \"{url}\"\n"),
    )
    .unwrap();

    let moved = sb.path().join(format!("{name}-moved"));
    std::fs::rename(&dir, &moved).unwrap();
    let out = sb.dx_ok(&moved, &["add", "goal", "after the move"]);

    assert_eq!(
        config_workspace(&moved).as_deref(),
        Some(name.as_str()),
        "{out}"
    );
    assert_eq!(
        live_titles(&export(&url, &token, &name)),
        ["after the move", "old1"]
    );
    assert!(live_titles(&export(&url, &token, &format!("{name}-moved"))).is_empty());
}

// Two ops merging different metadata keys into one node at the same moment:
// ops.ex read the map, merged, and wrote it back without a row lock.
#[test]
#[ignore = "needs a real server: set DECIDUOUS_TEST_SERVER and DECIDUOUS_TEST_TOKEN"]
fn concurrent_metadata_ops_on_one_node_keep_every_key() {
    let (url, token) = server();
    let ws = unique("wal-race");
    let post = {
        let (url, token, ws) = (url.clone(), token.clone(), ws.clone());
        std::sync::Arc::new(move |ops: Value| -> Value {
            ureq::post(&format!("{url}/ops"))
                .set("authorization", &format!("Bearer {token}"))
                .send_json(serde_json::json!({"workspace": ws, "ops": ops}))
                .unwrap()
                .into_json()
                .unwrap()
        })
    };
    post(serde_json::json!([{
        "op_id": uuid::Uuid::new_v4().to_string(), "at": "2026-01-01T00:00:00Z",
        "kind": "create_node", "change_id": "race-node", "node_type": "goal",
        "title": "race", "status": "pending", "metadata": {},
        "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-01T00:00:00Z"
    }]));

    let n = 24;
    let handles: Vec<_> = (0..n)
        .map(|i| {
            let post = post.clone();
            std::thread::spawn(move || {
                post(serde_json::json!([{
                    "op_id": uuid::Uuid::new_v4().to_string(), "at": "2026-01-01T00:00:01Z",
                    "kind": "update_node", "change_id": "race-node",
                    "metadata": {format!("k{i}"): i},
                    "was_metadata": {format!("k{i}"): null}
                }]))
            })
        })
        .collect();
    for h in handles {
        let r = h.join().unwrap();
        assert_eq!(r["results"][0]["result"], "applied", "{r}");
    }
    let g = export(&url, &token, &ws);
    let meta = server_node(&g, "race")["metadata"]
        .as_object()
        .unwrap()
        .clone();
    let missing: Vec<usize> = (0..n)
        .filter(|i| !meta.contains_key(&format!("k{i}")))
        .collect();
    assert!(missing.is_empty(), "lost keys {missing:?}: {meta:?}");
}

// ---------------------------------------------------------------------------
// Chapter 28: the log survives crashes. Round-2 findings, one test each,
// named after the finding.
// ---------------------------------------------------------------------------

/// A repository whose remote is a port nothing listens on, written by hand
/// (`remote init` refuses a server it cannot reach): every write queues.
fn offline_repo(sb: &Sandbox, rel: &str) -> PathBuf {
    let dir = sb.repo(rel);
    std::fs::write(
        dir.join(".deciduous").join("config.toml"),
        format!(
            "[remote]\nurl = \"{}\"\nworkspace = \"{rel}\"\n",
            dead_url()
        ),
    )
    .unwrap();
    dir
}

fn log_path(dir: &Path) -> PathBuf {
    dir.join(".deciduous").join("remote-log.jsonl")
}

fn queued_titles(dir: &Path) -> Vec<String> {
    log_lines(dir)
        .iter()
        .filter(|l| l["kind"] == "create_node")
        .map(|l| l["title"].as_str().unwrap().to_string())
        .collect()
}

// RUST-N6 / BRIDGE-N5: a crash or ENOSPC mid-append leaves a last line with
// no newline. The next O_APPEND write landed on that same line, the whole
// line stopped parsing, and the fix the warning gave (delete that line)
// deleted the new write with it.
#[test]
fn rust_n6_a_torn_last_line_does_not_swallow_the_next_write() {
    let sb = Sandbox::new("0123456789abcdef0123456789abcdef");
    let dir = offline_repo(&sb, "torn");
    sb.dx_ok(&dir, &["add", "goal", "before-torn"]);
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(log_path(&dir))
        .unwrap();
    std::io::Write::write_all(&mut f, br#"{"entry":"op","op_id":"torn"#).unwrap();
    drop(f);

    let out = sb.dx_ok(&dir, &["add", "goal", "after-torn"]);
    // Every line of the log is a whole entry again, and the new write is one.
    assert_eq!(queued_titles(&dir), ["before-torn", "after-torn"], "{out}");
    // The fragment is kept where it can be read, not deleted, and said.
    let aside = std::fs::read_to_string(dir.join(".deciduous").join("remote-log.unreadable"))
        .unwrap_or_default();
    assert!(
        aside.contains(r#"{"entry":"op","op_id":"torn"#),
        "{aside:?}"
    );
    assert!(out.contains("remote-log.unreadable"), "{out}");
    // Nothing was refused by a server: none was reached.
    assert!(!out.contains("server refused"), "{out}");
    assert!(out.contains("2 write(s)"), "{out}");
}

// BRIDGE-N5, the log an older build already damaged: the torn fragment and
// the whole op after it on one line. The op is intact and is still a write
// that has not reached the server.
#[test]
fn bridge_n5_an_op_glued_to_a_torn_fragment_is_still_sent() {
    let sb = Sandbox::new("0123456789abcdef0123456789abcdef");
    let dir = offline_repo(&sb, "glued");
    sb.dx_ok(&dir, &["add", "goal", "glued-op"]);
    let whole = std::fs::read_to_string(log_path(&dir)).unwrap();
    std::fs::write(
        log_path(&dir),
        format!("{{\"entry\":\"op\",\"op_id\":\"torn{whole}"),
    )
    .unwrap();

    let out = sb.dx(&dir, &["remote", "push"]);
    let all = format!("{}{}", text(&out.stdout), text(&out.stderr));
    // Push fails (no server), but it fails on the network, with the op
    // counted, not on the file.
    assert!(!all.contains("is not a log entry"), "{all}");
    assert!(all.contains("1 write(s) still waiting"), "{all}");
    assert!(
        all.contains("remote-log.unreadable") || all.contains("could not be read"),
        "{all}"
    );
}

// GITSYNC-N6: one unreadable line in the middle of the log. Every later write
// warned "the server refused it ... 0 write(s) wait", while nothing had been
// sent and two writes were waiting.
#[test]
fn gitsync_n6_a_corrupt_line_is_blamed_on_the_file_and_the_writes_are_counted() {
    let sb = Sandbox::new("0123456789abcdef0123456789abcdef");
    let dir = offline_repo(&sb, "corrupt");
    sb.dx_ok(&dir, &["add", "goal", "first"]);
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(log_path(&dir))
        .unwrap();
    std::io::Write::write_all(&mut f, b"{\"entry\":\"op\",\"op_id\":\"x\"\n").unwrap();
    drop(f);
    sb.dx_ok(&dir, &["add", "goal", "corrupt1"]);
    let out = sb.dx_ok(&dir, &["add", "goal", "corrupt2"]);
    assert!(!out.contains("server refused"), "{out}");
    assert!(out.contains("3 write(s)"), "{out}");
    assert!(
        out.contains("line 2"),
        "the unreadable line is named: {out}"
    );
    // The advice must not be to delete a line that holds a write.
    assert!(!out.contains("delete that line"), "{out}");
}

// RUST-N4 / GITSYNC-N10: the lock was a file created with create_new and
// removed in Drop. SIGKILL or SIGTERM mid-write left it, and every write for
// the next 60 s waited 10 s and then dropped its op ("could not be queued").
// A leftover file is what a killed process leaves; an OS lock leaves none.
#[test]
fn rust_n4_a_lock_left_by_a_killed_process_neither_stalls_nor_drops_a_write() {
    let sb = Sandbox::new("0123456789abcdef0123456789abcdef");
    let dir = offline_repo(&sb, "stalelock");
    sb.dx_ok(&dir, &["add", "goal", "before"]);
    std::fs::write(dir.join(".deciduous").join("remote-log.lock"), b"").unwrap();

    let t = std::time::Instant::now();
    let out = sb.dx_ok(&dir, &["add", "goal", "after-kill"]);
    let took = t.elapsed();
    assert!(
        !out.contains("could not be queued") && !out.contains("not queued"),
        "{out}"
    );
    assert_eq!(queued_titles(&dir), ["before", "after-kill"], "{out}");
    assert!(
        took < std::time::Duration::from_secs(5),
        "took {took:?}: {out}"
    );
}

// RUST-N5: an update op says what it replaced (`was`), and the server applies
// it only over that value. db.rs read `was` before the write's transaction
// and appended the op after the commit, so two local writers racing on one
// node queued ops whose `was` was not what they replaced, or in an order
// that was not the order of the writes. The server refused them as edits
// made "on the server after this edit" when nobody had touched the server.
//
// Checked offline, on the log itself: read in order, each update's `was`
// must be the previous update's value, and the last one must be what the
// database holds. That is exactly what the server's compare-and-set needs.
#[test]
fn rust_n5_concurrent_local_writers_queue_a_true_was_in_commit_order() {
    let sb = std::sync::Arc::new(Sandbox::new("0123456789abcdef0123456789abcdef"));
    let dir = offline_repo(&sb, "stale-was");
    sb.dx_ok(&dir, &["add", "goal", "raced"]);
    let statuses = ["active", "completed", "rejected", "pending", "superseded"];
    let writers: Vec<_> = (0..4)
        .map(|w| {
            let (sb, dir) = (sb.clone(), dir.clone());
            std::thread::spawn(move || {
                for i in 0..12 {
                    let s = statuses[(w + i) % statuses.len()];
                    sb.dx_ok(&dir, &["status", "1", s]);
                }
            })
        })
        .collect();
    for w in writers {
        w.join().unwrap();
    }

    let updates: Vec<Value> = log_lines(&dir)
        .into_iter()
        .filter(|l| l["kind"] == "update_node")
        .collect();
    assert_eq!(updates.len(), 48);
    let mut expect = Value::String("pending".into());
    let mut broken = Vec::new();
    for (i, op) in updates.iter().enumerate() {
        if op["was"]["status"] != expect {
            broken.push(format!(
                "op {i}: was {} after {}",
                op["was"]["status"], expect
            ));
        }
        expect = op["set"]["status"].clone();
    }
    let g: Value = serde_json::from_str(&sb.dx_ok(&dir, &["graph"])).unwrap();
    assert_eq!(
        g["nodes"][0]["status"], expect,
        "the last op is not the last write"
    );
    assert!(broken.is_empty(), "{} stale: {broken:?}", broken.len());
}

/// A server that answers /ops the way 1.0.8's did for an op it could not
/// store: an empty 500 for the whole request whenever the batch holds a node
/// titled "poison", and every op applied otherwise. Returns the URL and the
/// titles it applied.
fn poison_stub() -> (String, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
    let applied = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let port = server.server_addr().to_ip().unwrap().port();
    let seen = applied.clone();
    std::thread::spawn(move || {
        for mut req in server.incoming_requests() {
            let mut body = String::new();
            let _ = req.as_reader().read_to_string(&mut body);
            let path = req.url().split('?').next().unwrap_or("").to_string();
            let reply = match path.as_str() {
                "/health" => "ok".to_string(),
                "/claim" => r#"{"workspace":"stub","claim":"unchecked"}"#.to_string(),
                "/locate" => r#"{"workspaces":[]}"#.to_string(),
                "/ops" => {
                    let v: Value = serde_json::from_str(&body).unwrap();
                    let ops = v["ops"].as_array().unwrap();
                    if ops.iter().any(|op| op["title"] == "poison") {
                        let _ = req.respond(tiny_http::Response::empty(500));
                        continue;
                    }
                    let mut seen = seen.lock().unwrap();
                    let results: Vec<Value> = ops
                        .iter()
                        .map(|op| {
                            if let Some(t) = op["title"].as_str() {
                                seen.push(t.to_string());
                            }
                            serde_json::json!({"op_id": op["op_id"], "result": "applied"})
                        })
                        .collect();
                    serde_json::json!({"workspace": "stub", "results": results}).to_string()
                }
                _ => {
                    let _ = req.respond(tiny_http::Response::empty(404));
                    continue;
                }
            };
            let _ = req.respond(tiny_http::Response::from_string(reply));
        }
    });
    (format!("http://127.0.0.1:{port}"), applied)
}

// SERVER-N1, the client half: one op the server fails on (HTTP 500 for the
// whole request) wedged the log for good. Every later write joined the same
// batch, got the same 500, and `push --drop-rejected` dropped nothing,
// because nothing had been answered "rejected". The batch is now split, the
// op the server fails on alone (while it answers the others) is set aside
// as rejected, and it can be listed, resent or dropped by id.
#[test]
fn server_n1_an_op_the_server_fails_on_is_set_aside_and_the_rest_delivered() {
    let (url, applied) = poison_stub();
    let sb = Sandbox::new("0123456789abcdef0123456789abcdef");
    let dir = sb.repo("poisoned");
    std::fs::write(
        dir.join(".deciduous").join("config.toml"),
        format!("[remote]\nurl = \"{url}\"\nworkspace = \"poisoned\"\n"),
    )
    .unwrap();
    sb.dx_ok(&dir, &["add", "goal", "before"]);
    sb.dx_ok(&dir, &["add", "goal", "poison"]);
    let out = sb.dx_ok(&dir, &["add", "goal", "after"]);
    sb.dx_ok(&dir, &["add", "goal", "later"]);
    assert_eq!(
        *applied.lock().unwrap(),
        ["before", "after", "later"],
        "{out}"
    );
    assert!(out.contains("poison") && out.contains("500"), "{out}");

    // Set aside, not lost: the op is in the log with a rejection saying why.
    let lines = log_lines(&dir);
    let poison = lines
        .iter()
        .find(|l| l["title"] == "poison")
        .expect("the op is kept");
    let id = poison["op_id"].as_str().unwrap().to_string();
    let ack = lines
        .iter()
        .find(|l| l["entry"] == "ack" && l["op_id"] == id.as_str())
        .expect("and answered");
    assert_eq!(ack["result"], "rejected");
    assert!(ack["reason"].as_str().unwrap().contains("500"), "{ack}");

    // Resent on request (the server still fails on it, so it stays).
    let retry = sb.dx(&dir, &["remote", "push", "--retry-rejected"]);
    let retry = format!("{}{}", text(&retry.stdout), text(&retry.stderr));
    assert!(retry.contains("1 rejected op(s)"), "{retry}");
    assert!(log_lines(&dir).iter().any(|l| l["op_id"] == id.as_str()));

    // Dropped by its id, alone.
    sb.dx_ok(&dir, &["add", "goal", "keep-waiting-offline"]);
    let dropped = sb.dx_ok(&dir, &["remote", "push", "--drop", &id[..8]]);
    assert!(dropped.contains("create goal"), "{dropped}");
    assert!(!log_lines(&dir).iter().any(|l| l["op_id"] == id.as_str()));
}

// SERVER-N1 against the real server: a NUL in one queued op. The CLI now
// refuses such a write before making it (see
// new_a_write_holding_a_nul_is_refused_before_it_is_made), so the op is put
// in the log by hand, the way an older build or another client queued it.
#[test]
#[ignore = "needs a real server: set DECIDUOUS_TEST_SERVER and DECIDUOUS_TEST_TOKEN"]
fn server_n1_a_nul_in_one_write_does_not_stop_the_writes_after_it() {
    let (url, token) = server();
    let sb = Sandbox::new(&token);
    let ws = unique("wal-poison");
    let dir = sb.remote_repo("poison", &url, &ws);
    let now = "2026-09-23T00:00:00Z";
    let poison = serde_json::json!({"entry":"op","op_id":"poisoned-op-1","at":now,
        "kind":"create_node","change_id":"0badc0de-0000-4000-8000-000000000001",
        "node_type":"goal","title":"poison","status":"pending",
        "metadata":{"prompt":"has\u{0}nul"},"created_at":now,"updated_at":now});
    std::fs::write(log_path(&dir), format!("{poison}\n")).unwrap();
    let said = sb.dx_ok(&dir, &["add", "goal", "after"]);
    assert!(said.contains("NUL"), "the refusal names the cause: {said}");
    assert_eq!(live_titles(&export(&url, &token, &ws)), ["after"]);
    let st = sb.dx(&dir, &["remote", "status"]);
    let st = text(&st.stdout);
    assert!(st.contains("0 write(s) waiting, 1 rejected"), "{st}");
}

impl Sandbox {
    /// `deciduous` with DECIDUOUS_DB_PATH set: the database it writes is not
    /// the one the current directory would find.
    fn dx_db(&self, dir: &Path, db: &Path, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_deciduous"))
            .args(args)
            .current_dir(dir)
            .env("HOME", self.path().join("home"))
            .env("XDG_CONFIG_HOME", self.path().join("home").join(".config"))
            .env("DECIDUOUS_MCP_TOKEN", &self.token)
            .env("DECIDUOUS_NO_SERVER", "1")
            .env("DECIDUOUS_DB_PATH", db)
            .env("NO_COLOR", "1")
            .output()
            .unwrap()
    }
}

// RUST-N1 / BRIDGE-N3: the log sits beside the database, but it was replayed
// with the config and repository of the current directory. With
// DECIDUOUS_DB_PATH naming project 1's database from inside project 3, the
// op went to project 3's workspace: acked, compacted out of project 1's log,
// and missing from project 1's graph for good, while `remote pull` in
// project 3 imported project 1's node.
#[test]
#[ignore = "needs a real server: set DECIDUOUS_TEST_SERVER and DECIDUOUS_TEST_TOKEN"]
fn rust_n1_a_write_goes_to_its_databases_workspace_not_the_cwds() {
    let (url, token) = server();
    let sb = Sandbox::new(&token);
    let (ws1, ws3) = (unique("wal-p1"), unique("wal-p3"));
    let p1 = sb.remote_repo("p1", &url, &ws1);
    let p3 = sb.remote_repo("p3", &url, &ws3);
    let db1 = p1.join(".deciduous").join("deciduous.db");

    let out = sb.dx_db(&p3, &db1, &["add", "goal", "P1-SECRET-cli"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    sb.dx_db(&p3, &db1, &["status", "1", "completed"]);

    assert_eq!(
        live_titles(&export(&url, &token, &ws3)),
        Vec::<String>::new()
    );
    let g1 = export(&url, &token, &ws1);
    assert_eq!(server_node(&g1, "P1-SECRET-cli")["status"], "completed");
    let st = sb.dx(&p1, &["remote", "status"]);
    assert!(st.status.success(), "{}", text(&st.stdout));
}

// BRIDGE-N3, the silent half: `deciduous mcp` configured the way
// docs/mcp.html shows for Claude Desktop, from a directory with no
// .deciduous and DECIDUOUS_DB_PATH naming the project's database. The cwd
// had no [remote], so the replay returned without a word, and the op sat in
// the project's log until some later CLI write from inside the project.
#[test]
#[ignore = "needs a real server: set DECIDUOUS_TEST_SERVER and DECIDUOUS_TEST_TOKEN"]
fn bridge_n3_a_stdio_server_started_elsewhere_sends_to_its_databases_workspace() {
    use std::io::{BufRead, BufReader, Write};
    let (url, token) = server();
    let sb = Sandbox::new(&token);
    let ws = unique("wal-desktop");
    let proj = sb.remote_repo("desktop", &url, &ws);
    let elsewhere = sb.path().join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_deciduous"))
        .arg("mcp")
        .current_dir(&elsewhere)
        .env("HOME", sb.path().join("home"))
        .env("XDG_CONFIG_HOME", sb.path().join("home").join(".config"))
        .env("DECIDUOUS_MCP_TOKEN", &token)
        .env("DECIDUOUS_NO_SERVER", "1")
        .env(
            "DECIDUOUS_DB_PATH",
            proj.join(".deciduous").join("deciduous.db"),
        )
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    for (id, method, params) in [
        (
            1,
            "initialize",
            serde_json::json!({"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"0"}}),
        ),
        (
            2,
            "tools/call",
            serde_json::json!({"name":"add_node","arguments":{"node_type":"goal","title":"desktop-style"}}),
        ),
    ] {
        let msg = serde_json::json!({"jsonrpc":"2.0","id":id,"method":method,"params":params});
        writeln!(stdin, "{msg}").unwrap();
        stdin.flush().unwrap();
        let mut line = String::new();
        stdout.read_line(&mut line).unwrap();
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let titles = loop {
        let t = live_titles(&export(&url, &token, &ws));
        if !t.is_empty() || std::time::Instant::now() > deadline {
            break t;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    };
    let _ = child.kill();
    let _ = child.wait();
    assert_eq!(titles, ["desktop-style"]);
}

/// A server that accepts connections and never answers: a paused
/// container, a stalled tunnel, a captive portal.
fn black_hole() -> String {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    std::thread::spawn(move || {
        let mut held = Vec::new();
        for s in l.incoming() {
            held.push(s);
        }
    });
    format!("http://127.0.0.1:{port}")
}

// RUST-N2: the stdio server replayed the log on its only thread, after each
// write's answer, with a 120 s timeout and a health check behind it. Against
// a server that accepts and never answers, every request after a write,
// reads and pings included, waited about 135 s, and again after every write.
#[test]
fn rust_n2_a_black_hole_server_does_not_delay_stdio_replies() {
    use std::io::{BufRead, BufReader, Write};
    let sb = Sandbox::new("0123456789abcdef0123456789abcdef");
    let dir = sb.repo("blackhole");
    std::fs::write(
        dir.join(".deciduous").join("config.toml"),
        format!("[remote]\nurl = \"{}\"\nworkspace = \"bh\"\n", black_hole()),
    )
    .unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_deciduous"))
        .arg("mcp")
        .current_dir(&dir)
        .env("HOME", sb.path().join("home"))
        .env("XDG_CONFIG_HOME", sb.path().join("home").join(".config"))
        .env("DECIDUOUS_MCP_TOKEN", "0123456789abcdef0123456789abcdef")
        .env("DECIDUOUS_NO_SERVER", "1")
        .env_remove("DECIDUOUS_DB_PATH")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            if tx.send(line.unwrap()).is_err() {
                break;
            }
        }
    });
    let mut calls = vec![(
        "initialize",
        serde_json::json!({"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"0"}}),
    )];
    for i in 0..3 {
        calls.push((
            "tools/call",
            serde_json::json!({"name":"add_node","arguments":{"node_type":"goal","title":format!("bh{i}")}}),
        ));
    }
    calls.push(("ping", serde_json::json!({})));
    calls.push((
        "tools/call",
        serde_json::json!({"name":"list_nodes","arguments":{}}),
    ));
    let mut slow = None;
    for (id, (method, params)) in calls.iter().enumerate() {
        let msg = serde_json::json!({"jsonrpc":"2.0","id":id,"method":method,"params":params});
        writeln!(stdin, "{msg}").unwrap();
        stdin.flush().unwrap();
        let t = std::time::Instant::now();
        if rx.recv_timeout(std::time::Duration::from_secs(5)).is_err() {
            slow = Some(format!(
                "call {id} ({method}) had no answer after {:?}",
                t.elapsed()
            ));
            break;
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    assert_eq!(slow, None);
    assert_eq!(queued_titles(&dir), ["bh0", "bh1", "bh2"]);
}

// RUST-N2, the CLI: `deciduous add` against the same server took 2:15.
#[test]
fn rust_n2_a_cli_write_to_a_black_hole_server_returns_promptly() {
    let sb = Sandbox::new("0123456789abcdef0123456789abcdef");
    let dir = sb.repo("blackhole-cli");
    std::fs::write(
        dir.join(".deciduous").join("config.toml"),
        format!("[remote]\nurl = \"{}\"\nworkspace = \"bh\"\n", black_hole()),
    )
    .unwrap();
    let t = std::time::Instant::now();
    let (tx, rx) = std::sync::mpsc::channel();
    let d = dir.clone();
    std::thread::spawn(move || {
        let _ = tx.send(sb.dx(&d, &["add", "goal", "bh-cli"]));
    });
    let out = rx
        .recv_timeout(std::time::Duration::from_secs(20))
        .unwrap_or_else(|_| panic!("deciduous add still running after {:?}", t.elapsed()));
    assert!(out.status.success(), "{}", text(&out.stderr));
    let err = text(&out.stderr);
    assert!(
        err.contains("did not get it") && err.contains("1 write(s) queued"),
        "{err}"
    );
    assert_eq!(queued_titles(&dir), ["bh-cli"]);
}

/// A server that applies creates and refuses every update, the way the real
/// one refuses an edit to a node an agent deleted.
fn refusing_stub() -> String {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let port = server.server_addr().to_ip().unwrap().port();
    std::thread::spawn(move || {
        for mut req in server.incoming_requests() {
            let mut body = String::new();
            let _ = req.as_reader().read_to_string(&mut body);
            let path = req.url().split('?').next().unwrap_or("").to_string();
            let reply = match path.as_str() {
                "/health" => "ok".to_string(),
                "/claim" => r#"{"workspace":"stub","claim":"unchecked"}"#.to_string(),
                "/locate" => r#"{"workspaces":[]}"#.to_string(),
                "/ops" => {
                    let v: Value = serde_json::from_str(&body).unwrap();
                    let results: Vec<Value> = v["ops"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|op| {
                            if op["kind"] == "update_node" {
                                serde_json::json!({"op_id": op["op_id"], "result": "rejected",
                                    "reason": format!("node {} was deleted on the server", op["change_id"].as_str().unwrap())})
                            } else {
                                serde_json::json!({"op_id": op["op_id"], "result": "applied"})
                            }
                        })
                        .collect();
                    serde_json::json!({"workspace": "stub", "results": results}).to_string()
                }
                _ => {
                    let _ = req.respond(tiny_http::Response::empty(404));
                    continue;
                }
            };
            let _ = req.respond(tiny_http::Response::from_string(reply));
        }
    });
    format!("http://127.0.0.1:{port}")
}

/// A stdio MCP server in `dir`, and a function that sends one request and
/// returns its answer.
fn stdio_mcp(sb: &Sandbox, dir: &Path) -> (std::process::Child, impl FnMut(&str, Value) -> Value) {
    use std::io::{BufRead, BufReader, Write};
    let mut child = Command::new(env!("CARGO_BIN_EXE_deciduous"))
        .arg("mcp")
        .current_dir(dir)
        .env("HOME", sb.path().join("home"))
        .env("XDG_CONFIG_HOME", sb.path().join("home").join(".config"))
        .env("DECIDUOUS_MCP_TOKEN", &sb.token)
        .env("DECIDUOUS_NO_SERVER", "1")
        .env_remove("DECIDUOUS_DB_PATH")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut id = 0;
    let call = move |method: &str, params: Value| {
        id += 1;
        let msg = serde_json::json!({"jsonrpc":"2.0","id":id,"method":method,"params":params});
        writeln!(stdin, "{msg}").unwrap();
        stdin.flush().unwrap();
        let mut line = String::new();
        stdout.read_line(&mut line).unwrap();
        serde_json::from_str::<Value>(&line).unwrap()
    };
    (child, call)
}

fn tool_text(reply: &Value) -> String {
    reply["result"]["content"]
        .as_array()
        .map(|c| {
            c.iter()
                .filter_map(|x| x["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

// RUST-N7: the refusal of an agent's write went to stderr, which the agent
// never reads. update_status on a node deleted on the server returned plain
// success, and so did every call after it.
#[test]
fn rust_n7_a_write_the_server_refused_is_reported_in_a_tool_result() {
    let sb = Sandbox::new("0123456789abcdef0123456789abcdef");
    let dir = sb.repo("refused-mcp");
    std::fs::write(
        dir.join(".deciduous").join("config.toml"),
        format!(
            "[remote]\nurl = \"{}\"\nworkspace = \"r\"\n",
            refusing_stub()
        ),
    )
    .unwrap();
    let (mut child, mut call) = stdio_mcp(&sb, &dir);
    call(
        "initialize",
        serde_json::json!({"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"0"}}),
    );
    call(
        "tools/call",
        serde_json::json!({"name":"add_node","arguments":{"node_type":"goal","title":"g"}}),
    );
    call(
        "tools/call",
        serde_json::json!({"name":"update_status","arguments":{"node_id":1,"status":"completed"}}),
    );
    // The refusal arrives after the answer (the replay runs beside the
    // loop); it is in the next tool result, whichever tool that is.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut seen = String::new();
    while std::time::Instant::now() < deadline && !seen.contains("refused") {
        seen = tool_text(&call(
            "tools/call",
            serde_json::json!({"name":"list_nodes","arguments":{}}),
        ));
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let _ = child.kill();
    let _ = child.wait();
    assert!(
        seen.contains("refused") && seen.contains("was deleted on the server"),
        "no tool result said so: {seen}"
    );
}

// RUST-N7 / RUST-N4: a write made locally and then not queued (the log could
// not be written) answered plain success; the warning went to stderr only.
#[test]
fn rust_n7_a_write_that_could_not_be_queued_is_an_error_result() {
    let sb = Sandbox::new("0123456789abcdef0123456789abcdef");
    let dir = offline_repo(&sb, "unqueued-mcp");
    // The log path is a directory: every append fails.
    std::fs::create_dir_all(log_path(&dir)).unwrap();
    let (mut child, mut call) = stdio_mcp(&sb, &dir);
    call(
        "initialize",
        serde_json::json!({"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"0"}}),
    );
    let r = call(
        "tools/call",
        serde_json::json!({"name":"add_node","arguments":{"node_type":"goal","title":"unqueued"}}),
    );
    let _ = child.kill();
    let _ = child.wait();
    assert_eq!(r["result"]["isError"], true, "{r}");
    let t = tool_text(&r);
    assert!(t.to_lowercase().contains("not queued"), "{t}");
}

// RUST-N7, the volume: offline, every write of a stdio server printed the
// same ~430-byte warning to stderr. A client that does not drain stderr
// (the probe's first harness) stopped the server after about 150 writes.
// The same failure is said once, and again only when it changes.
#[test]
fn rust_n7_an_offline_stdio_server_says_it_is_offline_once() {
    use std::io::{BufRead, BufReader, Read, Write};
    let sb = Sandbox::new("0123456789abcdef0123456789abcdef");
    let dir = offline_repo(&sb, "quiet");
    let mut child = Command::new(env!("CARGO_BIN_EXE_deciduous"))
        .arg("mcp")
        .current_dir(&dir)
        .env("HOME", sb.path().join("home"))
        .env("XDG_CONFIG_HOME", sb.path().join("home").join(".config"))
        .env("DECIDUOUS_MCP_TOKEN", &sb.token)
        .env("DECIDUOUS_NO_SERVER", "1")
        .env_remove("DECIDUOUS_DB_PATH")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut send = |id: usize, method: &str, params: Value| {
        let msg = serde_json::json!({"jsonrpc":"2.0","id":id,"method":method,"params":params});
        writeln!(stdin, "{msg}").unwrap();
        stdin.flush().unwrap();
        let mut line = String::new();
        stdout.read_line(&mut line).unwrap();
    };
    send(
        0,
        "initialize",
        serde_json::json!({"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"0"}}),
    );
    for i in 1..=20 {
        send(
            i,
            "tools/call",
            serde_json::json!({"name":"add_node","arguments":{"node_type":"goal","title":format!("q{i}")}}),
        );
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
    drop(stdin);
    let mut err = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut err)
        .unwrap();
    child.wait().unwrap();
    assert_eq!(queued_titles(&dir).len(), 20);
    assert_eq!(err.matches("did not get it").count(), 1, "{err}");
}

// ---------------------------------------------------------------------------
// Chapter 28, round 2: findings from the adversarial verification of this
// chapter, each test named after its finding.
// ---------------------------------------------------------------------------

/// The ops in the log, in order, as `kind title-or-change_id`.
fn log_ops(dir: &Path) -> Vec<String> {
    log_lines(dir)
        .iter()
        .filter(|l| l["entry"] == "op")
        .map(|l| {
            let what = l["title"]
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| l["change_id"].as_str().unwrap_or("").to_string());
            format!("{} {what}", l["kind"].as_str().unwrap())
        })
        .collect()
}

// NEW (high): the op was appended after the database commit, so a write
// whose append did not happen (a kill in between, a log that could not be
// written) was made here and never queued. A lost delete was then undone by
// `remote pull`, and nothing but `--seed` (creates only) could resend
// anything. Deterministic here with a log that cannot be written: the op must
// survive the failed append and be queued, in order, once the log can be
// written again.
#[test]
fn new_wal_a_write_whose_append_failed_is_queued_by_the_next_write() {
    let sb = Sandbox::new("0123456789abcdef0123456789abcdef");
    let dir = offline_repo(&sb, "wal-append");
    sb.dx_ok(&dir, &["add", "goal", "doomed"]);
    let doomed = local_change_id(&sb, &dir, 1);
    let log = log_path(&dir);
    let aside = dir.join(".deciduous").join("log.bak");
    std::fs::rename(&log, &aside).unwrap();
    std::fs::create_dir_all(&log).unwrap();

    // Both made here while the log is a directory: neither op can be appended.
    let _ = sb.dx(&dir, &["add", "goal", "lost-create"]);
    let _ = sb.dx(&dir, &["delete", "1"]);

    std::fs::remove_dir(&log).unwrap();
    std::fs::rename(&aside, &log).unwrap();
    sb.dx_ok(&dir, &["add", "goal", "after"]);
    assert_eq!(
        log_ops(&dir),
        [
            "create_node doomed".to_string(),
            "create_node lost-create".to_string(),
            format!("delete_node {doomed}"),
            "create_node after".to_string(),
        ]
    );
}

// NEW (high), the kill itself: stdio MCP servers killed (SIGKILL) 0-80 ms
// after being handed creates and deletes. Afterwards every node this
// database holds has a create op, and every node it lost has a delete op.
// On the chapter's head the probe left 5 nodes in 30 trials with no op.
#[test]
fn new_wal_a_killed_writer_leaves_no_local_write_without_its_op() {
    use std::io::{BufRead, BufReader, Write};
    let sb = Sandbox::new("0123456789abcdef0123456789abcdef");
    let dir = offline_repo(&sb, "wal-kill");
    for i in 0..40 {
        sb.dx_ok(&dir, &["add", "goal", &format!("seed{i}")]);
    }
    let mut next_delete = 1;
    for trial in 0..25 {
        let mut child = Command::new(env!("CARGO_BIN_EXE_deciduous"))
            .arg("mcp")
            .current_dir(&dir)
            .env("HOME", sb.path().join("home"))
            .env("XDG_CONFIG_HOME", sb.path().join("home").join(".config"))
            .env("DECIDUOUS_MCP_TOKEN", &sb.token)
            .env("DECIDUOUS_NO_SERVER", "1")
            .env_remove("DECIDUOUS_DB_PATH")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let mut stdin = child.stdin.take().unwrap();
        // Started (a debug build takes a while to): the writes below are
        // then in flight when the kill lands.
        writeln!(stdin, "{}", serde_json::json!({"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"0"}}})).unwrap();
        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        let mut line = String::new();
        stdout.read_line(&mut line).unwrap();
        let mut msgs = Vec::new();
        for i in 0..3 {
            msgs.push(serde_json::json!({"jsonrpc":"2.0","id":10 + i,"method":"tools/call","params":{"name":"add_node","arguments":{"node_type":"goal","title":format!("k{trial}-{i}")}}}));
        }
        for i in 0..2 {
            msgs.push(serde_json::json!({"jsonrpc":"2.0","id":20 + i,"method":"tools/call","params":{"name":"delete_node","arguments":{"node_id":next_delete}}}));
            next_delete += 1;
        }
        for m in msgs {
            let _ = writeln!(stdin, "{m}");
        }
        let _ = stdin.flush();
        let jitter = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos()
            % 80) as u64;
        std::thread::sleep(std::time::Duration::from_millis(jitter));
        let _ = child.kill();
        let _ = child.wait();
    }
    // The next write is where a kill's leftovers are picked up.
    sb.dx_ok(&dir, &["add", "goal", "final"]);

    let g: Value = serde_json::from_str(&sb.dx_ok(&dir, &["graph"])).unwrap();
    let local: std::collections::HashSet<String> = g["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["change_id"].as_str().unwrap().to_string())
        .collect();
    let lines = log_lines(&dir);
    let ops = |kind: &str| -> std::collections::HashSet<String> {
        lines
            .iter()
            .filter(|l| l["kind"] == kind)
            .map(|l| l["change_id"].as_str().unwrap().to_string())
            .collect()
    };
    let (created, deleted) = (ops("create_node"), ops("delete_node"));
    let no_create: Vec<&String> = local.iter().filter(|c| !created.contains(*c)).collect();
    let no_delete: Vec<&String> = created
        .iter()
        .filter(|c| !local.contains(*c) && !deleted.contains(*c))
        .collect();
    assert!(
        no_create.is_empty() && no_delete.is_empty(),
        "{} node(s) here with no create op, {} deleted here with no delete op",
        no_create.len(),
        no_delete.len()
    );
}

// RUST-N1 / BRIDGE-N3, another spelling: DECIDUOUS_DB_PATH=deciduous.db from
// inside .deciduous/ has no directory part, and the log was not attached at
// all: "Created node 7", no warning, and the write never queued.
#[test]
fn rust_n1_a_db_path_with_no_directory_part_still_queues() {
    let sb = Sandbox::new("0123456789abcdef0123456789abcdef");
    let dir = offline_repo(&sb, "bare-db-path");
    let inside = dir.join(".deciduous");
    let out = sb.dx_db(
        &inside,
        Path::new("deciduous.db"),
        &["add", "goal", "relative-db-path"],
    );
    assert!(out.status.success(), "{}", all_of(&out));
    assert_eq!(
        queued_titles(&dir),
        ["relative-db-path"],
        "{}",
        all_of(&out)
    );
}

fn all_of(out: &Output) -> String {
    format!("{}{}", text(&out.stdout), text(&out.stderr))
}

/// A stand-in for the server whose /ops answers are decided per request by
/// `fail(n, ops)`: `Some((status, delay_ms))` answers that status after the
/// delay, `None` applies every op. `n` counts /ops requests from 1. Returns
/// the URL, what it applied (a create's title, else `kind change_id`), and
/// a switch that, once set, makes it apply everything.
#[allow(clippy::type_complexity)]
fn scripted_stub(
    fail: impl Fn(usize, &[Value]) -> Option<(u16, u64)> + Send + 'static,
) -> (
    String,
    std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    let applied = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let healthy = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let port = server.server_addr().to_ip().unwrap().port();
    let (seen, ok) = (applied.clone(), healthy.clone());
    std::thread::spawn(move || {
        let mut n = 0;
        for mut req in server.incoming_requests() {
            let mut body = String::new();
            let _ = req.as_reader().read_to_string(&mut body);
            let path = req.url().split('?').next().unwrap_or("").to_string();
            let reply = match path.as_str() {
                "/health" => "ok".to_string(),
                "/claim" => r#"{"workspace":"stub","claim":"unchecked"}"#.to_string(),
                "/locate" => r#"{"workspaces":[]}"#.to_string(),
                "/ops" => {
                    n += 1;
                    let v: Value = serde_json::from_str(&body).unwrap();
                    let ops = v["ops"].as_array().unwrap().clone();
                    if !ok.load(std::sync::atomic::Ordering::SeqCst) {
                        if let Some((status, delay)) = fail(n, &ops) {
                            std::thread::sleep(std::time::Duration::from_millis(delay));
                            let _ = req.respond(tiny_http::Response::empty(status));
                            continue;
                        }
                    }
                    let mut seen = seen.lock().unwrap();
                    let results: Vec<Value> = ops
                        .iter()
                        .map(|op| {
                            seen.push(match op["title"].as_str() {
                                Some(t) => t.to_string(),
                                None => format!(
                                    "{} {}",
                                    op["kind"].as_str().unwrap_or(""),
                                    op["change_id"].as_str().unwrap_or("")
                                ),
                            });
                            serde_json::json!({"op_id": op["op_id"], "result": "applied"})
                        })
                        .collect();
                    serde_json::json!({"workspace": "stub", "results": results}).to_string()
                }
                _ => {
                    let _ = req.respond(tiny_http::Response::empty(404));
                    continue;
                }
            };
            let _ = req.respond(tiny_http::Response::from_string(reply));
        }
    });
    (format!("http://127.0.0.1:{port}"), applied, healthy)
}

// NEW (medium): a 500 on the first request and on the first per-op resend
// (a DB restart, a pool timeout) set a good create aside as "the server
// failed on this op alone", and both edits of that node after it were then
// refused as if the node did not exist. Push exited 0.
#[test]
fn new_a_transient_500_sets_no_op_aside() {
    let (url, applied, _) = scripted_stub(|n, _| (n <= 2).then_some((500, 0)));
    let sb = Sandbox::new("0123456789abcdef0123456789abcdef");
    let dir = offline_repo(&sb, "transient");
    sb.dx_ok(&dir, &["add", "goal", "casc"]);
    sb.dx_ok(&dir, &["status", "1", "completed"]);
    sb.dx_ok(&dir, &["prompt", "1", "why"]);
    set_remote_url(&dir, &url);
    let out = sb.dx(&dir, &["remote", "push"]);
    let said = all_of(&out);
    assert!(said.contains("0 rejected"), "{said}");
    assert!(out.status.success(), "{said}");
    assert_eq!(applied.lock().unwrap().len(), 3, "{said}");
}

// SERVER-N1, client half: the server answered one op and then failed on
// every request (its database went down mid-replay). The rule "fails alone
// while the server answered others" set every later op aside as poisoned,
// they were never resent, and the hint printed for them discarded them.
#[test]
fn server_n1_an_outage_during_a_replay_sets_no_op_aside() {
    // Batches fail; the first single op is answered; then everything fails.
    let (url, applied, healthy) =
        scripted_stub(|n, ops| (ops.len() > 1 || n > 2).then_some((500, 0)));
    let sb = Sandbox::new("0123456789abcdef0123456789abcdef");
    let dir = offline_repo(&sb, "outage");
    for t in ["a", "b", "c", "d", "e"] {
        sb.dx_ok(&dir, &["add", "goal", t]);
    }
    sb.dx_ok(&dir, &["link", "1", "2"]);
    set_remote_url(&dir, &url);
    let out = sb.dx(&dir, &["remote", "push"]);
    let said = all_of(&out);
    let st = log_lines(&dir);
    let rejected = st
        .iter()
        .filter(|l| l["entry"] == "ack" && l["result"] == "rejected")
        .count();
    assert_eq!(rejected, 0, "{said}");
    assert!(
        !out.status.success(),
        "a push that left writes waiting: {said}"
    );
    assert!(!said.contains("--drop-rejected"), "{said}");

    healthy.store(true, std::sync::atomic::Ordering::SeqCst);
    let again = sb.dx(&dir, &["remote", "push"]);
    assert!(again.status.success(), "{}", all_of(&again));
    let mut got = applied.lock().unwrap().clone();
    got.retain(|t| t.len() == 1);
    got.sort();
    assert_eq!(got, ["a", "b", "c", "d", "e"], "{}", all_of(&again));
    assert!(log_ops(&dir).is_empty(), "{:?}", log_ops(&dir));
}

// NEW (medium), the order half: a set-aside create was followed by edits of
// the same node, which were sent and refused ("no node ... on the server")
// for want of it. They wait with it instead, and are resent with it.
#[test]
fn new_the_edits_of_a_set_aside_node_wait_with_it() {
    let (url, applied, _) = scripted_stub(|_, ops| {
        ops.iter()
            .any(|op| op["title"] == "poison")
            .then_some((500, 0))
    });
    let sb = Sandbox::new("0123456789abcdef0123456789abcdef");
    let dir = offline_repo(&sb, "held");
    sb.dx_ok(&dir, &["add", "goal", "before"]);
    sb.dx_ok(&dir, &["add", "goal", "poison"]);
    sb.dx_ok(&dir, &["status", "2", "completed"]);
    sb.dx_ok(&dir, &["add", "goal", "after"]);
    set_remote_url(&dir, &url);
    let out = sb.dx(&dir, &["remote", "push"]);
    let said = all_of(&out);
    assert_eq!(*applied.lock().unwrap(), ["before", "after"], "{said}");
    assert!(!out.status.success(), "writes were set aside: {said}");
    let ops = log_ops(&dir);
    assert_eq!(ops.len(), 2, "the create and its edit both kept: {ops:?}");
}

// RUST-N2 (a): with the server's database down, every /ops request answered
// 500 after about 3.5 s, and the replay after a CLI write resent every
// queued op alone: 108 s for 30 ops.
#[test]
fn rust_n2_a_cli_write_to_a_failing_server_is_bounded() {
    let (url, _, _) = scripted_stub(|_, _| Some((500, 1500)));
    let sb = Sandbox::new("0123456789abcdef0123456789abcdef");
    let dir = offline_repo(&sb, "slow500");
    for i in 0..30 {
        sb.dx_ok(&dir, &["add", "goal", &format!("q{i}")]);
    }
    set_remote_url(&dir, &url);
    let t = std::time::Instant::now();
    let out = sb.dx(&dir, &["add", "goal", "while-db-down"]);
    let took = t.elapsed();
    assert!(out.status.success(), "{}", all_of(&out));
    assert!(
        took < std::time::Duration::from_secs(15),
        "took {took:?}: {}",
        all_of(&out)
    );
    assert_eq!(queued_titles(&dir).len(), 31);
}

// RUST-N2 (b): a 1.0.7 config (a url, no workspace) asks the server where
// its nodes are (/locate, 15 s) before the replay's own 10 s: 25 s per
// write against a server that never answers.
#[test]
fn rust_n2_a_legacy_config_write_to_a_black_hole_is_bounded() {
    let sb = Sandbox::new("0123456789abcdef0123456789abcdef");
    let dir = sb.repo("legacy-bh");
    std::fs::write(
        dir.join(".deciduous").join("config.toml"),
        format!("[remote]\nurl = \"{}\"\n", black_hole()),
    )
    .unwrap();
    let t = std::time::Instant::now();
    let out = sb.dx(&dir, &["add", "goal", "first"]);
    let took = t.elapsed();
    assert!(out.status.success(), "{}", all_of(&out));
    assert!(
        took < std::time::Duration::from_secs(15),
        "took {took:?}: {}",
        all_of(&out)
    );
    assert_eq!(queued_titles(&dir), ["first"]);
}

// NEW (low): stdin closed, the stdio server's last replay against a server
// that never answers held the process about 10 s (20 s with a replay
// already running).
#[test]
fn new_a_stdio_server_exits_promptly_when_stdin_closes() {
    use std::io::{BufRead, BufReader, Write};
    let sb = Sandbox::new("0123456789abcdef0123456789abcdef");
    let dir = sb.repo("bh-exit");
    std::fs::write(
        dir.join(".deciduous").join("config.toml"),
        format!("[remote]\nurl = \"{}\"\nworkspace = \"bh\"\n", black_hole()),
    )
    .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_deciduous"))
        .arg("mcp")
        .current_dir(&dir)
        .env("HOME", sb.path().join("home"))
        .env("XDG_CONFIG_HOME", sb.path().join("home").join(".config"))
        .env("DECIDUOUS_MCP_TOKEN", &sb.token)
        .env("DECIDUOUS_NO_SERVER", "1")
        .env_remove("DECIDUOUS_DB_PATH")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    for (id, (m, p)) in [
        ("initialize", serde_json::json!({"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"0"}})),
        ("tools/call", serde_json::json!({"name":"add_node","arguments":{"node_type":"goal","title":"x1"}})),
        ("tools/call", serde_json::json!({"name":"add_node","arguments":{"node_type":"goal","title":"x2"}})),
    ]
    .into_iter()
    .enumerate()
    {
        writeln!(stdin, "{}", serde_json::json!({"jsonrpc":"2.0","id":id,"method":m,"params":p})).unwrap();
        stdin.flush().unwrap();
        let mut line = String::new();
        stdout.read_line(&mut line).unwrap();
    }
    drop(stdin);
    let t = std::time::Instant::now();
    let mut exited = None;
    while t.elapsed() < std::time::Duration::from_secs(15) {
        if let Some(s) = child.try_wait().unwrap() {
            exited = Some((s, t.elapsed()));
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let _ = child.kill();
    let _ = child.wait();
    let (_, took) = exited.expect("still running 15 s after stdin closed");
    assert!(
        took < std::time::Duration::from_secs(5),
        "exited after {took:?}"
    );
    assert_eq!(queued_titles(&dir), ["x1", "x2"]);
}

/// `deciduous add goal <title> --prompt-stdin` with `prompt` on stdin.
fn add_with_prompt(sb: &Sandbox, dir: &Path, title: &str, prompt: &[u8]) -> Output {
    use std::io::Write;
    let mut child = Command::new(env!("CARGO_BIN_EXE_deciduous"))
        .args(["add", "goal", title, "--prompt-stdin"])
        .current_dir(dir)
        .env("HOME", sb.path().join("home"))
        .env("XDG_CONFIG_HOME", sb.path().join("home").join(".config"))
        .env("DECIDUOUS_MCP_TOKEN", &sb.token)
        .env("DECIDUOUS_NO_SERVER", "1")
        .env("NO_COLOR", "1")
        .env_remove("DECIDUOUS_DB_PATH")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(prompt).unwrap();
    child.wait_with_output().unwrap()
}

// NEW (medium), the source: the CLI accepted a NUL in --prompt-stdin in a
// project whose writes go to a server that can never store it. It is
// refused before anything is written, naming where the NUL is.
#[test]
fn new_a_write_holding_a_nul_is_refused_before_it_is_made() {
    let sb = Sandbox::new("0123456789abcdef0123456789abcdef");
    let dir = offline_repo(&sb, "nul-write");
    let out = add_with_prompt(&sb, &dir, "poison", b"has\0nul");
    let said = all_of(&out);
    assert!(!out.status.success(), "{said}");
    assert!(said.contains("NUL") && said.contains("prompt"), "{said}");
    let g: Value = serde_json::from_str(&sb.dx_ok(&dir, &["graph"])).unwrap();
    assert_eq!(g["nodes"].as_array().unwrap().len(), 0, "nothing written");
    assert!(queued_titles(&dir).is_empty());
}

// NEW (medium): one row holding a NUL (written before this project had a
// remote) made `remote push --seed` fail as a whole ("nodes[0] ... NUL ...
// nothing was imported"), so every other row that no op covers could not
// be sent either, and `remote status` pointed the NUL row at --seed.
#[test]
#[ignore = "needs a real server: set DECIDUOUS_TEST_SERVER and DECIDUOUS_TEST_TOKEN"]
fn new_a_nul_row_does_not_stop_seed_sending_the_others() {
    let (url, token) = server();
    let sb = Sandbox::new(&token);
    let ws = unique("wal-nulseed");
    let dir = sb.repo("nulseed");
    let out = add_with_prompt(&sb, &dir, "poison", b"has\0nul");
    assert!(out.status.success(), "{}", all_of(&out));
    sb.dx_ok(&dir, &["add", "goal", "pre-remote-row"]);
    sb.dx_ok(&dir, &["remote", "init", &url, "--workspace", &ws]);

    let st = all_of(&sb.dx(&dir, &["remote", "status"]));
    let poison_line = st
        .lines()
        .find(|l| l.contains("\"poison\""))
        .unwrap_or_else(|| panic!("{st}"));
    assert!(
        poison_line.contains("NUL") && !poison_line.contains("--seed"),
        "{st}"
    );

    let seed = sb.dx(&dir, &["remote", "push", "--seed"]);
    let said = all_of(&seed);
    assert!(!seed.status.success(), "a row was not sent: {said}");
    assert!(said.contains("poison") && said.contains("NUL"), "{said}");
    assert_eq!(
        live_titles(&export(&url, &token, &ws)),
        ["pre-remote-row"],
        "{said}"
    );
}

// NEW (low): with the only difference an unreadable-lines file, `remote
// status` listed push, --seed, --repair and pull, none of which touches it,
// and its "set aside" line had a doubled verb.
#[test]
#[ignore = "needs a real server: set DECIDUOUS_TEST_SERVER and DECIDUOUS_TEST_TOKEN"]
fn new_status_says_what_to_do_about_unreadable_lines() {
    let (url, token) = server();
    let sb = Sandbox::new(&token);
    let ws = unique("wal-unreadable");
    let dir = sb.remote_repo("unreadable", &url, &ws);
    sb.dx_ok(&dir, &["add", "goal", "fine"]);
    std::fs::write(
        dir.join(".deciduous").join("remote-log.unreadable"),
        "{\"entry\":\"op\",\"op_\n",
    )
    .unwrap();
    let out = sb.dx(&dir, &["remote", "status"]);
    let st = all_of(&out);
    assert!(!out.status.success(), "{st}");
    assert!(!st.contains("were moved to"), "{st}");
    assert!(!st.contains("sends what is waiting"), "{st}");
    assert!(st.contains("remote-log.unreadable"), "{st}");
}
