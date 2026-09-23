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

    let out = sb.dx_ok(&dir, &["remote", "status"]);
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
    // A 1.0.7 config: URL only, workspace derived on every call.
    std::fs::write(
        main.join(".deciduous").join("config.toml"),
        format!("[remote]\nurl = \"{}\"\n", dead_url()),
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
    assert!(out.contains("queued"), "{out}");
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
