//! `deciduous transcript import` end to end: a Claude Code transcript goes in,
//! a graph with evidence on every node comes out, and a second import of the
//! same file changes nothing.

use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

fn run(args: &[&str], db_path: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_deciduous"))
        .args(args)
        .env("DECIDUOUS_DB_PATH", db_path)
        .env("DECIDUOUS_QUIET_LOCAL_ONLY", "1")
        .output()
        .expect("deciduous runs")
}

fn out(o: &std::process::Output) -> String {
    String::from_utf8_lossy(&o.stdout).to_string()
}

fn err(o: &std::process::Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}

fn line(v: Value) -> String {
    format!("{}\n", v)
}

/// A session with two prompts. In the first the agent reads a file, edits
/// two, runs the tests, commits (quietly, so the sha only shows in a
/// following `git log`), and reports. The second is a question answered
/// in prose. A `git commit` run through a heredoc edits nothing.
fn write_transcript(dir: &Path, project: &Path) -> PathBuf {
    let p = dir.join("0f1e2d3c-0000-4000-8000-000000000001.jsonl");
    let mut s = String::new();
    s.push_str(&line(json!({"type":"ai-title","aiTitle":"Add the widget","sessionId":"0f1e2d3c-0000-4000-8000-000000000001"})));
    s.push_str(&line(json!({"type":"user","uuid":"u1","timestamp":"2026-10-01T10:00:00Z","cwd":project,"gitBranch":"feat/widget","sessionId":"0f1e2d3c-0000-4000-8000-000000000001",
        "message":{"role":"user","content":"add a widget to the thing\n\nit should be blue"}})));
    s.push_str(&line(json!({"type":"assistant","uuid":"a1","timestamp":"2026-10-01T10:00:05Z","cwd":project,"gitBranch":"feat/widget",
        "message":{"role":"assistant","content":[{"type":"text","text":"Reading the module, then adding the widget."}]}})));
    s.push_str(&line(json!({"type":"assistant","uuid":"a2","timestamp":"2026-10-01T10:00:06Z","cwd":project,"gitBranch":"feat/widget",
        "message":{"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"Read","input":{"file_path":project.join("src/lib.rs")}}]}})));
    s.push_str(&line(json!({"type":"user","uuid":"r1","timestamp":"2026-10-01T10:00:07Z",
        "message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"pub fn thing() {}"}]}})));
    s.push_str(&line(json!({"type":"assistant","uuid":"a3","timestamp":"2026-10-01T10:00:08Z","cwd":project,"gitBranch":"feat/widget",
        "message":{"role":"assistant","content":[{"type":"tool_use","id":"t2","name":"Edit","input":{"file_path":project.join("src/lib.rs"),"old_string":"a","new_string":"b"}}]}})));
    s.push_str(&line(json!({"type":"user","uuid":"r2","timestamp":"2026-10-01T10:00:09Z",
        "message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t2","content":"ok"}]}})));
    s.push_str(&line(json!({"type":"assistant","uuid":"a4","timestamp":"2026-10-01T10:00:10Z","cwd":project,"gitBranch":"feat/widget",
        "message":{"role":"assistant","content":[{"type":"tool_use","id":"t3","name":"Write","input":{"file_path":project.join("src/widget.rs"),"content":"pub struct Widget;"}}]}})));
    s.push_str(&line(json!({"type":"user","uuid":"r3","timestamp":"2026-10-01T10:00:11Z",
        "message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t3","content":"ok"}]}})));
    s.push_str(&line(json!({"type":"assistant","uuid":"a5","timestamp":"2026-10-01T10:00:12Z","cwd":project,"gitBranch":"feat/widget",
        "message":{"role":"assistant","content":[{"type":"tool_use","id":"t4","name":"Bash","input":{"command":"cargo test 2>&1 | tail -20"}}]}})));
    s.push_str(&line(json!({"type":"user","uuid":"r4","timestamp":"2026-10-01T10:00:30Z",
        "toolUseResult":{"stdout":"running 4 tests\ntest result: ok. 4 passed; 0 failed; 0 ignored\nrunning 1 test\ntest result: ok. 1 passed; 0 failed; 0 ignored","stderr":""},
        "message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t4","content":"..."}]}})));
    s.push_str(&line(json!({"type":"assistant","uuid":"a6","timestamp":"2026-10-01T10:00:31Z","cwd":project,"gitBranch":"feat/widget",
        "message":{"role":"assistant","content":[{"type":"tool_use","id":"t5","name":"Bash","input":{"command":"git add src/lib.rs src/widget.rs && git commit -q -m \"feat: blue widget\" && git log --oneline -1"}}]}})));
    s.push_str(&line(json!({"type":"user","uuid":"r5","timestamp":"2026-10-01T10:00:32Z",
        "toolUseResult":{"stdout":"abc1234 feat: blue widget","stderr":""},
        "message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t5","content":"abc1234 feat: blue widget"}]}})));
    s.push_str(&line(json!({"type":"assistant","uuid":"a7","timestamp":"2026-10-01T10:00:33Z","cwd":project,"gitBranch":"feat/widget",
        "message":{"role":"assistant","content":[{"type":"text","text":"**Done.** The widget is in and all 5 tests pass."}]}})));
    s.push_str(&line(json!({"type":"user","uuid":"u2","timestamp":"2026-10-01T10:05:00Z","cwd":project,"gitBranch":"feat/widget",
        "message":{"role":"user","content":[{"type":"text","text":"why blue?"}]}})));
    s.push_str(&line(json!({"type":"assistant","uuid":"a8","timestamp":"2026-10-01T10:05:03Z","cwd":project,"gitBranch":"feat/widget",
        "message":{"role":"assistant","content":[{"type":"text","text":"You asked for blue."}]}})));
    fs::write(&p, s).unwrap();
    p
}

/// A project directory with a database inside `.deciduous/`, so paths in the
/// transcript are shown relative to it.
fn project() -> (TempDir, PathBuf, PathBuf) {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("proj");
    fs::create_dir_all(root.join(".deciduous")).unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/lib.rs"), "pub fn thing() {}\n").unwrap();
    let db = root.join(".deciduous").join("deciduous.db");
    (tmp, root, db)
}

#[test]
fn import_derives_the_graph_and_is_idempotent() {
    let (tmp, root, db) = project();
    let transcript = write_transcript(tmp.path(), &root);

    let first = run(&["transcript", "import", transcript.to_str().unwrap()], &db);
    assert!(first.status.success(), "{}", err(&first));
    let text = out(&first);
    assert!(text.contains("Imported Add the widget"), "{text}");
    assert!(
        text.contains("goals 2 · actions 2 · outcomes 1 proven, 1 inferred · observations 1"),
        "{text}"
    );
    assert!(text.contains("1 files hashed"), "{text}");

    let nodes = run(&["nodes"], &db);
    let listing = out(&nodes);
    for expected in [
        "Add the widget",
        "Edit src/lib.rs, src/widget.rs",
        "cargo test passed",
        "Committed: feat: blue widget",
        "Done. The widget is in and all 5 tests pass.",
        "why blue?",
        "You asked for blue.",
    ] {
        assert!(
            listing.contains(expected),
            "missing {expected:?} in\n{listing}"
        );
    }

    // Edges: root -> action, action -> test outcome, root -> commit,
    // root -> closing, root -> second goal, second goal -> observation.
    let edges = run(&["edges"], &db);
    let edge_text = out(&edges);
    assert!(
        edge_text.contains("files changed in this turn"),
        "{edge_text}"
    );
    assert!(
        edge_text.contains("command run in this turn"),
        "{edge_text}"
    );
    assert!(edge_text.contains("prompt 2 of the session"), "{edge_text}");

    let second = run(&["transcript", "import", transcript.to_str().unwrap()], &db);
    assert!(second.status.success(), "{}", err(&second));
    let text2 = out(&second);
    assert!(
        text2.contains(
            "goals 0 · actions 0 · outcomes 0 proven, 0 inferred · observations 0 · edges 0"
        ),
        "{text2}"
    );
    assert!(text2.contains("7 already present"), "{text2}");
    let nodes_after = run(&["nodes"], &db);
    assert_eq!(
        out(&nodes),
        out(&nodes_after),
        "a second import added nothing"
    );
}

#[test]
fn show_prints_evidence_prompt_and_hashes() {
    let (tmp, root, db) = project();
    let transcript = write_transcript(tmp.path(), &root);
    let o = run(
        &[
            "transcript",
            "import",
            transcript.to_str().unwrap(),
            "--json",
        ],
        &db,
    );
    assert!(o.status.success(), "{}", err(&o));
    let reports: Vec<Value> = serde_json::from_str(&out(&o)).expect("json report");
    let report = &reports[0];
    assert_eq!(report["turns"], 2);
    assert_eq!(report["created"]["actions"], 2);
    let root_id = report["root_id"].as_i64().expect("root id");

    let shown = out(&run(&["show", &root_id.to_string()], &db));
    assert!(shown.contains("Evidence"), "{shown}");
    assert!(shown.contains("proven"), "{shown}");
    assert!(shown.contains("add a widget to the thing"), "{shown}");
    assert!(
        shown.contains("it should be blue"),
        "verbatim prompt kept:\n{shown}"
    );

    // The edit action: files with hashes, the consulted file, the stated intent.
    let action = out(&run(&["show", &(root_id + 1).to_string()], &db));
    assert!(
        action.contains("Edit src/lib.rs, src/widget.rs"),
        "{action}"
    );
    assert!(
        action.contains("Reading the module, then adding the widget."),
        "{action}"
    );
    assert!(action.contains("Consulted: src/lib.rs"), "{action}");
    assert!(
        action.contains("src/lib.rs (git:") || action.contains("src/lib.rs (sha256:"),
        "the file that exists is hashed:\n{action}"
    );
    assert!(
        !action.contains("src/widget.rs (git:"),
        "a file that does not exist is not hashed"
    );

    // The closing report is inferred.
    let closing = out(&run(&["show", &(root_id + 4).to_string()], &db));
    assert!(closing.contains("inferred"), "{closing}");
}

#[test]
fn dry_run_writes_nothing_and_list_marks_imports() {
    let (tmp, root, db) = project();
    let transcript = write_transcript(tmp.path(), &root);
    let dry = run(
        &[
            "transcript",
            "import",
            "--dry-run",
            transcript.to_str().unwrap(),
        ],
        &db,
    );
    assert!(dry.status.success(), "{}", err(&dry));
    assert!(out(&dry).contains("Would import"), "{}", out(&dry));
    let nodes = out(&run(&["nodes"], &db));
    assert!(
        !nodes.contains("Add the widget"),
        "dry run must not write:\n{nodes}"
    );

    let dir = tmp.path().to_str().unwrap();
    let before = out(&run(&["transcript", "list", "--dir", dir], &db));
    assert!(before.contains("new"), "{before}");
    assert!(before.contains("Add the widget"), "{before}");
    assert!(before.contains("2 prompts"), "{before}");

    let real = run(&["transcript", "import", transcript.to_str().unwrap()], &db);
    assert!(real.status.success(), "{}", err(&real));
    let after = out(&run(&["transcript", "list", "--dir", dir], &db));
    assert!(after.contains("imported"), "{after}");

    // A session id prefix resolves only against the project's own
    // transcript directory, which this temp project does not have.
    let by_id = run(&["transcript", "import", "0f1e2d3c"], &db);
    assert!(!by_id.status.success());
    assert!(err(&by_id).contains("0f1e2d3c"), "{}", err(&by_id));
}

#[test]
fn a_transcript_without_a_prompt_is_refused_by_name() {
    let (tmp, _root, db) = project();
    let p = tmp.path().join("agent-only.jsonl");
    fs::write(
        &p,
        line(
            json!({"type":"assistant","uuid":"a1","timestamp":"2026-10-01T10:00:00Z",
            "message":{"role":"assistant","content":[{"type":"text","text":"hello"}]}}),
        ),
    )
    .unwrap();
    let o = run(&["transcript", "import", p.to_str().unwrap()], &db);
    assert!(!o.status.success());
    let e = err(&o);
    assert!(e.contains("no user prompt"), "{e}");
    assert!(e.contains("agent-only.jsonl"), "{e}");
}
