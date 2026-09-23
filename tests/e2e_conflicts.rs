//! Conflict semantics of the op log, end to end: three clones of one
//! repository sharing one server, some of them offline for a while, edits
//! reaching each other through git and through the server in both orders.
//!
//! Every op that changes existing state carries what it was made against,
//! and the server refuses a stale one instead of applying it. An edit that
//! arrived through git is newer state, not something an older queued op may
//! overwrite. The tests are named after the round-2 battery findings they
//! reproduce (findings/round2-gitsync.md, round2-bridge.md, round2-stack.md,
//! team.md).
//!
//! Needs `DECIDUOUS_E2E_SERVER` and `DECIDUOUS_E2E_TOKEN`.

mod e2e_support;

use e2e_support::*;
use serde_json::{json, Value};

/// Three machines on one repository and one workspace.
struct Team<'a> {
    alice: Project<'a>,
    bob: Project<'a>,
    carol: Project<'a>,
    ws: String,
    server: Server,
}

fn team<'a>(sb: &'a Sandbox, server: &Server, name: &str) -> Team<'a> {
    let origin = sb.origin("origin");
    let alice = sb.project("alice", Some(&origin));
    let ws = unique(name);
    alice.remote_init(&ws);
    alice.commit_graph("remote");
    alice.git_ok(&["push", "-q", "origin", "HEAD:main"]);
    let bob = sb.clone_of(&origin, "bob");
    let carol = sb.clone_of(&origin, "carol");
    Team {
        alice,
        bob,
        carol,
        ws,
        server: server.clone(),
    }
}

impl Team<'_> {
    fn offline(&self, p: &Project) {
        p.set_remote_url(&format!("http://127.0.0.1:{}", dead_port()));
    }

    fn online(&self, p: &Project) {
        p.set_remote_url(&self.server.url);
    }

    /// sync, commit, pull, sync, push. config.toml is committed, so the
    /// pull may bring another clone's dead URL; the clone's own state is put
    /// back afterwards.
    fn exchange(&self, p: &Project, online: bool) {
        p.git_exchange();
        if online {
            self.online(p);
        } else {
            self.offline(p);
        }
    }

    fn server_node(&self, cid: &str) -> Option<Value> {
        self.server.export(&self.ws)["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["change_id"].as_str() == Some(cid) && n["deleted_at"].is_null())
            .cloned()
    }

    fn agent(&self) -> HttpMcp {
        self.server.session(Some(&self.ws))
    }
}

fn status_of(p: &Project, cid: &str) -> Option<String> {
    p.view().nodes.get(cid).map(|n| n.status.clone())
}

fn has_edge(v: &View, from: &str, to: &str) -> bool {
    v.edges
        .iter()
        .any(|(f, t, _)| f.as_str() == from && t.as_str() == to)
}

// ---------------------------------------------------------------- NEW-1

/// NEW-1: alice queues `status C completed` offline and pushes git. Bob
/// pulls git and sets C to rejected, online. On 28 the server refused bob
/// (its C was still pending, bob's op said completed), then accepted
/// alice's older op when she came back, and `remote pull` spread
/// "completed" to every clone: bob's later edit was gone everywhere.
#[test]
fn new_1_a_queued_update_does_not_overwrite_a_newer_edit_that_came_through_git() {
    let Some(server) = remote("new_1") else {
        return;
    };
    let sb = Sandbox::with_server(server.clone());
    let t = team(&sb, &server, "new1");
    let c = t.alice.add("goal", "C");
    t.exchange(&t.alice, true);
    t.exchange(&t.bob, true);
    t.exchange(&t.carol, true);

    t.offline(&t.alice);
    t.alice.ok(&["status", &c, "completed"]);
    t.exchange(&t.alice, false);

    t.exchange(&t.bob, true);
    assert_eq!(status_of(&t.bob, &c).as_deref(), Some("completed"));
    let out = t.bob.dx(&["status", &c, "rejected"]);
    assert!(
        !out.all().contains("refused"),
        "bob's edit, made over the value git brought him, was refused:\n{}",
        out.all()
    );

    t.online(&t.alice);
    t.alice.dx(&["remote", "push"]);
    assert_eq!(
        t.server_node(&c).unwrap()["status"],
        json!("rejected"),
        "alice's older queued op overwrote bob's newer edit on the server"
    );

    for p in [&t.carol, &t.alice, &t.bob] {
        t.exchange(p, true);
        p.ok(&["remote", "pull"]);
    }
    for p in [&t.carol, &t.alice, &t.bob] {
        assert_eq!(
            status_of(p, &c).as_deref(),
            Some("rejected"),
            "{} lost bob's edit",
            p.dir.display()
        );
    }
}

// ---------------------------------------------------------------- NEW-2

/// NEW-2: a delete queued offline carried no precondition, so it won over
/// an edit made after it, and `remote pull` then deleted the node on every
/// clone. git's rule, which the merge driver applies, is the opposite:
/// an edit after a delete brings the node back.
#[test]
fn new_2_a_stale_queued_delete_does_not_win_over_a_newer_edit() {
    let Some(server) = remote("new_2") else {
        return;
    };
    let sb = Sandbox::with_server(server.clone());
    let t = team(&sb, &server, "new2");
    let c = t.alice.add("goal", "C");
    t.exchange(&t.alice, true);
    t.exchange(&t.bob, true);
    t.exchange(&t.carol, true);

    t.offline(&t.alice);
    t.alice.ok(&["delete", &c]);
    t.exchange(&t.alice, false);
    std::thread::sleep(std::time::Duration::from_millis(1100));

    // Bob has not pulled alice's delete: he edits C after it.
    t.bob.ok(&["status", &c, "completed"]);
    t.exchange(&t.bob, true);

    t.exchange(&t.alice, false);
    assert_eq!(
        status_of(&t.alice, &c).as_deref(),
        Some("completed"),
        "git's merge should have brought the edited node back to alice"
    );
    t.online(&t.alice);
    t.alice.dx(&["remote", "push"]);
    assert!(
        t.server_node(&c).is_some(),
        "alice's stale delete removed a node bob edited after it"
    );

    for p in [&t.carol, &t.alice, &t.bob] {
        t.exchange(p, true);
        p.ok(&["remote", "pull"]);
    }
    for p in [&t.carol, &t.alice, &t.bob] {
        assert_eq!(
            status_of(p, &c).as_deref(),
            Some("completed"),
            "{} lost the node",
            p.dir.display()
        );
    }
}

// ---------------------------------------------------------------- NEW-3

/// NEW-3 (node): alice creates T offline and pushes git; bob pulls it and
/// deletes it online (the server never had T, so on 28 the delete left
/// nothing behind there); alice's replayed create then put T on the server
/// for good.
#[test]
fn new_3_an_offline_create_replayed_after_a_delete_through_git_stays_deleted() {
    let Some(server) = remote("new_3_node") else {
        return;
    };
    let sb = Sandbox::with_server(server.clone());
    let t = team(&sb, &server, "new3n");
    t.offline(&t.alice);
    let tnode = t.alice.add("action", "T");
    t.exchange(&t.alice, false);

    t.exchange(&t.bob, true);
    assert!(status_of(&t.bob, &tnode).is_some());
    t.bob.ok(&["delete", &tnode]);
    t.exchange(&t.bob, true);

    t.online(&t.alice);
    t.alice.dx(&["remote", "push"]);
    assert!(
        t.server_node(&tnode).is_none(),
        "the replayed create put back a node the team deleted"
    );
    for p in [&t.carol, &t.alice] {
        t.exchange(p, true);
        p.ok(&["remote", "pull"]);
        assert!(status_of(p, &tnode).is_none());
    }
}

/// NEW-3 (edge): the same for a link: alice links offline and pushes git,
/// bob unlinks online, alice's replayed link came back on the server, and
/// `remote pull` brought it back into git for everyone.
#[test]
fn new_3_an_offline_link_replayed_after_an_unlink_through_git_stays_unlinked() {
    let Some(server) = remote("new_3_edge") else {
        return;
    };
    let sb = Sandbox::with_server(server.clone());
    let t = team(&sb, &server, "new3e");
    let a = t.alice.add("goal", "A");
    let b = t.alice.add("option", "B");
    t.exchange(&t.alice, true);
    t.exchange(&t.bob, true);
    t.exchange(&t.carol, true);

    t.offline(&t.alice);
    t.alice.ok(&["link", &a, &b, "-r", "edge-probe"]);
    t.exchange(&t.alice, false);
    std::thread::sleep(std::time::Duration::from_millis(50));

    t.exchange(&t.bob, true);
    assert!(has_edge(&t.bob.view(), &a, &b));
    t.bob.ok(&["unlink", &a, &b]);
    t.exchange(&t.bob, true);

    t.online(&t.alice);
    t.alice.dx(&["remote", "push"]);
    assert!(
        !has_edge(&t.server.view(&t.ws), &a, &b),
        "the replayed link put back an edge the team removed"
    );
    for p in [&t.carol, &t.alice, &t.bob] {
        p.ok(&["remote", "pull"]);
        t.exchange(p, true);
    }
    for p in [&t.carol, &t.alice, &t.bob] {
        assert!(
            !has_edge(&p.view(), &a, &b),
            "{} has the removed edge back",
            p.dir.display()
        );
    }
}

// ---------------------------------------------------------------- NEW-4

/// NEW-4: `remote pull` took the server's whole node because its stamp was
/// newer (bob changed the prompt), and so reverted the status alice had
/// set, which carol had from git and the server did not have yet.
#[test]
fn new_4_pull_keeps_a_git_edit_the_server_has_not_got_yet() {
    let Some(server) = remote("new_4") else {
        return;
    };
    let sb = Sandbox::with_server(server.clone());
    let t = team(&sb, &server, "new4");
    let c = t.alice.add("goal", "C");
    t.exchange(&t.alice, true);
    t.exchange(&t.bob, true);
    t.exchange(&t.carol, true);

    t.offline(&t.alice);
    t.alice.ok(&["status", &c, "completed"]);
    t.exchange(&t.alice, false);

    t.exchange(&t.bob, true);
    t.bob.ok(&["prompt", &c, "bob's prompt"]);

    t.exchange(&t.carol, true);
    assert_eq!(status_of(&t.carol, &c).as_deref(), Some("completed"));
    t.carol.ok(&["remote", "pull"]);
    let v = t.carol.view();
    assert_eq!(
        v.nodes[&c].status, "completed",
        "pull reverted a committed git edit"
    );
    assert_eq!(v.nodes[&c].prompt.as_deref(), Some("bob's prompt"));
}

/// NEW-4, the same with carol's copy of alice's edit still queued: carol
/// syncs git while her server is unreachable, so the edit git brought her
/// waits in her log, and then pulls. The pull must keep a field an op in
/// the log is still carrying.
#[test]
fn new_4_pull_keeps_a_field_whose_op_is_still_queued() {
    let Some(server) = remote("new_4_queued") else {
        return;
    };
    let sb = Sandbox::with_server(server.clone());
    let t = team(&sb, &server, "new4q");
    let c = t.alice.add("goal", "C");
    t.exchange(&t.alice, true);
    t.exchange(&t.bob, true);
    t.exchange(&t.carol, true);

    t.offline(&t.alice);
    t.alice.ok(&["status", &c, "completed"]);
    t.exchange(&t.alice, false);

    t.exchange(&t.bob, true);
    t.bob.ok(&["prompt", &c, "bob's prompt"]);

    // git first (the merge driver needs no server), then the sync that
    // applies it with the server unreachable.
    t.carol
        .git_ok(&["pull", "-q", "--no-rebase", "origin", "main"]);
    t.online(&t.carol);
    t.offline(&t.carol);
    t.carol.ok(&["sync"]);
    t.online(&t.carol);
    let log = std::fs::read_to_string(t.carol.dir.join(".deciduous/remote-log.jsonl"))
        .unwrap_or_default();
    assert!(
        log.contains("completed"),
        "the git edit is not queued:\n{log}"
    );
    t.carol.ok(&["remote", "pull"]);
    let v = t.carol.view();
    assert_eq!(
        v.nodes[&c].status, "completed",
        "pull reverted a field whose op is still queued"
    );
    assert_eq!(v.nodes[&c].prompt.as_deref(), Some("bob's prompt"));
    let doc = t.carol.graph_doc();
    assert_eq!(doc["nodes"][&c]["status"], json!("completed"));
}

// ---------------------------------------------------------------- stack

/// Stack report, model seeds 1790191170825091000 and 1790191560410294000:
/// the server hard-deleted edges, so an agent's unlink of an edge a clone
/// created never reached the clone.
#[test]
fn stack_an_agents_unlink_reaches_every_clone() {
    let Some(server) = remote("stack_unlink") else {
        return;
    };
    let sb = Sandbox::with_server(server.clone());
    let t = team(&sb, &server, "stackul");
    let a = t.alice.add("goal", "A");
    let b = t.alice.add("option", "B");
    t.alice.ok(&["link", &a, &b]);
    t.exchange(&t.alice, true);
    t.exchange(&t.bob, true);
    let mut agent = t.agent();
    let (ua, ub) = (agent.uuid_of(&t.ws, &a), agent.uuid_of(&t.ws, &b));
    agent.call_ok("delete_edge", json!({"from_node_id": ua, "to_node_id": ub}));
    for p in [&t.alice, &t.bob] {
        p.ok(&["remote", "pull"]);
        assert!(
            !has_edge(&p.view(), &a, &b),
            "{} still has the edge the agent removed",
            p.dir.display()
        );
    }
    t.exchange(&t.alice, true);
    t.exchange(&t.bob, true);
    for p in [&t.alice, &t.bob] {
        assert!(!has_edge(&p.view(), &a, &b));
        let st = p.dx(&["remote", "status"]);
        assert!(status_says_clean(&st), "{}", st.all());
    }
}
