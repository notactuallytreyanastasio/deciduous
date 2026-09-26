//! The agent message board, driven through the real `deciduous` binary in
//! real git repositories: one board for every worktree, nothing of it in the
//! graph file or the export, the server's `/messages` in remote mode, and a
//! loud error from a server that predates the board.

use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

struct Sandbox {
    root: TempDir,
}

impl Sandbox {
    fn new() -> Self {
        let root = TempDir::new().unwrap();
        std::fs::create_dir_all(root.path().join("home")).unwrap();
        Sandbox { root }
    }

    fn path(&self) -> &Path {
        self.root.path()
    }

    fn git(&self, dir: &Path, args: &[&str]) {
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
    }

    fn cmd(&self, dir: &Path, args: &[&str]) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_deciduous"));
        c.args(args)
            .current_dir(dir)
            .env("HOME", self.path().join("home"))
            .env("XDG_CONFIG_HOME", self.path().join("home").join(".config"))
            .env("DECIDUOUS_MCP_TOKEN", "0123456789abcdef0123456789abcdef")
            .env("DECIDUOUS_NO_SERVER", "1")
            .env_remove("DECIDUOUS_DB_PATH")
            .env_remove("DECIDUOUS_AGENT_LABEL")
            .env("NO_COLOR", "1");
        c
    }

    fn dx(&self, dir: &Path, args: &[&str]) -> Output {
        self.cmd(dir, args).output().unwrap()
    }

    fn dx_ok(&self, dir: &Path, args: &[&str]) -> String {
        let out = self.dx(dir, args);
        assert!(
            out.status.success(),
            "deciduous {args:?} failed\nstdout: {}\nstderr: {}",
            text(&out.stdout),
            text(&out.stderr)
        );
        text(&out.stdout)
    }

    fn dx_err(&self, dir: &Path, args: &[&str]) -> String {
        let out = self.dx(dir, args);
        assert!(
            !out.status.success(),
            "deciduous {args:?} should have failed\nstdout: {}",
            text(&out.stdout)
        );
        text(&out.stderr)
    }

    /// A git repository with one commit and `deciduous init` run in it.
    fn repo(&self, rel: &str) -> PathBuf {
        let dir = self.path().join(rel);
        std::fs::create_dir_all(&dir).unwrap();
        self.git(&dir, &["init", "-q", "-b", "main"]);
        self.git(&dir, &["commit", "-q", "--allow-empty", "-m", "init"]);
        self.dx_ok(&dir, &["init"]);
        dir
    }

    /// A linked worktree of `repo` on a new branch, with the `.deciduous/`
    /// a checkout of a project that tracks it would have.
    fn worktree(&self, repo: &Path, rel: &str, branch: &str) -> PathBuf {
        let wt = self.path().join(rel);
        self.git(
            repo,
            &["worktree", "add", "-q", wt.to_str().unwrap(), "-b", branch],
        );
        std::fs::create_dir_all(wt.join(".deciduous")).unwrap();
        wt
    }
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).to_string()
}

fn json_of(s: &str) -> Value {
    serde_json::from_str(s).unwrap_or_else(|e| panic!("not JSON ({e}): {s}"))
}

fn ids(read: &Value) -> Vec<i64> {
    read["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_i64().unwrap())
        .collect()
}

#[test]
fn two_worktrees_one_board() {
    let sb = Sandbox::new();
    let main = sb.repo("proj");
    let wt = sb.worktree(&main, "proj-feat", "feat");

    sb.dx_ok(
        &main,
        &[
            "board",
            "post",
            "--as",
            "lead",
            "-s",
            "interface",
            "-m",
            "@w1 build to this",
        ],
    );
    let out = sb.dx_ok(
        &wt,
        &[
            "board",
            "post",
            "--as",
            "w1",
            "-s",
            "re interface",
            "-m",
            "done, @lead",
            "--reply-to",
            "1",
        ],
    );
    assert_eq!(out.trim(), "Posted #2 (reply to #1), to lead");

    for dir in [&main, &wt] {
        let r = json_of(&sb.dx_ok(dir, &["board", "read", "--json"]));
        assert_eq!(ids(&r), [1, 2], "from {}", dir.display());
    }
    // The branch defaults to the poster's own.
    let r = json_of(&sb.dx_ok(&main, &["board", "read", "--json"]));
    assert_eq!(r["messages"][0]["branch"], "main");
    assert_eq!(r["messages"][1]["branch"], "feat");

    // Also when the worktree reaches its database through DECIDUOUS_DB_PATH,
    // the way agents in worktrees are told to.
    let out = sb
        .cmd(&wt, &["board", "read", "--json"])
        .env("DECIDUOUS_DB_PATH", wt.join(".deciduous/deciduous.db"))
        .output()
        .unwrap();
    assert_eq!(ids(&json_of(&text(&out.stdout))), [1, 2]);

    // The worktree's graph database, where one exists, holds none of it.
    let wt_db = wt.join(".deciduous/deciduous.db");
    if wt_db.exists() {
        let conn = rusqlite::Connection::open(&wt_db).unwrap();
        let n: i64 = conn
            .query_row("SELECT count(*) FROM agent_messages", [], |r| r.get(0))
            .unwrap_or(0);
        assert_eq!(n, 0);
    }
}

#[test]
fn messages_never_reach_the_graph_file_or_the_export() {
    let sb = Sandbox::new();
    let dir = sb.repo("proj");
    sb.dx_ok(&dir, &["add", "goal", "a real goal"]);
    sb.dx_ok(
        &dir,
        &[
            "board",
            "post",
            "--as",
            "lead",
            "-s",
            "SECRET-SUBJECT",
            "-m",
            "SECRET-BODY @w1",
        ],
    );
    sb.dx_ok(&dir, &["sync"]);
    let mut looked = 0;
    for f in [".deciduous/graph.json", "docs/graph-data.json"] {
        let p = dir.join(f);
        if let Ok(s) = std::fs::read_to_string(&p) {
            looked += 1;
            assert!(s.contains("a real goal"), "{f} should hold the graph");
            assert!(!s.contains("SECRET"), "{f} carries a board message");
        }
    }
    assert!(looked >= 1, "sync wrote neither graph file");
    let graph = sb.dx_ok(&dir, &["graph"]);
    assert!(graph.contains("a real goal") && !graph.contains("SECRET"));
    let dot = sb.dx_ok(&dir, &["dot"]);
    assert!(!dot.contains("SECRET"));
}

#[test]
fn cli_post_read_show_output() {
    let sb = Sandbox::new();
    let dir = sb.repo("proj");

    // No label: refused, naming both ways to give one.
    let e = sb.dx_err(&dir, &["board", "post", "-s", "x", "-m", "y"]);
    assert!(
        e.contains("--as LABEL") && e.contains("DECIDUOUS_AGENT_LABEL"),
        "{e}"
    );

    // Label from the environment, body from stdin.
    let mut child = sb
        .cmd(&dir, &["board", "post", "-s", "which port? @w1", "--json"])
        .env("DECIDUOUS_AGENT_LABEL", "lead")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"line 1\nask @w2 too\n")
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    let posted = json_of(&text(&out.stdout));
    assert_eq!(posted["id"], 1);
    assert_eq!(posted["mentions"], json!(["w1", "w2"]));

    let long: String = (1..=20).map(|i| format!("l{i}\n")).collect();
    sb.dx_ok(
        &dir,
        &[
            "board",
            "post",
            "--as",
            "w1",
            "-s",
            "long",
            "-m",
            &long,
            "--reply-to",
            "1",
        ],
    );

    let read = sb.dx_ok(&dir, &["board", "read"]);
    assert!(read.contains("#1  lead -> w1, w2  [main]"), "{read}");
    assert!(
        read.contains("    which port? @w1\n    | line 1\n    | ask @w2 too\n"),
        "{read}"
    );
    assert!(read.contains("#2  w1  re #1  [main]"), "{read}");
    assert!(
        read.contains("    | ... 8 more lines: deciduous board show 2"),
        "{read}"
    );

    let show = sb.dx_ok(&dir, &["board", "show", "2"]);
    assert!(show.contains("    | l20\n"), "{show}");
    let show = json_of(&sb.dx_ok(&dir, &["board", "show", "1", "--json"]));
    assert_eq!(show["author"], "lead");
    assert!(sb
        .dx_err(&dir, &["board", "show", "9"])
        .contains("no message 9"));

    // w1 answered #1 with a reply; w2 did not.
    let un = sb.dx_ok(&dir, &["board", "read", "--unanswered", "w2"]);
    assert!(
        un.starts_with("#1  lead -> w1, w2") && !un.contains("#2"),
        "{un}"
    );
    assert_eq!(
        sb.dx_ok(&dir, &["board", "read", "--unanswered", "w1"])
            .trim(),
        "Nothing addressed to w1 is waiting for an answer."
    );
    let r = json_of(&sb.dx_ok(&dir, &["board", "read", "--from", "w1", "--json"]));
    assert_eq!(ids(&r), [2]);
    let r = json_of(&sb.dx_ok(
        &dir,
        &["board", "read", "--to", "w2", "-q", "PORT", "--json"],
    ));
    assert_eq!(ids(&r), [1]);
    let r = json_of(&sb.dx_ok(&dir, &["board", "read", "--since", "1", "--json"]));
    assert_eq!((ids(&r), r["latest_id"].as_i64()), (vec![2], Some(2)));
    let r = sb.dx_ok(&dir, &["board", "read", "--limit", "1"]);
    assert!(
        r.contains("More messages match: rerun with --since 1"),
        "{r}"
    );

    let e = sb.dx_err(
        &dir,
        &[
            "board",
            "post",
            "--as",
            "w1",
            "-s",
            "x",
            "-m",
            "y",
            "--reply-to",
            "77",
        ],
    );
    assert!(e.contains("reply_to 77: no such message"), "{e}");
}

/// What the stub server saw.
#[derive(Default)]
struct Seen {
    requests: Vec<(String, String, String, Option<String>)>,
}

/// A server with `/messages` answering like the 1.0.9 server, or, with
/// `board = false`, like one that predates it.
fn stub(board: bool) -> (String, Arc<Mutex<Seen>>) {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let port = server.server_addr().to_ip().unwrap().port();
    let seen = Arc::new(Mutex::new(Seen::default()));
    let log = seen.clone();
    std::thread::spawn(move || {
        for mut req in server.incoming_requests() {
            let mut body = String::new();
            let _ = req.as_reader().read_to_string(&mut body);
            let url = req.url().to_string();
            let auth = req
                .headers()
                .iter()
                .find(|h| h.field.equiv("authorization"))
                .map(|h| h.value.to_string());
            log.lock().unwrap().requests.push((
                req.method().to_string(),
                url.clone(),
                body.clone(),
                auth,
            ));
            let path = url.split('?').next().unwrap_or("").to_string();
            let (code, reply) = match (board, req.method().to_string().as_str(), path.as_str()) {
                (true, "POST", "/messages") => {
                    let v: Value = serde_json::from_str(&body).unwrap();
                    if v.get("reply_to") == Some(&json!(99)) {
                        (
                            422,
                            json!({"error": "reply_to 99: no message 99 in this workspace"}),
                        )
                    } else {
                        (
                            201,
                            json!({"id": 7, "mentions": ["w1"], "reply_to": null,
                                   "created_at": "2026-09-26T18:00:00.123456Z"}),
                        )
                    }
                }
                (true, "GET", "/messages") => (
                    200,
                    json!({"messages": [{"id": 7, "branch": null, "author": "lead",
                        "subject": "from the server", "body": "@w1 hi", "mentions": ["w1"],
                        "reply_to": null, "created_at": "2026-09-26T18:00:00.123456Z"}],
                        "latest_id": 7, "truncated": false}),
                ),
                _ => (404, json!({"error": "Not Found"})),
            };
            let resp = tiny_http::Response::from_string(reply.to_string())
                .with_status_code(code)
                .with_header(
                    "content-type: application/json"
                        .parse::<tiny_http::Header>()
                        .unwrap(),
                );
            let _ = req.respond(resp);
        }
    });
    (format!("http://127.0.0.1:{port}"), seen)
}

fn remote_repo(sb: &Sandbox, url: &str) -> PathBuf {
    let dir = sb.repo("proj");
    std::fs::write(
        dir.join(".deciduous/config.toml"),
        format!("[remote]\nurl = \"{url}\"\nworkspace = \"proj-ws\"\n"),
    )
    .unwrap();
    dir
}

#[test]
fn remote_mode_uses_the_servers_messages_endpoint() {
    let sb = Sandbox::new();
    let (url, seen) = stub(true);
    let dir = remote_repo(&sb, &url);

    let out = sb.dx_ok(
        &dir,
        &[
            "board", "post", "--as", "lead", "-s", "hi", "-m", "@w1 hi", "--branch", "b1",
        ],
    );
    assert_eq!(out.trim(), "Posted #7, to w1");
    let r = json_of(&sb.dx_ok(
        &dir,
        &[
            "board",
            "read",
            "--unanswered",
            "w1",
            "--since",
            "3",
            "--json",
        ],
    ));
    assert_eq!(r["messages"][0]["subject"], "from the server");
    let e = sb.dx_err(
        &dir,
        &[
            "board",
            "post",
            "--as",
            "w1",
            "-s",
            "re",
            "-m",
            "x",
            "--reply-to",
            "99",
        ],
    );
    assert!(e.contains("422") && e.contains("no message 99"), "{e}");

    let seen = seen.lock().unwrap();
    let (m, u, body, auth) = &seen.requests[0];
    assert_eq!((m.as_str(), u.as_str()), ("POST", "/messages"));
    assert_eq!(
        json_of(body),
        json!({"workspace": "proj-ws", "author": "lead", "subject": "hi",
               "body": "@w1 hi", "branch": "b1"})
    );
    assert_eq!(
        auth.as_deref(),
        Some("Bearer 0123456789abcdef0123456789abcdef")
    );
    let (m, u, _, _) = &seen.requests[1];
    assert_eq!(m, "GET");
    assert_eq!(
        u,
        "/messages?workspace=proj-ws&since_id=3&unanswered_for=w1"
    );

    // Nothing went to the local table.
    let conn = rusqlite::Connection::open(dir.join(".deciduous/deciduous.db")).unwrap();
    let n: i64 = conn
        .query_row("SELECT count(*) FROM agent_messages", [], |r| r.get(0))
        .unwrap_or(0);
    assert_eq!(n, 0);
}

#[test]
fn a_server_without_the_board_is_an_error_naming_the_version_not_a_local_fallback() {
    let sb = Sandbox::new();
    let (url, _) = stub(false);
    let dir = remote_repo(&sb, &url);
    for args in [
        &["board", "post", "--as", "lead", "-s", "hi", "-m", "@w1"][..],
        &["board", "read"][..],
    ] {
        let e = sb.dx_err(&dir, args);
        assert!(
            e.contains("404")
                && e.contains("server 1.0.9 or later")
                && e.contains("Nothing was written locally"),
            "{e}"
        );
    }
    let conn = rusqlite::Connection::open(dir.join(".deciduous/deciduous.db")).unwrap();
    let n: i64 = conn
        .query_row("SELECT count(*) FROM agent_messages", [], |r| r.get(0))
        .unwrap_or(0);
    assert_eq!(n, 0);
}
