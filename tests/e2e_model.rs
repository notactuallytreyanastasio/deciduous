//! Model-based, randomized interleaving across every surface that writes a
//! graph: the CLI and a long-lived stdio MCP server in one clone, the CLI in
//! a second clone that only meets the first through git, and (when a server
//! is under test) an agent writing the shared workspace over HTTP MCP. Some
//! writes happen while a clone's server is unreachable. Writes that do not
//! touch the same thing run concurrently.
//!
//! A plain in-memory model applies every acknowledged write. At each barrier
//! everything is synced and replayed (remote push, git exchange, remote
//! pull) and every copy is compared with the model:
//!
//! * every acknowledged write is present everywhere;
//! * no deleted node comes back (the generator never edits a deleted node,
//!   so any resurrection is wrong);
//! * no edit is silently reverted; statuses, titles and prompts converge;
//! * no edge dangles, in any copy;
//! * `remote status` says clean when content agrees, and never says clean
//!   while a clone holds writes the server certainly lacks (or the reverse).
//!
//! Between barriers a (node, field) or edge written at one location is not
//! written at another, so the expected value never depends on an ordering
//! the test cannot observe. The same-field race is its own test in
//! e2e_bridge.rs.
//!
//! Seeded: every failure prints the seed and the operation log.
//! `DECIDUOUS_E2E_SEED=<seed>` replays a run; `DECIDUOUS_E2E_STEPS=<n>`
//! lengthens it. Local-only needs `DECIDUOUS_E2E=1`; the server variant
//! needs `DECIDUOUS_E2E_SERVER` + `DECIDUOUS_E2E_TOKEN`.

mod e2e_support;

use e2e_support::*;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Loc {
    /// Clone 1's database: the CLI and the stdio MCP server write here.
    L1,
    /// Clone 2's database: the CLI writes here.
    L2,
    /// The shared server's workspace: the HTTP agent writes here.
    S,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Actor {
    Cli1,
    Stdio1,
    Cli2,
    Agent,
}

impl Actor {
    fn loc(self) -> Loc {
        match self {
            Actor::Cli1 | Actor::Stdio1 => Loc::L1,
            Actor::Cli2 => Loc::L2,
            Actor::Agent => Loc::S,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    Status(String),
    Prompt(String),
    Title(String),
    Edge(String, String),
}

impl Key {
    fn mentions(&self, cid: &str) -> bool {
        match self {
            Key::Status(c) | Key::Prompt(c) | Key::Title(c) => c == cid,
            Key::Edge(a, b) => a == cid || b == cid,
        }
    }
}

#[derive(Debug, Clone)]
enum Op {
    Add {
        actor: Actor,
        node_type: &'static str,
        title: String,
    },
    Status {
        actor: Actor,
        cid: String,
        status: &'static str,
    },
    Prompt {
        actor: Actor,
        cid: String,
        prompt: String,
    },
    Title {
        cid: String,
        title: String,
    },
    Link {
        actor: Actor,
        from: String,
        to: String,
    },
    Unlink {
        actor: Actor,
        from: String,
        to: String,
    },
    Delete {
        actor: Actor,
        cid: String,
    },
}

impl Op {
    fn actor(&self) -> Actor {
        match self {
            Op::Title { .. } => Actor::Agent,
            Op::Add { actor, .. }
            | Op::Status { actor, .. }
            | Op::Prompt { actor, .. }
            | Op::Link { actor, .. }
            | Op::Unlink { actor, .. }
            | Op::Delete { actor, .. } => *actor,
        }
    }
    fn nodes(&self) -> Vec<&str> {
        match self {
            Op::Add { .. } => vec![],
            Op::Status { cid, .. }
            | Op::Prompt { cid, .. }
            | Op::Title { cid, .. }
            | Op::Delete { cid, .. } => vec![cid],
            Op::Link { from, to, .. } | Op::Unlink { from, to, .. } => vec![from, to],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MNode {
    node_type: String,
    title: String,
    status: String,
    prompt: Option<String>,
}

#[derive(Default)]
struct Model {
    nodes: BTreeMap<String, MNode>,
    deleted: BTreeSet<String>,
    edges: BTreeSet<(String, String)>,
    /// Which location may see a node (conservatively: its creator until the
    /// next barrier, everyone after).
    visible: BTreeMap<Loc, BTreeSet<String>>,
    /// Keys written since the last barrier, and where.
    owner: BTreeMap<Key, Loc>,
    /// Location that certainly differs from the server until its next push
    /// (offline writes) or pull (agent writes).
    offline_dirty: BTreeSet<Loc>,
    agent_dirty: BTreeSet<Loc>,
}

impl Model {
    fn view(&self) -> View {
        let mut v = View::default();
        for (cid, n) in &self.nodes {
            v.nodes.insert(
                cid.clone(),
                NodeView {
                    node_type: n.node_type.clone(),
                    title: n.title.clone(),
                    status: n.status.clone(),
                    prompt: n.prompt.clone(),
                },
            );
        }
        for (a, b) in &self.edges {
            v.edges
                .insert((a.clone(), b.clone(), "leads_to".to_string()));
        }
        v
    }

    fn may_write(&self, key: &Key, loc: Loc) -> bool {
        self.owner.get(key).is_none_or(|o| *o == loc)
    }

    fn live_at(&self, loc: Loc) -> Vec<String> {
        self.visible
            .get(&loc)
            .map(|s| {
                s.iter()
                    .filter(|c| self.nodes.contains_key(*c))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }
}

const TYPES: &[&str] = &[
    "goal",
    "option",
    "decision",
    "action",
    "outcome",
    "observation",
];
const STATUSES: &[&str] = &["pending", "active", "completed", "rejected"];

struct World<'a> {
    seed: u64,
    c1: Project<'a>,
    c2: Project<'a>,
    stdio: Mutex<StdioMcp>,
    agent: Option<Mutex<HttpMcp>>,
    server: Option<Server>,
    ws: String,
    offline: BTreeSet<Loc>,
    log: Vec<String>,
    uuid: Mutex<BTreeMap<String, String>>,
}

impl World<'_> {
    fn fail(&self, msg: impl std::fmt::Display) -> ! {
        let tail: Vec<&String> = self.log.iter().rev().take(40).collect::<Vec<_>>();
        let tail: Vec<&String> = tail.into_iter().rev().collect();
        panic!(
            "\n{msg}\n\nseed = {} (replay with DECIDUOUS_E2E_SEED={})\nlast operations:\n  {}",
            self.seed,
            self.seed,
            tail.iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join("\n  ")
        )
    }

    fn clone_of(&self, loc: Loc) -> &Project<'_> {
        match loc {
            Loc::L1 => &self.c1,
            Loc::L2 => &self.c2,
            Loc::S => unreachable!("the server is not a clone"),
        }
    }

    fn uuid_of(&self, cid: &str) -> Result<String, String> {
        if let Some(u) = self.uuid.lock().unwrap().get(cid) {
            return Ok(u.clone());
        }
        let server = self.server.as_ref().unwrap();
        let ex = server.export(&self.ws);
        let mut map = self.uuid.lock().unwrap();
        for n in ex["nodes"].as_array().unwrap() {
            map.insert(
                n["change_id"].as_str().unwrap().to_string(),
                n["id"].as_str().unwrap().to_string(),
            );
        }
        map.get(cid)
            .cloned()
            .ok_or_else(|| format!("the server has no node {cid} for the agent to address"))
    }

    /// Runs one operation against the real thing. Ok(change_id) for adds,
    /// Ok("") otherwise, Err(why) when the surface refused or failed.
    fn exec(&self, op: &Op) -> Result<String, String> {
        let cli = |p: &Project, args: &[&str]| -> Result<String, String> {
            let o = p.dx(args);
            if o.ok() {
                Ok(o.stdout)
            } else {
                Err(format!("deciduous {args:?}: {}", o.all()))
            }
        };
        match op {
            Op::Add {
                actor,
                node_type,
                title,
            } => match actor {
                Actor::Cli1 | Actor::Cli2 => {
                    let p = self.clone_of(actor.loc());
                    let out = cli(p, &["add", node_type, title])?;
                    Ok(p.change_id_of(created_id(&out)))
                }
                Actor::Stdio1 => {
                    let mut m = self.stdio.lock().unwrap();
                    let r = m.call("add_node", json!({"node_type": node_type, "title": title}))?;
                    let id = r["node_id"].clone();
                    let shown = m.call("show_node", json!({"node_id": id}))?;
                    Ok(shown["change_id"].as_str().unwrap().to_string())
                }
                Actor::Agent => {
                    let mut a = self.agent.as_ref().unwrap().lock().unwrap();
                    let r = a.call(
                        "add_node",
                        json!({"node_type": node_type, "title": title, "workspace": self.ws}),
                    )?;
                    let cid = r["change_id"].as_str().unwrap().to_string();
                    self.uuid
                        .lock()
                        .unwrap()
                        .insert(cid.clone(), r["id"].as_str().unwrap().to_string());
                    Ok(cid)
                }
            },
            Op::Status { actor, cid, status } => match actor {
                Actor::Cli1 | Actor::Cli2 => {
                    cli(self.clone_of(actor.loc()), &["status", cid, status]).map(|_| String::new())
                }
                Actor::Stdio1 => self
                    .stdio
                    .lock()
                    .unwrap()
                    .call("update_status", json!({"node_id": cid, "status": status}))
                    .map(|_| String::new()),
                Actor::Agent => {
                    let u = self.uuid_of(cid)?;
                    self.agent
                        .as_ref()
                        .unwrap()
                        .lock()
                        .unwrap()
                        .call("update_node", json!({"node_id": u, "status": status}))
                        .map(|_| String::new())
                }
            },
            Op::Prompt { actor, cid, prompt } => match actor {
                Actor::Cli1 | Actor::Cli2 => {
                    cli(self.clone_of(actor.loc()), &["prompt", cid, prompt]).map(|_| String::new())
                }
                Actor::Stdio1 => self
                    .stdio
                    .lock()
                    .unwrap()
                    .call("update_prompt", json!({"node_id": cid, "prompt": prompt}))
                    .map(|_| String::new()),
                Actor::Agent => {
                    let u = self.uuid_of(cid)?;
                    self.agent
                        .as_ref()
                        .unwrap()
                        .lock()
                        .unwrap()
                        .call(
                            "update_node",
                            json!({"node_id": u, "metadata": {"prompt": prompt}}),
                        )
                        .map(|_| String::new())
                }
            },
            Op::Title { cid, title } => {
                let u = self.uuid_of(cid)?;
                self.agent
                    .as_ref()
                    .unwrap()
                    .lock()
                    .unwrap()
                    .call("update_node", json!({"node_id": u, "title": title}))
                    .map(|_| String::new())
            }
            Op::Link { actor, from, to } | Op::Unlink { actor, from, to } => {
                let link = matches!(op, Op::Link { .. });
                match actor {
                    Actor::Cli1 | Actor::Cli2 => cli(
                        self.clone_of(actor.loc()),
                        &[if link { "link" } else { "unlink" }, from, to],
                    )
                    .map(|_| String::new()),
                    Actor::Stdio1 => self
                        .stdio
                        .lock()
                        .unwrap()
                        .call(
                            if link { "link_nodes" } else { "unlink_nodes" },
                            json!({"from_id": from, "to_id": to}),
                        )
                        .map(|_| String::new()),
                    Actor::Agent => {
                        let (f, t) = (self.uuid_of(from)?, self.uuid_of(to)?);
                        self.agent
                            .as_ref()
                            .unwrap()
                            .lock()
                            .unwrap()
                            .call(
                                if link { "add_edge" } else { "delete_edge" },
                                json!({"from_node_id": f, "to_node_id": t}),
                            )
                            .map(|_| String::new())
                    }
                }
            }
            Op::Delete { actor, cid } => match actor {
                Actor::Cli1 | Actor::Cli2 => {
                    cli(self.clone_of(actor.loc()), &["delete", cid]).map(|_| String::new())
                }
                Actor::Stdio1 => self
                    .stdio
                    .lock()
                    .unwrap()
                    .call("delete_node", json!({"node_id": cid}))
                    .map(|_| String::new()),
                Actor::Agent => {
                    let u = self.uuid_of(cid)?;
                    self.agent
                        .as_ref()
                        .unwrap()
                        .lock()
                        .unwrap()
                        .call("delete_node", json!({"node_id": u}))
                        .map(|_| String::new())
                }
            },
        }
    }

    /// A git exchange for one clone. config.toml is committed, so the pull
    /// can bring the other clone's remote URL with it; when that clone was
    /// offline, that is a dead port, and this clone would then fail to reach
    /// a server the model says it can reach. Each clone's URL is put back to
    /// what its own offline state says after every exchange.
    fn exchange(&self, loc: Loc) {
        self.clone_of(loc).git_exchange();
        if let Some(server) = &self.server {
            let url = if self.offline.contains(&loc) {
                format!("http://127.0.0.1:{}", dead_port())
            } else {
                server.url.clone()
            };
            self.clone_of(loc).set_remote_url(&url);
        }
    }

    fn set_offline(&mut self, loc: Loc, off: bool) {
        let Some(server) = &self.server else { return };
        let url = if off {
            format!("http://127.0.0.1:{}", dead_port())
        } else {
            server.url.clone()
        };
        self.clone_of(loc).set_remote_url(&url);
        if off {
            self.offline.insert(loc);
        } else {
            self.offline.remove(&loc);
        }
        self.log.push(format!(
            "{loc:?} server {}",
            if off { "unreachable" } else { "back" }
        ));
    }

    fn step_ok(&self, p: &Project, args: &[&str]) {
        let o = p.dx(args);
        if !o.ok() {
            self.fail(format!(
                "deciduous {args:?} in {} failed:\n{}",
                p.dir.display(),
                o.all()
            ));
        }
    }
}

/// Picks an operation the model can predict the outcome of, or None.
fn generate(
    rng: &mut Rng,
    m: &Model,
    actors: &[Actor],
    busy: &BTreeSet<String>,
    n: usize,
) -> Option<Op> {
    let actor = *rng.pick(actors)?;
    let loc = actor.loc();
    let live: Vec<String> = m
        .live_at(loc)
        .into_iter()
        .filter(|c| !busy.contains(c))
        .collect();
    let roll = rng.below(100);
    let pick = |rng: &mut Rng| rng.pick(&live).cloned();
    match roll {
        0..=24 => Some(Op::Add {
            actor,
            node_type: TYPES[rng.below(TYPES.len())],
            title: format!("n{n} by {actor:?} \u{2014} \"quoted\" & <tagged>"),
        }),
        25..=44 => {
            let cid = pick(rng)?;
            m.may_write(&Key::Status(cid.clone()), loc)
                .then(|| Op::Status {
                    actor,
                    cid,
                    status: STATUSES[rng.below(STATUSES.len())],
                })
        }
        45..=54 => {
            let cid = pick(rng)?;
            m.may_write(&Key::Prompt(cid.clone()), loc)
                .then(|| Op::Prompt {
                    actor,
                    cid,
                    prompt: format!("prompt {n}\nsecond line from {actor:?}"),
                })
        }
        55..=61 if actor == Actor::Agent => {
            let cid = pick(rng)?;
            m.may_write(&Key::Title(cid.clone()), loc)
                .then(|| Op::Title {
                    cid,
                    title: format!("retitled {n} by the agent"),
                })
        }
        62..=81 => {
            let from = pick(rng)?;
            let to = pick(rng)?;
            let key = Key::Edge(from.clone(), to.clone());
            (from != to
                && !m.edges.contains(&(from.clone(), to.clone()))
                && !m.edges.contains(&(to.clone(), from.clone()))
                && m.may_write(&key, loc))
            .then_some(Op::Link { actor, from, to })
        }
        82..=89 => {
            let visible: BTreeSet<String> = live.iter().cloned().collect();
            let edges: Vec<(String, String)> = m
                .edges
                .iter()
                .filter(|(a, b)| visible.contains(a) && visible.contains(b))
                .filter(|(a, b)| m.may_write(&Key::Edge(a.clone(), b.clone()), loc))
                .cloned()
                .collect();
            let (from, to) = rng.pick(&edges)?.clone();
            Some(Op::Unlink { actor, from, to })
        }
        _ => {
            let cid = pick(rng)?;
            // Never delete what another location wrote since the last
            // barrier: which of the two happened "later" is not observable.
            let foreign = m.owner.iter().any(|(k, o)| *o != loc && k.mentions(&cid));
            (!foreign).then_some(Op::Delete { actor, cid })
        }
    }
}

fn apply(m: &mut Model, op: &Op, added: Option<String>) {
    let loc = op.actor().loc();
    match op {
        Op::Add {
            node_type, title, ..
        } => {
            let cid = added.expect("an add yields a change_id");
            m.nodes.insert(
                cid.clone(),
                MNode {
                    node_type: node_type.to_string(),
                    title: title.clone(),
                    status: "pending".to_string(),
                    prompt: None,
                },
            );
            m.visible.entry(loc).or_default().insert(cid);
        }
        Op::Status { cid, status, .. } => {
            m.nodes.get_mut(cid).unwrap().status = status.to_string();
            m.owner.insert(Key::Status(cid.clone()), loc);
        }
        Op::Prompt { cid, prompt, .. } => {
            m.nodes.get_mut(cid).unwrap().prompt = Some(prompt.clone());
            m.owner.insert(Key::Prompt(cid.clone()), loc);
        }
        Op::Title { cid, title } => {
            m.nodes.get_mut(cid).unwrap().title = title.clone();
            m.owner.insert(Key::Title(cid.clone()), loc);
        }
        Op::Link { from, to, .. } => {
            m.edges.insert((from.clone(), to.clone()));
            m.owner.insert(Key::Edge(from.clone(), to.clone()), loc);
        }
        Op::Unlink { from, to, .. } => {
            m.edges.remove(&(from.clone(), to.clone()));
            m.owner.insert(Key::Edge(from.clone(), to.clone()), loc);
        }
        Op::Delete { cid, .. } => {
            m.nodes.remove(cid);
            m.deleted.insert(cid.clone());
            m.edges.retain(|(a, b)| a != cid && b != cid);
        }
    }
}

fn check_view(w: &World, m: &Model, model: &View, got: &View, name: &str) {
    let back: Vec<&String> = m
        .deleted
        .iter()
        .filter(|d| got.nodes.contains_key(*d))
        .collect();
    if !back.is_empty() {
        w.fail(format!(
            "{name}: {} deleted node(s) came back with no later edit: {back:?}",
            back.len()
        ));
    }
    if got != model {
        w.fail(format!(
            "{name} does not match the model:\n{}",
            model.diff(got, "model", name)
        ));
    }
    let d = got.dangling();
    if !d.is_empty() {
        w.fail(format!("{name} has dangling edges: {d:?}"));
    }
}

/// Everything synced and replayed, then every copy compared with the model.
fn barrier(w: &mut World, m: &mut Model, final_round: bool) {
    w.log.push("-- barrier --".to_string());
    for loc in [Loc::L1, Loc::L2] {
        if w.offline.contains(&loc) {
            w.set_offline(loc, false);
        }
    }
    let server = w.server.is_some();
    if server {
        w.step_ok(&w.c1, &["remote", "push"]);
        w.step_ok(&w.c2, &["remote", "push"]);
    }
    w.exchange(Loc::L1);
    w.exchange(Loc::L2);
    w.exchange(Loc::L1);
    if server {
        w.step_ok(&w.c1, &["remote", "pull"]);
        w.step_ok(&w.c2, &["remote", "pull"]);
        w.step_ok(&w.c1, &["remote", "push"]);
        w.step_ok(&w.c2, &["remote", "push"]);
    }
    let expected = m.view();
    check_view(w, m, &expected, &w.c1.view(), "clone 1");
    check_view(w, m, &expected, &w.c2.view(), "clone 2");
    if let Some(s) = &w.server {
        check_view(w, m, &expected, &s.view(&w.ws), "the server");
        for (p, name) in [(&w.c1, "clone 1"), (&w.c2, "clone 2")] {
            let st = p.dx(&["remote", "status"]);
            if !status_says_clean(&st) {
                w.fail(format!(
                    "content agrees but `remote status` in {name} says:\n{}",
                    st.all()
                ));
            }
        }
    }
    if !final_round {
        let all: BTreeSet<String> = m.nodes.keys().cloned().collect();
        for loc in [Loc::L1, Loc::L2, Loc::S] {
            m.visible.insert(loc, all.clone());
        }
        m.owner.clear();
        m.offline_dirty.clear();
        m.agent_dirty.clear();
    }
}

fn run(server: Option<Server>, name: &str) {
    let seed = seed();
    let steps = steps(if server.is_some() { 45 } else { 35 });
    eprintln!("{name}: seed = {seed} steps = {steps} (replay with DECIDUOUS_E2E_SEED={seed})");
    let mut rng = Rng::new(seed);
    let sb = match &server {
        Some(s) => Sandbox::with_server(s.clone()),
        None => Sandbox::new(),
    };
    let origin = sb.origin("origin");
    let c1 = sb.project("clone1", Some(&origin));
    let ws = unique("model");
    if server.is_some() {
        c1.remote_init(&ws);
        c1.commit_graph("remote");
        c1.git_ok(&["push", "-q", "origin", "HEAD:main"]);
    }
    let c2 = sb.clone_of(&origin, "clone2");
    let stdio = StdioMcp::spawn(&sb, &c1.dir);
    let agent = server.as_ref().map(|s| Mutex::new(s.session(Some(&ws))));
    let mut actors = vec![Actor::Cli1, Actor::Stdio1, Actor::Cli2];
    if server.is_some() {
        actors.push(Actor::Agent);
    }
    let mut w = World {
        seed,
        c1,
        c2,
        stdio: Mutex::new(stdio),
        agent,
        server,
        ws,
        offline: BTreeSet::new(),
        log: Vec::new(),
        uuid: Mutex::new(BTreeMap::new()),
    };
    let mut m = Model::default();
    let mut n = 0usize;
    while n < steps {
        // Occasionally take a clone's server away, or bring it back.
        if w.server.is_some() && rng.chance(12) {
            let loc = if rng.chance(50) { Loc::L1 } else { Loc::L2 };
            let off = !w.offline.contains(&loc);
            w.set_offline(loc, off);
        }
        // Occasionally a partial sync that the model does not rely on.
        if rng.chance(8) {
            let loc = if rng.chance(50) { Loc::L1 } else { Loc::L2 };
            if w.server.is_some() && !w.offline.contains(&loc) && rng.chance(50) {
                let cmd = if rng.chance(50) { "push" } else { "pull" };
                let p = w.clone_of(loc);
                w.step_ok(p, &["remote", cmd]);
                if cmd == "push" {
                    m.offline_dirty.remove(&loc);
                } else {
                    m.agent_dirty.remove(&loc);
                }
                w.log.push(format!("{loc:?} remote {cmd}"));
            } else {
                w.exchange(loc);
                w.log.push(format!("{loc:?} git exchange"));
            }
        }
        // `remote status` must not say clean when a difference is certain.
        if w.server.is_some() && rng.chance(15) {
            let loc = if rng.chance(50) { Loc::L1 } else { Loc::L2 };
            let certain = m.offline_dirty.contains(&loc) || m.agent_dirty.contains(&loc);
            if certain && !w.offline.contains(&loc) {
                let st = w.clone_of(loc).dx(&["remote", "status"]);
                if status_says_clean(&st) {
                    w.fail(format!(
                        "{loc:?} holds writes the server lacks (or lacks the agent's), and `remote status` says clean:\n{}",
                        st.all()
                    ));
                }
            }
        }

        // A batch of 1-3 operations on disjoint nodes, run concurrently.
        let width = 1 + rng.below(3);
        let mut batch: Vec<Op> = Vec::new();
        let mut busy = BTreeSet::new();
        let mut used_actors = BTreeSet::new();
        for _ in 0..width * 4 {
            if batch.len() >= width {
                break;
            }
            let free: Vec<Actor> = actors
                .iter()
                .copied()
                .filter(|a| !used_actors.contains(&format!("{a:?}")))
                .collect();
            if let Some(op) = generate(&mut rng, &m, &free, &busy, n) {
                for c in op.nodes() {
                    busy.insert(c.to_string());
                }
                used_actors.insert(format!("{:?}", op.actor()));
                n += 1;
                batch.push(op);
            }
        }
        if batch.is_empty() {
            continue;
        }
        let results: Vec<Result<String, String>> = std::thread::scope(|s| {
            let hs: Vec<_> = batch.iter().map(|op| s.spawn(|| w.exec(op))).collect();
            hs.into_iter().map(|h| h.join().unwrap()).collect()
        });
        for (op, r) in batch.iter().zip(results) {
            let tag = if batch.len() > 1 { " (concurrent)" } else { "" };
            let loc = op.actor().loc();
            let off = w.offline.contains(&loc);
            w.log.push(format!(
                "{op:?}{tag}{}",
                if off { " [offline]" } else { "" }
            ));
            match r {
                Ok(cid) => {
                    apply(&mut m, op, (!cid.is_empty()).then_some(cid));
                    if off {
                        m.offline_dirty.insert(loc);
                    }
                    if loc == Loc::S {
                        m.agent_dirty.insert(Loc::L1);
                        m.agent_dirty.insert(Loc::L2);
                    }
                }
                Err(e) => w.fail(format!("{op:?} was refused or failed:\n{e}")),
            }
        }
        if rng.chance(10) {
            barrier(&mut w, &mut m, false);
        }
    }
    barrier(&mut w, &mut m, true);
    eprintln!(
        "{name}: seed {seed}: {} ops, {} live nodes, {} edges, {} deleted, all copies agree",
        n,
        m.nodes.len(),
        m.edges.len(),
        m.deleted.len()
    );
}

#[test]
fn model_cli_stdio_and_git_converge() {
    let Some(()) = local("model_cli_stdio_and_git_converge") else {
        return;
    };
    run(None, "model_cli_stdio_and_git_converge");
}

#[test]
fn model_every_surface_with_an_unreliable_server_converges() {
    let Some(server) = remote("model_every_surface_with_an_unreliable_server_converges") else {
        return;
    };
    run(
        Some(server),
        "model_every_surface_with_an_unreliable_server_converges",
    );
}
