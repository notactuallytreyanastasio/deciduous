//! Many processes writing one project at once: CLI invocations and stdio
//! MCP servers, each its own OS process, all on one `.deciduous/`.
//!
//! What must hold under reasonable load:
//! * every write a caller was told succeeded is in the database AND in
//!   graph.json (the committed copy teammates get);
//! * no caller ever sees "database is locked";
//! * a delete stays deleted after `sync`.
//!
//! Gated on `DECIDUOUS_E2E=1` (see `tests/e2e_support/mod.rs`).

mod e2e_support;

use e2e_support::*;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::sync::Mutex;
use std::time::Duration;

#[derive(Default)]
struct Ledger {
    acked_ids: BTreeSet<i64>,
    errors: Vec<String>,
}

fn stdio_writer(sb: &Sandbox, dir: &std::path::Path, tag: &str, n: usize, ledger: &Mutex<Ledger>) {
    let mut m = StdioMcp::spawn(sb, dir);
    for i in 0..n {
        match m.call(
            "add_node",
            json!({"node_type": "action", "title": format!("{tag}-{i}")}),
        ) {
            Ok(v) => {
                let id = v["node_id"].as_i64().expect("node_id");
                ledger.lock().unwrap().acked_ids.insert(id);
            }
            Err(e) => ledger
                .lock()
                .unwrap()
                .errors
                .push(format!("{tag}-{i}: {e}")),
        }
    }
}

fn cli_writer(p: &Project, tag: &str, n: usize, ledger: &Mutex<Ledger>) {
    for i in 0..n {
        let o = p.dx(&["add", "action", &format!("{tag}-{i}")]);
        if o.ok() {
            ledger
                .lock()
                .unwrap()
                .acked_ids
                .insert(created_id(&o.stdout));
        } else {
            ledger
                .lock()
                .unwrap()
                .errors
                .push(format!("{tag}-{i}: {}", o.all()));
        }
        if o.all().contains("database is locked") && o.ok() {
            ledger
                .lock()
                .unwrap()
                .errors
                .push(format!("{tag}-{i} succeeded but printed: {}", o.all()));
        }
    }
}

/// R1 + R3 + G10: two stdio MCP servers and three CLI loops writing at once.
/// Before: 48-67% of MCP writes failed "database is locked", and of the
/// writes that succeeded, 40 of 191 were missing from graph.json because
/// each process renamed its own stale copy over the others'.
#[test]
fn stress_every_acknowledged_write_lands_in_db_and_graph_json() {
    let Some(()) = local("stress_every_acknowledged_write_lands_in_db_and_graph_json") else {
        return;
    };
    let sb = Sandbox::new();
    let p = sb.project("stress", None);
    let ledger = Mutex::new(Ledger::default());
    std::thread::scope(|s| {
        for w in 0..2 {
            let (sb, dir, ledger) = (&sb, &p.dir, &ledger);
            s.spawn(move || stdio_writer(sb, dir, &format!("mcp{w}"), 40, ledger));
        }
        for w in 0..3 {
            let (p, ledger) = (&p, &ledger);
            s.spawn(move || cli_writer(p, &format!("cli{w}"), 15, ledger));
        }
    });
    let ledger = ledger.into_inner().unwrap();
    let locked: Vec<&String> = ledger
        .errors
        .iter()
        .filter(|e| e.contains("locked") || e.contains("busy"))
        .collect();
    assert!(
        locked.is_empty(),
        "{} of 125 writes surfaced a lock error, e.g. {:?}",
        locked.len(),
        locked.first()
    );
    assert!(
        ledger.errors.is_empty(),
        "writes failed: {:?}",
        ledger.errors
    );

    let g = p.graph();
    let in_db: BTreeSet<i64> = g["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|n| n["id"].as_i64())
        .collect();
    let missing_db: Vec<&i64> = ledger.acked_ids.difference(&in_db).collect();
    assert!(
        missing_db.is_empty(),
        "acknowledged but not in the db: {missing_db:?}"
    );

    let doc = p.graph_doc();
    let cid_of = |id: i64| -> String {
        g["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"].as_i64() == Some(id))
            .and_then(|n| n["change_id"].as_str())
            .unwrap()
            .to_string()
    };
    let missing_file: Vec<i64> = ledger
        .acked_ids
        .iter()
        .copied()
        .filter(|id| doc["nodes"][cid_of(*id)].is_null())
        .collect();
    assert!(
        missing_file.is_empty(),
        "{} of {} acknowledged writes are missing from graph.json: {:?}",
        missing_file.len(),
        ledger.acked_ids.len(),
        missing_file
    );
    let check = p.dx(&["sync", "--check"]);
    assert!(
        check.ok(),
        "after the storm, db and graph.json disagree:\n{}",
        check.all()
    );
}

/// R1: one server deleting while another adds lost tombstones from
/// graph.json, and the next `sync` imported the deleted nodes back (2, 1, 3
/// resurrections in three runs).
#[test]
fn stress_deletes_under_concurrent_adds_stay_deleted() {
    let Some(()) = local("stress_deletes_under_concurrent_adds_stay_deleted") else {
        return;
    };
    let sb = Sandbox::new();
    let p = sb.project("resur", None);
    let mut first = StdioMcp::spawn(&sb, &p.dir);
    let doomed: Vec<i64> = (0..50)
        .map(|i| {
            first.call_ok(
                "add_node",
                json!({"node_type": "action", "title": format!("doomed-{i}")}),
            )["node_id"]
                .as_i64()
                .unwrap()
        })
        .collect();
    let doomed_cids: Vec<String> = {
        let g = p.graph();
        doomed
            .iter()
            .map(|id| {
                g["nodes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|n| n["id"].as_i64() == Some(*id))
                    .unwrap()["change_id"]
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect()
    };
    let errors = Mutex::new(Vec::<String>::new());
    std::thread::scope(|s| {
        let errors = &errors;
        let doomed = &doomed;
        s.spawn(move || {
            for id in doomed {
                if let Err(e) = first.call("delete_node", json!({"node_id": id})) {
                    errors.lock().unwrap().push(format!("delete {id}: {e}"));
                }
            }
        });
        let (sb, dir) = (&sb, &p.dir);
        s.spawn(move || {
            let mut m = StdioMcp::spawn(sb, dir);
            for i in 0..150 {
                if let Err(e) = m.call(
                    "add_node",
                    json!({"node_type": "goal", "title": format!("adder-{i}")}),
                ) {
                    errors.lock().unwrap().push(format!("add {i}: {e}"));
                }
            }
        });
    });
    let errors = errors.into_inner().unwrap();
    assert!(errors.is_empty(), "writes failed: {errors:?}");
    p.ok(&["sync"]);
    let v = p.view();
    let back: Vec<&String> = doomed_cids
        .iter()
        .filter(|c| v.nodes.contains_key(*c))
        .collect();
    assert!(
        back.is_empty(),
        "{} deleted node(s) came back after sync",
        back.len()
    );
    assert_eq!(v.nodes.len(), 150, "adds were lost");
}

/// R3: `deciduous mcp` started while another process held a write lock
/// exited at startup ("Failed to open database: database is locked") with a
/// hint about being in the wrong directory, and the client lost the server
/// for the whole session.
#[test]
fn stress_mcp_starts_while_another_process_holds_the_lock() {
    let Some(()) = local("stress_mcp_starts_while_another_process_holds_the_lock") else {
        return;
    };
    let sb = Sandbox::new();
    let p = sb.project("locked", None);
    p.add("goal", "present");
    let db = p.dir.join(".deciduous/deciduous.db");
    let (tx, rx) = std::sync::mpsc::channel();
    let holder = std::thread::spawn(move || {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch("BEGIN EXCLUSIVE;").unwrap();
        tx.send(()).unwrap();
        std::thread::sleep(Duration::from_secs(3));
        conn.execute_batch("COMMIT;").unwrap();
    });
    rx.recv().unwrap();
    let mut m = StdioMcp::spawn(&sb, &p.dir);
    let r = m.call("list_nodes", json!({}));
    holder.join().unwrap();
    let text = format!("{r:?}");
    assert!(
        r.is_ok() && text.contains("present"),
        "a server started during a 3s write lock did not serve: {text}\nstderr: {}",
        m.stderr.lock().unwrap()
    );
    let add = m.call("add_node", json!({"node_type": "goal", "title": "after"}));
    assert!(add.is_ok(), "{add:?}");
}

/// Twenty parallel `deciduous add` from a shell loop: before, 16 of 20
/// failed "database is locked" (loudly, but failed).
#[test]
fn stress_twenty_parallel_cli_adds_all_succeed() {
    let Some(()) = local("stress_twenty_parallel_cli_adds_all_succeed") else {
        return;
    };
    let sb = Sandbox::new();
    let p = sb.project("par", None);
    let outs: Vec<Out> = std::thread::scope(|s| {
        let hs: Vec<_> = (0..20)
            .map(|i| {
                let p = &p;
                s.spawn(move || p.dx(&["add", "goal", &format!("p{i}")]))
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let failed: Vec<String> = outs.iter().filter(|o| !o.ok()).map(Out::all).collect();
    assert!(
        failed.is_empty(),
        "{} of 20 failed, e.g. {:?}",
        failed.len(),
        failed.first()
    );
    let doc: Value = p.graph_doc();
    assert_eq!(
        doc["nodes"].as_object().unwrap().len(),
        20,
        "graph.json lost records"
    );
}

/// T9: in the team probe, 10 parallel `dx add` in one clone with a remote
/// configured gave 9-10 "Failed to open database: database is locked". The
/// test above covers a clone without a remote; with one, every add also
/// appends to remote-log.jsonl and replays to the server, which is where a
/// second lock (the log's) and a second writer (the replay) come in.
#[test]
fn t9_twenty_parallel_adds_with_a_remote_all_land_locally_and_on_the_server() {
    let Some(server) =
        remote("t9_twenty_parallel_adds_with_a_remote_all_land_locally_and_on_the_server")
    else {
        return;
    };
    let sb = Sandbox::with_server(server.clone());
    let ws = unique("t9");
    let p = sb.project(&ws, None);
    p.remote_init(&ws);
    for round in 0..3 {
        let outs: Vec<Out> = std::thread::scope(|s| {
            let hs: Vec<_> = (0..20)
                .map(|i| {
                    let p = &p;
                    s.spawn(move || p.dx(&["add", "goal", &format!("r{round}-p{i}")]))
                })
                .collect();
            hs.into_iter().map(|h| h.join().unwrap()).collect()
        });
        let failed: Vec<String> = outs
            .iter()
            .filter(|o| !o.ok() || o.all().contains("locked") || o.all().contains("Warning"))
            .map(Out::all)
            .collect();
        assert!(
            failed.is_empty(),
            "round {round}: {} of 20 failed or warned, e.g. {:?}",
            failed.len(),
            failed.first()
        );
    }
    let doc: Value = p.graph_doc();
    assert_eq!(doc["nodes"].as_object().unwrap().len(), 60, "graph.json");
    // Every write reached the server, and nothing is left queued.
    let push = p.dx(&["remote", "push"]);
    assert!(push.ok(), "{}", push.all());
    let live = server.export(&ws)["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|n| n["deleted_at"].is_null())
        .count();
    assert_eq!(live, 60, "the server has {live} of 60 nodes");
    let st = p.dx(&["remote", "status"]);
    assert!(
        st.ok(),
        "remote status after 60 parallel adds:\n{}",
        st.all()
    );
}
