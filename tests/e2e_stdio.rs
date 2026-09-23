//! The Rust MCP server over its real stdio (`deciduous mcp`) and the
//! multi-graph API daemon over real TCP (`deciduous serve --api`), each run
//! as its own process against a real project on disk.
//!
//! Gated on `DECIDUOUS_E2E=1` (see `tests/e2e_support/mod.rs`).

mod e2e_support;

use e2e_support::*;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

fn project(sb: &Sandbox) -> Project<'_> {
    sb.project("repo", None)
}

// ---------------------------------------------------------------- R2

/// R2: ids were truncated to 32 bits (`as i64 as i32`), so 4294967298
/// addressed node 2: `delete_node` deleted the wrong node, `show_node` showed
/// it, `resume_session` resumed it, `link_nodes` linked it, over stdio and
/// over the API.
#[test]
fn r2_ids_beyond_i32_are_refused_not_wrapped() {
    let Some(()) = local("r2_ids_beyond_i32_are_refused_not_wrapped") else {
        return;
    };
    let sb = Sandbox::new();
    let p = project(&sb);
    p.add("goal", "n1");
    p.add("goal", "n2");
    let mut m = StdioMcp::spawn(&sb, &p.dir);
    for id in [
        json!(4294967297_i64),
        json!(-4294967295_i64),
        json!(9223372036854775807_i64),
    ] {
        let r = m.call("show_node", json!({"node_id": id}));
        assert!(r.is_err(), "show_node {id} answered a node: {r:?}");
    }
    let r = m.call(
        "link_nodes",
        json!({"from_id": 4294967297_i64, "to_id": 4294967298_i64}),
    );
    assert!(r.is_err(), "link_nodes with wrapped ids succeeded: {r:?}");
    let r = m.call("delete_node", json!({"node_id": 4294967298_i64}));
    assert!(r.is_err(), "delete_node 4294967298 succeeded: {r:?}");
    let r = m.call("resume_session", json!({"session_id": 4294967297_i64}));
    assert!(
        r.is_err(),
        "resume_session with a wrapped id succeeded: {r:?}"
    );
    drop(m);
    let titles: Vec<String> = p.view().nodes.values().map(|n| n.title.clone()).collect();
    assert!(
        titles.contains(&"n2".to_string()),
        "node 2 was deleted through a wrapped id"
    );
    assert!(
        p.view().edges.is_empty(),
        "an edge was created through wrapped ids"
    );

    let d = api(&sb);
    d.tool(
        "g",
        "add_node",
        json!({"node_type": "goal", "title": "a1", "branch": "b"}),
    );
    d.tool(
        "g",
        "add_node",
        json!({"node_type": "goal", "title": "a2", "branch": "b"}),
    );
    let (_, body) = d.tool("g", "show_node", json!({"node_id": 4294967297_i64}));
    assert!(
        body["data"]["is_error"] == json!(true) || body["ok"] == json!(false),
        "API show_node with a wrapped id answered: {body}"
    );
}

// ---------------------------------------------------------------- R4 / R5

fn attach(m: &mut StdioMcp, path: &str) -> Result<Value, String> {
    m.call("attach_document", json!({"node_id": 1, "file_path": path}))
}

/// R4: attach_document read any path: /etc/passwd, ../../ traversal, and a
/// symlink inside the repository pointing out of it were all copied into
/// .deciduous/documents/.
#[test]
fn r4_attach_document_is_confined_to_the_project() {
    let Some(()) = local("r4_attach_document_is_confined_to_the_project") else {
        return;
    };
    let sb = Sandbox::new();
    let p = project(&sb);
    p.add("goal", "g");
    let secret_dir = sb.base().join("secret");
    std::fs::create_dir_all(&secret_dir).unwrap();
    std::fs::write(secret_dir.join("id_rsa"), "PRIVATE KEY MATERIAL").unwrap();
    std::os::unix::fs::symlink(secret_dir.join("id_rsa"), p.dir.join("innocent.png")).unwrap();
    std::fs::write(p.dir.join("inside.md"), "fine").unwrap();

    let mut m = StdioMcp::spawn(&sb, &p.dir);
    for bad in [
        "/etc/passwd",
        "../../../../../../etc/hosts",
        "../secret/id_rsa",
        secret_dir.join("id_rsa").to_str().unwrap(),
        "innocent.png",
    ] {
        let r = attach(&mut m, bad);
        assert!(r.is_err(), "attach_document {bad:?} was accepted: {r:?}");
    }
    let docs = p.dir.join(".deciduous/documents");
    let leaked = walk_contains(&docs, b"PRIVATE KEY MATERIAL") || walk_contains(&docs, b"root:");
    assert!(
        !leaked,
        "a file from outside the project was copied into documents/"
    );
    attach(&mut m, "inside.md").expect("a file inside the project must attach");
}

fn walk_contains(dir: &std::path::Path, needle: &[u8]) -> bool {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return false;
    };
    rd.flatten().any(|e| {
        let p = e.path();
        if p.is_dir() {
            walk_contains(&p, needle)
        } else {
            std::fs::read(&p)
                .map(|b| b.windows(needle.len()).any(|w| w == needle))
                .unwrap_or(false)
        }
    })
}

/// R4: a FIFO hung the single-threaded server forever; /dev/zero grew it to
/// gigabytes. Each must fail fast and leave the server answering.
#[test]
fn r4_attach_document_never_hangs_on_special_files() {
    let Some(()) = local("r4_attach_document_never_hangs_on_special_files") else {
        return;
    };
    let sb = Sandbox::new();
    let p = project(&sb);
    p.add("goal", "g");
    let fifo = p.dir.join("pipe");
    let mk = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .unwrap();
    assert!(mk.success());
    std::os::unix::fs::symlink("/dev/zero", p.dir.join("zero.bin")).unwrap();
    let mut m = StdioMcp::spawn(&sb, &p.dir);
    for special in ["pipe", "/dev/zero", "zero.bin", "/dev/null"] {
        let started = Instant::now();
        let id = 900;
        m.send_raw(
            json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":"attach_document","arguments":{"node_id":1,"file_path":special}}})
                .to_string()
                .as_bytes(),
        );
        let reply = wait_for(Duration::from_secs(10), || {
            m.recv(Duration::from_millis(200))
        });
        let reply = reply.unwrap_or_else(|| {
            panic!("attach_document {special:?} gave no answer in 10s (server hung)")
        });
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "attach_document {special:?} took {:?}",
            started.elapsed()
        );
        assert!(
            tool_result(&reply).is_err(),
            "attach_document {special:?} was accepted: {reply}"
        );
    }
    assert!(m.alive(), "the server died");
}

/// R5: an MCP server started from a subdirectory created a second
/// `.deciduous` there (attach_document and the session file used cwd-relative
/// paths), after which every later command in that directory opened a new,
/// empty graph.
#[test]
fn r5_a_server_started_in_a_subdirectory_uses_the_project_root() {
    let Some(()) = local("r5_a_server_started_in_a_subdirectory_uses_the_project_root") else {
        return;
    };
    let sb = Sandbox::new();
    let p = project(&sb);
    p.add("goal", "root goal");
    let src = p.dir.join("src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("notes.md"), "notes").unwrap();
    {
        let mut m = StdioMcp::spawn(&sb, &src);
        m.call_ok(
            "start_session",
            json!({"name": "from src", "goal_title": "session goal"}),
        );
        attach(&mut m, "notes.md").expect("attaching a file in the cwd");
    }
    assert!(
        !src.join(".deciduous").exists(),
        "a second .deciduous was created in the subdirectory"
    );
    let nodes = p.dx_in(&src, &["nodes"]);
    assert!(
        nodes.stdout.contains("root goal"),
        "from the subdirectory the graph is gone:\n{}",
        nodes.all()
    );
    // The session survives a restart of the server from the same place.
    let mut m = StdioMcp::spawn(&sb, &src);
    let s = m.call_ok("get_session", json!({}));
    assert_eq!(
        s["name"],
        json!("from src"),
        "the active session was lost: {s}"
    );
}

// ---------------------------------------------------------------- R6 / R7 / R8

const API_TOKEN: &str = "e2e-api-token-0123456789";

fn api(sb: &Sandbox) -> ApiDaemon {
    let d = ApiDaemon::spawn(sb, &sb.base().join("apidata"), API_TOKEN);
    let s = d.as_server();
    let r = s.request("PUT", "/api/v1/graphs/g", Some(&s.bearer()), &[], None);
    assert!(r.status == 201 || r.status == 200, "{}", r.body);
    d
}

fn query(d: &ApiDaemon, sql: &str) -> (u16, Value, Duration) {
    let s = d.as_server();
    let t = Instant::now();
    let r = s.try_request(
        "POST",
        "/api/v1/graphs/g/query",
        Some(&s.bearer()),
        &[("content-type", "application/json")],
        Some(json!({"sql": sql}).to_string().as_bytes()),
        Duration::from_secs(20),
    );
    match r {
        Ok(r) => (
            r.status,
            serde_json::from_str(&r.body).unwrap_or(Value::String(r.body)),
            t.elapsed(),
        ),
        Err(e) => panic!("{sql:?} got no answer within 20s ({e}): the query has no time limit"),
    }
}

/// R6: /query had no time limit: an endless recursive CTE pinned a core per
/// request until the daemon was killed.
#[test]
fn r6_api_query_has_a_time_and_size_limit() {
    let Some(()) = local("r6_api_query_has_a_time_and_size_limit") else {
        return;
    };
    let sb = Sandbox::new();
    let d = api(&sb);
    let (st, body, took) = query(
        &d,
        "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM c) SELECT x FROM c WHERE x<0",
    );
    assert!(
        took < Duration::from_secs(15),
        "an endless query ran {took:?}"
    );
    assert!(st >= 400, "an endless query returned {st}: {body}");
    let (st, body, _) = query(&d, "SELECT printf('%.*c', 200000000, 'x')");
    assert!(
        st >= 400 || body.to_string().len() < 50_000_000,
        "a 200 MB string was built and returned"
    );
    // Still serving.
    let (st, _, _) = query(&d, "SELECT 1");
    assert_eq!(
        st, 200,
        "the daemon stopped answering after a runaway query"
    );
}

/// The r6 flake (chapter 30 verification): the first request to a freshly
/// spawned API daemon was refused, "Connection refused (os error 61)", in 1
/// run of 4. ApiDaemon::spawn chose a free port by binding and releasing it
/// (dead_port) and handed it to the child, and took any successful TCP
/// connect to it as the child listening. Many daemons started at once, with
/// other sockets opening beside them, is the shape of a battery run.
#[test]
fn r6_flake_daemons_started_at_once_each_answer_their_first_request() {
    let Some(()) = local("r6_flake_daemons_started_at_once_each_answer_their_first_request") else {
        return;
    };
    let sb = std::sync::Arc::new(Sandbox::new());
    let threads: Vec<_> = (0..12)
        .map(|i| {
            let sb = sb.clone();
            std::thread::spawn(move || {
                let d = ApiDaemon::spawn(&sb, &sb.base().join(format!("flake{i}")), API_TOKEN);
                let s = d.as_server();
                let r = s.try_request(
                    "PUT",
                    &format!("/api/v1/graphs/g{i}"),
                    Some(&s.bearer()),
                    &[],
                    None,
                    Duration::from_secs(20),
                );
                match r {
                    Ok(r) if r.status == 201 || r.status == 200 => {}
                    other => panic!(
                        "daemon {i} on port {}: {:?}",
                        d.port,
                        other.map(|r| r.status)
                    ),
                }
                // Its own data directory, not a neighbour's.
                assert!(
                    sb.base().join(format!("flake{i}/graphs/g{i}")).exists(),
                    "daemon {i} on port {} wrote somewhere else",
                    d.port
                );
            })
        })
        .collect();
    for t in threads {
        t.join().unwrap();
    }
}

/// One `instr()` over the 1 MB value cap is a single VM op that runs for
/// seconds, so a progress handler that looks at the clock every 10,000 ops
/// never gets to look. Eight rows of it ran 35.89 s against a 5 s limit.
const R6_SLOW_ROWS: &str =
    "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM c WHERE x<8) \
     SELECT x, instr(printf('%.*c',999999,'a'), printf('%.*c',499999-x,'a')||'b') FROM c";

/// R6 bypass: one slow op per row walked past the 5 s limit.
#[test]
fn r6_bypass_one_slow_op_per_row_is_still_stopped() {
    let Some(()) = local("r6_bypass_one_slow_op_per_row_is_still_stopped") else {
        return;
    };
    let sb = Sandbox::new();
    let d = api(&sb);
    let (st, body, took) = query(&d, R6_SLOW_ROWS);
    assert!(
        took < Duration::from_millis(6_500),
        "a query of slow single ops ran {took:?} against a 5 s limit ({st}: {body})"
    );
    assert!(st >= 400, "the stopped query answered {st}: {body}");
    assert!(
        body["error"].as_str().unwrap_or("").contains("time limit"),
        "the refusal does not say why: {body}"
    );
}

/// R6 bypass: clients that gave up left their queries running, four of
/// them held the daemon at 400% CPU for 40 s. A query past its limit must
/// stop, whoever is still waiting for it, and the daemon runs only a few
/// at once, saying so to the rest.
#[test]
fn r6_bypass_abandoned_queries_do_not_pile_up() {
    let Some(()) = local("r6_bypass_abandoned_queries_do_not_pile_up") else {
        return;
    };
    let sb = Sandbox::new();
    let d = api(&sb);
    let s = d.as_server();
    let t = Instant::now();
    let statuses: Vec<Result<u16, String>> = std::thread::scope(|scope| {
        let hs: Vec<_> = (0..12)
            .map(|_| {
                let s = s.clone();
                scope.spawn(move || {
                    s.try_request(
                        "POST",
                        "/api/v1/graphs/g/query",
                        Some(&s.bearer()),
                        &[("content-type", "application/json")],
                        Some(json!({"sql": R6_SLOW_ROWS}).to_string().as_bytes()),
                        Duration::from_secs(30),
                    )
                    .map(|r| r.status)
                })
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let took = t.elapsed();
    // Either the twelve arrived together, four ran and the rest were turned
    // away (503), in about one 5 s limit; or, on a loaded machine, they
    // arrived spread out and ran in waves of four, each stopped at 5 s: at
    // most three waves. Both show the bound. What must never happen is a
    // query running to its end (35 s) or more than four at once with none
    // turned away (all done in one wave).
    // (r6 pile-up flake: one loaded run took 15.05 s with 12 x 400, three
    // waves, no 503, and failed the old "under 8 s and some 503" check.)
    assert!(
        took < Duration::from_secs(20),
        "12 slow queries at once took {took:?}: {statuses:?}"
    );
    let waited_in_waves = took > Duration::from_secs(9);
    assert!(
        statuses.iter().any(|s| s == &Ok(503)) || waited_in_waves,
        "12 slow queries all ran at once in {took:?}, none was turned away: {statuses:?}"
    );
    // A stopped query's process is killed at the limit, so the daemon is
    // free again soon after the answers, not after the abandoned queries
    // would have finished (35 s each).
    let t = Instant::now();
    loop {
        let (st, body, _) = query(&d, "SELECT 1");
        if st == 200 {
            break;
        }
        assert_eq!(st, 503, "SELECT 1 got {st}: {body}");
        assert!(
            t.elapsed() < Duration::from_secs(7),
            "stopped queries still held the daemon {:?} after they were answered",
            t.elapsed()
        );
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// R6 bypass, round 3: SQLITE_LIMIT_LENGTH caps a finished value, not an
/// aggregate while it is being built. This one held 1.5 GB in its /query
/// child from 1 s until it was killed at 5 s; four slots made that about
/// 6 GB, for as long as a caller kept sending it.
#[test]
fn r6_bypass_query_memory_is_bounded() {
    let Some(()) = local("r6_bypass_query_memory_is_bounded") else {
        return;
    };
    let sb = Sandbox::new();
    let d = api(&sb);
    let daemon = d.child.id().to_string();
    for agg in [
        "json_group_object(x, printf('%.*c',100000,'a'))",
        "json_group_array(printf('%.*c',100000,'a'))",
    ] {
        let sql = format!(
            "SELECT {agg} FROM (WITH RECURSIVE r(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM r) SELECT x FROM r)"
        );
        let peak_kb = std::thread::scope(|scope| {
            let q = scope.spawn(|| query(&d, &sql));
            let mut peak = 0u64;
            // The /query child is the daemon's child; sample its RSS.
            for _ in 0..16 {
                std::thread::sleep(Duration::from_millis(250));
                let ps = std::process::Command::new("ps")
                    .args(["-A", "-o", "ppid=,rss="])
                    .output()
                    .unwrap();
                for line in String::from_utf8_lossy(&ps.stdout).lines() {
                    let mut f = line.split_whitespace();
                    if f.next() == Some(daemon.as_str()) {
                        peak = peak.max(f.next().and_then(|r| r.parse().ok()).unwrap_or(0));
                    }
                }
            }
            let (st, body, _) = q.join().unwrap();
            assert_eq!(st, 400, "{agg}: {body}");
            peak
        });
        assert!(peak_kb > 0, "{agg}: never saw the /query child");
        assert!(
            peak_kb < 200 * 1024,
            "{agg}: the /query child held {} MB",
            peak_kb / 1024
        );
    }
}

/// R6 bypass, round 2: sqlite3_interrupt is seen between ops, and one op can
/// run far past the limit. LIKE with a leading % and a 50,000-byte pattern
/// over a 1 MB value is a single op of about 50 s (GLOB the same): four of
/// them sent by clients that gave up at 2 s held the daemon at 398% CPU and
/// every /query got 503 for about 50 s, repeatably.
#[test]
fn r6_bypass_one_long_op_is_stopped_at_the_limit_too() {
    let Some(()) = local("r6_bypass_one_long_op_is_stopped_at_the_limit_too") else {
        return;
    };
    let sb = Sandbox::new();
    let d = api(&sb);
    let s = d.as_server();
    let like = "SELECT printf('%.*c',999999,'a') LIKE ('%' || printf('%.*c',49990,'a') || 'b')";
    let glob = "SELECT printf('%.*c',999999,'a') GLOB '*'||printf('%.*c',49990,'a')||'b'";

    // Waited for: answered at the limit with the reason.
    let t = Instant::now();
    let (st, body, _) = query(&d, like);
    assert_eq!(st, 400, "{body}");
    assert!(
        body["error"].as_str().unwrap_or("").contains("time limit"),
        "{body}"
    );
    assert!(
        t.elapsed() < Duration::from_millis(6500),
        "{:?}",
        t.elapsed()
    );

    // Abandoned: four clients give up at 2 s.
    let t = Instant::now();
    std::thread::scope(|scope| {
        for sql in [like, glob, like, glob] {
            let s = s.clone();
            scope.spawn(move || {
                let _ = s.try_request(
                    "POST",
                    "/api/v1/graphs/g/query",
                    Some(&s.bearer()),
                    &[("content-type", "application/json")],
                    Some(json!({ "sql": sql }).to_string().as_bytes()),
                    Duration::from_secs(2),
                );
            });
        }
    });
    loop {
        let (st, body, _) = query(&d, "SELECT 1");
        if st == 200 {
            break;
        }
        assert_eq!(st, 503, "SELECT 1 got {st}: {body}");
        assert!(
            t.elapsed() < Duration::from_secs(8),
            "four abandoned single-op queries still held every /query slot {:?} after they were sent: {body}",
            t.elapsed()
        );
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// R7: /query ATTACH passed the read-only check, and its error told apart an
/// existing file from a missing one; pragma_database_list leaked the data
/// directory's absolute path.
#[test]
fn r7_api_query_cannot_probe_the_filesystem() {
    let Some(()) = local("r7_api_query_cannot_probe_the_filesystem") else {
        return;
    };
    let sb = Sandbox::new();
    let d = api(&sb);
    let (st1, b1, _) = query(&d, "ATTACH DATABASE '/etc/hosts' AS x");
    let (st2, b2, _) = query(&d, "ATTACH DATABASE '/nonexistent/e2e/nothing' AS y");
    assert!(st1 >= 400 && st2 >= 400, "ATTACH was allowed: {b1} / {b2}");
    let norm = |v: &Value| {
        v.to_string()
            .replace("/etc/hosts", "P")
            .replace("/nonexistent/e2e/nothing", "P")
            .replace(" AS x", "")
            .replace(" AS y", "")
    };
    assert_eq!(
        norm(&b1),
        norm(&b2),
        "ATTACH errors reveal whether a file exists"
    );
    let (_, b, _) = query(&d, "SELECT file FROM pragma_database_list");
    let dir = d.data_dir.canonicalize().unwrap();
    assert!(
        !b.to_string().contains(dir.to_str().unwrap())
            && !b.to_string().contains(d.data_dir.to_str().unwrap()),
        "the data directory's absolute path leaked: {b}"
    );
}

/// R8: an API add_node without a branch got the daemon's own git branch and
/// HEAD commit; the docs say the server never injects one.
#[test]
fn r8_api_never_injects_the_daemons_branch() {
    let Some(()) = local("r8_api_never_injects_the_daemons_branch") else {
        return;
    };
    let sb = Sandbox::new();
    let data = sb.base().join("daemon-repo");
    std::fs::create_dir_all(&data).unwrap();
    sb.git_ok(&data, &["init", "-q"]);
    sb.git_ok(&data, &["checkout", "-q", "-b", "daemon-branch"]);
    sb.git_ok(&data, &["commit", "-q", "--allow-empty", "-m", "x"]);
    let d = ApiDaemon::spawn(&sb, &data, API_TOKEN);
    let s = d.as_server();
    s.request("PUT", "/api/v1/graphs/g", Some(&s.bearer()), &[], None);
    let (st, body) = d.tool("g", "add_node", json!({"node_type": "goal", "title": "t"}));
    assert_eq!(st, 200, "{body}");
    let id = body["data"]["result"]["node_id"].clone();
    let (_, shown) = d.tool("g", "show_node", json!({"node_id": id}));
    let text = shown.to_string();
    assert!(
        !text.contains("daemon-branch"),
        "the daemon's branch was injected: {text}"
    );
}

/// A remote caller's add_node refused only the exact string "HEAD"; "HEAD~1",
/// "@" and "main" were stored as the commit, literally. A remote caller's
/// revs cannot be resolved (the daemon's checkout is not theirs), so only a
/// hash is taken.
#[test]
fn remote_add_node_stores_no_unresolved_rev() {
    let Some(()) = local("remote_add_node_stores_no_unresolved_rev") else {
        return;
    };
    let sb = Sandbox::new();
    let d = api(&sb);
    for rev in [
        "HEAD~1",
        "@",
        "main",
        "origin/main",
        "abc",
        "0123456789abcdefg",
    ] {
        let (st, body) = d.tool(
            "g",
            "add_node",
            json!({"node_type": "goal", "title": rev, "commit": rev}),
        );
        let text = body.to_string();
        assert!(
            st != 200 || body["data"]["result"]["node_id"].is_null(),
            "commit {rev:?} was accepted: {text}"
        );
        assert!(
            text.contains(rev),
            "the refusal does not name {rev:?}: {text}"
        );
    }
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let (st, body) = d.tool(
        "g",
        "add_node",
        json!({"node_type": "goal", "title": "hash", "commit": sha}),
    );
    assert_eq!(st, 200, "{body}");
    assert!(!body["data"]["result"]["node_id"].is_null(), "{body}");
}

// ---------------------------------------------------------------- R9 / R10

/// R9: errors for missing/invalid params and for a lone surrogate replied
/// with "id": null, and a request with "id": null got no reply at all, so a
/// client waiting on its id hangs.
#[test]
fn r9_errors_carry_the_request_id() {
    let Some(()) = local("r9_errors_carry_the_request_id") else {
        return;
    };
    let sb = Sandbox::new();
    let p = project(&sb);
    let mut m = StdioMcp::spawn(&sb, &p.dir);
    for (id, line) in [
        (41, r#"{"jsonrpc":"2.0","id":41,"method":"tools/call"}"#.to_string()),
        (42, r#"{"jsonrpc":"2.0","id":42,"method":"tools/call","params":{"nope":1}}"#.to_string()),
        (43, r#"{"jsonrpc":"2.0","id":43,"method":"tools/call","params":"str"}"#.to_string()),
        (
            44,
            r#"{"jsonrpc":"2.0","id":44,"method":"tools/call","params":{"name":"add_node","arguments":{"node_type":"goal","title":"\ud800"}}}"#
                .to_string(),
        ),
    ] {
        m.send_raw(line.as_bytes());
        let r = m
            .recv(Duration::from_secs(10))
            .unwrap_or_else(|| panic!("no reply to {line}"));
        assert_eq!(r["id"], json!(id), "reply to {line} lost its id: {r}");
    }
    m.send_raw(br#"{"jsonrpc":"2.0","id":null,"method":"ping"}"#);
    let r = m.recv(Duration::from_secs(5));
    assert!(r.is_some(), "a request with id null got no reply");
}

/// R10: one invalid UTF-8 byte on stdin killed the server ("stream did not
/// contain valid UTF-8"), and the client lost it for the session.
#[test]
fn r10_invalid_utf8_is_a_parse_error_not_a_crash() {
    let Some(()) = local("r10_invalid_utf8_is_a_parse_error_not_a_crash") else {
        return;
    };
    let sb = Sandbox::new();
    let p = project(&sb);
    let mut m = StdioMcp::spawn(&sb, &p.dir);
    m.send_raw(b"\xff\xfe");
    let r = m.recv(Duration::from_secs(5));
    assert!(
        r.as_ref()
            .is_some_and(|v| v["error"]["code"] == json!(-32700)),
        "invalid UTF-8 did not produce a parse error: {r:?}\nstderr: {}",
        m.stderr.lock().unwrap()
    );
    m.send_raw(
        br#"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"add_node","arguments":{"node_type":"goal","title":"after \xff"}}}"#,
    );
    let r = m.recv(Duration::from_secs(5));
    assert!(r.is_some(), "no reply to invalid UTF-8 inside a string");
    let pong = m.request("ping", json!({}));
    assert!(
        pong.get("result").is_some(),
        "the server stopped answering: {pong}"
    );
}

// ---------------------------------------------------------------- R11

/// R11: resume_session on an ended session said "Resumed" but re-ended it
/// (is_active false, new ended_at, summary wiped).
#[test]
fn r11_resume_session_resumes_or_refuses() {
    let Some(()) = local("r11_resume_session_resumes_or_refuses") else {
        return;
    };
    let sb = Sandbox::new();
    let p = project(&sb);
    let mut m = StdioMcp::spawn(&sb, &p.dir);
    let s = m.call_ok("start_session", json!({"name": "s", "goal_title": "g"}));
    let sid = s["session_id"].clone();
    m.call_ok("end_session", json!({"summary": "the summary"}));
    match m.call("resume_session", json!({"session_id": sid})) {
        Err(_) => {
            let got = m.call_ok("get_session", json!({"session_id": sid}));
            assert_eq!(
                got["summary"],
                json!("the summary"),
                "a refused resume wiped: {got}"
            );
        }
        Ok(_) => {
            let got = m.call_ok("get_session", json!({"session_id": sid}));
            assert_eq!(
                got["is_active"],
                json!(true),
                "\"Resumed\" but not active: {got}"
            );
            drop(m);
            let mut m2 = StdioMcp::spawn(&sb, &p.dir);
            let cur = m2.call_ok("get_session", json!({}));
            assert_eq!(
                cur["session_id"], sid,
                "the resumed session did not survive a restart"
            );
        }
    }
}

// ---------------------------------------------------------------- R12 / R13

/// R12: invalid input reported success: a status on a node that does not
/// exist, node type "banana", empty titles, self-loops, unknown statuses,
/// and out-of-range or string confidences (silently dropped).
#[test]
fn r12_invalid_writes_are_refused() {
    let Some(()) = local("r12_invalid_writes_are_refused") else {
        return;
    };
    let sb = Sandbox::new();
    let p = project(&sb);
    p.add("goal", "one");
    let mut m = StdioMcp::spawn(&sb, &p.dir);
    let cases = [
        (
            "update_status",
            json!({"node_id": 99999, "status": "completed"}),
        ),
        ("add_node", json!({"node_type": "banana", "title": "t"})),
        ("add_node", json!({"node_type": "goal", "title": ""})),
        ("add_node", json!({"node_type": "goal", "title": "   "})),
        ("link_nodes", json!({"from_id": 1, "to_id": 1})),
        ("update_status", json!({"node_id": 1, "status": "done"})),
        (
            "add_node",
            json!({"node_type": "goal", "title": "t", "confidence": -5}),
        ),
        (
            "add_node",
            json!({"node_type": "goal", "title": "t", "confidence": "90"}),
        ),
        (
            "add_node",
            json!({"node_type": "goal", "title": "t", "confidence": 101}),
        ),
    ];
    for (tool, args) in cases {
        let r = m.call(tool, args.clone());
        assert!(r.is_err(), "{tool} {args} was accepted: {r:?}");
    }
    let v = p.view();
    assert_eq!(
        v.nodes.len(),
        1,
        "invalid adds created nodes: {:?}",
        v.nodes
    );
    assert!(v.edges.is_empty(), "a self-loop was created");
}

/// R13: export_dot put `rankdir` into the DOT unescaped, so an argument
/// could add nodes and edges to the rendered graph.
#[test]
fn r13_export_dot_rankdir_cannot_inject() {
    let Some(()) = local("r13_export_dot_rankdir_cannot_inject") else {
        return;
    };
    let sb = Sandbox::new();
    let p = project(&sb);
    p.add("goal", "g");
    let mut m = StdioMcp::spawn(&sb, &p.dir);
    let r = m.call(
        "export_dot",
        json!({"rankdir": "LR; injected_node [label=\"INJECTED\"]; 1 -> injected_node"}),
    );
    if let Ok(dot) = r {
        assert!(
            !dot.to_string().contains("injected_node ["),
            "rankdir injected DOT: {dot}"
        );
    }
    let t = "line one\n## injected heading";
    p.ok(&["add", "goal", t]);
    let w = m.call_ok("generate_writeup", json!({"title": "w"}));
    let text = w
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| w.to_string());
    assert!(
        !text
            .lines()
            .any(|l| l.trim_start() == "## injected heading"),
        "a title with a newline added a markdown heading:\n{text}"
    );
}

// ---------------------------------------------------------------- R14

/// R14: 30 racing PUTs of the same new graph answered "created": true to
/// more than one caller.
#[test]
fn r14_racing_graph_creates_report_one_creation() {
    let Some(()) = local("r14_racing_graph_creates_report_one_creation") else {
        return;
    };
    let sb = Sandbox::new();
    let d = ApiDaemon::spawn(&sb, &sb.base().join("race"), API_TOKEN);
    let s = d.as_server();
    let created: usize = std::thread::scope(|scope| {
        let hs: Vec<_> = (0..30)
            .map(|_| {
                let s = s.clone();
                scope.spawn(move || {
                    let r = s.request("PUT", "/api/v1/graphs/raced", Some(&s.bearer()), &[], None);
                    let v: Value = serde_json::from_str(&r.body).unwrap_or(Value::Null);
                    usize::from(v["data"]["created"] == json!(true))
                })
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).sum()
    });
    assert_eq!(
        created, 1,
        "{created} callers were told they created the graph"
    );
}

/// The r14 flake: in one of six whole-battery runs, one of the 30 racing
/// PUTs above got "Connection reset by peer". The mechanism found behind
/// it: tiny_http ends its accept thread on the first accept() error
/// (EMFILE, ECONNABORTED), which closes the listening socket, resets every
/// connection queued on it, and makes `serve --api` exit 0 without a word.
/// Driven here with EMFILE, the one accept error a test can cause at will:
/// a descriptor limit of 64 and 80 idle connections.
#[test]
fn r14_flake_an_accept_error_does_not_stop_the_daemon() {
    let Some(()) = local("r14_flake_an_accept_error_does_not_stop_the_daemon") else {
        return;
    };
    let sb = Sandbox::new();
    let data = sb.base().join("fd");
    std::fs::create_dir_all(&data).unwrap();

    let mut c = sb.cmd("/bin/sh", &data);
    c.args([
        "-c",
        "ulimit -n 64 && exec \"$0\" serve --api --port \"$1\" --data-dir \"$2\"",
        bin().to_str().unwrap(),
        "0",
        data.to_str().unwrap(),
    ])
    .env("DECIDUOUS_API_TOKEN", API_TOKEN)
    .stderr(std::process::Stdio::piped());
    let (mut child, port) = spawn_listening(c);
    let s = Server {
        url: format!("http://127.0.0.1:{port}"),
        token: API_TOKEN.to_string(),
    };
    let up = wait_for(Duration::from_secs(10), || {
        std::net::TcpStream::connect(("127.0.0.1", port))
            .ok()
            .map(|_| ())
    });
    assert!(up.is_some(), "serve --api never listened");

    // More connections than the daemon has descriptors: its accept() fails.
    let idle: Vec<_> = (0..80)
        .filter_map(|_| std::net::TcpStream::connect(("127.0.0.1", port)).ok())
        .collect();
    std::thread::sleep(Duration::from_millis(500));
    drop(idle);

    let exited = child.try_wait().unwrap();
    let answered = wait_for(Duration::from_secs(10), || {
        s.try_request(
            "GET",
            "/api/v1/graphs",
            Some(&s.bearer()),
            &[],
            None,
            Duration::from_secs(2),
        )
        .ok()
        .filter(|r| r.status == 200)
    });
    let _ = child.kill();
    let out = child.wait_with_output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        exited.is_none(),
        "serve --api exited ({exited:?}) after an accept error; stderr:\n{stderr}"
    );
    assert!(
        answered.is_some(),
        "serve --api stopped answering after an accept error; stderr:\n{stderr}"
    );
    // Whether accept() failed, or tiny_http panicked just after an accept,
    // or the kernel kept the extra connections queued, depends on timing;
    // what must hold in every case is the two assertions above. When it
    // did fail, it says so.
    if !stderr.is_empty() {
        assert!(
            stderr.contains("accepting again") || stderr.contains("Too many open files"),
            "unexpected daemon stderr:\n{stderr}"
        );
    }
}

/// R14, round 2: the recovery above pushed a new tiny_http server after every
/// accept death and never dropped the old ones, then polled each in turn for
/// 5 ms, so every request waited about 5 ms more per restart, without bound:
/// median GET latency 0.000 s before, 0.209 s after 25 bursts of 120 idle
/// connections, 0.823 s after 100.
#[test]
fn r14_restarts_do_not_slow_every_later_request() {
    let Some(()) = local("r14_restarts_do_not_slow_every_later_request") else {
        return;
    };
    let sb = Sandbox::new();
    let data = sb.base().join("fd2");
    std::fs::create_dir_all(&data).unwrap();

    let mut c = sb.cmd("/bin/sh", &data);
    c.args([
        "-c",
        "ulimit -n 64 && exec \"$0\" serve --api --port \"$1\" --data-dir \"$2\"",
        bin().to_str().unwrap(),
        "0",
        data.to_str().unwrap(),
    ])
    .env("DECIDUOUS_API_TOKEN", API_TOKEN)
    .stderr(std::process::Stdio::piped());
    let (mut child, port) = spawn_listening(c);
    let s = Server {
        url: format!("http://127.0.0.1:{port}"),
        token: API_TOKEN.to_string(),
    };
    let get = || {
        let t = Instant::now();
        let r = s.try_request(
            "GET",
            "/api/v1/graphs",
            Some(&s.bearer()),
            &[],
            None,
            Duration::from_secs(10),
        );
        (r.map(|r| r.status).unwrap_or(0), t.elapsed())
    };
    let up = wait_for(Duration::from_secs(10), || (get().0 == 200).then_some(()));
    assert!(up.is_some(), "serve --api never answered");

    for _ in 0..25 {
        let idle: Vec<_> = (0..120)
            .filter_map(|_| std::net::TcpStream::connect(("127.0.0.1", port)).ok())
            .collect();
        std::thread::sleep(Duration::from_millis(300));
        drop(idle);
    }
    // Let the last restart's backoff finish.
    let answered = wait_for(Duration::from_secs(15), || (get().0 == 200).then_some(()));
    let mut times: Vec<Duration> = (0..15)
        .map(|_| get())
        .filter(|(st, _)| *st == 200)
        .map(|(_, t)| t)
        .collect();
    times.sort();
    let _ = child.kill();
    let out = child.wait_with_output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    let restarts = stderr.matches("accepting again").count();
    assert!(
        answered.is_some(),
        "the daemon stopped answering; stderr:\n{stderr}"
    );
    assert!(
        restarts >= 8,
        "only {restarts} accept restarts in 25 bursts; this test proves nothing"
    );
    assert_eq!(times.len(), 15, "some GETs failed after the bursts");
    let median = times[times.len() / 2];
    eprintln!("{restarts} accept restarts; median GET {median:?}");
    assert!(
        median < Duration::from_millis(100),
        "after {restarts} accept restarts the median GET takes {median:?} (all: {times:?})"
    );
}

/// R14: `--token ""` did not fall back to DECIDUOUS_API_TOKEN, and a token
/// of only whitespace started a daemon nobody could authenticate to.
#[test]
fn r14_api_token_edge_cases_fail_loudly() {
    let Some(()) = local("r14_api_token_edge_cases_fail_loudly") else {
        return;
    };
    let sb = Sandbox::new();
    let data = sb.base().join("tok");
    std::fs::create_dir_all(&data).unwrap();
    let port = dead_port();
    let mut c = sb.cmd(bin(), &data);
    c.args([
        "serve",
        "--api",
        "--port",
        &port.to_string(),
        "--data-dir",
        data.to_str().unwrap(),
        "--token",
        "   ",
    ])
    .stdout(std::process::Stdio::null())
    .stderr(std::process::Stdio::piped());
    let mut child = c.spawn().unwrap();
    let exited = wait_for(Duration::from_secs(5), || child.try_wait().ok().flatten());
    if exited.is_none() {
        let _ = child.kill();
        let _ = child.wait();
        panic!("serve --api started with a whitespace-only token nobody can present");
    }
    assert!(
        !exited.unwrap().success(),
        "a whitespace-only token exited 0"
    );
}

// ---------------------------------------------------------------- sync tool

/// R14: the MCP `sync` tool on a corrupt graph.json told the agent to run
/// `deciduous sync`, which is what it had just done.
#[test]
fn r14_sync_tool_error_is_not_self_referential() {
    let Some(()) = local("r14_sync_tool_error_is_not_self_referential") else {
        return;
    };
    let sb = Sandbox::new();
    let p = project(&sb);
    p.add("goal", "g");
    std::fs::write(p.graph_file(), "{\"nodes\": {").unwrap();
    let mut m = StdioMcp::spawn(&sb, &p.dir);
    let r = m.call("sync", json!({}));
    let text = format!("{r:?}");
    assert!(r.is_err(), "sync accepted a corrupt graph.json: {text}");
    assert!(
        !text.contains("run `deciduous sync`"),
        "self-referential advice: {text}"
    );
}
