//! Multi-user sync through real git: a bare origin, several clones, the real
//! `deciduous` binary as the CLI and as the registered merge driver.
//!
//! `sync_integration.rs` stands in for git by calling `merge-record` by hand.
//! These tests do not: they `git pull`, `git merge`, `git checkout`, so what
//! they check is what a team actually sees, including the cases where git
//! does something other than what its documentation suggests.

#![allow(dead_code)] // helpers are shared by tests added one finding at a time

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

const BIN: &str = env!("CARGO_BIN_EXE_deciduous");

/// A shared origin and a scratch home. Every clone lives under one TempDir.
struct Team {
    root: TempDir,
}

/// One developer's clone.
struct Dev {
    name: String,
    dir: PathBuf,
    home: PathBuf,
    /// PATH for every process this developer runs. By default the
    /// directory holding the deciduous under test comes first, so git
    /// finds the merge driver the way it would after `cargo install`.
    path: String,
}

/// git and a shell, and no deciduous: an installed one (Homebrew, cargo)
/// must never stand in for the binary under test.
fn base_path() -> String {
    let git = Command::new("sh")
        .args(["-c", "command -v git"])
        .output()
        .unwrap();
    let git = String::from_utf8_lossy(&git.stdout).trim().to_string();
    let git_dir = Path::new(&git).parent().unwrap().display().to_string();
    assert!(
        !Path::new(&git_dir).join("deciduous").exists(),
        "git shares a directory with an installed deciduous ({git_dir})"
    );
    format!("{git_dir}:/usr/bin:/bin:/usr/sbin:/sbin")
}

fn with_bin_on_path() -> String {
    let bin_dir = Path::new(BIN).parent().unwrap();
    format!("{}:{}", bin_dir.display(), base_path())
}

impl Team {
    fn new() -> Self {
        let root = TempDir::new().unwrap();
        let origin = root.path().join("origin.git");
        let out = Command::new("git")
            .args(["init", "--bare", "-q", "-b", "main"])
            .arg(&origin)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        Self { root }
    }

    fn origin(&self) -> PathBuf {
        self.root.path().join("origin.git")
    }

    /// The first developer: creates the project, runs `deciduous sync` once
    /// (which creates the graph file and registers the driver), commits the
    /// setup and pushes it.
    fn founder(&self, name: &str) -> Dev {
        let dev = self.dev(name);
        fs::create_dir_all(&dev.dir).unwrap();
        dev.git(&["init", "-q", "-b", "main"]);
        dev.configure();
        fs::create_dir_all(dev.dir.join(".deciduous")).unwrap();
        fs::write(
            dev.dir.join(".gitattributes"),
            ".deciduous/graph.json merge=deciduous linguist-generated=true\n",
        )
        .unwrap();
        fs::write(
            dev.dir.join(".gitignore"),
            ".deciduous/*.db\n.deciduous/*.db-*\n",
        )
        .unwrap();
        dev.ok(&["sync"]);
        dev.commit_all("setup");
        dev.git(&["remote", "add", "origin", self.origin().to_str().unwrap()]);
        dev.git(&["push", "-q", "-u", "origin", "main"]);
        dev
    }

    /// Everyone else clones and syncs once.
    fn join(&self, name: &str) -> Dev {
        let dev = self.dev(name);
        let out = Command::new("git")
            .args(["clone", "-q"])
            .arg(self.origin())
            .arg(&dev.dir)
            .env("HOME", &dev.home)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        dev.configure();
        dev.ok(&["sync"]);
        dev
    }

    fn dev(&self, name: &str) -> Dev {
        let home = self.root.path().join("homes").join(name);
        fs::create_dir_all(&home).unwrap();
        Dev {
            name: name.to_string(),
            dir: self.root.path().join(name),
            home,
            path: with_bin_on_path(),
        }
    }
}

impl Dev {
    /// The same clone, to change how its processes run (e.g. PATH).
    fn clone_handle(&self) -> Dev {
        Dev {
            name: self.name.clone(),
            dir: self.dir.clone(),
            home: self.home.clone(),
            path: self.path.clone(),
        }
    }

    fn configure(&self) {
        self.git(&["config", "user.name", &self.name]);
        self.git(&[
            "config",
            "user.email",
            &format!("{}@example.test", self.name),
        ]);
        self.git(&["config", "pull.rebase", "false"]);
        self.git(&["config", "commit.gpgsign", "false"]);
    }

    fn cmd(&self, program: &str) -> Command {
        let mut c = Command::new(program);
        c.current_dir(&self.dir)
            .env("HOME", &self.home)
            .env("PATH", &self.path)
            .env("DECIDUOUS_NO_SERVER", "1")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env_remove("DECIDUOUS_DB_PATH")
            .env_remove("DECIDUOUS_MCP_TOKEN")
            .env_remove("DECIDUOUS_API_TOKEN")
            .env_remove("XDG_CONFIG_HOME")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE");
        c
    }

    fn run(&self, args: &[&str]) -> Output {
        self.cmd(BIN).args(args).output().expect("run deciduous")
    }

    fn ok(&self, args: &[&str]) -> String {
        let out = self.run(args);
        assert!(
            out.status.success(),
            "[{}] deciduous {:?} failed:\n{}\n{}",
            self.name,
            args,
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).to_string()
    }

    fn fails(&self, args: &[&str]) -> (String, String) {
        let out = self.run(args);
        assert!(
            !out.status.success(),
            "[{}] deciduous {:?} should have failed:\n{}",
            self.name,
            args,
            String::from_utf8_lossy(&out.stdout)
        );
        (
            String::from_utf8_lossy(&out.stdout).to_string(),
            String::from_utf8_lossy(&out.stderr).to_string(),
        )
    }

    fn git_out(&self, args: &[&str]) -> Output {
        self.cmd("git").args(args).output().expect("run git")
    }

    fn git(&self, args: &[&str]) -> String {
        let out = self.git_out(args);
        assert!(
            out.status.success(),
            "[{}] git {:?} failed:\n{}\n{}",
            self.name,
            args,
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).to_string()
    }

    fn commit_all(&self, msg: &str) {
        self.git(&[
            "add",
            ".gitattributes",
            ".gitignore",
            ".deciduous/graph.json",
        ]);
        let out = self.git_out(&["commit", "-q", "-m", msg]);
        // Nothing to commit is fine: the graph may already be committed.
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(
            out.status.success() || text.contains("nothing to commit"),
            "[{}] commit failed: {}{}",
            self.name,
            text,
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn commit_graph(&self, msg: &str) {
        self.git(&["add", ".deciduous/graph.json"]);
        self.git(&["commit", "-q", "-m", msg]);
    }

    fn graph_path(&self) -> PathBuf {
        self.dir.join(".deciduous").join("graph.json")
    }

    fn graph_text(&self) -> String {
        fs::read_to_string(self.graph_path()).unwrap()
    }

    fn doc(&self) -> Value {
        serde_json::from_str(&self.graph_text()).unwrap()
    }

    fn write_doc(&self, doc: &Value) {
        let mut s = serde_json::to_string_pretty(doc).unwrap();
        s.push('\n');
        fs::write(self.graph_path(), s).unwrap();
    }

    /// Add a node and return its local id.
    fn add(&self, node_type: &str, title: &str, extra: &[&str]) -> i32 {
        let mut args = vec!["add", node_type, title];
        args.extend_from_slice(extra);
        let out = self.ok(&args);
        out.split_whitespace()
            .nth(2)
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(|| panic!("no id in: {out}"))
    }

    fn change_id(&self, id: i32) -> String {
        let shown: Value =
            serde_json::from_str(&self.ok(&["show", &id.to_string(), "--json"])).unwrap();
        shown["change_id"]
            .as_str()
            .or_else(|| shown["node"]["change_id"].as_str())
            .unwrap_or_else(|| panic!("no change_id in {shown}"))
            .to_string()
    }

    /// Nodes as the CLI reports them: (change_id, title, status).
    fn nodes(&self) -> Vec<Value> {
        let graph: Value = serde_json::from_str(&self.ok(&["graph"])).unwrap();
        graph["nodes"].as_array().cloned().unwrap_or_default()
    }

    fn node_by_title(&self, title: &str) -> Option<Value> {
        self.nodes().into_iter().find(|n| n["title"] == title)
    }
}

/// A node record as it sits in graph.json, for seeding teammates' records.
fn node_record(change_id: &str, title: &str, updated_at: &str) -> Value {
    serde_json::json!({
        "change_id": change_id,
        "node_type": "goal",
        "title": title,
        "status": "pending",
        "created_at": "2026-01-01T00:00:00+00:00",
        "updated_at": updated_at,
        "author": "teammate",
    })
}

// ============================================================================
// G3: a change_id prefix made of digits
// ============================================================================

#[test]
fn a_digit_only_change_id_prefix_is_never_silently_a_local_id() {
    let team = Team::new();
    let alice = team.founder("alice");
    let one = alice.add("goal", "local one", &[]);
    let two = alice.add("goal", "local two", &[]);
    assert_eq!((one, two), (1, 2));

    // A teammate's nodes arrive through the graph file. About 15% of
    // change_ids start with four digits and 2.5% with eight.
    let mut doc = alice.doc();
    let nodes = doc["nodes"].as_object_mut().unwrap();
    for (cid, title) in [
        ("0002abcd-1111-4111-8111-111111111111", "teammate target"),
        ("37650685-2222-4222-8222-222222222222", "all digits"),
        ("c0ffee00", "short id"),
        ("c0ffee00-4444-4444-8444-444444444444", "long id"),
    ] {
        nodes.insert(
            cid.into(),
            node_record(cid, title, "2026-01-02T00:00:00+00:00"),
        );
    }
    alice.write_doc(&doc);
    let out = alice.ok(&["sync"]);
    assert!(out.contains("4 nodes imported"), "{out}");

    // "0002" is local #2 and the prefix of the teammate's node. Deleting
    // either on a guess would tombstone it for everyone.
    let (_, err) = alice.fails(&["delete", "0002"]);
    assert!(err.contains("#2") && err.contains("local two"), "{err}");
    assert!(
        err.contains("0002abcd") && err.contains("teammate target"),
        "{err}"
    );
    assert!(alice.node_by_title("local two").is_some());
    assert!(alice.node_by_title("teammate target").is_some());

    // Exactly the CHANGE column string, which is all digits: the node, not
    // "Node #37650685 not found".
    let shown = alice.ok(&["show", "37650685"]);
    assert!(shown.contains("all digits"), "{shown}");

    // `#` always means a local id; short numbers are local ids.
    assert!(alice.ok(&["show", "#2"]).contains("local two"));
    assert!(alice.ok(&["show", "2"]).contains("local two"));
    // A four-digit number with no matching change_id is still an id.
    let (_, err) = alice.fails(&["show", "9999"]);
    assert!(err.contains("9999"), "{err}");

    // A change_id that is a prefix of another is reachable by its full id.
    let shown = alice.ok(&["show", "c0ffee00"]);
    assert!(shown.contains("short id"), "{shown}");
    let shown = alice.ok(&["show", "c0ffee00-4444"]);
    assert!(shown.contains("long id"), "{shown}");
}

// ============================================================================
// G4: fields this version does not know about
// ============================================================================

#[test]
fn fields_this_version_does_not_know_survive_every_write_and_merge() {
    let team = Team::new();
    let alice = team.founder("alice");
    let goal = alice.add("goal", "Ship it", &[]);
    let action = alice.add("action", "Write it", &[]);
    alice.ok(&["link", &goal.to_string(), &action.to_string(), "-r", "how"]);
    let goal_cid = alice.change_id(goal);

    // A newer deciduous (or a teammate's tool) wrote fields we do not model.
    let mut doc = alice.doc();
    doc["documents"] = serde_json::json!({"d1": {"node": goal_cid, "sha": "abc"}});
    let node = &mut doc["nodes"][&goal_cid];
    node["priority"] = "p1".into();
    node["reviewers"] = serde_json::json!(["bob", "carol"]);
    let edge_key = doc["edges"]
        .as_object()
        .unwrap()
        .keys()
        .next()
        .unwrap()
        .clone();
    doc["edges"][&edge_key]["confidence_note"] = "strong".into();
    alice.write_doc(&doc);
    alice.commit_graph("fields from the future");
    alice.git(&["push", "-q"]);

    // Unrelated writes on the same machine.
    alice.add("observation", "Unrelated", &[]);
    alice.ok(&["status", &goal.to_string(), "active"]);
    alice.ok(&["prompt", &goal.to_string(), "why"]);
    let doc = alice.doc();
    assert_eq!(doc["nodes"][&goal_cid]["priority"], "p1", "{doc:#}");
    assert_eq!(doc["nodes"][&goal_cid]["reviewers"][1], "carol", "{doc:#}");
    assert_eq!(doc["nodes"][&goal_cid]["status"], "active", "{doc:#}");
    assert_eq!(
        doc["edges"][&edge_key]["confidence_note"], "strong",
        "{doc:#}"
    );
    assert_eq!(doc["documents"]["d1"]["sha"], "abc", "{doc:#}");

    // Sync agrees with itself: the extra fields are not "differences" that
    // re-import on every run.
    alice.ok(&["sync"]);
    let out = alice.ok(&["sync", "--check"]);
    assert!(out.contains("already agree"), "{out}");
    alice.commit_graph("unrelated work");

    // Through a real merge with a teammate who touched the same records.
    let bob = team.join("bob");
    bob.ok(&["status", &goal_cid[..8], "completed"]);
    bob.add("goal", "Bob's own", &[]);
    bob.commit_graph("bob");
    bob.git(&["push", "-q"]);
    alice.git(&["pull", "-q", "--no-edit"]);
    let doc = alice.doc();
    assert_eq!(doc["nodes"][&goal_cid]["priority"], "p1", "{doc:#}");
    assert_eq!(
        doc["edges"][&edge_key]["confidence_note"], "strong",
        "{doc:#}"
    );
    assert_eq!(doc["documents"]["d1"]["sha"], "abc", "{doc:#}");
    alice.ok(&["sync"]);
    let doc = alice.doc();
    assert_eq!(doc["nodes"][&goal_cid]["priority"], "p1", "{doc:#}");
    assert_eq!(doc["nodes"][&goal_cid]["status"], "completed", "{doc:#}");
    assert_eq!(doc["documents"]["d1"]["sha"], "abc", "{doc:#}");
}

// ============================================================================
// G1: a record stamped in the future
// ============================================================================

fn status_of(dev: &Dev, reference: &str) -> String {
    let shown: Value = serde_json::from_str(&dev.ok(&["show", reference, "--json"])).unwrap();
    shown["status"]
        .as_str()
        .or_else(|| shown["node"]["status"].as_str())
        .unwrap_or_else(|| panic!("no status in {shown}"))
        .to_string()
}

#[test]
fn a_local_edit_is_never_reverted_by_a_future_updated_at() {
    let team = Team::new();
    let alice = team.founder("alice");

    // Backdating is for archaeology, but nothing stops a date ahead of now.
    let dated = alice.add("action", "dated", &["--date", "2099-01-01"]);
    let out = alice.ok(&["status", &dated.to_string(), "completed"]);
    assert!(out.to_lowercase().contains("updated"), "{out}");
    alice.ok(&["prompt", &dated.to_string(), "the real prompt"]);
    let out = alice.ok(&["sync"]);
    assert_eq!(status_of(&alice, &dated.to_string()), "completed", "{out}");
    let rec = alice.doc()["nodes"][alice.change_id(dated)].clone();
    assert_eq!(rec["status"], "completed", "{rec:#}");
    assert_eq!(rec["metadata"]["prompt"], "the real prompt", "{rec:#}");
    let out = alice.ok(&["sync", "--check"]);
    assert!(out.contains("already agree"), "{out}");

    // Deleting it sticks too: a tombstone older than the record's
    // updated_at used to lose to it, and the node came back.
    let doomed = alice.add("action", "doomed", &["--date", "2099-01-01"]);
    let doomed_cid = alice.change_id(doomed);
    alice.ok(&["delete", &doomed.to_string()]);
    alice.ok(&["sync"]);
    assert!(
        alice.node_by_title("doomed").is_none(),
        "deleted node came back"
    );
    assert!(
        alice.doc()["nodes"][&doomed_cid]["deleted_at"].is_string(),
        "{:#}",
        alice.doc()["nodes"][&doomed_cid]
    );

    // A teammate whose clock runs a day ahead.
    let goal = alice.add("goal", "Shared goal", &[]);
    let goal_cid = alice.change_id(goal);
    alice.commit_graph("goal");
    alice.git(&["push", "-q"]);
    let bob = team.join("bob");
    let mut doc = bob.doc();
    let tomorrow = (chrono_like_now_plus_days(1)).to_string();
    doc["nodes"][&goal_cid]["status"] = "active".into();
    doc["nodes"][&goal_cid]["updated_at"] = tomorrow.clone().into();
    bob.write_doc(&doc);
    bob.ok(&["sync"]);
    bob.commit_graph("skewed clock");
    bob.git(&["push", "-q"]);

    alice.git(&["pull", "-q", "--no-edit"]);
    alice.ok(&["sync"]);
    assert_eq!(status_of(&alice, &goal.to_string()), "active");
    // Alice decides later, by her clock and by causality.
    alice.ok(&["status", &goal.to_string(), "superseded"]);
    alice.ok(&["sync"]);
    assert_eq!(status_of(&alice, &goal.to_string()), "superseded");
    alice.commit_graph("supersede");
    alice.git(&["push", "-q"]);
    bob.git(&["pull", "-q", "--no-edit"]);
    bob.ok(&["sync"]);
    assert_eq!(status_of(&bob, &goal_cid[..8]), "superseded");
}

/// RFC 3339 timestamp `days` from now.
fn chrono_like_now_plus_days(days: i64) -> String {
    (chrono::Utc::now() + chrono::Duration::days(days)).to_rfc3339()
}

// ============================================================================
// G2: the merge driver fails, and git does not leave markers behind
// ============================================================================

fn titles(dev: &Dev) -> Vec<String> {
    let mut t: Vec<String> = dev
        .nodes()
        .iter()
        .map(|n| n["title"].as_str().unwrap().to_string())
        .collect();
    t.sort();
    t
}

fn unmerged(dev: &Dev) -> String {
    dev.git(&["ls-files", "-u", "--", ".deciduous/graph.json"])
}

#[test]
fn a_merge_driver_that_is_not_on_path_is_caught_and_finished_by_sync() {
    let team = Team::new();
    let alice = team.founder("alice");
    let shared = alice.add("goal", "Shared", &[]);
    alice.commit_graph("shared");
    alice.git(&["push", "-q"]);

    let bob = team.join("bob");
    bob.ok(&["status", &alice.change_id(shared)[..8], "completed"]);
    bob.add("goal", "Bob only", &[]);
    bob.commit_graph("bob");
    bob.git(&["push", "-q"]);

    alice.add("goal", "Alice only", &[]);
    alice.commit_graph("alice");

    // A GUI client or CI job: git is on PATH, deciduous is not. Git runs the
    // driver, the shell says "command not found", and git leaves our side
    // in the file with no markers and the index unmerged.
    let gui = Dev {
        path: base_path(),
        ..alice.clone_handle()
    };
    let out = gui.git_out(&["pull", "--no-edit"]);
    assert!(!out.status.success());
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(text.contains("CONFLICT"), "{text}");
    assert!(!alice.graph_text().contains("<<<<<<<"));
    assert!(!unmerged(&alice).is_empty());

    // The check must not call this clean: committing now drops Bob's work.
    let (out, _) = alice.fails(&["sync", "--check"]);
    assert!(out.contains("unmerged"), "{out}");

    // sync does the merge the driver should have done, from git's own three
    // versions, and marks the file resolved.
    let out = alice.ok(&["sync"]);
    assert!(out.contains("unmerged"), "{out}");
    assert!(unmerged(&alice).is_empty(), "still unmerged after sync");
    assert_eq!(
        titles(&alice),
        ["Alice only", "Bob only", "Shared"],
        "{out}"
    );
    assert_eq!(status_of(&alice, &shared.to_string()), "completed");
    alice.git(&["commit", "-q", "--no-edit"]);
    let out = alice.ok(&["sync", "--check"]);
    assert!(out.contains("already agree"), "{out}");
}

#[test]
fn a_committed_conflict_does_not_break_every_later_merge() {
    let team = Team::new();
    let alice = team.founder("alice");
    alice.add("goal", "Base", &[]);
    alice.commit_graph("base");
    alice.git(&["push", "-q"]);

    // Carol's clone has no driver registered (she cloned but never ran
    // deciduous), so her merge leaves markers, and she commits them.
    let carol = team.join("carol");
    carol.git(&["config", "--remove-section", "merge.deciduous"]);
    let bob = team.join("bob");
    // Both change the same record, so the text merge collides.
    bob.ok(&["status", "1", "active"]);
    bob.add("goal", "Bob first", &[]);
    bob.commit_graph("bob first");
    bob.git(&["push", "-q"]);
    carol.ok(&["status", "1", "completed"]);
    carol.add("goal", "Carol first", &[]);
    carol.commit_graph("carol first");
    let out = carol.git_out(&["pull", "--no-edit"]);
    assert!(
        !out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(
        carol.graph_text().contains("<<<<<<<"),
        "{}{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
        carol.graph_text()
    );
    carol.git(&["add", ".deciduous/graph.json"]);
    carol.git(&["commit", "-q", "--no-edit"]);
    carol.git(&["push", "-q"]);

    // Alice and Bob both build on the conflicted commit.
    bob.git(&["pull", "-q", "--no-edit"]);
    bob.ok(&["sync"]);
    bob.add("goal", "Bob after", &[]);
    bob.commit_graph("bob after");
    alice.git(&["pull", "-q", "--no-edit"]);
    alice.ok(&["sync"]);
    alice.add("goal", "Alice after", &[]);
    alice.commit_graph("alice after");
    alice.git(&["push", "-q"]);

    // The merge base is the commit with markers. The driver used to refuse
    // it ("key must be a string"), leaving Bob's side with no markers.
    let out = bob.git_out(&["pull", "--no-edit"]);
    assert!(
        out.status.success(),
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    bob.ok(&["sync"]);
    assert_eq!(
        titles(&bob),
        [
            "Alice after",
            "Base",
            "Bob after",
            "Bob first",
            "Carol first"
        ]
    );
}

// ============================================================================
// G5: sync --check as a pre-push guard
// ============================================================================

#[test]
fn sync_check_fails_while_edges_wait_or_records_are_unreadable() {
    let team = Team::new();
    let alice = team.founder("alice");
    let goal = alice.add("goal", "Here", &[]);
    let goal_cid = alice.change_id(goal);
    let out = alice.ok(&["sync", "--check"]);
    assert!(out.contains("already agree"), "{out}");
    let clean = alice.doc();

    // An edge to a node nobody has pushed yet: a teammate forgot a commit.
    let mut doc = clean.clone();
    let missing = "99999999-0000-4000-8000-000000000000";
    let id = deciduous::edge_id(&goal_cid, missing, "leads_to");
    doc["edges"][&id] = serde_json::json!({
        "edge_id": id,
        "from_change_id": goal_cid,
        "to_change_id": missing,
        "edge_type": "leads_to",
        "created_at": "2026-01-01T00:00:00+00:00",
    });
    alice.write_doc(&doc);
    let (out, _) = alice.fails(&["sync", "--check"]);
    assert!(out.contains("waits for node 99999999"), "{out}");
    // sync itself succeeds (it did what it could) and still says so.
    let out = alice.ok(&["sync"]);
    assert!(out.contains("waits for node 99999999"), "{out}");
    alice.fails(&["sync", "--check"]);

    // A record filed under the wrong key (a bad hand edit or merge).
    let mut doc = clean.clone();
    let mut rec = doc["nodes"][&goal_cid].clone();
    rec["change_id"] = "aaaaaaaa-0000-4000-8000-000000000000".into();
    doc["nodes"]["bbbbbbbb-0000-4000-8000-000000000000"] = rec;
    alice.write_doc(&doc);
    let (out, _) = alice.fails(&["sync", "--check"]);
    assert!(out.contains("could not be read"), "{out}");

    alice.write_doc(&clean);
    let out = alice.ok(&["sync", "--check"]);
    assert!(out.contains("already agree"), "{out}");
}

// ============================================================================
// G8: an edge's rationale changes after others have the edge
// ============================================================================

fn edges(dev: &Dev) -> Vec<Value> {
    let graph: Value = serde_json::from_str(&dev.ok(&["graph"])).unwrap();
    graph["edges"].as_array().cloned().unwrap_or_default()
}

#[test]
fn a_relinked_edge_carries_its_new_rationale_to_clones_that_had_the_edge() {
    let team = Team::new();
    let alice = team.founder("alice");
    let goal = alice.add("goal", "Pick a store", &[]);
    let option = alice.add("option", "Postgres", &[]);
    alice.ok(&[
        "link",
        &goal.to_string(),
        &option.to_string(),
        "-r",
        "option",
    ]);
    alice.commit_graph("option");
    alice.git(&["push", "-q"]);

    let carol = team.join("carol");
    assert_eq!(edges(&carol)[0]["rationale"], "option");

    // Alice rewrites the rationale the only way the CLI offers.
    alice.ok(&["unlink", &goal.to_string(), &option.to_string()]);
    alice.ok(&[
        "link",
        &goal.to_string(),
        &option.to_string(),
        "-r",
        "chosen",
    ]);
    alice.commit_graph("chosen");
    alice.git(&["push", "-q"]);

    carol.git(&["pull", "-q", "--no-edit"]);
    let out = carol.ok(&["sync"]);
    let e = edges(&carol);
    assert_eq!(e.len(), 1, "{out}");
    assert_eq!(e[0]["rationale"], "chosen", "{out}");
    let out = carol.ok(&["sync", "--check"]);
    assert!(out.contains("already agree"), "{out}");

    // And back: Carol relinks again. Alice's row was re-created later than
    // the file's record (a merge keeps the earliest created_at), which must
    // not make her stale "chosen" win over Carol's newer "final".
    carol.ok(&[
        "unlink",
        &alice_cid(&alice, goal),
        &alice_cid(&alice, option),
    ]);
    carol.ok(&[
        "link",
        &alice_cid(&alice, goal),
        &alice_cid(&alice, option),
        "-r",
        "final",
    ]);
    carol.commit_graph("final");
    carol.git(&["push", "-q"]);
    alice.git(&["pull", "-q", "--no-edit"]);
    let out = alice.ok(&["sync"]);
    assert_eq!(edges(&alice)[0]["rationale"], "final", "{out}");
    let rec = alice.doc()["edges"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap()
        .clone();
    assert_eq!(rec["rationale"], "final", "{rec:#}");
}

fn alice_cid(alice: &Dev, id: i32) -> String {
    alice.change_id(id)[..8].to_string()
}
