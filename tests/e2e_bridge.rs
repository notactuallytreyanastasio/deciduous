//! The CLI -> shared server bridge, driven end to end: the real binary in a
//! real git repository, the real server over HTTP, and an agent writing to the
//! same workspace through HTTP MCP at the same time.
//!
//! Gated on `DECIDUOUS_E2E_SERVER` + `DECIDUOUS_E2E_TOKEN` (see
//! `tests/e2e_support/mod.rs`). Each test owns a fresh workspace.
//!
//! The contract these tests hold the bridge to is the write-ahead log one:
//! every local mutation is an operation, carrying only the fields it changed,
//! that reaches the server exactly once whether or not the server was
//! reachable when it was made, and that never overwrites a field it did not
//! change. Deletes and unlinks are operations too.

mod e2e_support;

use e2e_support::*;
use serde_json::json;

/// A project pointed at the server under test, in its own workspace.
fn remote_project<'a>(sb: &'a Sandbox, name: &str) -> (Project<'a>, String) {
    let ws = unique(name);
    let p = sb.project(&ws, None);
    p.remote_init(&ws);
    (p, ws)
}

fn agent(server: &Server, ws: &str) -> HttpMcp {
    server.session(Some(ws))
}

/// Local and server content agree, edges included.
fn assert_converged(p: &Project, server: &Server, ws: &str) {
    let local = p.view();
    let remote = server.view(ws);
    assert!(
        local == remote,
        "local and server differ:\n{}",
        local.diff(&remote, "local", "server")
    );
}

// ---------------------------------------------------------------- C1

/// C1: `status`, `link` and `prompt` sent the whole stale local row, so an
/// agent's retitle and description were reverted by an unrelated local write.
#[test]
fn c1_local_writes_never_revert_fields_an_agent_changed() {
    let Some(server) = remote("c1_local_writes_never_revert_fields_an_agent_changed") else {
        return;
    };
    let sb = Sandbox::with_server(server.clone());
    let (p, ws) = remote_project(&sb, "c1");
    let g = p.add("goal", "g1");
    let a = p.add("action", "a1");
    let o = p.add("outcome", "o1");

    let mut ag = agent(&server, &ws);
    let a_uuid = ag.uuid_of(&ws, &a);
    let g_uuid = ag.uuid_of(&ws, &g);
    ag.call_ok(
        "update_node",
        json!({"node_id": a_uuid, "title": "a1 retitled by agent", "description": "agent detail"}),
    );
    ag.call_ok(
        "update_node",
        json!({"node_id": g_uuid, "title": "g1 retitled by agent", "status": "completed"}),
    );

    // Three unrelated local writes that each touch one of those nodes.
    p.ok(&["status", &a, "active"]);
    p.ok(&["link", &g, &o, "-r", "later link"]);
    p.ok(&["prompt", &a, "a prompt set locally"]);

    let ex = server.export(&ws);
    let node = |cid: &str| {
        ex["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["change_id"] == json!(cid))
            .cloned()
            .unwrap_or_else(|| panic!("server lost node {cid}"))
    };
    let an = node(&a);
    assert_eq!(
        an["title"],
        json!("a1 retitled by agent"),
        "agent title reverted: {an}"
    );
    assert_eq!(
        an["description"],
        json!("agent detail"),
        "agent description reverted: {an}"
    );
    assert_eq!(
        an["status"],
        json!("active"),
        "local status did not arrive: {an}"
    );
    assert_eq!(
        an["metadata"]["prompt"],
        json!("a prompt set locally"),
        "local prompt did not arrive: {an}"
    );
    let gn = node(&g);
    assert_eq!(
        gn["title"],
        json!("g1 retitled by agent"),
        "link reverted the goal: {gn}"
    );
    assert_eq!(
        gn["status"],
        json!("completed"),
        "link reverted the goal: {gn}"
    );
}

// ---------------------------------------------------------------- C2

/// C2: an edit made while the server was down never reached it; `remote
/// push` said "Nothing to push" and `remote status` said OK.
#[test]
fn c2_offline_edits_replay_on_push_and_status_is_not_clean_until_then() {
    let Some(server) = remote("c2_offline_edits_replay_on_push_and_status_is_not_clean_until_then")
    else {
        return;
    };
    let sb = Sandbox::with_server(server.clone());
    let (p, ws) = remote_project(&sb, "c2");
    let g = p.add("goal", "g1");
    let a = p.add("action", "a1");
    let o = p.add("outcome", "o1");
    p.ok(&["link", &g, &a]);
    p.ok(&["link", &a, &o]);
    assert_converged(&p, &server, &ws);

    p.set_remote_url(&format!("http://127.0.0.1:{}", dead_port()));
    let st = p.dx(&["status", &g, "completed"]);
    assert!(
        st.ok(),
        "an offline write must still succeed locally: {}",
        st.all()
    );
    assert!(
        st.stderr.contains("Warning") || st.stderr.contains("server"),
        "an offline write must say the server did not get it: {}",
        st.all()
    );
    p.ok(&["prompt", &a, "offline prompt"]);
    p.ok(&["unlink", &a, &o]);
    p.ok(&["delete", &o]);
    p.set_remote_url(&server.url);

    let before = p.dx(&["remote", "status"]);
    assert!(
        !status_says_clean(&before),
        "remote status called a diverged graph clean:\n{}",
        before.all()
    );

    let push = p.dx(&["remote", "push"]);
    assert!(push.ok(), "remote push failed: {}", push.all());
    assert!(
        !push.stdout.contains("Nothing to push"),
        "push claimed nothing was pending while four operations were:\n{}",
        push.all()
    );
    assert_converged(&p, &server, &ws);
    let after = p.dx(&["remote", "status"]);
    assert!(
        status_says_clean(&after),
        "status after replay:\n{}",
        after.all()
    );

    // Replaying twice is harmless: nothing re-applies, nothing duplicates.
    p.ok(&["remote", "push"]);
    assert_converged(&p, &server, &ws);
}

// ---------------------------------------------------------------- C3

/// C3: archaeology pivot and supersede wrote locally and never reached the
/// server, with no warning; after a push the superseded statuses stayed.
#[test]
fn c3_archaeology_writes_reach_the_server() {
    let Some(server) = remote("c3_archaeology_writes_reach_the_server") else {
        return;
    };
    let sb = Sandbox::with_server(server.clone());
    let (p, ws) = remote_project(&sb, "c3");
    let g = p.add("goal", "g");
    let d = p.add("decision", "old approach");
    p.ok(&["link", &g, &d]);
    let a = p.add("action", "old action");
    p.ok(&["link", &d, &a]);

    p.ok(&[
        "archaeology",
        "pivot",
        &d,
        "it did not scale",
        "new approach",
    ]);
    p.ok(&["archaeology", "supersede", &d, "--cascade"]);
    assert_converged(&p, &server, &ws);
    let st = p.dx(&["remote", "status"]);
    assert!(status_says_clean(&st), "{}", st.all());
}

// ---------------------------------------------------------------- C4

/// C4: a node an agent deleted on the server never left the local graph, and
/// every later local write re-upserted it into rows no one could see.
#[test]
fn c4_server_side_delete_reaches_the_local_graph_on_pull() {
    let Some(server) = remote("c4_server_side_delete_reaches_the_local_graph_on_pull") else {
        return;
    };
    let sb = Sandbox::with_server(server.clone());
    let (p, ws) = remote_project(&sb, "c4");
    let g = p.add("goal", "g");
    let doomed = p.add("action", "doomed");
    p.ok(&["link", &g, &doomed]);

    let mut ag = agent(&server, &ws);
    let uuid = ag.uuid_of(&ws, &doomed);
    ag.call_ok("delete_node", json!({"node_id": uuid}));

    p.ok(&["remote", "pull"]);
    let local = p.view();
    assert!(
        !local.nodes.contains_key(&doomed),
        "a node deleted on the server is still local after pull"
    );
    assert!(
        local.dangling().is_empty(),
        "dangling edges: {:?}",
        local.dangling()
    );

    // And a later write must not resurrect it on the server.
    let _ = p.add("observation", "later");
    let remote_view = server.view(&ws);
    assert!(
        !remote_view.nodes.contains_key(&doomed),
        "a later local write resurrected the deleted node on the server"
    );
    assert_converged(&p, &server, &ws);
    let st = p.dx(&["remote", "status"]);
    assert!(status_says_clean(&st), "{}", st.all());
}

/// The other direction of C4, and the reason for a write-ahead log: a local
/// delete and unlink are operations and reach the server.
#[test]
fn c4_local_delete_and_unlink_reach_the_server() {
    let Some(server) = remote("c4_local_delete_and_unlink_reach_the_server") else {
        return;
    };
    let sb = Sandbox::with_server(server.clone());
    let (p, ws) = remote_project(&sb, "c4b");
    let g = p.add("goal", "g");
    let a = p.add("action", "a");
    let b = p.add("action", "b");
    p.ok(&["link", &g, &a]);
    p.ok(&["link", &g, &b]);
    p.ok(&["unlink", &g, &a]);
    p.ok(&["delete", &b]);
    assert_converged(&p, &server, &ws);
}

// ---------------------------------------------------------------- C5

/// C5: two different repositories with the same root name (or one differing
/// only in case) silently shared one workspace, and a pull imported one
/// team's goals into the other's committable graph.json.
#[test]
fn c5_same_basename_different_repos_never_share_a_graph() {
    let Some(server) = remote("c5_same_basename_different_repos_never_share_a_graph") else {
        return;
    };
    let sb = Sandbox::with_server(server.clone());
    let name = unique("c5");
    let a_dir = sb.base().join("team-a");
    let b_dir = sb.base().join("team-b");
    std::fs::create_dir_all(&a_dir).unwrap();
    std::fs::create_dir_all(&b_dir).unwrap();
    // Built by hand so both repositories are literally called `name`.
    let make = |parent: &std::path::Path, n: &str| -> std::path::PathBuf {
        let d = parent.join(n);
        std::fs::create_dir_all(&d).unwrap();
        sb.git_ok(&d, &["init", "-q"]);
        // Two repositories are told apart by their root commit. An empty
        // commit with the same message, author and second is the same
        // commit in both, which is one repository as far as git (and the
        // claim) can tell; the parent directory makes them two.
        let msg = format!("init {}", parent.display());
        sb.git_ok(&d, &["commit", "-q", "--allow-empty", "-m", &msg]);
        let o = sb.cmd(bin(), &d).arg("init").output().unwrap();
        assert!(o.status.success());
        d
    };
    let a = Project {
        dir: make(&a_dir, &name),
        sb: &sb,
    };
    let b = Project {
        dir: make(&b_dir, &name.to_uppercase()),
        sb: &sb,
    };
    a.ok(&["remote", "init", &server.url]);
    let secret = a.add("goal", "team A's private goal");

    let init_b = b.dx(&["remote", "init", &server.url]);
    if init_b.ok() {
        let pull = b.dx(&["remote", "pull"]);
        let leaked = b.view().nodes.contains_key(&secret);
        assert!(
            !leaked,
            "a different repository with the same name imported team A's goal:\ninit: {}\npull: {}",
            init_b.all(),
            pull.all()
        );
    } else {
        assert!(
            init_b.all().contains(&name.to_lowercase()) || init_b.all().contains("workspace"),
            "a refusal must name the colliding workspace: {}",
            init_b.all()
        );
    }
}

// ---------------------------------------------------------------- C6

/// C6: renaming or moving a repository forked its graph into a new
/// workspace, because the workspace was re-derived from the directory name
/// on every call.
#[test]
fn c6_moving_a_repo_keeps_writing_to_the_same_workspace() {
    let Some(server) = remote("c6_moving_a_repo_keeps_writing_to_the_same_workspace") else {
        return;
    };
    let sb = Sandbox::with_server(server.clone());
    let name = unique("c6");
    let p = sb.project(&name, None);
    p.ok(&["remote", "init", &server.url]);
    let first = p.add("goal", "before the move");
    let ws = name.to_lowercase();
    assert!(
        server.view(&ws).nodes.contains_key(&first),
        "precondition: derived workspace {ws} holds the first node"
    );

    let moved = sb.base().join(format!("{name}-renamed"));
    std::fs::rename(&p.dir, &moved).unwrap();
    let p2 = Project {
        dir: moved,
        sb: &sb,
    };
    let second = p2.add("goal", "after the move");
    let v = server.view(&ws);
    assert!(
        v.nodes.contains_key(&second),
        "after a rename the write went somewhere other than workspace {ws}: {:?}",
        v.nodes.keys().collect::<Vec<_>>()
    );
    let forked = server.view(&format!("{name}-renamed").to_lowercase());
    assert!(
        forked.nodes.is_empty(),
        "the rename forked the graph into a second workspace with {} node(s)",
        forked.nodes.len()
    );
}

// ---------------------------------------------------------------- C7

/// C7: a git worktree got its own workspace (`<repo>-<worktree>`), while
/// the server's instructions say worktrees share the repository root's name.
#[test]
fn c7_a_worktree_writes_to_its_repository_workspace() {
    let Some(server) = remote("c7_a_worktree_writes_to_its_repository_workspace") else {
        return;
    };
    let sb = Sandbox::with_server(server.clone());
    let name = unique("c7");
    let p = sb.project(&name, None);
    p.ok(&["remote", "init", &server.url]);
    p.commit_graph("remote");
    let ws = name.to_lowercase();
    let wt = sb.base().join(format!("{name}-feature"));
    p.git_ok(&[
        "worktree",
        "add",
        "-q",
        wt.to_str().unwrap(),
        "-b",
        "feature",
    ]);
    let w = Project { dir: wt, sb: &sb };
    let cid = w.add("goal", "from the worktree");
    assert!(
        server.view(&ws).nodes.contains_key(&cid),
        "a worktree's write did not reach its repository's workspace {ws}"
    );
}

// ---------------------------------------------------------------- C8

/// C8: a repository whose name is not ASCII could not use a remote at all:
/// the workspace was percent-encoded per code point (é -> %E9), not per UTF-8
/// byte, and the server answered 400.
#[test]
fn c8_non_ascii_repository_names_work() {
    let Some(server) = remote("c8_non_ascii_repository_names_work") else {
        return;
    };
    let sb = Sandbox::with_server(server.clone());
    let name = format!("{}-café-ünï", unique("c8"));
    let p = sb.project(&name, None);
    let init = p.dx(&["remote", "init", &server.url]);
    assert!(init.ok(), "remote init in {name}: {}", init.all());
    let add = p.dx(&["add", "goal", "g"]);
    assert!(add.ok(), "{}", add.all());
    assert!(
        !add.stderr.contains("Warning"),
        "a reachable server was reported unreachable: {}",
        add.all()
    );
    let st = p.dx(&["remote", "status"]);
    assert!(status_says_clean(&st), "{}", st.all());
}

// ---------------------------------------------------------------- C9

/// C9: `remote status` compared counts only. One unpushed local node plus
/// one different agent node gave equal counts and "OK".
#[test]
fn c9_status_compares_content_not_counts() {
    let Some(server) = remote("c9_status_compares_content_not_counts") else {
        return;
    };
    let sb = Sandbox::with_server(server.clone());
    let (p, ws) = remote_project(&sb, "c9");
    p.add("goal", "shared");
    // One node only local (written while the server was away)...
    p.set_remote_url(&format!("http://127.0.0.1:{}", dead_port()));
    p.ok(&["add", "goal", "local only"]);
    p.set_remote_url(&server.url);
    // ...and one node only on the server.
    let mut ag = agent(&server, &ws);
    ag.call_ok(
        "add_node",
        json!({"node_type": "goal", "title": "server only"}),
    );

    let st = p.dx(&["remote", "status"]);
    assert!(
        !status_says_clean(&st),
        "equal counts, different content, and status said clean:\n{}",
        st.all()
    );

    // An edited title with identical counts must not read as clean either.
    p.ok(&["remote", "push"]);
    p.ok(&["remote", "pull"]);
    assert!(status_says_clean(&p.dx(&["remote", "status"])));
    let cid = p
        .view()
        .nodes
        .iter()
        .find(|(_, n)| n.title == "shared")
        .unwrap()
        .0
        .clone();
    let uuid = ag.uuid_of(&ws, &cid);
    ag.call_ok(
        "update_node",
        json!({"node_id": uuid, "status": "completed"}),
    );
    let st = p.dx(&["remote", "status"]);
    assert!(
        !status_says_clean(&st),
        "a status changed on the server and remote status said clean:\n{}",
        st.all()
    );
}

// ---------------------------------------------------------------- cosmetic

/// A single `link` printed "Pushed 2 node(s), 1 edge(s)", the message meant
/// for clearing a backlog; `remote pull` said "imported 0 nodes" when it had
/// applied server edits.
#[test]
fn c_cosmetic_messages_describe_what_happened() {
    let Some(server) = remote("c_cosmetic_messages_describe_what_happened") else {
        return;
    };
    let sb = Sandbox::with_server(server.clone());
    let (p, ws) = remote_project(&sb, "cmsg");
    let a = p.add("goal", "a");
    let b = p.add("action", "b");
    let link = p.ok(&["link", &a, &b]);
    assert!(
        !link.contains("Pushed"),
        "a plain link reported a backlog push: {link}"
    );
    let mut ag = agent(&server, &ws);
    let uuid = ag.uuid_of(&ws, &a);
    ag.call_ok(
        "update_node",
        json!({"node_id": uuid, "status": "completed"}),
    );
    let pull = p.ok(&["remote", "pull"]);
    assert!(
        !pull.contains("imported 0 nodes") || pull.contains("updated"),
        "pull applied an edit and reported nothing: {pull}"
    );
    assert_eq!(p.view().nodes[&a].status, "completed");
}

// ---------------------------------------------------------------- WAL

/// The contract the fix is built on: a same-field conflict between an
/// offline local edit and a later agent edit resolves to the later one. An
/// operation queued at 10:00 and replayed at 10:10 must not overwrite what
/// an agent wrote at 10:05. (Replaying in arrival order would.)
#[test]
fn wal_replay_does_not_overwrite_a_newer_server_edit_of_the_same_field() {
    let Some(server) =
        remote("wal_replay_does_not_overwrite_a_newer_server_edit_of_the_same_field")
    else {
        return;
    };
    let sb = Sandbox::with_server(server.clone());
    let (p, ws) = remote_project(&sb, "walorder");
    let n = p.add("action", "n");
    p.set_remote_url(&format!("http://127.0.0.1:{}", dead_port()));
    p.ok(&["status", &n, "active"]);
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let mut ag = agent(&server, &ws);
    let uuid = ag.uuid_of(&ws, &n);
    ag.call_ok(
        "update_node",
        json!({"node_id": uuid, "status": "completed"}),
    );
    p.set_remote_url(&server.url);
    // Refused, so the push exits 1 (round-2 BRIDGE-N10), and the pull
    // takes the server's value, which settles the refusal (BRIDGE-N9).
    let push = p.dx(&["remote", "push"]);
    assert!(!push.ok(), "a refused push exited 0:\n{}", push.all());
    p.ok(&["remote", "pull"]);
    assert_eq!(
        server.view(&ws).nodes[&n].status,
        "completed",
        "the older offline status replayed over the newer agent status"
    );
    assert_converged(&p, &server, &ws);
}

/// Writes made through the stdio MCP server are local mutations like the
/// CLI's and must reach the shared server the same way.
#[test]
fn wal_stdio_mcp_writes_reach_the_server() {
    let Some(server) = remote("wal_stdio_mcp_writes_reach_the_server") else {
        return;
    };
    let sb = Sandbox::with_server(server.clone());
    let (p, ws) = remote_project(&sb, "walstdio");
    let g = p.add("goal", "g");
    let mut m = StdioMcp::spawn(&sb, &p.dir);
    let r = m.call_ok(
        "add_node",
        json!({"node_type": "action", "title": "via stdio"}),
    );
    let id = r["node_id"].as_i64().unwrap();
    m.call_ok("link_nodes", json!({"from_id": g, "to_id": id}));
    m.call_ok(
        "update_status",
        json!({"node_id": g, "status": "completed"}),
    );
    drop(m);
    p.ok(&["remote", "push"]);
    assert_converged(&p, &server, &ws);
}
