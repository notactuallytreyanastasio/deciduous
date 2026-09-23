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

/// NEW-1, the other side of the fix: what `sync` queues from git can be
/// older than the server. Alice sets C completed offline and pushes git;
/// meanwhile an agent sets C active. Bob's sync queues git's
/// pending -> completed, which the server refuses: it holds a newer value.
/// Nobody's write was lost (alice's own op is refused to her), so bob's
/// sync and push must not report a failed write of his and exit 1.
#[test]
fn new_1_a_git_edit_older_than_the_server_is_not_bobs_refusal() {
    let Some(server) = remote("new_1b") else {
        return;
    };
    let sb = Sandbox::with_server(server.clone());
    let t = team(&sb, &server, "new1b");
    let c = t.alice.add("goal", "C");
    t.exchange(&t.alice, true);
    t.exchange(&t.bob, true);

    t.offline(&t.alice);
    t.alice.ok(&["status", &c, "completed"]);
    t.exchange(&t.alice, false);
    let mut agent = t.agent();
    let u = agent.uuid_of(&t.ws, &c);
    agent.call_ok("update_node", json!({"node_id": u, "status": "active"}));

    let sync = t.bob.git(&["pull", "-q", "--no-rebase", "origin", "main"]);
    assert!(sync.ok(), "{}", sync.all());
    t.online(&t.bob);
    let sync = t.bob.dx(&["sync"]);
    let push = t.bob.dx(&["remote", "push"]);
    for (what, out) in [("sync", &sync), ("push", &push)] {
        assert!(out.ok(), "bob's {what} failed:\n{}", out.all());
        assert!(
            !out.all().contains("the server refused"),
            "bob's {what} said the server refused his write; it was git's, and older:\n{}",
            out.all()
        );
        assert!(
            !out.all().contains("had not reached"),
            "bob's {what} said a write made here had not been queued:\n{}",
            out.all()
        );
    }
    assert!(
        sync.all().contains("Behind the server"),
        "bob's sync did not say the server is ahead of git:\n{}",
        sync.all()
    );
    assert_eq!(t.server_node(&c).unwrap()["status"], json!("active"));
}

// ---------------------------------------------------------------- NEW-9

/// NEW-9: one node deleted on the server, one pull, "removed 2".
#[test]
fn new_9_pull_counts_a_server_delete_once() {
    let Some(server) = remote("new_9") else {
        return;
    };
    let sb = Sandbox::with_server(server.clone());
    let t = team(&sb, &server, "new9");
    let f = t.alice.add("goal", "F");
    t.alice.add("goal", "kept");
    t.exchange(&t.alice, true);
    t.exchange(&t.carol, true);
    // Carol's copy of F is edited after the agent's delete: the pull
    // deletes it over that edit (the overridden path) and must count it
    // once.
    let g = t.alice.add("goal", "G");
    t.exchange(&t.alice, true);
    t.exchange(&t.carol, true);
    let mut agent = t.agent();
    for cid in [&f, &g] {
        let u = agent.uuid_of(&t.ws, cid);
        agent.call_ok("delete_node", json!({"node_id": u}));
    }
    // G: carol's copy is untouched, so reconcile applies the tombstone.
    t.offline(&t.carol);
    t.carol.ok(&["status", &f, "completed"]);
    t.online(&t.carol);
    let out = t.carol.ok(&["remote", "pull"]);
    assert!(
        out.contains("removed 2"),
        "two nodes were deleted on the server:\n{out}"
    );
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

// ---------------------------------------------------------------- T4 / BRIDGE-N2

/// T4 and BRIDGE-N2: a local delete that never became an op (made before
/// this clone had a [remote], by 1.0.7, or while the log could not be
/// written) could never be sent: status listed the node "only on the
/// server" forever and nothing resolved it. `remote push --repair` sends it
/// now, guarded like any delete, and a pull does not bring it back.
#[test]
fn t4_bridge_n2_a_delete_missing_from_the_log_can_be_sent() {
    let Some(server) = remote("t4_bridge_n2") else {
        return;
    };
    let sb = Sandbox::with_server(server.clone());
    let t = team(&sb, &server, "t4n2");
    let keep = t.alice.add("goal", "keep");
    let gone = t.alice.add("goal", "gone");
    // The delete is made with no [remote] in the config, so no op is
    // written, which is what 1.0.7 and a failed append both leave.
    let cfg = t.alice.dir.join(".deciduous/config.toml");
    let text = std::fs::read_to_string(&cfg).unwrap();
    let mut doc: toml_edit::DocumentMut = text.parse().unwrap();
    doc.remove("remote");
    std::fs::write(&cfg, doc.to_string()).unwrap();
    t.alice.ok(&["delete", &gone]);
    std::fs::write(&cfg, text).unwrap();

    let st = t.alice.dx(&["remote", "status"]);
    assert!(!st.ok(), "status must see the difference:\n{}", st.all());
    assert!(
        st.all().contains("--repair"),
        "status must name the command that sends the delete:\n{}",
        st.all()
    );
    let out = t.alice.dx(&["remote", "push", "--repair"]);
    assert!(out.ok(), "{}", out.all());
    assert!(t.server_node(&gone).is_none(), "{}", out.all());
    assert!(t.server_node(&keep).is_some());
    t.alice.ok(&["remote", "pull"]);
    assert!(status_of(&t.alice, &gone).is_none(), "pull resurrected it");
    let st = t.alice.dx(&["remote", "status"]);
    assert!(status_says_clean(&st), "{}", st.all());
}

// ---------------------------------------------------------------- BRIDGE-N9 / N10

/// BRIDGE-N9: the refusal said "`deciduous remote pull` takes the server's
/// value", and the pull kept the refused local edit (the newer stamp) and
/// changed nothing. BRIDGE-N10: the refusal then stayed in the log, so
/// `remote status` exited 1 with every row matching, and the `remote push`
/// that printed "the server refused 1 write(s)" exited 0.
#[test]
fn bridge_n9_n10_pull_takes_the_refused_field_and_the_refusal_settles() {
    let Some(server) = remote("bridge_n9_n10") else {
        return;
    };
    let sb = Sandbox::with_server(server.clone());
    let t = team(&sb, &server, "n9n10");
    let a = t.alice.add("goal", "A");
    let mut agent = t.agent();
    let u = agent.uuid_of(&t.ws, &a);
    agent.call_ok("update_node", json!({"node_id": u, "status": "active"}));

    t.offline(&t.alice);
    t.alice.ok(&["status", &a, "completed"]);
    t.online(&t.alice);
    let push = t.alice.dx(&["remote", "push"]);
    assert!(
        push.all().contains("the server refused 1 write(s)"),
        "{}",
        push.all()
    );
    assert!(
        !push.ok(),
        "a push the server refused exited 0:\n{}",
        push.all()
    );
    assert!(push
        .all()
        .contains("`deciduous remote pull` takes the server's value"));

    let pull = t.alice.ok(&["remote", "pull"]);
    assert_eq!(
        status_of(&t.alice, &a).as_deref(),
        Some("active"),
        "pull kept the refused edit:\n{pull}"
    );
    assert_eq!(t.alice.graph_doc()["nodes"][&a]["status"], json!("active"));
    assert!(pull.contains("settled"), "{pull}");
    let st = t.alice.dx(&["remote", "status"]);
    assert!(
        status_says_clean(&st),
        "every row matches and nothing waits:\n{}",
        st.all()
    );
}

// ---------------------------------------------------------------- BRIDGE-N1

/// Makes a local write that never becomes an op: the config has no
/// [remote] while it runs, which is what 1.0.7, a clone before `remote
/// init`, and a failed append all leave behind.
fn unlogged(p: &Project, args: &[&str]) {
    let cfg = p.dir.join(".deciduous/config.toml");
    let text = std::fs::read_to_string(&cfg).unwrap();
    let mut doc: toml_edit::DocumentMut = text.parse().unwrap();
    doc.remove("remote");
    std::fs::write(&cfg, doc.to_string()).unwrap();
    let out = p.dx(args);
    std::fs::write(&cfg, text).unwrap();
    assert!(out.ok(), "{args:?}: {}", out.all());
}

/// BRIDGE-N1: `remote push --repair` sent every differing field with the
/// server's current value as `was`, so an agent's edit this copy had not
/// pulled was overwritten, title, description and status, and the refusal
/// message told the user to run it. It now sends only fields the server
/// has not changed since this copy last pulled, lists the others with both
/// values, and overwrites them only with --overwrite-server.
#[test]
fn bridge_n1_repair_never_overwrites_an_edit_it_has_not_pulled() {
    let Some(server) = remote("bridge_n1") else {
        return;
    };
    let sb = Sandbox::with_server(server.clone());
    let t = team(&sb, &server, "n1");
    let p = &t.alice;
    let a = p.add("goal", "A");
    let b = p.ok(&["add", "goal", "B", "-d", "B local"]);
    let b = p.change_id_of(created_id(&b));
    let mut agent = t.agent();
    let ub = agent.uuid_of(&t.ws, &b);
    agent.call_ok(
        "update_node",
        json!({"node_id": ub, "title": "B by agent", "description": "agent desc", "status": "completed"}),
    );
    let ua = agent.uuid_of(&t.ws, &a);
    agent.call_ok("update_node", json!({"node_id": ua, "status": "active"}));
    p.dx(&["status", &a, "completed"]);

    let out = p.dx(&["remote", "push", "--repair"]);
    let sb_ = t.server_node(&b).unwrap();
    assert_eq!(
        (sb_["title"].as_str(), sb_["status"].as_str()),
        (Some("B by agent"), Some("completed")),
        "--repair reverted the agent's edit:\n{}",
        out.all()
    );
    assert_eq!(t.server_node(&a).unwrap()["status"], json!("active"));
    assert!(!out.ok(), "withheld fields must show in the exit code");
    assert!(
        out.all().contains("B by agent") && out.all().contains("--overwrite-server"),
        "it must say what it would overwrite:\n{}",
        out.all()
    );

    // After a pull, an edit made here with no op is this copy's: --repair
    // sends it, and nothing else.
    p.ok(&["remote", "pull"]);
    unlogged(p, &["status", &b, "abandoned"]);
    let out = p.dx(&["remote", "push", "--repair"]);
    assert!(out.ok(), "{}", out.all());
    let sb_ = t.server_node(&b).unwrap();
    assert_eq!(sb_["status"], json!("abandoned"));
    assert_eq!(sb_["title"], json!("B by agent"));
    let st = p.dx(&["remote", "status"]);
    assert!(status_says_clean(&st), "{}", st.all());

    // Both at once: the status is this copy's and is sent; the title the
    // agent changed is listed, kept, and the exit code says so.
    agent.call_ok(
        "update_node",
        json!({"node_id": ub, "title": "B again by agent"}),
    );
    unlogged(p, &["status", &b, "active"]);
    let out = p.dx(&["remote", "push", "--repair"]);
    assert!(!out.ok(), "{}", out.all());
    assert!(out.all().contains("B again by agent"), "{}", out.all());
    assert_eq!(t.server_node(&b).unwrap()["status"], json!("active"));
    assert_eq!(
        t.server_node(&b).unwrap()["title"],
        json!("B again by agent")
    );

    // --overwrite-server does what it says, for what was listed.
    let out = p.dx(&["remote", "push", "--repair", "--overwrite-server"]);
    assert!(out.ok(), "{}", out.all());
    assert_eq!(t.server_node(&b).unwrap()["title"], json!("B by agent"));
}

// ---------------------------------------------------------------- BRIDGE-N4

/// The workspace's attached documents (the test's workspace has one node
/// that holds any).
fn server_docs(t: &Team) -> Vec<Value> {
    t.server.export(&t.ws)["documents"]
        .as_array()
        .unwrap()
        .to_vec()
}

/// BRIDGE-N4: documents bypassed the log. An attach reached the server
/// only through `--seed`, and a detach or a new description never did: a
/// document detached to take a pasted secret out of the graph stayed on
/// the shared server, and `remote status` said nothing.
#[test]
fn bridge_n4_attach_describe_and_detach_go_through_the_log() {
    let Some(server) = remote("bridge_n4") else {
        return;
    };
    let sb = Sandbox::with_server(server.clone());
    let t = team(&sb, &server, "n4");
    let p = &t.alice;
    p.add("goal", "holds a file");
    std::fs::write(p.dir.join("secret.txt"), "hunter2\n").unwrap();
    p.ok(&["doc", "attach", "1", "secret.txt"]);
    let docs = server_docs(&t);
    assert_eq!(docs.len(), 1, "the attach did not reach the server");
    assert_eq!(docs[0]["original_filename"], json!("secret.txt"));
    let id = docs[0]["id"].as_str().unwrap();
    let got = t.server.request(
        "GET",
        &format!("/documents/{id}"),
        Some(&t.server.bearer()),
        &[("x-deciduous-workspace", t.ws.as_str())],
        None,
    );
    assert!(
        got.status == 200 && got.body.contains("hunter2"),
        "the bytes did not go with it: {} {}",
        got.status,
        got.body
    );

    p.ok(&["doc", "describe", "1", "what it is"]);
    assert_eq!(server_docs(&t)[0]["description"], json!("what it is"));

    p.ok(&["doc", "detach", "1"]);
    assert!(
        server_docs(&t).is_empty(),
        "the detached document is still on the server"
    );
    let st = p.dx(&["remote", "status"]);
    assert!(status_says_clean(&st), "{}", st.all());

    // A detach from before documents went through the log: status names
    // it, --repair sends it.
    std::fs::write(p.dir.join("second.txt"), "another\n").unwrap();
    p.ok(&["doc", "attach", "1", "second.txt"]);
    assert_eq!(server_docs(&t).len(), 1);
    unlogged(p, &["doc", "detach", "2"]);
    let st = p.dx(&["remote", "status"]);
    assert!(!st.ok(), "{}", st.all());
    assert!(st.all().contains("Documents detached here"), "{}", st.all());
    p.ok(&["remote", "push", "--repair"]);
    assert!(server_docs(&t).is_empty());
    let st = p.dx(&["remote", "status"]);
    assert!(status_says_clean(&st), "{}", st.all());
}

// ---------------------------------------------------------------- BRIDGE-N7 / N8

/// A git repository with `deciduous init` run in it and no commit.
fn unborn<'a>(sb: &'a Sandbox, name: &str) -> Project<'a> {
    let dir = sb.base().join(name);
    std::fs::create_dir_all(&dir).unwrap();
    sb.git_ok(&dir, &["init", "-q"]);
    let p = Project { dir, sb };
    p.ok(&["init"]);
    p
}

/// BRIDGE-N7: `git init && deciduous init` (no commit yet) wrote into a
/// workspace nobody had claimed; an unrelated repository of the same name
/// then claimed it, with the first one's nodes in it, and pulled them into
/// its committable graph.json.
#[test]
fn bridge_n7_a_repository_with_no_commit_writes_nothing_to_the_server() {
    let Some(server) = remote("bridge_n7") else {
        return;
    };
    let sb = Sandbox::with_server(server.clone());
    let ws = unique("n7");
    let a = unborn(&sb, "a/same");
    let init = a.ok(&["remote", "init", &server.url, "--workspace", &ws]);
    assert!(init.contains("no commit yet"), "{init}");
    let out = a.dx(&["add", "goal", "A secret plan"]);
    assert!(out.ok(), "the local write still happens: {}", out.all());
    assert!(
        out.all().contains("no commit yet"),
        "it must say why nothing was sent: {}",
        out.all()
    );
    let v = server.view(&ws);
    assert!(
        v.nodes.values().all(|n| n.title != "A secret plan"),
        "a repository with no commit wrote into an unclaimed workspace"
    );

    // An unrelated repository takes the name, and finds nothing of a's.
    let b = sb.project("b/same", None);
    b.ok(&["remote", "init", &server.url, "--workspace", &ws]);
    let pull = b.ok(&["remote", "pull"]);
    assert!(
        !b.graph_file().exists()
            || !std::fs::read_to_string(b.graph_file())
                .unwrap()
                .contains("A secret plan"),
        "{pull}"
    );

    // a's write waits, and a status says so.
    let st = a.dx(&["remote", "status"]);
    assert!(st.all().contains("1 write(s) waiting"), "{}", st.all());
}

/// BRIDGE-N8: a shallow clone was told it "has no commit yet" and that its
/// write would be "refused if another repository has" claimed the
/// workspace; it had commits, and its write was accepted. A shallow clone
/// cannot show the server which repository it is (that is what lets a CI
/// clone write to its own workspace; see remote_oplog's
/// a_shallow_clone_writes_to_its_repositorys_workspace), so the note now
/// says what happens: its writes go in unchecked.
#[test]
fn bridge_n8_a_shallow_clone_is_told_its_writes_are_unchecked() {
    let Some(server) = remote("bridge_n8") else {
        return;
    };
    let sb = Sandbox::with_server(server.clone());
    let ws = unique("n8");
    let other = sb.project("other", None);
    other.git_ok(&["commit", "-q", "--allow-empty", "-m", "second"]);
    let url = format!("file://{}", other.dir.display());
    let dir = sb.base().join("shallow/same");
    std::fs::create_dir_all(dir.parent().unwrap()).unwrap();
    sb.git_ok(
        &sb.base(),
        &["clone", "-q", "--depth", "1", &url, dir.to_str().unwrap()],
    );
    let shallow = Project { dir, sb: &sb };
    shallow.ok(&["sync"]);
    let init = shallow.ok(&["remote", "init", &server.url, "--workspace", &ws]);
    assert!(!init.contains("no commit yet"), "{init}");
    assert!(
        init.contains("shallow clone") && init.contains("unchecked"),
        "{init}"
    );
    assert!(!init.contains("is refused"), "{init}");
}

// ---------------------------------------------------------------- BRIDGE-N6

/// BRIDGE-N6: `remote watch --edges` printed 2 lines for 4 server updates
/// (the repeated frames were byte-identical and a 60 s filter dropped
/// them), a node delete as "(updated)", and nothing for an unlink; and on
/// a reconnect it had no way to ask for what it missed.
#[test]
fn bridge_n6_watch_shows_every_update_delete_and_unlink() {
    let Some(server) = remote("bridge_n6") else {
        return;
    };
    let sb = Sandbox::with_server(server.clone());
    let t = team(&sb, &server, "n6");
    let p = &t.alice;
    let a = p.add("goal", "Wa");
    let b = p.add("option", "Wb");
    p.ok(&["link", &a, &b]);

    let mut child = p
        .dx_cmd()
        .args(["remote", "watch", "--edges"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        use std::io::BufRead;
        for line in std::io::BufReader::new(stdout)
            .lines()
            .map_while(Result::ok)
        {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    std::thread::sleep(std::time::Duration::from_millis(1500));
    p.ok(&["status", &a, "completed"]);
    p.ok(&["status", &a, "pending"]);
    p.ok(&["status", &a, "completed"]);
    p.ok(&["prompt", &a, "a prompt"]);
    p.ok(&["unlink", &a, &b]);
    p.ok(&["delete", &b]);

    let mut lines = Vec::new();
    let until = std::time::Instant::now() + std::time::Duration::from_secs(8);
    while std::time::Instant::now() < until && lines.len() < 6 {
        if let Ok(l) = rx.recv_timeout(std::time::Duration::from_millis(200)) {
            lines.push(l);
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    let all = lines.join("\n");
    assert_eq!(
        lines.iter().filter(|l| l.contains("goal (updated")).count(),
        4,
        "four updates, four lines:\n{all}"
    );
    assert!(
        all.contains("edge leads_to (deleted)"),
        "the unlink:\n{all}"
    );
    assert!(all.contains("option (deleted)"), "the delete:\n{all}");
    assert!(
        !all.contains("option (updated"),
        "a delete is not an update:\n{all}"
    );
}

/// BRIDGE-N6, the reconnect: `/events?since=<seq>` replays what came after
/// that event, each once, in order, which is what the watcher asks for
/// when it reconnects.
#[test]
fn bridge_n6_a_stream_resumes_from_the_last_event_it_saw() {
    let Some(server) = remote("bridge_n6_resume") else {
        return;
    };
    let sb = Sandbox::with_server(server.clone());
    let t = team(&sb, &server, "n6r");
    let p = &t.alice;
    let a = p.add("goal", "R");
    p.ok(&["status", &a, "completed"]);
    p.ok(&["status", &a, "pending"]);

    let url = format!(
        "{}/events?workspace={}&token={}&since=0",
        server.url.replacen("http", "ws", 1),
        urlencode(&t.ws),
        urlencode(&server.token)
    );
    let (mut ws, _) = tungstenite::connect(url.as_str()).expect("connect");
    if let tungstenite::stream::MaybeTlsStream::Plain(s) = ws.get_ref() {
        s.set_read_timeout(Some(std::time::Duration::from_secs(3)))
            .unwrap();
    }
    let mut got: Vec<Value> = Vec::new();
    while got.len() < 3 {
        match ws.read() {
            Ok(tungstenite::Message::Text(t)) => {
                got.push(serde_json::from_str(t.as_str()).unwrap())
            }
            Ok(_) => {}
            Err(e) => panic!("after {} event(s): {e}", got.len()),
        }
    }
    let ops: Vec<(&str, Option<&str>)> = got
        .iter()
        .map(|e| (e["op"].as_str().unwrap(), e["changed"][0].as_str()))
        .collect();
    assert_eq!(
        ops,
        [
            ("INSERT", None),
            ("UPDATE", Some("status")),
            ("UPDATE", Some("status"))
        ]
    );
    let seqs: Vec<u64> = got.iter().map(|e| e["seq"].as_u64().unwrap()).collect();
    assert!(seqs.windows(2).all(|w| w[0] < w[1]), "{seqs:?}");

    // Resuming after the first replays only the two after it.
    let url2 = url.replace("since=0", &format!("since={}", seqs[0]));
    let (mut ws2, _) = tungstenite::connect(url2.as_str()).expect("connect");
    let first = match ws2.read().unwrap() {
        tungstenite::Message::Text(t) => serde_json::from_str::<Value>(t.as_str()).unwrap(),
        other => panic!("{other:?}"),
    };
    assert_eq!(first["seq"].as_u64(), Some(seqs[1]));
}
