//! `deciduous why <file>` end to end: a real git repository, a graph whose
//! action carries the commit, and the binary asked about the file.

use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tempfile::TempDir;

fn git(root: &Path, args: &[&str]) {
    let o = Command::new("git")
        .current_dir(root)
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args(args)
        .output()
        .expect("git runs");
    assert!(
        o.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&o.stderr)
    );
}

fn deciduous(root: &Path, args: &[&str], stdin: Option<&str>) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_deciduous"))
        .current_dir(root)
        .args(args)
        .env("DECIDUOUS_DB_PATH", root.join(".deciduous/deciduous.db"))
        .env("DECIDUOUS_QUIET_LOCAL_ONLY", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("deciduous runs");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.unwrap_or("").as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn ok(root: &Path, args: &[&str]) -> String {
    let o = deciduous(root, args, None);
    assert!(
        o.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&o.stderr)
    );
    String::from_utf8_lossy(&o.stdout).to_string()
}

/// goal 1 -> decision 2 (chose option 3, rejected option 4) -> action 5,
/// which carries the commit that wrote src/limit.rs. A second commit to the
/// same file is in no node, and src/other.rs is in nothing.
fn project() -> (TempDir, PathBuf) {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    fs::create_dir_all(root.join(".deciduous")).unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    git(&root, &["init", "-q"]);
    fs::write(root.join(".gitignore"), ".deciduous/\n").unwrap();
    fs::write(root.join("src/limit.rs"), "fn limit() {}\n").unwrap();
    git(&root, &["add", ".gitignore", "src/limit.rs"]);
    git(&root, &["commit", "-q", "-m", "add the limiter"]);

    ok(&root, &["add", "goal", "Rate limit the API"]);
    ok(&root, &["add", "decision", "Limit by token, not by IP"]);
    ok(&root, &["add", "option", "Per-token buckets"]);
    ok(&root, &["add", "option", "Per-IP buckets"]);
    ok(
        &root,
        &["add", "action", "Add the limiter", "--commit", "HEAD"],
    );
    ok(&root, &["link", "1", "2"]);
    ok(
        &root,
        &["link", "2", "3", "-t", "chosen", "-r", "survives NAT"],
    );
    ok(
        &root,
        &[
            "link",
            "2",
            "4",
            "-t",
            "rejected",
            "-r",
            "an office is one IP",
        ],
    );
    ok(&root, &["link", "2", "5"]);

    fs::write(root.join("src/limit.rs"), "fn limit() {}\nfn burst() {}\n").unwrap();
    fs::write(root.join("src/other.rs"), "fn other() {}\n").unwrap();
    git(&root, &["add", "src/limit.rs", "src/other.rs"]);
    git(&root, &["commit", "-q", "-m", "add burst, with no node"]);
    (tmp, root)
}

#[test]
fn a_file_answers_with_the_decision_behind_it_and_what_was_rejected() {
    let (_tmp, root) = project();
    let text = ok(&root, &["why", "src/limit.rs"]);
    assert!(
        text.starts_with(
            "src/limit.rs: 1 of 2 commits that touched it are in the graph; 1 nodes\n"
        ),
        "{text}"
    );
    for want in [
        "goal 1 Rate limit the API",
        "  decision 2 Limit by token, not by IP",
        "    chose    3 Per-token buckets [pending]  (survives NAT)",
        "    rejected 4 Per-IP buckets [pending]  (an office is one IP)",
        "  action 5 Add the limiter",
    ] {
        assert!(text.contains(want), "missing {want:?} in:\n{text}");
    }

    // From a subdirectory, and by absolute path: the same file.
    let abs = root.join("src/limit.rs");
    let o = Command::new(env!("CARGO_BIN_EXE_deciduous"))
        .current_dir(root.join("src"))
        .args(["why", abs.to_str().unwrap(), "--json"])
        .env("DECIDUOUS_DB_PATH", root.join(".deciduous/deciduous.db"))
        .output()
        .unwrap();
    let v: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["path"], "src/limit.rs");
    assert_eq!(v["threads"][0]["choices"][0]["rejected"][0]["id"], 4);
    assert_eq!(v["threads"][0]["hits"][0]["via"], "commit");
}

#[test]
fn a_line_range_follows_only_the_commits_that_touched_those_lines() {
    let (_tmp, root) = project();
    // Line 1 was written by the commit the action carries.
    let first = ok(&root, &["why", "src/limit.rs:1"]);
    assert!(
        first.starts_with("src/limit.rs:1: 1 of 1 commits"),
        "{first}"
    );
    assert!(first.contains("decision 2"), "{first}");
    // Line 2 came with the commit no node carries.
    let second = ok(&root, &["why", "src/limit.rs:2"]);
    assert!(
        second.starts_with("src/limit.rs:2: 0 of 1 commits"),
        "{second}"
    );
    assert!(
        second.contains("Nothing in the graph is tied to this file"),
        "{second}"
    );
}

#[test]
fn a_file_the_graph_knows_nothing_about_says_so() {
    let (_tmp, root) = project();
    let text = ok(&root, &["why", "src/other.rs"]);
    assert!(text.starts_with("src/other.rs: 0 of 1 commits"), "{text}");
    assert!(
        text.contains("Nothing in the graph is tied to this file"),
        "{text}"
    );
    // A file git has never seen is not an error either.
    let text = ok(&root, &["why", "src/never.rs"]);
    assert!(text.starts_with("src/never.rs: 0 of 0 commits"), "{text}");
}

/// What Claude Code 2.1.325 sends a PreToolUse hook, cut to the fields read.
fn hook_input(session: &str, file: &Path) -> String {
    json!({
        "session_id": session,
        "hook_event_name": "PreToolUse",
        "tool_name": "Edit",
        "tool_input": {"file_path": file, "old_string": "a", "new_string": "b"}
    })
    .to_string()
}

#[test]
fn as_a_hook_it_names_the_decision_once_per_file_per_session() {
    let (_tmp, root) = project();
    let session = format!("why-hook-test-{}", std::process::id());
    let record = std::env::temp_dir().join(format!("deciduous-why-{session}"));
    let _ = fs::remove_file(&record);
    let input = hook_input(&session, &root.join("src/limit.rs"));

    let o = deciduous(&root, &["why", "--hook"], Some(&input));
    assert!(o.status.success());
    let v: Value = serde_json::from_slice(&o.stdout).expect("the hook's reply is JSON");
    assert_eq!(v["hookSpecificOutput"]["hookEventName"], "PreToolUse");
    let said = v["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap();
    assert!(
        said.contains("decision 2 [pending]: Limit by token, not by IP"),
        "{said}"
    );
    assert!(
        said.contains("rejected 4: Per-IP buckets (an office is one IP)"),
        "{said}"
    );

    // The second edit of the same file in the session: nothing.
    let again = deciduous(&root, &["why", "--hook"], Some(&input));
    assert!(again.status.success());
    assert!(again.stdout.is_empty());

    // A file with no decision behind it: nothing, the first time too.
    let other = hook_input(&session, &root.join("src/other.rs"));
    let o = deciduous(&root, &["why", "--hook"], Some(&other));
    assert!(o.status.success());
    assert!(o.stdout.is_empty());
    let _ = fs::remove_file(&record);
}

#[test]
fn as_a_hook_it_never_fails_the_edit() {
    let (_tmp, root) = project();
    for input in [
        "",
        "not json",
        r#"{"tool_input":{}}"#,
        r#"{"tool_input":{"file_path":"/somewhere/else/entirely.rs"}}"#,
    ] {
        let o = deciduous(&root, &["why", "--hook"], Some(input));
        assert!(o.status.success(), "{input:?}");
        assert!(o.stdout.is_empty(), "{input:?}");
    }
    // Outside any project: silent, exit 0, and no .deciduous made.
    let bare = TempDir::new().unwrap();
    let o = Command::new(env!("CARGO_BIN_EXE_deciduous"))
        .current_dir(bare.path())
        .args(["why", "--hook"])
        .env_remove("DECIDUOUS_DB_PATH")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(o.status.success());
    assert!(o.stdout.is_empty() && o.stderr.is_empty());
    assert!(!bare.path().join(".deciduous").exists());
}
