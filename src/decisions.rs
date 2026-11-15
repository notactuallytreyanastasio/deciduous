//! `deciduous transcript decisions`: the decisions of a session, proposed by
//! a model that read the transcript, each backed by a quote.
//!
//! `transcript import` derives goals, actions and outcomes because a tool
//! call and its result prove them. It does not derive a decision: whether
//! one approach was chosen over another is a judgement about prose. This is
//! that judgement, made by a model and then checked the only way it can be
//! checked mechanically: every proposal must quote the turn it came from,
//! and a proposal whose quote is not in that turn, character for character
//! once whitespace is collapsed, is dropped and named in the report. A
//! model can still misread what a real sentence means, which is why what
//! survives is a proposal in `.deciduous/proposals.json` and not a node,
//! until `deciduous proposals accept` (or `--accept`) says otherwise.
//!
//! An accepted decision is put where `deciduous why` will find it: under
//! the turn's goal, above the actions that came after it in the same turn.

use crate::db::{Database, DbError};
use crate::transcript::{self, derive_change_id, DigestItem, TurnDigest};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Characters of digest sent to the model in one call. A session longer
/// than this is read in several calls, split between turns.
const CHUNK_CHARS: usize = 120_000;
const PROMPT_CHARS: usize = 3_000;
const SAID_CHARS: usize = 4_000;
/// A quote shorter than this proves nothing: "yes" is in every turn.
const MIN_QUOTE_CHARS: usize = 20;
/// Set in the environment of the model call, so a hook that fires inside
/// it does not start another.
/// (The name is from when `transcript decisions` was the only caller: every
/// `claude -p` deciduous spawns sets it now.)
pub const GUARD_ENV: &str = "DECIDUOUS_DERIVING";
/// A command to run instead of `claude -p`: the prompt arrives on stdin and
/// the reply is read from stdout. For tests, and for another model.
pub const COMMAND_ENV: &str = "DECIDUOUS_DECISIONS_CMD";

const SYSTEM: &str = "You read the record of a coding session and list the decisions made in it.\n\
\n\
A decision is a point where two or more concrete approaches were on the table and the record shows which one was taken and why. These are not decisions: a plan with one path, a step of work, a fix with one obvious fix, a preference with no alternative named, a question. Most turns contain none; an empty list is a correct answer.\n\
\n\
Reply with JSON only, no prose and no code fence:\n\
{\"decisions\":[{\"turn\":<the TURN number>,\"title\":\"<what was chosen and what it beat, under 100 characters>\",\"quote\":\"<one passage from a USER or ASSISTANT line of that turn that shows the choice being made>\",\"options\":[{\"title\":\"<the approach>\",\"verdict\":\"chosen\" or \"rejected\",\"rationale\":\"<the reason the record gives>\"}],\"files\":[\"<paths from DID lines of that turn that the choice governs, if clear>\"]}]}\n\
\n\
Rules:\n\
- quote is copied character for character from the record, 20 to 300 characters, from a single line. It is checked by machine and a decision whose quote is not found is discarded. Never quote across a [...] gap.\n\
- Every decision has at least two options, at least one chosen and at least one rejected.\n\
- A rationale is the reason given in the record, not one you supply. If the record gives none, leave it empty.\n\
- Do not list the same decision twice.";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    Chosen,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProposedOption {
    pub title: String,
    pub verdict: Verdict,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub rationale: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Proposed,
    Accepted,
    Rejected,
}

/// A decision a model read out of a transcript.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposal {
    /// The change_id the decision node gets: a function of the session, the
    /// turn and the quote, so reading a session twice proposes it once.
    pub id: String,
    pub session: String,
    pub turn_number: usize,
    /// When the quoted words were said.
    pub at: String,
    pub title: String,
    pub quote: String,
    pub options: Vec<ProposedOption>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
    /// change_id of the goal the import derives for the turn.
    pub goal: String,
    /// change_ids of every node the import derived under that goal: the way
    /// back to a goal the agent logged itself, which has another change_id.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub turn_nodes: Vec<String>,
    /// change_ids of the actions the decision is put above.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<String>,
    pub state: State,
}

/// A proposal the check refused, and why.
#[derive(Debug, Clone, Serialize)]
pub struct Dropped {
    pub title: String,
    pub reason: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Store {
    #[serde(default)]
    pub proposals: Vec<Proposal>,
    /// Sessions already read, by id, with the number of turns read: a
    /// session that has grown since is read again.
    #[serde(default)]
    pub sessions: BTreeMap<String, usize>,
}

impl Store {
    pub fn path(db: &Database) -> PathBuf {
        db.project_root()
            .map(|r| r.join(".deciduous"))
            .or_else(|| Database::db_path().parent().map(Path::to_path_buf))
            .unwrap_or_else(|| PathBuf::from(".deciduous"))
            .join("proposals.json")
    }

    pub fn load(path: &Path) -> Result<Store, String> {
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text)
                .map_err(|e| format!("{} is not a proposals file: {e}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Store::default()),
            Err(e) => Err(format!("could not read {}: {e}", path.display())),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let body = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, body + "\n")
            .map_err(|e| format!("could not write {}: {e}", path.display()))
    }
}

fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The middle of a long text is cut: a choice is announced at the start of
/// what the assistant says or summed up at the end.
fn clip(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max {
        return s.to_string();
    }
    let half = max / 2;
    let head: String = chars[..half].iter().collect();
    let tail: String = chars[chars.len() - half..].iter().collect();
    format!("{head} [...] {tail}")
}

fn render_turn(number: usize, turn: &TurnDigest) -> String {
    let mut out = format!(
        "=== TURN {number}\nUSER: {}\n",
        squash(&clip(&turn.prompt, PROMPT_CHARS))
    );
    let mut did_run = 0;
    for item in &turn.items {
        match item {
            DigestItem::Said { text, .. } => {
                did_run = 0;
                out.push_str(&format!("ASSISTANT: {}\n", squash(&clip(text, SAID_CHARS))));
            }
            DigestItem::Did(what) => {
                did_run += 1;
                // A long run of tool calls is one piece of work.
                if did_run <= 8 {
                    out.push_str(&format!("DID: {what}\n"));
                } else if did_run == 9 {
                    out.push_str("DID: (more tool calls)\n");
                }
            }
        }
    }
    out
}

/// The session as one or more prompts, each under [`CHUNK_CHARS`] unless a
/// single turn is longer than that.
pub fn prompts(turns: &[TurnDigest]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    for (i, turn) in turns.iter().enumerate() {
        let text = render_turn(i + 1, turn);
        if !current.is_empty() && current.len() + text.len() > CHUNK_CHARS {
            out.push(std::mem::take(&mut current));
        }
        current.push_str(&text);
        current.push('\n');
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// Ask the model. `claude -p` with no tools worth having, no MCP servers,
/// no saved session, and a working directory that is not the project, so
/// the call leaves no transcript for `transcript list` to find and runs no
/// project hook.
fn ask(prompt: &str, model: Option<&str>) -> Result<String, String> {
    let scratch = std::env::temp_dir();
    let mut cmd = match std::env::var(COMMAND_ENV) {
        Ok(line) if !line.trim().is_empty() => {
            let mut c = Command::new("sh");
            c.arg("-c").arg(line);
            c
        }
        _ => {
            let mut c = Command::new("claude");
            c.args([
                "-p",
                "--no-session-persistence",
                "--strict-mcp-config",
                "--mcp-config",
                r#"{"mcpServers":{}}"#,
                "--system-prompt",
                SYSTEM,
            ]);
            if let Some(m) = model {
                c.args(["--model", m]);
            }
            c.current_dir(&scratch);
            c
        }
    };
    let mut child = cmd
        .env(GUARD_ENV, "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("the model could not be run (is `claude` on the PATH?): {e}"))?;
    // The reply is read after the prompt is written; a prompt of 120,000
    // characters fits a pipe only because the reader on the other end is
    // consuming it, and `claude -p` reads stdin to the end before answering.
    let mut stdin = child.stdin.take().ok_or("no stdin")?;
    let body = prompt.to_string();
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(body.as_bytes());
    });
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    let _ = writer.join();
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let said = String::from_utf8_lossy(&out.stdout);
        return Err(format!(
            "the model call failed ({}): {}",
            out.status,
            squash(if err.trim().is_empty() { &said } else { &err })
                .chars()
                .take(300)
                .collect::<String>()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

#[derive(Debug, Deserialize)]
struct RawOption {
    #[serde(default)]
    title: String,
    #[serde(default)]
    verdict: String,
    #[serde(default)]
    rationale: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawDecision {
    turn: usize,
    #[serde(default)]
    title: String,
    #[serde(default)]
    quote: String,
    #[serde(default)]
    options: Vec<RawOption>,
    #[serde(default)]
    files: Vec<String>,
}

/// The reply's JSON object, wherever in the reply it is. A reply with no
/// `decisions` array is an error, not an empty list: a model that answered
/// in prose did not say "none".
fn parse_reply(reply: &str) -> Result<Vec<RawDecision>, String> {
    let start = reply.find('{').ok_or("the model's reply holds no JSON")?;
    let end = reply.rfind('}').ok_or("the model's reply holds no JSON")?;
    if end < start {
        return Err("the model's reply holds no JSON".into());
    }
    let v: Value = serde_json::from_str(&reply[start..=end])
        .map_err(|e| format!("the model's reply is not JSON: {e}"))?;
    let list = v
        .get("decisions")
        .and_then(Value::as_array)
        .ok_or("the model's reply has no `decisions` array")?;
    Ok(list
        .iter()
        .filter_map(|d| serde_json::from_value(d.clone()).ok())
        .collect())
}

/// What the import derived for one turn, by change_id.
#[derive(Debug, Clone, Default)]
pub struct TurnNodes {
    pub goal: String,
    pub all: Vec<String>,
    /// (change_id, created_at, files) of each action.
    pub actions: Vec<(String, String, Vec<String>)>,
}

/// The import's plan, regrouped by turn. Goals appear in the plan in turn
/// order, and every node after a goal and before the next belongs to it.
pub fn turn_nodes(plan: &transcript::Plan) -> Vec<TurnNodes> {
    let mut out: Vec<TurnNodes> = Vec::new();
    for n in &plan.nodes {
        if n.node_type == "goal" {
            out.push(TurnNodes {
                goal: n.change_id.clone(),
                ..Default::default()
            });
            continue;
        }
        let Some(turn) = out.last_mut() else { continue };
        turn.all.push(n.change_id.clone());
        if n.node_type == "action" {
            let files = n
                .metadata
                .get("files")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|f| f.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            turn.actions
                .push((n.change_id.clone(), n.created_at.clone(), files));
        }
    }
    out
}

/// Check what the model said against the record it was shown.
fn validate(
    raw: Vec<RawDecision>,
    session: &str,
    turns: &[TurnDigest],
    nodes: &[TurnNodes],
) -> (Vec<Proposal>, Vec<Dropped>) {
    let mut kept: Vec<Proposal> = Vec::new();
    let mut dropped = Vec::new();
    for d in raw {
        let title = squash(&d.title);
        let mut drop = |reason: String| {
            dropped.push(Dropped {
                title: if title.is_empty() {
                    "(untitled)".into()
                } else {
                    title.clone()
                },
                reason,
            })
        };
        let Some(turn) = d.turn.checked_sub(1).and_then(|i| turns.get(i)) else {
            drop(format!(
                "names turn {}, which the session does not have",
                d.turn
            ));
            continue;
        };
        if title.is_empty() {
            drop("has no title".into());
            continue;
        }
        let quote = squash(&d.quote);
        if quote.chars().count() < MIN_QUOTE_CHARS {
            drop(format!(
                "its quote is under {MIN_QUOTE_CHARS} characters: {quote:?}"
            ));
            continue;
        }
        // Where the words are: the prompt, or something the assistant said.
        let said_at = turn.items.iter().find_map(|item| match item {
            DigestItem::Said { text, timestamp } if squash(text).contains(&quote) => {
                Some(timestamp.clone())
            }
            _ => None,
        });
        let at = match said_at {
            Some(at) => at,
            None if squash(&turn.prompt).contains(&quote) => turn.timestamp.clone(),
            None => {
                drop(format!("its quote is not in turn {}: {quote:?}", d.turn));
                continue;
            }
        };
        let options: Vec<ProposedOption> = d
            .options
            .into_iter()
            .filter_map(|o| {
                let verdict = match o.verdict.trim().to_ascii_lowercase().as_str() {
                    "chosen" => Verdict::Chosen,
                    "rejected" => Verdict::Rejected,
                    _ => return None,
                };
                let title = squash(&o.title);
                (!title.is_empty()).then(|| ProposedOption {
                    title,
                    verdict,
                    rationale: squash(o.rationale.as_deref().unwrap_or("")),
                })
            })
            .collect();
        let chosen = options.iter().any(|o| o.verdict == Verdict::Chosen);
        let rejected = options.iter().any(|o| o.verdict == Verdict::Rejected);
        if !chosen || !rejected {
            drop("does not have both a chosen and a rejected option".into());
            continue;
        }
        let id = derive_change_id(session, "decision", &format!("{}:{quote}", turn.uuid));
        if kept.iter().any(|p| p.id == id) {
            drop("quotes the same words as another decision of the turn".into());
            continue;
        }
        let tn = nodes.get(d.turn - 1).cloned().unwrap_or_default();
        // The actions the choice governs: those after it in the turn, and
        // of those the ones that touch a file the model named, if any do.
        let touches = |files: &Vec<String>| {
            files.iter().any(|f| {
                d.files
                    .iter()
                    .any(|g| !g.is_empty() && (f.ends_with(g.as_str()) || g.ends_with(f.as_str())))
            })
        };
        let after: Vec<&(String, String, Vec<String>)> = tn
            .actions
            .iter()
            .filter(|(_, when, _)| when.as_str() >= at.as_str())
            .collect();
        let named: Vec<&(String, String, Vec<String>)> = after
            .iter()
            .copied()
            .filter(|(_, _, files)| touches(files))
            .collect();
        // A choice is often stated in the closing report, after the work it
        // governed. With nothing after the words, the actions of the turn
        // that touch a file the model named are the ones.
        let named_anywhere: Vec<&(String, String, Vec<String>)> = tn
            .actions
            .iter()
            .filter(|(_, _, files)| touches(files))
            .collect();
        let actions = if !named.is_empty() {
            named
        } else if after.is_empty() {
            named_anywhere
        } else {
            after
        };
        kept.push(Proposal {
            id,
            session: session.to_string(),
            turn_number: d.turn,
            at,
            title,
            quote,
            options,
            files: d
                .files
                .into_iter()
                .filter(|f| !f.trim().is_empty())
                .collect(),
            goal: tn.goal,
            turn_nodes: tn.all,
            actions: actions.into_iter().map(|(id, _, _)| id.clone()).collect(),
            state: State::Proposed,
        });
    }
    (kept, dropped)
}

#[derive(Debug, Default, Serialize)]
pub struct DeriveReport {
    pub session_id: String,
    pub turns: usize,
    pub model_calls: usize,
    /// New proposals, as their numbers in the proposals file.
    pub proposed: Vec<usize>,
    pub already_proposed: usize,
    pub dropped: Vec<Dropped>,
    pub accepted: Vec<i32>,
    /// True when the session was read before and has not grown.
    pub skipped: bool,
}

#[derive(Debug, Default, Clone)]
pub struct DeriveOptions {
    pub accept: bool,
    pub again: bool,
    pub model: Option<String>,
}

/// Read one transcript for decisions. The session is imported first, so
/// the goals and actions a decision hangs between exist.
pub fn derive(db: &Database, path: &Path, opts: &DeriveOptions) -> Result<DeriveReport, String> {
    let t = transcript::parse(path).map_err(|e| e.to_string())?;
    let root = db.project_root().map(Path::to_path_buf);
    let plan = transcript::plan(&t, root.as_deref()).map_err(|e| e.to_string())?;
    let turns = transcript::digest(&t);
    let store_path = Store::path(db);
    let mut store = Store::load(&store_path)?;
    let mut report = DeriveReport {
        session_id: plan.session_id.clone(),
        turns: turns.len(),
        ..Default::default()
    };
    if !opts.again && store.sessions.get(&plan.session_id) == Some(&turns.len()) {
        report.skipped = true;
        return Ok(report);
    }
    transcript::import(
        db,
        path,
        &transcript::ImportOptions {
            dry_run: false,
            hash_files: true,
        },
    )
    .map_err(|e| e.to_string())?;

    let nodes = turn_nodes(&plan);
    let mut raw = Vec::new();
    for prompt in prompts(&turns) {
        report.model_calls += 1;
        raw.extend(parse_reply(&ask(&prompt, opts.model.as_deref())?)?);
    }
    let (proposals, dropped) = validate(raw, &plan.session_id, &turns, &nodes);
    report.dropped = dropped;
    for p in proposals {
        if store.proposals.iter().any(|q| q.id == p.id) {
            report.already_proposed += 1;
            continue;
        }
        store.proposals.push(p);
        report.proposed.push(store.proposals.len());
    }
    store.sessions.insert(plan.session_id.clone(), turns.len());
    if opts.accept {
        for n in report.proposed.clone() {
            report
                .accepted
                .push(accept(db, &mut store.proposals[n - 1]).map_err(|e| e.to_string())?);
        }
    }
    store.save(&store_path)?;
    Ok(report)
}

/// The goal a proposal belongs under: the one the import derived for the
/// turn, or, when the agent logged the goal itself and the import attached
/// to that, the goal above anything else the import derived for the turn.
fn goal_of(db: &Database, p: &Proposal) -> Result<Option<i32>, DbError> {
    if let Some(id) = db.node_id_by_change_id(&p.goal)? {
        return Ok(Some(id));
    }
    let edges = db.get_all_edges()?;
    for change_id in &p.turn_nodes {
        let Some(child) = db.node_id_by_change_id(change_id)? else {
            continue;
        };
        for e in edges.iter().filter(|e| e.to_node_id == child) {
            if db
                .get_node(e.from_node_id)?
                .is_some_and(|n| n.node_type == "goal")
            {
                return Ok(Some(e.from_node_id));
            }
        }
    }
    Ok(None)
}

fn link(
    db: &Database,
    from: i32,
    to: i32,
    edge_type: &str,
    why: Option<&str>,
) -> Result<(), DbError> {
    if from != to && !db.edge_exists(from, to, edge_type)? {
        db.create_edge(from, to, edge_type, why.filter(|w| !w.is_empty()))?;
    }
    Ok(())
}

/// Write a proposal into the graph: goal -> options -> decision -> the
/// actions that followed. Doing it twice changes nothing.
pub fn accept(db: &Database, p: &mut Proposal) -> Result<i32, DbError> {
    let goal = goal_of(db, p)?;
    let evidence = json!({
        "kind": "inferred",
        "source": "transcript-model",
        "session": p.session,
        "quote": p.quote,
    });
    let decision = match db.node_id_by_change_id(&p.id)? {
        Some(id) => id,
        None => {
            let id = db.create_derived_node(
                &p.id,
                "decision",
                &p.title,
                Some(&format!("\"{}\"", p.quote)),
                Some(&json!({ "evidence": evidence })),
                &p.at,
            )?;
            db.update_node_status(id, "completed")?;
            id
        }
    };
    for (i, o) in p.options.iter().enumerate() {
        let change_id = derive_change_id(&p.session, "option", &format!("{}:{i}", p.id));
        let option = match db.node_id_by_change_id(&change_id)? {
            Some(id) => id,
            None => {
                let id = db.create_derived_node(
                    &change_id,
                    "option",
                    &o.title,
                    None,
                    Some(&json!({ "evidence": evidence })),
                    &p.at,
                )?;
                db.update_node_status(
                    id,
                    match o.verdict {
                        Verdict::Chosen => "completed",
                        Verdict::Rejected => "rejected",
                    },
                )?;
                id
            }
        };
        if let Some(goal) = goal {
            link(db, goal, option, "leads_to", None)?;
        }
        let edge_type = match o.verdict {
            Verdict::Chosen => "chosen",
            Verdict::Rejected => "rejected",
        };
        link(db, option, decision, edge_type, Some(&o.rationale))?;
    }
    for change_id in &p.actions {
        if let Some(action) = db.node_id_by_change_id(change_id)? {
            link(
                db,
                decision,
                action,
                "leads_to",
                Some("what followed the choice"),
            )?;
        }
    }
    p.state = State::Accepted;
    Ok(decision)
}

/// `[transcript] derive_decisions` in the project's config: `"propose"` (or
/// `true`) reads each session for decisions when it ends, `"accept"` also
/// writes them to the graph. Off unless set: it is a model call per session.
pub fn configured(project: &Path) -> Option<DeriveOptions> {
    let text = std::fs::read_to_string(project.join(".deciduous").join("config.toml")).ok()?;
    let doc = toml::from_str::<toml::Value>(&text).ok()?;
    let t = doc.get("transcript")?;
    let accept = match t.get("derive_decisions")? {
        toml::Value::Boolean(true) => false,
        toml::Value::String(s) if s == "propose" => false,
        toml::Value::String(s) if s == "accept" => true,
        _ => return None,
    };
    Some(DeriveOptions {
        accept,
        again: false,
        model: t
            .get("decisions_model")
            .and_then(|m| m.as_str())
            .map(str::to_string),
    })
}

/// One proposal as text.
pub fn render(number: usize, p: &Proposal) -> String {
    let state = match p.state {
        State::Proposed => "proposed",
        State::Accepted => "accepted",
        State::Rejected => "rejected",
    };
    let mut out = format!("#{number} [{state}] {}\n", p.title);
    out.push_str(&format!(
        "    session {} turn {}, {}\n",
        p.session.chars().take(8).collect::<String>(),
        p.turn_number,
        p.at.get(..10).unwrap_or(&p.at)
    ));
    out.push_str(&format!("    \"{}\"\n", p.quote));
    for o in &p.options {
        let label = match o.verdict {
            Verdict::Chosen => "chose   ",
            Verdict::Rejected => "rejected",
        };
        let why = if o.rationale.is_empty() {
            String::new()
        } else {
            format!("  ({})", o.rationale)
        };
        out.push_str(&format!("    {label} {}{why}\n", o.title));
    }
    if !p.actions.is_empty() {
        out.push_str(&format!(
            "    goes above {} action(s) of the turn\n",
            p.actions.len()
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn(uuid: &str, prompt: &str, said: &[(&str, &str)]) -> TurnDigest {
        TurnDigest {
            uuid: uuid.into(),
            timestamp: "2026-10-01T10:00:00Z".into(),
            prompt: prompt.into(),
            items: said
                .iter()
                .map(|(at, text)| DigestItem::Said {
                    timestamp: (*at).into(),
                    text: (*text).into(),
                })
                .collect(),
        }
    }

    fn raw(turn: usize, quote: &str, options: &[(&str, &str)]) -> RawDecision {
        RawDecision {
            turn,
            title: "Token buckets over IP buckets".into(),
            quote: quote.into(),
            options: options
                .iter()
                .map(|(title, verdict)| RawOption {
                    title: (*title).into(),
                    verdict: (*verdict).into(),
                    rationale: Some("an office is one IP".into()),
                })
                .collect(),
            files: vec!["src/limit.rs".into()],
        }
    }

    const BOTH: &[(&str, &str)] = &[("per token", "chosen"), ("per IP", "rejected")];

    fn turns() -> Vec<TurnDigest> {
        vec![turn(
            "u1",
            "rate limit the api",
            &[(
                "2026-10-01T10:00:05Z",
                "Two ways: per IP or per token.\nI'll limit per token, because an office\n  is one IP.",
            )],
        )]
    }

    fn nodes() -> Vec<TurnNodes> {
        vec![TurnNodes {
            goal: "g1".into(),
            all: vec!["a0".into(), "a1".into(), "a2".into()],
            actions: vec![
                (
                    "a0".into(),
                    "2026-10-01T10:00:01Z".into(),
                    vec!["src/limit.rs".into()],
                ),
                (
                    "a1".into(),
                    "2026-10-01T10:00:09Z".into(),
                    vec!["/p/src/limit.rs".into()],
                ),
                (
                    "a2".into(),
                    "2026-10-01T10:00:20Z".into(),
                    vec!["README.md".into()],
                ),
            ],
        }]
    }

    #[test]
    fn a_quote_found_in_the_turn_is_a_proposal_above_the_actions_that_followed() {
        // The model normalised the line break; the check does too.
        let q = "I'll limit per token, because an office is one IP.";
        let (kept, dropped) = validate(vec![raw(1, q, BOTH)], "s", &turns(), &nodes());
        assert!(dropped.is_empty(), "{dropped:?}");
        let p = &kept[0];
        assert_eq!(p.at, "2026-10-01T10:00:05Z");
        assert_eq!(p.goal, "g1");
        // a0 came before the words; a2 touches a file the model did not name.
        assert_eq!(p.actions, vec!["a1".to_string()]);
        assert_eq!(p.options[1].verdict, Verdict::Rejected);
        // Said in the closing report, after all the work: the actions that
        // touched the named file, whenever they ran.
        let mut late = turns();
        late[0].items = vec![DigestItem::Said {
            timestamp: "2026-10-01T10:05:00Z".into(),
            text: "Done. I limited per token and not per IP, since an office is one IP.".into(),
        }];
        let q2 = "I limited per token and not per IP, since an office is one IP.";
        let (kept, _) = validate(vec![raw(1, q2, BOTH)], "s", &late, &nodes());
        assert_eq!(kept[0].actions, vec!["a0".to_string(), "a1".to_string()]);
        // The same words again are the same proposal.
        let (again, _) = validate(vec![raw(1, q, BOTH)], "s", &turns(), &nodes());
        assert_eq!(again[0].id, p.id);
    }

    #[test]
    fn a_decision_the_record_does_not_say_is_dropped_and_named() {
        let cases = [
            (
                raw(1, "I decided to use per-token buckets instead.", BOTH),
                "is not in turn 1",
            ),
            (raw(1, "per token", BOTH), "under 20 characters"),
            (
                raw(4, "I'll limit per token, because an office", BOTH),
                "names turn 4",
            ),
            (
                raw(
                    1,
                    "I'll limit per token, because an office",
                    &[("per token", "chosen")],
                ),
                "both a chosen and a rejected",
            ),
            (
                raw(
                    1,
                    "I'll limit per token, because an office",
                    &[("a", "chosen"), ("b", "maybe")],
                ),
                "both a chosen and a rejected",
            ),
        ];
        for (decision, want) in cases {
            let (kept, dropped) = validate(vec![decision], "s", &turns(), &nodes());
            assert!(kept.is_empty(), "{want}");
            assert!(
                dropped[0].reason.contains(want),
                "{:?} / {want}",
                dropped[0]
            );
            assert_eq!(dropped[0].title, "Token buckets over IP buckets");
        }
    }

    #[test]
    fn a_quote_from_the_prompt_is_dated_by_the_prompt() {
        let t = vec![turn(
            "u1",
            "use sqlite here, not postgres: one file to back up",
            &[],
        )];
        let d = raw(
            1,
            "use sqlite here, not postgres",
            &[("sqlite", "chosen"), ("postgres", "rejected")],
        );
        let (kept, _) = validate(vec![d], "s", &t, &[]);
        assert_eq!(kept[0].at, "2026-10-01T10:00:00Z");
        assert!(kept[0].actions.is_empty());
    }

    #[test]
    fn the_reply_is_json_or_an_error() {
        let fenced = "```json\n{\"decisions\":[{\"turn\":1,\"title\":\"t\",\"quote\":\"q\",\"options\":[]}]}\n```";
        assert_eq!(parse_reply(fenced).unwrap().len(), 1);
        assert_eq!(parse_reply("{\"decisions\":[]}").unwrap().len(), 0);
        assert!(parse_reply("There were no decisions.").is_err());
        assert!(parse_reply("{\"answer\":[]}")
            .unwrap_err()
            .contains("no `decisions` array"));
    }

    #[test]
    fn a_long_session_is_several_prompts_split_between_turns() {
        let long = "x".repeat(SAID_CHARS * 3);
        let many: Vec<TurnDigest> = (0..80)
            .map(|i| turn(&format!("u{i}"), "p", &[("t", long.as_str())]))
            .collect();
        let ps = prompts(&many);
        assert!(ps.len() > 1);
        assert!(ps.iter().all(|p| p.len() <= CHUNK_CHARS + SAID_CHARS + 200));
        let turns_seen: usize = ps.iter().map(|p| p.matches("=== TURN ").count()).sum();
        assert_eq!(turns_seen, 80);
        // A text too long to send whole keeps both ends.
        assert!(ps[0].contains(" [...] "));
    }
}
