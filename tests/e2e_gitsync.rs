//! Multi-user sync through git, driven the way people drive it: separate
//! clones of one bare origin, the real `git pull` invoking the real merge
//! driver (whatever `deciduous` is first on PATH), and `deciduous sync`.
//!
//! Gated on `DECIDUOUS_E2E=1` (see `tests/e2e_support/mod.rs`).

mod e2e_support;

use e2e_support::*;
use serde_json::{json, Value};
use std::path::Path;

struct Team<'a> {
    alice: Project<'a>,
    bob: Project<'a>,
    origin: std::path::PathBuf,
}

fn team(sb: &Sandbox) -> Team<'_> {
    let origin = sb.origin("origin");
    let alice = sb.project("alice", Some(&origin));
    let bob = sb.clone_of(&origin, "bob");
    Team { alice, bob, origin }
}

fn node<'v>(doc: &'v Value, cid: &str) -> &'v Value {
    &doc["nodes"][cid]
}

fn write_doc(p: &Project, doc: &Value) {
    std::fs::write(p.graph_file(), serde_json::to_string_pretty(doc).unwrap()).unwrap();
}

fn show_status(p: &Project, cid: &str) -> String {
    p.view()
        .nodes
        .get(cid)
        .unwrap_or_else(|| panic!("{cid} is not in {}", p.dir.display()))
        .status
        .clone()
}

// ---------------------------------------------------------------- G1

/// G1: a node whose updated_at is in the future cannot be edited: the DB
/// takes the edit, graph.json keeps the future record, and the next sync
/// silently reverts the edit.
#[test]
fn g1_edits_to_a_future_dated_node_survive_sync() {
    let Some(()) = local("g1_edits_to_a_future_dated_node_survive_sync") else {
        return;
    };
    let sb = Sandbox::new();
    let p = sb.project("solo", None);
    let future = (chrono::Local::now() + chrono::Duration::days(7))
        .format("%Y-%m-%d")
        .to_string();
    let out = p.ok(&["add", "action", "dated", "--date", &future]);
    let cid = p.change_id_of(created_id(&out));
    p.ok(&["status", &cid, "completed"]);
    p.ok(&["prompt", &cid, "a prompt added after"]);
    p.ok(&["sync"]);
    let v = p.view();
    assert_eq!(
        v.nodes[&cid].status, "completed",
        "sync reverted the status edit"
    );
    assert_eq!(
        v.nodes[&cid].prompt.as_deref(),
        Some("a prompt added after"),
        "sync reverted the prompt edit"
    );
    let doc = p.graph_doc();
    assert_eq!(
        node(&doc, &cid)["status"],
        json!("completed"),
        "graph.json kept the old status"
    );
}

/// G1, the clock-skew form: a teammate's record one day ahead made every
/// later edit by anyone else report success and be discarded.
#[test]
fn g1_a_skewed_teammate_clock_does_not_swallow_later_edits() {
    let Some(()) = local("g1_a_skewed_teammate_clock_does_not_swallow_later_edits") else {
        return;
    };
    let sb = Sandbox::new();
    let t = team(&sb);
    let cid = t.alice.add("goal", "shared");
    t.alice.git_exchange();
    t.bob.git_exchange();
    // Alice's clock is a day fast when she edits it.
    let mut doc = t.alice.graph_doc();
    let skewed = (chrono::Utc::now() + chrono::Duration::days(1)).to_rfc3339();
    doc["nodes"][&cid]["updated_at"] = json!(skewed);
    doc["nodes"][&cid]["title"] = json!("shared (alice, skewed clock)");
    write_doc(&t.alice, &doc);
    t.alice.ok(&["sync"]);
    t.alice.git_exchange();
    t.bob.git_exchange();
    // Bob edits it afterwards, in real time.
    t.bob.ok(&["status", &cid, "completed"]);
    t.bob.git_exchange();
    t.bob.ok(&["sync"]);
    assert_eq!(
        show_status(&t.bob, &cid),
        "completed",
        "bob's later edit was discarded"
    );
    t.alice.git_exchange();
    assert_eq!(
        show_status(&t.alice, &cid),
        "completed",
        "bob's edit never reached alice"
    );
}

// ---------------------------------------------------------------- G2

/// G2a: when the merge driver cannot run (deciduous not on PATH, as in a GUI
/// client or CI), git leaves "ours" untouched and marks the file unmerged.
/// `sync --check` said clean and `sync` said "already agree"; committing that
/// dropped the other side's records.
#[test]
fn g2_a_failed_merge_driver_is_noticed_and_both_sides_survive() {
    let Some(()) = local("g2_a_failed_merge_driver_is_noticed_and_both_sides_survive") else {
        return;
    };
    let sb = Sandbox::new();
    let t = team(&sb);
    let shared = t.alice.add("goal", "shared");
    t.alice.git_exchange();
    t.bob.git_exchange();

    let a_only = t.alice.add("action", "alice only");
    t.alice.git_exchange();
    let b_only = t.bob.add("action", "bob only");
    t.bob.ok(&["status", &shared, "completed"]);
    t.bob.ok(&["sync"]);
    t.bob.commit_graph("bob's work");

    // Pull without deciduous on PATH: the driver fails.
    let pull = sb
        .cmd("git", &t.bob.dir)
        .args(["pull", "-q", "--no-rebase", "origin", "main"])
        .env("PATH", "/usr/bin:/bin")
        .output()
        .unwrap();
    assert!(
        !pull.status.success(),
        "precondition: the driver-less merge must conflict"
    );

    let check = t.bob.dx(&["sync", "--check"]);
    assert!(
        !check.ok(),
        "sync --check exited 0 on an unmerged graph.json:\n{}",
        check.all()
    );
    let sync = t.bob.dx(&["sync"]);
    assert!(
        !sync.stdout.contains("already agree"),
        "sync called an unmerged graph.json in agreement:\n{}",
        sync.all()
    );
    if sync.ok() {
        let v = View::from_local(&t.bob.graph());
        let doc = t.bob.graph_doc();
        for (cid, what) in [(&a_only, "alice's node"), (&b_only, "bob's node")] {
            assert!(
                v.nodes.contains_key(cid),
                "{what} missing from the db after sync"
            );
            assert!(
                !node(&doc, cid).is_null(),
                "{what} missing from graph.json after sync"
            );
        }
        assert_eq!(
            v.nodes[&shared].status, "completed",
            "bob's newer status was dropped"
        );
    }
}

/// G2b: conflict markers committed once (by a clone without the driver)
/// made every later merge whose base is that commit fail in the driver,
/// because it refused an unparseable base instead of merging two ways.
#[test]
fn g2_a_committed_conflict_marker_base_still_merges() {
    let Some(()) = local("g2_a_committed_conflict_marker_base_still_merges") else {
        return;
    };
    let sb = Sandbox::new();
    let t = team(&sb);
    let shared = t.alice.add("goal", "shared");
    t.alice.git_exchange();
    // Carol's clone never ran deciduous, so it has no merge driver: git
    // merges graph.json as text.
    let carol_dir = sb.base().join("carol");
    sb.git_ok(
        &sb.base(),
        &[
            "clone",
            "-q",
            t.origin.to_str().unwrap(),
            carol_dir.to_str().unwrap(),
        ],
    );
    let carol = Project {
        dir: carol_dir,
        sb: &sb,
    };
    t.alice.ok(&["status", &shared, "completed"]);
    t.alice.git_exchange();
    let mut doc = carol.graph_doc();
    doc["nodes"][&shared]["status"] = json!("active");
    write_doc(&carol, &doc);
    carol.git_ok(&["commit", "-q", "-am", "carol edits by hand"]);
    let pull = carol.git(&["pull", "-q", "--no-rebase", "origin", "main"]);
    assert!(!pull.ok(), "precondition: carol's merge must conflict");
    let text = std::fs::read_to_string(carol.graph_file()).unwrap();
    assert!(
        text.contains("<<<<<<<"),
        "precondition: markers in the file"
    );
    carol.git_ok(&["add", ".deciduous/graph.json"]);
    carol.git_ok(&["commit", "-q", "-m", "merge (with markers)"]);
    carol.git_ok(&["push", "-q", "origin", "HEAD:main"]);

    // Both others start from the marker commit and diverge.
    for p in [&t.alice, &t.bob] {
        p.git_ok(&["pull", "-q", "--no-rebase", "origin", "main"]);
        p.ok(&["sync"]);
    }
    let a = t.alice.add("action", "alice after");
    t.alice.ok(&["sync"]);
    t.alice.commit_graph("alice after");
    t.alice.git_ok(&["push", "-q", "origin", "HEAD:main"]);
    let b = t.bob.add("action", "bob after");
    t.bob.ok(&["sync"]);
    t.bob.commit_graph("bob after");
    let pull = t.bob.git(&["pull", "-q", "--no-rebase", "origin", "main"]);
    assert!(
        pull.ok(),
        "a merge whose base has conflict markers failed:\n{}",
        pull.all()
    );
    t.bob.ok(&["sync"]);
    let v = t.bob.view();
    assert!(
        v.nodes.contains_key(&a) && v.nodes.contains_key(&b),
        "a side was lost"
    );
}

// ---------------------------------------------------------------- G3

/// A node with a crafted change_id, imported the way a teammate's arrives.
fn import_node(p: &Project, cid: &str, title: &str) {
    let mut doc = p.graph_doc();
    let now = chrono::Local::now().to_rfc3339();
    doc["nodes"][cid] = json!({
        "author": "teammate", "change_id": cid, "created_at": now, "updated_at": now,
        "metadata": {}, "node_type": "goal", "status": "pending", "title": title
    });
    write_doc(p, &doc);
    p.ok(&["sync"]);
}

/// G3: a digit-only change_id prefix was taken as a local integer id.
/// `show 37650685`, exactly the CHANGE column, said "Node #37650685 not
/// found"; `delete 0084` deleted local node 84 instead.
#[test]
fn g3_digit_only_change_id_prefixes_address_the_change_id() {
    let Some(()) = local("g3_digit_only_change_id_prefixes_address_the_change_id") else {
        return;
    };
    let sb = Sandbox::new();
    let p = sb.project("solo", None);
    let bystander = p.add("goal", "local node 1");
    p.add("goal", "local node 2");
    let cid = "37650685-1111-4111-8111-111111111111";
    import_node(&p, cid, "teammate node");
    let show = p.dx(&["show", "37650685"]);
    assert!(
        show.ok() && show.stdout.contains("teammate node"),
        "the CHANGE column string did not address the node:\n{}",
        show.all()
    );

    // A prefix that is also an existing local id: never a silent guess.
    let colliding = "00000002-2222-4222-8222-222222222222";
    import_node(&p, colliding, "teammate node two");
    let del = p.dx(&["delete", "00000002"]);
    let v = p.view();
    let local_two_alive = v.nodes.values().any(|n| n.title == "local node 2");
    assert!(
        local_two_alive || !del.ok(),
        "`delete 00000002` deleted local node #2, not change_id 00000002-...:\n{}",
        del.all()
    );
    assert!(v.nodes.contains_key(&bystander));
}

// ---------------------------------------------------------------- G4

/// G4: fields a newer version wrote (on a node, on an edge, a top-level
/// section) were stripped from graph.json by any unrelated write.
#[test]
fn g4_unknown_fields_survive_unrelated_writes() {
    let Some(()) = local("g4_unknown_fields_survive_unrelated_writes") else {
        return;
    };
    let sb = Sandbox::new();
    let p = sb.project("solo", None);
    let a = p.add("goal", "a");
    let b = p.add("action", "b");
    p.ok(&["link", &a, &b]);
    let mut doc = p.graph_doc();
    doc["nodes"][&a]["priority"] = json!("p1");
    doc["nodes"][&a]["reviewers"] = json!(["carol"]);
    let edge_key = doc["edges"]
        .as_object()
        .unwrap()
        .keys()
        .next()
        .unwrap()
        .clone();
    doc["edges"][&edge_key]["reviewers"] = json!(["dave"]);
    doc["documents"] = json!({"d1": {"name": "from the future"}});
    write_doc(&p, &doc);

    p.ok(&["add", "goal", "unrelated"]);
    p.ok(&["status", &b, "completed"]);
    let after = p.graph_doc();
    assert_eq!(
        after["nodes"][&a]["priority"],
        json!("p1"),
        "node field stripped"
    );
    assert_eq!(
        after["nodes"][&a]["reviewers"],
        json!(["carol"]),
        "node field stripped"
    );
    assert_eq!(
        after["edges"][&edge_key]["reviewers"],
        json!(["dave"]),
        "edge field stripped"
    );
    assert_eq!(
        after["documents"],
        json!({"d1": {"name": "from the future"}}),
        "top-level section stripped"
    );
}

// ---------------------------------------------------------------- G5

/// G5: `sync --check` exited 0 with a pending edge and with an unreadable
/// record, which breaks its documented use as a pre-push hook.
#[test]
fn g5_sync_check_fails_on_pending_and_unreadable_records() {
    let Some(()) = local("g5_sync_check_fails_on_pending_and_unreadable_records") else {
        return;
    };
    let sb = Sandbox::new();
    let p = sb.project("solo", None);
    let a = p.add("goal", "a");
    assert!(p.dx(&["sync", "--check"]).ok(), "precondition: clean");

    let mut doc = p.graph_doc();
    doc["edges"]["pendingedge0000000001"] = json!({
        "author": "t", "created_at": chrono::Local::now().to_rfc3339(),
        "edge_id": "pendingedge0000000001", "edge_type": "leads_to",
        "from_change_id": a, "to_change_id": "99999999-9999-4999-8999-999999999999",
        "weight": 1.0
    });
    write_doc(&p, &doc);
    let check = p.dx(&["sync", "--check"]);
    assert!(
        !check.ok(),
        "sync --check exited 0 with an edge waiting for a missing node:\n{}",
        check.all()
    );

    let mut doc = p.graph_doc();
    doc["edges"]
        .as_object_mut()
        .unwrap()
        .remove("pendingedge0000000001");
    let mut rec = doc["nodes"][&a].clone();
    rec["title"] = json!("misfiled");
    doc["nodes"]["aaaaaaaa-0000-4000-8000-000000000000"] = rec;
    write_doc(&p, &doc);
    let check = p.dx(&["sync", "--check"]);
    assert!(
        !check.ok(),
        "sync --check exited 0 with a record filed under the wrong key:\n{}",
        check.all()
    );
}

// ---------------------------------------------------------------- G6 / G7

/// G6: a node added on a feature branch was written into main's graph.json
/// by the first `sync` after `git checkout main`.
#[test]
#[ignore = "G6: expected branch semantics of the one local DB not decided; run with --ignored"]
fn g6_nodes_do_not_leak_across_branches() {
    let Some(()) = local("g6_nodes_do_not_leak_across_branches") else {
        return;
    };
    let sb = Sandbox::new();
    let p = sb.project("solo", None);
    p.git_ok(&["checkout", "-q", "-b", "spike"]);
    let spike = p.add("goal", "spike only");
    p.commit_graph("spike");
    p.git_ok(&["checkout", "-q", "main"]);
    p.ok(&["sync"]);
    assert!(
        node(&p.graph_doc(), &spike).is_null(),
        "the spike node leaked into main's graph.json"
    );
}

/// G7: `sync` on an old commit dirtied the tree so `git checkout main` aborted.
#[test]
fn g7_sync_on_an_old_commit_does_not_block_checkout() {
    let Some(()) = local("g7_sync_on_an_old_commit_does_not_block_checkout") else {
        return;
    };
    let sb = Sandbox::new();
    let p = sb.project("solo", None);
    p.add("goal", "one");
    p.commit_graph("one");
    let old = p.git_ok(&["rev-parse", "HEAD"]).trim().to_string();
    let two = p.add("goal", "two");
    p.commit_graph("two");
    p.git_ok(&["checkout", "-q", &old]);
    let before = p.git_ok(&["status", "--porcelain"]);
    let check = p.dx(&["sync", "--check"]);
    let out = p.ok(&["sync"]);
    assert_eq!(
        p.git_ok(&["status", "--porcelain"]),
        before,
        "sync on a detached old commit dirtied the tree:\n{out}"
    );
    assert!(
        out.contains("detached") && out.contains("not exported"),
        "sync on a detached commit does not say what it left out:\n{out}"
    );
    assert!(
        check.ok(),
        "sync --check on the old commit disagrees with the sync that followed:\n{}",
        check.all()
    );
    let co = p.git(&["checkout", "-q", "main"]);
    assert!(
        co.ok(),
        "sync on an old commit blocked checkout:\n{}",
        co.all()
    );
    // Nothing was lost: "two" is still in the database and back in the
    // file on main, and main needs no sync.
    assert!(!node(&p.graph_doc(), &two).is_null());
    let settled = p.dx(&["sync", "--check"]);
    assert!(settled.ok(), "{}", settled.all());
}

// ---------------------------------------------------------------- G8 / G9

/// G8: an edge's rationale changed by unlink + relink never reached a
/// machine that already had the edge; sync said "agree" forever.
#[test]
fn g8_edge_rationale_changes_propagate() {
    let Some(()) = local("g8_edge_rationale_changes_propagate") else {
        return;
    };
    let sb = Sandbox::new();
    let t = team(&sb);
    let a = t.alice.add("goal", "a");
    let b = t.alice.add("option", "b");
    t.alice.ok(&["link", &a, &b, "-r", "first reason"]);
    t.alice.git_exchange();
    t.bob.git_exchange();
    t.alice.ok(&["unlink", &a, &b]);
    t.alice.ok(&["link", &a, &b, "-r", "second reason"]);
    t.alice.git_exchange();
    t.bob.git_exchange();
    let g = t.bob.graph();
    let rationales: Vec<&str> = g["edges"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|e| e["rationale"].as_str())
        .collect();
    assert_eq!(
        rationales,
        vec!["second reason"],
        "bob kept the stale rationale"
    );
}

/// G9: a node deleted on one clone and edited later on another comes back
/// (the edit wins, as documented) but without its incoming edge.
#[test]
#[ignore = "G9: whether a resurrected node's edges come back is a design decision; run with --ignored"]
fn g9_a_resurrected_node_keeps_its_edges() {
    let Some(()) = local("g9_a_resurrected_node_keeps_its_edges") else {
        return;
    };
    let sb = Sandbox::new();
    let t = team(&sb);
    let g = t.alice.add("goal", "g");
    let x = t.alice.add("action", "x");
    t.alice.ok(&["link", &g, &x]);
    t.alice.git_exchange();
    t.bob.git_exchange();
    t.alice.ok(&["delete", &x]);
    std::thread::sleep(std::time::Duration::from_millis(1100));
    t.bob.ok(&["status", &x, "completed"]);
    t.alice.git_exchange();
    t.bob.git_exchange();
    t.alice.git_exchange();
    for p in [&t.alice, &t.bob] {
        let v = p.view();
        assert!(v.nodes.contains_key(&x), "edit did not win over delete");
        assert!(
            v.edges.iter().any(|(f, to, _)| f == &g && to == &x),
            "the resurrected node came back without its edge in {}",
            p.dir.display()
        );
    }
}

// ---------------------------------------------------------------- G10 / R14

/// G10 / R14: the corrupt-file error told the user to run `deciduous sync`,
/// which is the command that just failed.
#[test]
fn g10_a_corrupt_graph_file_error_is_not_self_referential() {
    let Some(()) = local("g10_a_corrupt_graph_file_error_is_not_self_referential") else {
        return;
    };
    let sb = Sandbox::new();
    let p = sb.project("solo", None);
    p.add("goal", "a");
    let text = std::fs::read_to_string(p.graph_file()).unwrap();
    std::fs::write(p.graph_file(), &text[..text.len() / 2]).unwrap();
    let out = p.dx(&["sync"]);
    assert!(!out.ok(), "sync accepted a truncated graph.json");
    assert!(
        !out.all().contains("run `deciduous sync`"),
        "the failing sync tells the user to run sync:\n{}",
        out.all()
    );
    assert_eq!(
        std::fs::read_to_string(p.graph_file()).unwrap(),
        text[..text.len() / 2],
        "a refused file must be left exactly as it was"
    );
}

/// G10: `sync --check` must not create a database as a side effect.
#[test]
fn g10_sync_check_writes_nothing() {
    let Some(()) = local("g10_sync_check_writes_nothing") else {
        return;
    };
    let sb = Sandbox::new();
    let origin = sb.origin("origin");
    let a = sb.project("alice", Some(&origin));
    a.add("goal", "a");
    a.git_exchange();
    let dir = sb.base().join("fresh");
    sb.git_ok(
        &sb.base(),
        &[
            "clone",
            "-q",
            origin.to_str().unwrap(),
            dir.to_str().unwrap(),
        ],
    );
    let fresh = Project {
        dir: dir.clone(),
        sb: &sb,
    };
    let listing = |d: &Path| -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(d.join(".deciduous"))
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    };
    let before = listing(&dir);
    let out = fresh.dx(&["sync", "--check"]);
    assert!(
        !Path::new(&dir).join(".deciduous/deciduous.db").exists(),
        "sync --check created deciduous.db"
    );
    assert_eq!(listing(&dir), before, "sync --check changed .deciduous/");
    // A clone that has never synced is not settled: the graph file holds a
    // node its (absent) database does not.
    assert!(
        !out.ok(),
        "sync --check in a clone that never synced exited 0:\n{}",
        out.all()
    );
    assert!(
        out.all().contains("deciduous sync"),
        "sync --check does not say what to run:\n{}",
        out.all()
    );
    // And the check agrees with the sync that follows.
    let synced = fresh.dx(&["sync"]);
    assert!(synced.ok(), "{}", synced.all());
    let again = fresh.dx(&["sync", "--check"]);
    assert!(again.ok(), "sync --check after sync:\n{}", again.all());
}

/// NEW-7: linking two nodes that are already linked printed SQLite's own
/// "Query error: UNIQUE constraint failed: decision_edges.from_node_id, ...".
#[test]
fn new7_linking_an_existing_edge_says_so() {
    let Some(()) = local("new7_linking_an_existing_edge_says_so") else {
        return;
    };
    let sb = Sandbox::new();
    let p = sb.project("solo", None);
    p.add("goal", "from here");
    p.add("option", "to there");
    p.ok(&["link", "1", "2", "-r", "first"]);
    let again = p.dx(&["link", "1", "2", "-r", "again"]);
    assert!(
        !again.ok(),
        "a second identical link exited 0:\n{}",
        again.all()
    );
    let text = again.all();
    assert!(
        !text.contains("UNIQUE constraint"),
        "the duplicate link shows SQLite's error:\n{text}"
    );
    assert!(
        text.contains("already") && text.contains("unlink"),
        "the refusal does not say the edge exists and how to change it:\n{text}"
    );
    let mut m = StdioMcp::spawn(&sb, &p.dir);
    let r = m.call(
        "link_nodes",
        json!({"from_id": 1, "to_id": 2, "rationale": "via mcp"}),
    );
    let err = r.expect_err("link_nodes of an existing edge succeeded");
    assert!(
        !err.contains("UNIQUE constraint") && err.contains("already"),
        "link_nodes of an existing edge: {err}"
    );
    drop(m);
    let edges: Vec<Value> = p.graph()["edges"].as_array().unwrap().clone();
    assert_eq!(edges.len(), 1, "{edges:?}");
    assert_eq!(edges[0]["rationale"], json!("first"));
}

/// T8: `dx add --commit origin/main` stored the literal "origin/main" (only
/// HEAD was resolved) and the confirmation printed "[commit: origin/]".
#[test]
fn t8_commit_resolves_any_rev_and_is_shown_whole() {
    let Some(()) = local("t8_commit_resolves_any_rev_and_is_shown_whole") else {
        return;
    };
    let sb = Sandbox::new();
    let origin = sb.origin("origin");
    let p = sb.project("revs", Some(&origin));
    p.git_ok(&["fetch", "-q", "origin"]);
    let want = p.git_ok(&["rev-parse", "origin/main"]).trim().to_string();
    assert_eq!(want.len(), 40);
    let out = p.ok(&["add", "action", "pinned", "--commit", "origin/main"]);
    let id = created_id(&out);
    assert!(
        out.contains(&want),
        "the confirmation does not show the whole commit {want}:\n{out}"
    );
    let g = p.graph();
    let node = g["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"].as_i64() == Some(id))
        .unwrap()
        .clone();
    let meta: Value = serde_json::from_str(node["metadata_json"].as_str().unwrap()).unwrap();
    assert_eq!(
        meta["commit"],
        json!(want),
        "--commit origin/main was stored as {}",
        meta["commit"]
    );

    // HEAD and a branch name resolve the same way.
    let head = p.git_ok(&["rev-parse", "HEAD"]).trim().to_string();
    let out = p.ok(&["add", "action", "at head", "--commit", "HEAD"]);
    assert!(out.contains(&head), "{out}");

    // A rev this repository does not have is refused, by name, and nothing
    // is written.
    let before = p.graph()["nodes"].as_array().unwrap().len();
    let bad = p.dx(&["add", "action", "bogus", "--commit", "no-such-branch"]);
    assert!(
        !bad.ok(),
        "--commit no-such-branch exited 0:\n{}",
        bad.all()
    );
    assert!(
        bad.all().contains("no-such-branch"),
        "the refusal does not name the rev:\n{}",
        bad.all()
    );
    assert_eq!(p.graph()["nodes"].as_array().unwrap().len(), before);
}

/// G7's fix leaves the graph file alone on a detached HEAD. A rebase that
/// stopped is detached too, and there the file is being rewritten on
/// purpose: sync must still export into it.
#[test]
fn g7_sync_during_a_stopped_rebase_still_exports() {
    let Some(()) = local("g7_sync_during_a_stopped_rebase_still_exports") else {
        return;
    };
    let sb = Sandbox::new();
    let p = sb.project("solo", None);
    std::fs::write(p.dir.join("x.txt"), "base\n").unwrap();
    p.git_ok(&["add", "x.txt"]);
    p.git_ok(&["commit", "-q", "-m", "base"]);
    p.git_ok(&["checkout", "-q", "-b", "feat"]);
    std::fs::write(p.dir.join("x.txt"), "feat\n").unwrap();
    p.git_ok(&["commit", "-q", "-am", "feat"]);
    p.git_ok(&["checkout", "-q", "main"]);
    std::fs::write(p.dir.join("x.txt"), "main\n").unwrap();
    p.git_ok(&["commit", "-q", "-am", "main"]);
    p.git_ok(&["checkout", "-q", "feat"]);
    let stopped = p.git(&["rebase", "main"]);
    assert!(!stopped.ok(), "the rebase was meant to stop on x.txt");
    // A row the database has and the file does not: written, then the
    // file put back, as a rebase step can.
    let late = p.add("goal", "written mid-rebase");
    p.git_ok(&["checkout", "--", ".deciduous/graph.json"]);
    assert!(node(&p.graph_doc(), &late).is_null());
    let out = p.ok(&["sync"]);
    assert!(
        !node(&p.graph_doc(), &late).is_null(),
        "sync during a stopped rebase did not export:\n{out}"
    );
    let _ = p.git(&["rebase", "--abort"]);
}
