//! End-to-end multi-user sync through the CLI.
//!
//! Two "machines" each have their own database. Their graph files are merged
//! into each other by hand, standing in for `git push` / `git pull` — which
//! is what git does too, by calling `deciduous merge-record`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

struct Machine {
    _dir: TempDir,
    db: PathBuf,
    graph: PathBuf,
    /// Where the 0.17 per-record directory would sit, for migration tests.
    legacy: PathBuf,
}

impl Machine {
    fn new() -> Self {
        let dir = TempDir::new().unwrap();
        let db = dir.path().join("deciduous.db");
        let graph = dir.path().join("graph.json");
        let legacy = dir.path().join("sync");
        Self {
            _dir: dir,
            db,
            graph,
            legacy,
        }
    }

    fn doc(&self) -> serde_json::Value {
        serde_json::from_str(&fs::read_to_string(&self.graph).unwrap()).unwrap()
    }

    fn records(&self, kind: &str) -> serde_json::Map<String, serde_json::Value> {
        self.doc()[kind].as_object().cloned().unwrap_or_default()
    }

    fn run(&self, args: &[&str]) -> (bool, String, String) {
        let out = Command::new(env!("CARGO_BIN_EXE_deciduous"))
            .args(args)
            .env("DECIDUOUS_DB_PATH", &self.db)
            .output()
            .expect("failed to run deciduous");
        (
            out.status.success(),
            String::from_utf8_lossy(&out.stdout).to_string(),
            String::from_utf8_lossy(&out.stderr).to_string(),
        )
    }

    fn ok(&self, args: &[&str]) -> String {
        let (success, stdout, stderr) = self.run(args);
        assert!(success, "{:?} failed:\n{}\n{}", args, stdout, stderr);
        stdout
    }

    fn graph(&self) -> serde_json::Value {
        serde_json::from_str(&self.ok(&["graph"])).unwrap()
    }

    fn node_count(&self) -> usize {
        self.graph()["nodes"].as_array().unwrap().len()
    }

    fn edge_count(&self) -> usize {
        self.graph()["edges"].as_array().unwrap().len()
    }

    /// Simulate `git pull` from another machine. With one shared file this
    /// is a merge, not a copy, and it goes through the same code path git
    /// calls: `deciduous merge-record <base> <ours> <theirs>`.
    fn pull_from(&self, other: &Machine) {
        if !self.graph.exists() {
            fs::copy(&other.graph, &self.graph).unwrap();
            return;
        }
        let base = self.graph.with_extension("base.json");
        fs::write(&base, "").unwrap(); // no known ancestor
        let out = Command::new(env!("CARGO_BIN_EXE_deciduous"))
            .args(["merge-record"])
            .arg(&base)
            .arg(&self.graph)
            .arg(&other.graph)
            .output()
            .expect("failed to run deciduous merge-record");
        assert!(
            out.status.success(),
            "merge-record failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

fn parse_created_id(stdout: &str) -> i32 {
    // "Created node 3 (type: ..." / "Created edge 1 (..."
    stdout
        .split_whitespace()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .expect("no id in output")
}

#[test]
fn two_machines_share_a_graph_and_link_across_it() {
    let alice = Machine::new();
    let bob = Machine::new();

    // Alice logs a goal before sync exists, then syncs: the store is created
    // and her node exported.
    let goal_id =
        parse_created_id(&alice.ok(&["add", "goal", "Rate limit the public API", "-c", "90"]));
    let out = alice.ok(&["sync", "--no-pages"]);
    assert!(out.contains("Created"), "{out}");
    assert!(out.contains("1 nodes exported"), "{out}");
    assert!(alice.graph.is_file());
    assert_eq!(alice.records("nodes").len(), 1);

    // Once the file exists, writes publish immediately (no sync needed).
    alice.ok(&["status", &goal_id.to_string(), "active"]);
    let nodes = alice.records("nodes");
    let rec = nodes.values().next().unwrap();
    assert_eq!(rec["status"], "active", "{rec}");

    // Bob pulls and syncs: he gets Alice's goal under his own local id.
    bob.pull_from(&alice);
    let out = bob.ok(&["sync", "--no-pages"]);
    assert!(out.contains("1 nodes imported"), "{out}");
    let goal = bob.graph()["nodes"][0].clone();
    assert_eq!(goal["title"], "Rate limit the public API");
    assert_eq!(goal["status"], "active");
    let goal_cid = goal["change_id"].as_str().unwrap().to_string();
    let prefix = &goal_cid[..8];

    // `nodes` shows the change_id prefix Bob should use to refer to it.
    let listing = bob.ok(&["nodes"]);
    assert!(listing.contains("CHANGE"), "{listing}");
    assert!(listing.contains(prefix), "{listing}");

    // Bob adds an action and links it to Alice's goal by prefix, not local id.
    // An all-digit prefix (about 2% of uuids) reads as a local id by design,
    // so name it with its hyphen then: still a prefix, never a number.
    let prefix = if prefix.chars().all(|c| c.is_ascii_digit()) {
        &goal_cid[..9]
    } else {
        prefix
    };
    let action_id = parse_created_id(&bob.ok(&["add", "action", "Add token bucket middleware"]));
    let out = bob.ok(&[
        "link",
        prefix,
        &action_id.to_string(),
        "-r",
        "implements the goal",
    ]);
    assert!(out.contains("Created edge"), "{out}");
    let shown = bob.ok(&["show", prefix]);
    assert!(shown.contains("Rate limit the public API"), "{shown}");

    // Records were written as he went; sync has nothing left to do.
    let out = bob.ok(&["sync", "--check"]);
    assert!(out.contains("already agree"), "{out}");

    // Alice pulls Bob's records: the edge resolves against her local ids.
    alice.pull_from(&bob);
    let out = alice.ok(&["sync", "--no-pages"]);
    assert!(
        out.contains("1 nodes imported") && out.contains("1 edges imported"),
        "{out}"
    );
    assert_eq!(alice.node_count(), 2);
    assert_eq!(alice.edge_count(), 1);
    let edge = alice.graph()["edges"][0].clone();
    assert_eq!(edge["from_change_id"], goal_cid);
    assert_eq!(edge["rationale"], "implements the goal");

    // A second sync is a no-op and --check exits 0.
    let (success, out, _) = alice.run(&["sync", "--check"]);
    assert!(success, "{out}");
    assert!(out.contains("already agree"), "{out}");

    // Bob deletes his action; the tombstone reaches Alice and cascades.
    bob.ok(&["delete", &action_id.to_string()]);
    alice.pull_from(&bob);
    let (success, out, _) = alice.run(&["sync", "--check"]);
    assert!(
        !success,
        "check must exit 1 while a change is pending:\n{out}"
    );
    let out = alice.ok(&["sync", "--no-pages"]);
    assert!(out.contains("1 nodes deleted"), "{out}");
    assert_eq!(alice.node_count(), 1);
    assert_eq!(alice.edge_count(), 0);
}

#[test]
fn ambiguous_or_unknown_prefix_is_a_clear_error() {
    let m = Machine::new();
    m.ok(&["add", "goal", "Something"]);
    let (success, _, stderr) = m.run(&["show", "ffffffff"]);
    assert!(!success);
    assert!(
        stderr.contains("No node has a change_id starting with"),
        "{stderr}"
    );
    let (success, _, stderr) = m.run(&["status", "zz", "active"]);
    assert!(!success);
    assert!(stderr.contains("not a node id"), "{stderr}");
}

#[test]
fn legacy_jsonl_log_is_imported_once_then_removed() {
    let m = Machine::new();
    let events = m.legacy.join("events");
    fs::create_dir_all(&events).unwrap();
    // Two objects glued on one line, as the old appender could produce.
    fs::write(
        events.join("Alice.jsonl"),
        concat!(
            r#"{"op":"add_node","change_id":"aaaa1111-aaaa-4aaa-8aaa-aaaaaaaaaaaa","node_type":"goal","title":"Old goal","description":null,"status":"pending","metadata_json":"{\"confidence\":80}","timestamp":1784149119459,"author":"Alice"}"#,
            r#"{"op":"add_node","change_id":"bbbb2222-bbbb-4bbb-8bbb-bbbbbbbbbbbb","node_type":"action","title":"Old action","description":null,"status":"pending","metadata_json":null,"timestamp":1784149119460,"author":"Alice"}"#,
            "\n",
            r#"{"op":"add_edge","edge_id":"edge-x","from_change_id":"aaaa1111-aaaa-4aaa-8aaa-aaaaaaaaaaaa","to_change_id":"bbbb2222-bbbb-4bbb-8bbb-bbbbbbbbbbbb","edge_type":"leads_to","rationale":"then","timestamp":1784149119470,"author":"Alice"}"#,
            "\n",
            r#"{"op":"update_node","change_id":"aaaa1111-aaaa-4aaa-8aaa-aaaaaaaaaaaa","title":null,"description":null,"status":"completed","metadata_json":null,"timestamp":1784149119480,"author":"Alice"}"#,
            "\n",
        ),
    )
    .unwrap();

    let out = m.ok(&["sync", "--no-pages"]);
    assert!(out.contains("Imported legacy event log: 4 events"), "{out}");
    assert!(
        out.contains("2 nodes imported") && out.contains("1 edges imported"),
        "{out}"
    );
    assert!(
        !events.exists(),
        "legacy log should be removed after a clean import"
    );
    assert_eq!(m.node_count(), 2);
    assert_eq!(m.edge_count(), 1);
    let goal = m.ok(&["show", "aaaa1111"]);
    assert!(goal.contains("completed"), "{goal}");

    // The deprecated command still answers, pointing at sync.
    let (success, out, stderr) = m.run(&["events", "status"]);
    assert!(success, "{stderr}");
    assert!(stderr.contains("deprecated"), "{stderr}");
    assert!(out.contains("already agree"), "{out}");
}

/// Run git in `repo`, panicking with output on failure.
fn git(repo: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(repo)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .output()
        .expect("git not runnable");
    assert!(
        out.status.success(),
        "git {:?} failed:\n{}\n{}",
        args,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn node_json(change_id: &str, title: &str, status: &str, updated: &str) -> serde_json::Value {
    serde_json::json!({
        "change_id": change_id,
        "node_type": "goal",
        "title": title,
        "status": status,
        "metadata": {"confidence": 80},
        "created_at": "2026-01-01T00:00:00+00:00",
        "updated_at": updated,
    })
}

fn write_doc(path: &Path, nodes: serde_json::Value) {
    let doc = serde_json::json!({"version": 1, "nodes": nodes, "edges": {}});
    fs::write(
        path,
        format!("{}\n", serde_json::to_string_pretty(&doc).unwrap()),
    )
    .unwrap();
}

/// The whole reason one shared file is workable: git hands both versions to
/// `deciduous merge-record`, which merges the document record by record.
/// Two people adding different nodes must both keep theirs, and the one
/// record they both edited must merge field by field.
#[test]
fn git_merges_concurrent_edits_of_the_graph_file_through_the_driver() {
    let dir = TempDir::new().unwrap();
    let repo = dir.path();
    git(repo, &["init", "-q", "-b", "main"]);
    // What `deciduous init`/`update`/`sync` write, but pointing at this test binary.
    fs::write(
        repo.join(".gitattributes"),
        ".deciduous/graph.json merge=deciduous linguist-generated=true\n",
    )
    .unwrap();
    git(
        repo,
        &[
            "config",
            "merge.deciduous.driver",
            &format!("{} merge-record %O %A %B", env!("CARGO_BIN_EXE_deciduous")),
        ],
    );

    fs::create_dir_all(repo.join(".deciduous")).unwrap();
    let graph = repo.join(".deciduous/graph.json");
    write_doc(
        &graph,
        serde_json::json!({
            "n1": node_json("n1", "Goal", "pending", "2026-01-01T00:00:00+00:00")
        }),
    );
    git(repo, &["add", "."]);
    git(repo, &["commit", "-q", "-m", "base"]);

    // Alice, on main: activates n1 and adds a node of her own.
    write_doc(
        &graph,
        serde_json::json!({
            "n1": node_json("n1", "Goal", "active", "2026-01-03T00:00:00+00:00"),
            "alice": node_json("alice", "Alice's node", "pending", "2026-01-03T00:00:00+00:00"),
        }),
    );
    git(repo, &["commit", "-q", "-am", "alice: activate + add"]);

    // Bob, on a branch from base: renames n1 and adds a node of his own.
    git(repo, &["checkout", "-q", "-b", "bob", "HEAD~1"]);
    write_doc(
        &graph,
        serde_json::json!({
            "n1": node_json("n1", "Goal, renamed", "pending", "2026-01-02T00:00:00+00:00"),
            "bob": node_json("bob", "Bob's node", "pending", "2026-01-02T00:00:00+00:00"),
        }),
    );
    git(repo, &["commit", "-q", "-am", "bob: rename + add"]);

    // Merge Alice's branch into Bob's: the one file changed on both sides.
    git(repo, &["merge", "-q", "--no-edit", "main"]);

    let merged = fs::read_to_string(&graph).unwrap();
    assert!(!merged.contains("<<<<<<<"), "{merged}");
    let v: serde_json::Value = serde_json::from_str(&merged).unwrap();
    // Neither side's addition is lost.
    assert_eq!(v["nodes"]["alice"]["title"], "Alice's node", "{merged}");
    assert_eq!(v["nodes"]["bob"]["title"], "Bob's node", "{merged}");
    // The record both touched merges field by field: Alice is newer, so her
    // status wins; Bob's rename is the only change to the title, so it stays.
    assert_eq!(v["nodes"]["n1"]["status"], "active", "{merged}");
    assert_eq!(v["nodes"]["n1"]["title"], "Goal, renamed", "{merged}");
    assert_eq!(v["nodes"]["n1"]["updated_at"], "2026-01-03T00:00:00+00:00");
    assert!(
        git(repo, &["status", "--porcelain"]).trim().is_empty(),
        "merge should have committed cleanly"
    );
}

#[test]
fn sync_repairs_conflict_markers_left_by_a_merge_without_the_driver() {
    let m = Machine::new();
    m.ok(&["add", "goal", "Seed"]);
    m.ok(&["sync", "--no-pages"]);

    // A clone without `merge.deciduous.driver` configured gets this instead.
    fs::write(
        &m.graph,
        concat!(
            "{\n",
            "  \"edges\": {},\n",
            "  \"nodes\": {\n",
            "    \"c0ffee11\": {\n",
            "      \"change_id\": \"c0ffee11\",\n",
            "      \"created_at\": \"2026-01-01T00:00:00+00:00\",\n",
            "      \"node_type\": \"action\",\n",
            "<<<<<<< HEAD\n",
            "      \"status\": \"completed\",\n",
            "      \"title\": \"Do the thing\",\n",
            "      \"updated_at\": \"2026-01-04T00:00:00+00:00\"\n",
            "=======\n",
            "      \"status\": \"pending\",\n",
            "      \"title\": \"Do the thing, carefully\",\n",
            "      \"updated_at\": \"2026-01-02T00:00:00+00:00\"\n",
            ">>>>>>> theirs\n",
            "    }\n",
            "  },\n",
            "  \"version\": 1\n",
            "}\n"
        ),
    )
    .unwrap();

    let (ok, out, _) = m.run(&["sync", "--check"]);
    assert!(!ok);
    assert!(
        out.contains("Conflict:") && out.contains("graph.json"),
        "{out}"
    );

    let out = m.ok(&["sync", "--no-pages"]);
    assert!(
        out.contains("Merged") && out.contains("graph.json"),
        "{out}"
    );
    let shown = m.ok(&["show", "c0ffee11"]);
    assert!(shown.contains("completed"), "{shown}");
    assert!(shown.contains("Do the thing"), "{shown}");
    let (ok, out, _) = m.run(&["sync", "--check"]);
    assert!(ok, "{out}");
}

/// Upgrading from 0.17: the directory of per-record files is folded into the
/// graph file and removed.
#[test]
fn a_0_17_record_directory_is_migrated_on_sync() {
    let m = Machine::new();
    fs::create_dir_all(m.legacy.join("nodes")).unwrap();
    fs::create_dir_all(m.legacy.join("edges")).unwrap();
    for (id, title) in [
        ("aaaa1111-aaaa-4aaa-8aaa-aaaaaaaaaaaa", "Old goal"),
        ("bbbb2222-bbbb-4bbb-8bbb-bbbbbbbbbbbb", "Old action"),
    ] {
        fs::write(
            m.legacy.join("nodes").join(format!("{id}.json")),
            serde_json::to_string_pretty(&node_json(
                id,
                title,
                "pending",
                "2026-01-01T00:00:00+00:00",
            ))
            .unwrap(),
        )
        .unwrap();
    }

    let out = m.ok(&["sync", "--no-pages"]);
    assert!(out.contains("Folded"), "{out}");
    assert!(out.contains("2 nodes"), "{out}");
    assert!(!m.legacy.exists(), "the 0.17 directory should be removed");
    assert_eq!(m.records("nodes").len(), 2);
    assert_eq!(m.node_count(), 2);

    // And it is a one-time thing: a second sync has nothing to do.
    let (ok, out, _) = m.run(&["sync", "--check"]);
    assert!(ok, "{out}");
    assert!(out.contains("already agree"), "{out}");
}
