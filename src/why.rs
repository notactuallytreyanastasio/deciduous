//! `deciduous why <file>`: the decisions behind a file.
//!
//! The obvious join is `metadata.files`: find the nodes that name the file.
//! It finds almost nothing. In this repository's own graph 166 of 1,491
//! nodes name a file and one of 127 decisions does, because a decision is
//! made before anybody knows which files it will touch. What a graph does
//! have is commits on its actions and outcomes (362 nodes), so the join goes
//! the way `git blame` does: the commits that touched the file (or the line
//! range), the nodes carrying those commits, and from each of those up its
//! incoming edges to the decisions on the way and the goal at the top.
//! `metadata.files` and the transcript import's `metadata.file_hashes` are
//! the second source, for work not committed yet.
//!
//! Only ancestors are reported. A decision that hangs beside the action
//! under the same goal is not on the path from the file to the goal, and
//! listing siblings turns a goal with forty decisions into forty answers.

use crate::db::{DecisionEdge, DecisionNode};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Command;

/// How far up the graph a file's node is followed. Deep enough for
/// goal -> option -> decision -> action -> action -> outcome with room to
/// spare; a cap at all because a graph may hold a cycle.
const MAX_CLIMB: usize = 12;

/// old decision -> observation -> revisit -> new decision is three hops.
const MAX_REPLACEMENT_HOPS: usize = 4;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Commit {
    pub hash: String,
    pub date: String,
    pub subject: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct NodeRef {
    pub id: i32,
    pub node_type: String,
    pub title: String,
    pub status: String,
    /// The rationale on the edge that chose or rejected an option.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
}

impl NodeRef {
    fn of(n: &DecisionNode) -> Self {
        NodeRef {
            id: n.id,
            node_type: n.node_type.clone(),
            title: n.title.clone(),
            status: n.status.clone(),
            rationale: None,
        }
    }
}

/// How a node was tied to the file.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "via", rename_all = "snake_case")]
pub enum Via {
    /// The node carries a commit that touched the file.
    Commit { hash: String, subject: String },
    /// The node names the file in `metadata.files` or `metadata.file_hashes`.
    Files,
}

#[derive(Debug, Clone, Serialize)]
pub struct Hit {
    pub node: NodeRef,
    #[serde(flatten)]
    pub via: Via,
    pub date: String,
}

/// A decision on the path from the file to its goal, with its options.
#[derive(Debug, Clone, Serialize)]
pub struct Choice {
    pub decision: NodeRef,
    pub chosen: Vec<NodeRef>,
    pub rejected: Vec<NodeRef>,
    /// Options linked to the decision that nothing marks either way.
    pub considered: Vec<NodeRef>,
    /// For a decision that was superseded or abandoned: the decisions
    /// reached from it, which is where `archaeology pivot` puts the
    /// replacement (old -> observation -> revisit -> new).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub replaced_by: Vec<NodeRef>,
}

/// Everything one goal has to say about the file.
#[derive(Debug, Clone, Serialize)]
pub struct Thread {
    pub goal: Option<NodeRef>,
    pub choices: Vec<Choice>,
    pub hits: Vec<Hit>,
    pub last_touched: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WhyReport {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lines: Option<(u32, u32)>,
    /// Commits that touched the file (or the lines), and how many of them a
    /// node carries. The difference is history the graph cannot explain.
    pub commits: usize,
    pub commits_in_graph: usize,
    /// Why git history was not used, when it was not.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_note: Option<String>,
    pub threads: Vec<Thread>,
}

impl WhyReport {
    /// True when the report holds something an agent about to edit the file
    /// should weigh: a decision on the path, or a node that was rejected,
    /// superseded or abandoned. A goal with an action and an outcome under
    /// it says what happened, which the file already shows.
    pub fn worth_interrupting(&self) -> bool {
        self.threads.iter().any(|t| {
            !t.choices.is_empty()
                || t.hits.iter().any(|h| is_reversed(&h.node.status))
                || t.goal.as_ref().is_some_and(|g| is_reversed(&g.status))
        })
    }
}

fn is_reversed(status: &str) -> bool {
    matches!(status, "rejected" | "superseded" | "abandoned")
}

/// `src/db.rs`, `src/db.rs:120` or `src/db.rs:120-180`. A name that exists
/// on disk is a path whatever it ends in.
pub fn parse_target(arg: &str) -> (String, Option<(u32, u32)>) {
    if Path::new(arg).exists() {
        return (arg.to_string(), None);
    }
    let Some((path, tail)) = arg.rsplit_once(':') else {
        return (arg.to_string(), None);
    };
    let range = match tail.split_once('-') {
        Some((a, b)) => a.parse().ok().zip(b.parse().ok()),
        None => tail.parse().ok().map(|n| (n, n)),
    };
    match range {
        Some((a, b)) if a >= 1 && b >= a && !path.is_empty() => (path.to_string(), Some((a, b))),
        _ => (arg.to_string(), None),
    }
}

/// `path` as the project sees it: relative to `root`, forward slashes. A
/// path outside the project comes back as given.
pub fn project_relative(root: &Path, cwd: &Path, path: &str) -> String {
    let p = Path::new(path);
    let abs: PathBuf = if p.is_absolute() {
        p.to_path_buf()
    } else {
        cwd.join(p)
    };
    let abs = lexical_clean(&abs);
    // /tmp and /private/tmp are one directory on macOS; try the root both
    // as given and resolved.
    let roots = [Some(root.to_path_buf()), root.canonicalize().ok()];
    let abses = [Some(abs.clone()), canonical_parent(&abs)];
    for r in roots.iter().flatten() {
        for a in abses.iter().flatten() {
            if let Ok(rel) = a.strip_prefix(r) {
                return rel.to_string_lossy().replace('\\', "/");
            }
        }
    }
    path.trim_start_matches("./").to_string()
}

fn lexical_clean(p: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

/// The path with its directory resolved, for a file that may not exist.
fn canonical_parent(p: &Path) -> Option<PathBuf> {
    let parent = p.parent()?.canonicalize().ok()?;
    Some(parent.join(p.file_name()?))
}

/// The commits that touched `path` (following renames), or the line range
/// as it stands in HEAD, newest first.
pub fn commits_touching(
    repo: &Path,
    path: &str,
    lines: Option<(u32, u32)>,
) -> Result<Vec<Commit>, String> {
    let mut cmd = Command::new("git");
    cmd.current_dir(repo)
        .args(["log", "--format=%H%x09%cI%x09%s"]);
    match lines {
        Some((a, b)) => {
            cmd.arg("--no-patch").arg(format!("-L{a},{b}:{path}"));
        }
        None => {
            cmd.args(["--follow", "--", path]);
        }
    }
    let out = cmd
        .output()
        .map_err(|e| format!("git could not be run: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(err
            .lines()
            .next()
            .unwrap_or("git log failed")
            .trim()
            .to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, '\t');
            Some(Commit {
                hash: parts.next()?.to_string(),
                date: parts.next()?.to_string(),
                subject: parts.next().unwrap_or("").to_string(),
            })
        })
        .filter(|c| c.hash.len() == 40)
        .collect())
}

/// The report for a project-relative path, read from the database and
/// from git. History that cannot be read (no repository, a line range of a
/// file git has never seen) is noted in the report, and the nodes that
/// name the file are still found.
pub fn report(
    db: &crate::db::Database,
    root: &Path,
    path: &str,
    lines: Option<(u32, u32)>,
) -> Result<WhyReport, String> {
    let nodes = db.get_all_nodes().map_err(|e| e.to_string())?;
    let edges = db.get_all_edges().map_err(|e| e.to_string())?;
    let (commits, note) = match commits_touching(root, path, lines) {
        Ok(commits) => (commits, None),
        Err(e) => (Vec::new(), Some(e)),
    };
    let mut report = why(&nodes, &edges, path, lines, &commits);
    report.git_note = note;
    Ok(report)
}

fn same_file(named: &str, rel: &str) -> bool {
    let named = named.trim_start_matches("./").replace('\\', "/");
    named == rel || (named.starts_with('/') && named.ends_with(&format!("/{rel}")))
}

fn names_file(meta: &serde_json::Value, rel: &str) -> bool {
    let in_files = meta
        .get("files")
        .and_then(|f| f.as_array())
        .is_some_and(|a| {
            a.iter()
                .filter_map(|v| v.as_str())
                .any(|f| same_file(f, rel))
        });
    let in_hashes = meta
        .get("file_hashes")
        .and_then(|f| f.as_object())
        .is_some_and(|o| o.keys().any(|f| same_file(f, rel)));
    in_files || in_hashes
}

/// The report for `path`, from the graph and the commits that touched it.
pub fn why(
    nodes: &[DecisionNode],
    edges: &[DecisionEdge],
    path: &str,
    lines: Option<(u32, u32)>,
    commits: &[Commit],
) -> WhyReport {
    let by_id: HashMap<i32, &DecisionNode> = nodes.iter().map(|n| (n.id, n)).collect();
    let mut parents: HashMap<i32, Vec<&DecisionEdge>> = HashMap::new();
    let mut around: HashMap<i32, Vec<&DecisionEdge>> = HashMap::new();
    for e in edges {
        if e.from_node_id == e.to_node_id {
            continue;
        }
        parents.entry(e.to_node_id).or_default().push(e);
        around.entry(e.from_node_id).or_default().push(e);
        around.entry(e.to_node_id).or_default().push(e);
    }

    // A node records a commit as 7 or 40 characters.
    let mut by_prefix: HashMap<&str, Vec<&Commit>> = HashMap::new();
    for c in commits {
        by_prefix.entry(&c.hash[..7]).or_default().push(c);
    }

    let mut hits: Vec<Hit> = Vec::new();
    let mut explained: HashSet<&str> = HashSet::new();
    for n in nodes {
        let meta: serde_json::Value = n
            .metadata_json
            .as_deref()
            .and_then(|m| serde_json::from_str(m).ok())
            .unwrap_or(serde_json::Value::Null);
        let commit = meta
            .get("commit")
            .and_then(|c| c.as_str())
            .filter(|c| c.len() >= 7 && c.is_char_boundary(7))
            .and_then(|c| {
                by_prefix
                    .get(&c[..7])?
                    .iter()
                    .find(|g| g.hash.starts_with(c))
            });
        if let Some(c) = commit {
            explained.insert(c.hash.as_str());
            hits.push(Hit {
                node: NodeRef::of(n),
                via: Via::Commit {
                    hash: c.hash[..7].to_string(),
                    subject: c.subject.clone(),
                },
                date: c.date.clone(),
            });
        } else if lines.is_none() && names_file(&meta, path) {
            // A node names a file, not lines of it: asked about a range,
            // only the commits that touched the range can answer.
            hits.push(Hit {
                node: NodeRef::of(n),
                via: Via::Files,
                date: n.created_at.clone(),
            });
        }
    }

    // Each hit climbs to its topmost goal; hits that reach the same goal
    // are one thread.
    let mut threads: BTreeMap<i32, Thread> = BTreeMap::new();
    let mut choice_ids: HashMap<i32, Vec<i32>> = HashMap::new();
    let mut taken: HashMap<i32, HashSet<i32>> = HashMap::new();
    for hit in hits {
        let mut seen: HashSet<i32> = HashSet::from([hit.node.id]);
        let mut queue: VecDeque<(i32, usize)> = VecDeque::from([(hit.node.id, 0)]);
        let mut goal: Option<i32> = None;
        let mut decisions: Vec<i32> = Vec::new();
        let mut options: Vec<i32> = Vec::new();
        while let Some((id, depth)) = queue.pop_front() {
            if let Some(n) = by_id.get(&id) {
                match n.node_type.as_str() {
                    // Breadth first, so the last goal seen is the farthest.
                    "goal" => goal = Some(id),
                    "decision" => decisions.push(id),
                    "option" => options.push(id),
                    _ => {}
                }
            }
            if depth == MAX_CLIMB {
                continue;
            }
            for e in parents.get(&id).into_iter().flatten() {
                if seen.insert(e.from_node_id) {
                    queue.push_back((e.from_node_id, depth + 1));
                }
            }
        }
        // A thread with no goal above it is keyed by its own node, negated
        // so it cannot collide with a goal's id.
        let key = goal.unwrap_or(-hit.node.id);
        let thread = threads.entry(key).or_insert_with(|| Thread {
            goal: goal.and_then(|g| by_id.get(&g)).map(|n| NodeRef::of(n)),
            choices: Vec::new(),
            hits: Vec::new(),
            last_touched: String::new(),
        });
        let ids = choice_ids.entry(key).or_default();
        for d in decisions {
            if !ids.contains(&d) {
                ids.push(d);
            }
        }
        taken.entry(key).or_default().extend(options);
        thread.hits.push(hit);
    }

    let mut threads: Vec<Thread> = threads
        .into_iter()
        .map(|(key, mut thread)| {
            let taken = taken.remove(&key).unwrap_or_default();
            let mut ids = choice_ids.remove(&key).unwrap_or_default();
            ids.sort_unstable();
            thread.choices = ids
                .into_iter()
                .filter_map(|d| Some(choice(by_id.get(&d)?, &around, &by_id, &taken)))
                .collect();
            thread.hits.sort_by(|a, b| {
                day(&b.date)
                    .cmp(day(&a.date))
                    .then(b.node.id.cmp(&a.node.id))
            });
            thread.last_touched = thread
                .hits
                .first()
                .map(|h| day(&h.date).to_string())
                .unwrap_or_default();
            thread
        })
        .collect();
    threads.sort_by(|a, b| {
        b.last_touched
            .cmp(&a.last_touched)
            .then(b.hits[0].node.id.cmp(&a.hits[0].node.id))
    });

    WhyReport {
        path: path.to_string(),
        lines,
        commits: commits.len(),
        commits_in_graph: explained.len(),
        git_note: None,
        threads,
    }
}

/// Commit dates carry an offset and node dates do not; the day is what the
/// two have in common.
fn day(date: &str) -> &str {
    date.get(..10).unwrap_or(date)
}

/// The decision with its options. Graphs link the two both ways round
/// (this repository's has 129 decision -> option edges and 39 the other
/// way), so an option is any option node on either end of an edge.
fn choice(
    decision: &DecisionNode,
    around: &HashMap<i32, Vec<&DecisionEdge>>,
    by_id: &HashMap<i32, &DecisionNode>,
    taken: &HashSet<i32>,
) -> Choice {
    let mut out = Choice {
        decision: NodeRef::of(decision),
        chosen: Vec::new(),
        rejected: Vec::new(),
        considered: Vec::new(),
        replaced_by: Vec::new(),
    };
    if is_reversed(&decision.status) {
        // Forward, and no further than the first decision on each path:
        // what replaced the replacement is that decision's own story.
        let mut seen: HashSet<i32> = HashSet::from([decision.id]);
        let mut queue: VecDeque<(i32, usize)> = VecDeque::from([(decision.id, 0)]);
        while let Some((id, depth)) = queue.pop_front() {
            if depth == MAX_REPLACEMENT_HOPS {
                continue;
            }
            for e in around.get(&id).into_iter().flatten() {
                if e.from_node_id != id || !seen.insert(e.to_node_id) {
                    continue;
                }
                match by_id.get(&e.to_node_id) {
                    Some(n) if n.node_type == "decision" => out.replaced_by.push(NodeRef::of(n)),
                    Some(n) if matches!(n.node_type.as_str(), "observation" | "revisit") => {
                        queue.push_back((e.to_node_id, depth + 1))
                    }
                    _ => {}
                }
            }
        }
    }
    let mut seen: HashSet<i32> = HashSet::new();
    let mut edges: Vec<&&DecisionEdge> = around.get(&decision.id).into_iter().flatten().collect();
    // An edge that says chosen or rejected outranks a plain one to the
    // same option.
    edges.sort_by_key(|e| (e.edge_type == "leads_to", e.id));
    for e in edges {
        let other = if e.from_node_id == decision.id {
            e.to_node_id
        } else {
            e.from_node_id
        };
        let Some(option) = by_id.get(&other).filter(|n| n.node_type == "option") else {
            continue;
        };
        if !seen.insert(other) {
            continue;
        }
        let mut r = NodeRef::of(option);
        let typed = matches!(e.edge_type.as_str(), "chosen" | "rejected");
        r.rationale = e
            .rationale
            .clone()
            .filter(|t| typed && !t.trim().is_empty());
        if e.edge_type == "rejected" || is_reversed(&option.status) {
            out.rejected.push(r);
        } else if e.edge_type == "chosen"
            // An option above the file's node was the one taken only when
            // it hangs below the decision. In goal -> option -> decision
            // every option is above everything the decision led to.
            || (taken.contains(&other) && e.from_node_id == decision.id)
        {
            out.chosen.push(r);
        } else {
            out.considered.push(r);
        }
    }
    out
}

fn clip(s: &str, max: usize) -> String {
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.chars().count() <= max {
        s
    } else {
        let cut: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{}…", cut.trim_end())
    }
}

fn line(indent: usize, label: &str, n: &NodeRef) -> String {
    let rationale = n
        .rationale
        .as_deref()
        .map(|r| format!("  ({})", clip(r, 100)))
        .unwrap_or_default();
    format!(
        "{:indent$}{label} {} {} [{}]{rationale}\n",
        "",
        n.id,
        clip(&n.title, 100),
        n.status,
    )
}

fn header(report: &WhyReport) -> String {
    let target = match report.lines {
        Some((a, b)) if a == b => format!("{}:{a}", report.path),
        Some((a, b)) => format!("{}:{a}-{b}", report.path),
        None => report.path.clone(),
    };
    let nodes: usize = report.threads.iter().map(|t| t.hits.len()).sum();
    let history = match &report.git_note {
        Some(note) => format!("git history not used ({note})"),
        None => format!(
            "{} of {} commits that touched it are in the graph",
            report.commits_in_graph, report.commits
        ),
    };
    format!("{target}: {history}; {nodes} nodes\n")
}

/// The report as text: one block per goal, newest first, at most `limit`
/// goals and `hits` nodes under each (0 for no limit).
pub fn render(report: &WhyReport, limit: usize, hits: usize) -> String {
    let mut out = header(report);
    if report.threads.is_empty() {
        out.push_str(
            "Nothing in the graph is tied to this file: no node carries a commit that touched it, and none names it.\n",
        );
        return out;
    }
    let shown = if limit == 0 {
        report.threads.len()
    } else {
        limit.min(report.threads.len())
    };
    for t in &report.threads[..shown] {
        out.push('\n');
        match &t.goal {
            Some(g) => out.push_str(&line(0, "goal", g)),
            None => out.push_str("(no goal above these)\n"),
        }
        for c in &t.choices {
            out.push_str(&line(2, "decision", &c.decision));
            for o in &c.chosen {
                out.push_str(&line(4, "chose   ", o));
            }
            for o in &c.rejected {
                out.push_str(&line(4, "rejected", o));
            }
            for o in &c.considered {
                out.push_str(&line(4, "option  ", o));
            }
            for o in &c.replaced_by {
                out.push_str(&line(4, "replaced by", o));
            }
        }
        let n = if hits == 0 {
            t.hits.len()
        } else {
            hits.min(t.hits.len())
        };
        for h in &t.hits[..n] {
            let via = match &h.via {
                Via::Commit { hash, .. } => hash.clone(),
                Via::Files => "names it".to_string(),
            };
            out.push_str(&format!(
                "  {} {} {} [{}]  {} {}\n",
                h.node.node_type,
                h.node.id,
                clip(&h.node.title, 90),
                h.node.status,
                via,
                day(&h.date),
            ));
        }
        if n < t.hits.len() {
            out.push_str(&format!("  … {} more nodes\n", t.hits.len() - n));
        }
    }
    if shown < report.threads.len() {
        out.push_str(&format!(
            "\n… {} more goals touched this file (--all)\n",
            report.threads.len() - shown
        ));
    }
    out
}

/// What the pre-edit hook says: the decisions only, a few lines, and where
/// the rest is. `None` when the report holds nothing worth interrupting an
/// edit for.
pub fn brief(report: &WhyReport) -> Option<String> {
    const MAX: usize = 4;
    if !report.worth_interrupting() {
        return None;
    }
    let mut lines: Vec<String> = Vec::new();
    for t in &report.threads {
        let under = t
            .goal
            .as_ref()
            .map(|g| format!(" (goal {}: {})", g.id, clip(&g.title, 70)))
            .unwrap_or_default();
        for c in &t.choices {
            let mut s = format!(
                "- decision {} [{}]: {}{under}",
                c.decision.id,
                c.decision.status,
                clip(&c.decision.title, 110)
            );
            for (label, set) in [("chose", &c.chosen), ("rejected", &c.rejected)] {
                for o in set {
                    s.push_str(&format!("; {label} {}: {}", o.id, clip(&o.title, 90)));
                    if let Some(r) = &o.rationale {
                        s.push_str(&format!(" ({})", clip(r, 90)));
                    }
                }
            }
            for o in &c.replaced_by {
                s.push_str(&format!(
                    "; replaced by decision {}: {}",
                    o.id,
                    clip(&o.title, 110)
                ));
            }
            lines.push(s);
        }
        for h in t.hits.iter().filter(|h| is_reversed(&h.node.status)) {
            lines.push(format!(
                "- {} {} is {}: {}{under}",
                h.node.node_type,
                h.node.id,
                h.node.status,
                clip(&h.node.title, 110)
            ));
        }
        if let Some(g) = t.goal.as_ref().filter(|g| is_reversed(&g.status)) {
            if t.choices.is_empty() {
                lines.push(format!(
                    "- goal {} is {}: {}",
                    g.id,
                    g.status,
                    clip(&g.title, 110)
                ));
            }
        }
    }
    if lines.is_empty() {
        return None;
    }
    let more = lines.len().saturating_sub(MAX);
    lines.truncate(MAX);
    let mut out = format!(
        "deciduous: decisions already made about {} (`deciduous why {}` shows all of it):\n{}",
        report.path,
        report.path,
        lines.join("\n")
    );
    if more > 0 {
        out.push_str(&format!("\n… and {more} more"));
    }
    Some(out)
}

/// True the first time `path` is asked about in `session`, false after:
/// the hook speaks once per file per session. The record is a file in the
/// system temp directory, so it leaves nothing in the project.
pub fn first_ask(session: &str, path: &str) -> bool {
    let safe: String = session
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .take(64)
        .collect();
    if safe.is_empty() {
        return true;
    }
    let record = std::env::temp_dir().join(format!("deciduous-why-{safe}"));
    let seen = std::fs::read_to_string(&record).unwrap_or_default();
    if seen.lines().any(|l| l == path) {
        return false;
    }
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&record)
    {
        let _ = writeln!(f, "{path}");
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: i32, node_type: &str, title: &str, status: &str, meta: &str) -> DecisionNode {
        DecisionNode {
            id,
            change_id: format!("c{id}"),
            node_type: node_type.into(),
            title: title.into(),
            description: None,
            status: status.into(),
            created_at: "2026-01-02T00:00:00Z".into(),
            updated_at: "2026-01-02T00:00:00Z".into(),
            metadata_json: Some(meta.into()),
        }
    }

    fn edge(id: i32, from: i32, to: i32, edge_type: &str, rationale: Option<&str>) -> DecisionEdge {
        DecisionEdge {
            id,
            from_node_id: from,
            to_node_id: to,
            from_change_id: None,
            to_change_id: None,
            edge_type: edge_type.into(),
            weight: None,
            rationale: rationale.map(String::from),
            created_at: "2026-01-02T00:00:00Z".into(),
        }
    }

    fn commit(hash_start: &str, date: &str) -> Commit {
        Commit {
            hash: format!("{hash_start:0<40}"),
            date: date.into(),
            subject: "a change".into(),
        }
    }

    /// goal 1 -> decision 2 -> action 5 (commit); decision 2 chose option 3
    /// and rejected option 4.
    fn graph() -> (Vec<DecisionNode>, Vec<DecisionEdge>) {
        (
            vec![
                node(1, "goal", "Rate limit the API", "active", "{}"),
                node(2, "decision", "Limit by token, not IP", "completed", "{}"),
                node(3, "option", "Per-token buckets", "completed", "{}"),
                node(4, "option", "Per-IP buckets", "pending", "{}"),
                node(
                    5,
                    "action",
                    "Add the limiter",
                    "completed",
                    r#"{"commit":"abc1234"}"#,
                ),
                node(6, "decision", "A sibling decision", "completed", "{}"),
            ],
            vec![
                edge(1, 1, 2, "leads_to", None),
                edge(2, 2, 3, "chosen", Some("survives NAT")),
                edge(3, 2, 4, "rejected", Some("an office is one IP")),
                edge(4, 2, 5, "leads_to", None),
                edge(5, 1, 6, "leads_to", None),
            ],
        )
    }

    #[test]
    fn a_commit_leads_to_its_decision_and_what_the_decision_rejected() {
        let (nodes, edges) = graph();
        let commits = [
            commit("abc1234", "2026-03-01T10:00:00-05:00"),
            commit("fff0000", "2026-02-01T10:00:00-05:00"),
        ];
        let r = why(&nodes, &edges, "src/limit.rs", None, &commits);
        assert_eq!((r.commits, r.commits_in_graph), (2, 1));
        assert_eq!(r.threads.len(), 1);
        let t = &r.threads[0];
        assert_eq!(t.goal.as_ref().unwrap().id, 1);
        assert_eq!(
            t.choices.len(),
            1,
            "the sibling decision is not on the path"
        );
        let c = &t.choices[0];
        assert_eq!(c.decision.id, 2);
        assert_eq!(c.chosen[0].id, 3);
        assert_eq!(c.rejected[0].id, 4);
        assert_eq!(
            c.rejected[0].rationale.as_deref(),
            Some("an office is one IP")
        );
        assert!(r.worth_interrupting());
        let b = brief(&r).unwrap();
        assert!(
            b.contains("rejected 4: Per-IP buckets (an office is one IP)"),
            "{b}"
        );
    }

    #[test]
    fn a_node_that_names_the_file_counts_without_a_commit() {
        let nodes = vec![
            node(1, "goal", "g", "active", "{}"),
            node(
                2,
                "action",
                "by files",
                "pending",
                r#"{"files":["./src/a.rs"]}"#,
            ),
            node(
                3,
                "action",
                "by hash",
                "completed",
                r#"{"file_hashes":{"src/a.rs":"00"}}"#,
            ),
            node(
                4,
                "action",
                "another file",
                "pending",
                r#"{"files":["src/b.rs"]}"#,
            ),
        ];
        let edges = vec![
            edge(1, 1, 2, "leads_to", None),
            edge(2, 1, 3, "leads_to", None),
        ];
        let r = why(&nodes, &edges, "src/a.rs", None, &[]);
        let ids: Vec<i32> = r.threads[0].hits.iter().map(|h| h.node.id).collect();
        assert_eq!(ids, vec![3, 2]);
        assert!(
            !r.worth_interrupting(),
            "a goal and its actions are not news"
        );
        assert!(brief(&r).is_none());
    }

    #[test]
    fn an_option_above_the_action_was_the_one_taken() {
        // decision -> option -> action, with no edge typed `chosen`.
        let nodes = vec![
            node(1, "decision", "d", "pending", "{}"),
            node(2, "option", "taken", "pending", "{}"),
            node(3, "option", "not taken", "pending", "{}"),
            node(4, "action", "a", "completed", r#"{"commit":"abc1234def"}"#),
        ];
        let edges = vec![
            edge(1, 1, 2, "leads_to", None),
            edge(2, 1, 3, "leads_to", None),
            edge(3, 2, 4, "leads_to", None),
        ];
        let r = why(
            &nodes,
            &edges,
            "f",
            None,
            &[commit("abc1234def", "2026-03-01")],
        );
        let c = &r.threads[0].choices[0];
        assert_eq!(c.chosen.iter().map(|o| o.id).collect::<Vec<_>>(), vec![2]);
        assert_eq!(
            c.considered.iter().map(|o| o.id).collect::<Vec<_>>(),
            vec![3]
        );
        assert!(r.threads[0].goal.is_none());
    }

    #[test]
    fn options_that_lead_to_the_decision_are_not_all_the_one_taken() {
        // The documented flow: goal -> options -> decision -> action.
        let nodes = vec![
            node(1, "option", "a", "pending", "{}"),
            node(2, "option", "b", "pending", "{}"),
            node(3, "decision", "d", "pending", "{}"),
            node(4, "action", "x", "completed", r#"{"commit":"abc1234"}"#),
        ];
        let edges = vec![
            edge(1, 1, 3, "leads_to", None),
            edge(2, 2, 3, "leads_to", None),
            edge(3, 3, 4, "leads_to", None),
        ];
        let r = why(
            &nodes,
            &edges,
            "f",
            None,
            &[commit("abc1234", "2026-03-01")],
        );
        let c = &r.threads[0].choices[0];
        assert!(c.chosen.is_empty());
        assert_eq!(c.considered.len(), 2);
    }

    #[test]
    fn a_line_range_is_answered_by_commits_only() {
        let nodes = vec![node(1, "action", "a", "pending", r#"{"files":["f"]}"#)];
        assert_eq!(why(&nodes, &[], "f", None, &[]).threads.len(), 1);
        assert!(why(&nodes, &[], "f", Some((1, 9)), &[]).threads.is_empty());
    }

    #[test]
    fn a_superseded_decision_names_what_replaced_it() {
        // What `archaeology pivot` builds.
        let nodes = vec![
            node(
                1,
                "decision",
                "Tokens table is the truth",
                "superseded",
                "{}",
            ),
            node(
                2,
                "action",
                "revoke by token",
                "completed",
                r#"{"commit":"abc1234"}"#,
            ),
            node(3, "observation", "tokens went stale", "completed", "{}"),
            node(4, "revisit", "reconsider", "completed", "{}"),
            node(
                5,
                "decision",
                "Sessions store is the truth",
                "pending",
                "{}",
            ),
            node(6, "decision", "Something later still", "pending", "{}"),
        ];
        let edges = vec![
            edge(1, 1, 2, "leads_to", None),
            edge(2, 1, 3, "leads_to", None),
            edge(3, 3, 4, "leads_to", None),
            edge(4, 4, 5, "leads_to", None),
            edge(5, 5, 6, "leads_to", None),
        ];
        let r = why(
            &nodes,
            &edges,
            "f",
            None,
            &[commit("abc1234", "2026-03-01")],
        );
        let c = &r.threads[0].choices[0];
        assert_eq!(
            c.replaced_by.iter().map(|n| n.id).collect::<Vec<_>>(),
            vec![5]
        );
        let b = brief(&r).unwrap();
        assert!(b.contains("decision 1 [superseded]"), "{b}");
        assert!(
            b.contains("replaced by decision 5: Sessions store is the truth"),
            "{b}"
        );
        assert!(render(&r, 0, 0).contains("    replaced by 5 Sessions store is the truth"));
    }

    #[test]
    fn a_cycle_ends_and_a_superseded_node_is_worth_saying() {
        let nodes = vec![
            node(1, "action", "a", "superseded", r#"{"commit":"abc1234"}"#),
            node(2, "action", "b", "completed", "{}"),
        ];
        let edges = vec![
            edge(1, 1, 2, "leads_to", None),
            edge(2, 2, 1, "leads_to", None),
        ];
        let r = why(
            &nodes,
            &edges,
            "f",
            None,
            &[commit("abc1234", "2026-03-01")],
        );
        assert_eq!(r.threads.len(), 1);
        assert!(brief(&r).unwrap().contains("action 1 is superseded"));
    }

    #[test]
    fn threads_are_newest_first_and_render_says_what_was_left_out() {
        let nodes = vec![
            node(1, "goal", "old", "completed", "{}"),
            node(
                2,
                "action",
                "old work",
                "completed",
                r#"{"commit":"aaaaaaa"}"#,
            ),
            node(3, "goal", "new", "active", "{}"),
            node(
                4,
                "action",
                "new work",
                "completed",
                r#"{"commit":"bbbbbbb"}"#,
            ),
        ];
        let edges = vec![
            edge(1, 1, 2, "leads_to", None),
            edge(2, 3, 4, "leads_to", None),
        ];
        let commits = [
            commit("bbbbbbb", "2026-05-01"),
            commit("aaaaaaa", "2026-01-01"),
        ];
        let r = why(&nodes, &edges, "f", None, &commits);
        assert_eq!(r.threads[0].goal.as_ref().unwrap().id, 3);
        let text = render(&r, 1, 0);
        assert!(text.starts_with("f: 2 of 2 commits that touched it are in the graph; 2 nodes\n"));
        assert!(text.contains("1 more goals touched this file"), "{text}");
        assert!(!text.contains("old work"));
    }

    #[test]
    fn targets_parse_as_path_line_or_range() {
        assert_eq!(parse_target("src/x.rs"), ("src/x.rs".into(), None));
        assert_eq!(
            parse_target("src/x.rs:12"),
            ("src/x.rs".into(), Some((12, 12)))
        );
        assert_eq!(
            parse_target("src/x.rs:12-40"),
            ("src/x.rs".into(), Some((12, 40)))
        );
        assert_eq!(
            parse_target("src/x.rs:40-12"),
            ("src/x.rs:40-12".into(), None)
        );
        assert_eq!(parse_target("src/x.rs:0"), ("src/x.rs:0".into(), None));
    }

    #[test]
    fn paths_become_project_relative() {
        let root = Path::new("/p/repo");
        assert_eq!(
            project_relative(root, Path::new("/p/repo/src"), "db.rs"),
            "src/db.rs"
        );
        assert_eq!(
            project_relative(root, root, "/p/repo/src/db.rs"),
            "src/db.rs"
        );
        assert_eq!(
            project_relative(root, Path::new("/p/repo/src"), "../web/a.ts"),
            "web/a.ts"
        );
        assert_eq!(project_relative(root, root, "./src/db.rs"), "src/db.rs");
    }

    #[test]
    fn the_hook_speaks_once_per_file_per_session() {
        let session = format!("test-{}", std::process::id());
        let _ = std::fs::remove_file(std::env::temp_dir().join(format!("deciduous-why-{session}")));
        assert!(first_ask(&session, "src/a.rs"));
        assert!(!first_ask(&session, "src/a.rs"));
        assert!(first_ask(&session, "src/b.rs"));
        let _ = std::fs::remove_file(std::env::temp_dir().join(format!("deciduous-why-{session}")));
    }
}
