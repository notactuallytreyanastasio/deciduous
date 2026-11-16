//! Build the graph from a Claude Code transcript.
//!
//! An agent that logs its own decisions as it works is an agent that can
//! forget to. The transcript cannot forget: every prompt the user typed, every
//! file the agent edited, every command it ran and what came back are in the
//! session's JSONL under `~/.claude/projects/<slug>/`. `deciduous transcript
//! import` reads that file and derives the graph from it, so a session that
//! logged nothing still leaves a record of what was asked, what was changed,
//! and what the tests said.
//!
//! What a node is derived from decides how much it can be trusted, and each
//! node says which in `metadata.evidence`:
//!
//! | node                       | from                                  | evidence kind |
//! |----------------------------|---------------------------------------|---------------|
//! | goal                       | the user's prompt, verbatim           | proven        |
//! | action (edit run)          | Edit / Write / MultiEdit tool calls   | proven        |
//! | action (commit)            | a `git commit` and its `[branch sha]` | proven        |
//! | outcome (command)          | a test / build / lint run's exit      | proven        |
//! | outcome (PR)               | `gh pr create` printing the URL       | proven        |
//! | outcome / observation      | the assistant's closing text          | inferred      |
//!
//! "Proven" means the transcript recorded the thing happening: a tool call
//! with its result. "Inferred" means the assistant said so. The assistant's
//! text before an edit run is kept as that action's description, so the
//! stated intent sits next to the proof of what was done.
//!
//! The evidence trail does not need a filesystem watcher: the transcript
//! already records every `Read` the agent made. Each action lists the files
//! consulted since the previous action in `metadata.consulted`. Files the
//! action touched are hashed at import time (`git hash-object`, so the id is
//! the blob git would store; sha256 without git) into `metadata.file_hashes`.
//!
//! Change ids are derived from the session id and the entry uuid, so the same
//! transcript imported twice, or on two machines, produces the same nodes and
//! the second import changes nothing.
//!
//! Decisions and options are not derived: whether the assistant chose one
//! approach over another is a judgement about prose, not a fact in a tool
//! result. That pass belongs to an agent reading the imported graph.

use crate::db::{Database, DbError};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Edit tools: a call to one of these changed a file.
const EDIT_TOOLS: &[&str] = &["Edit", "MultiEdit", "Write", "NotebookEdit"];

/// Longest title derived from free text.
const TITLE_LEN: usize = 100;

#[derive(Debug)]
pub enum TranscriptError {
    Io(PathBuf, std::io::Error),
    /// The file parsed but held no user prompt.
    Empty(PathBuf),
    /// No transcript directory: Claude Code has not run in this project, or
    /// it ran from another path.
    NoDirectory(PathBuf),
    Db(DbError),
}

impl std::fmt::Display for TranscriptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TranscriptError::Io(p, e) => write!(f, "{}: {}", p.display(), e),
            TranscriptError::Empty(p) => write!(
                f,
                "{}: no user prompt in this transcript (a subagent file, or a session that \
                 never got a message); nothing to derive",
                p.display()
            ),
            TranscriptError::NoDirectory(p) => write!(
                f,
                "no transcripts at {}: Claude Code has not run in this project directory, or \
                 it ran from another path (pass --dir)",
                p.display()
            ),
            TranscriptError::Db(e) => write!(f, "{}", e),
        }
    }
}

impl std::error::Error for TranscriptError {}

impl From<DbError> for TranscriptError {
    fn from(e: DbError) -> Self {
        TranscriptError::Db(e)
    }
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// One content block of a user or assistant message.
#[derive(Debug, Clone)]
enum Block {
    Text(String),
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
        is_error: bool,
    },
}

/// A `user` or `assistant` line of the transcript.
#[derive(Debug, Clone)]
struct Entry {
    role: String,
    uuid: String,
    timestamp: String,
    git_branch: Option<String>,
    is_meta: bool,
    is_sidechain: bool,
    blocks: Vec<Block>,
}

/// What a tool call got back.
#[derive(Debug, Clone, Default)]
struct ToolResult {
    content: String,
    is_error: bool,
    structured: Option<Value>,
}

/// A parsed transcript: the session's lines that carry a message, plus the
/// title Claude Code gave the session.
#[derive(Debug)]
pub struct Transcript {
    pub path: PathBuf,
    pub session_id: String,
    pub title: Option<String>,
    entries: Vec<Entry>,
    results: HashMap<String, ToolResult>,
    /// Lines that were not JSON. Reported, never fatal: Claude Code appends
    /// while a session runs and the last line can be half written.
    pub unreadable_lines: usize,
}

fn text_of(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(items) => items
            .iter()
            .filter_map(|b| match b.get("type").and_then(Value::as_str) {
                Some("text") => b.get("text").and_then(Value::as_str).map(str::to_string),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn parse_blocks(content: &Value) -> Vec<Block> {
    match content {
        Value::String(s) => vec![Block::Text(s.clone())],
        Value::Array(items) => items
            .iter()
            .filter_map(|b| match b.get("type").and_then(Value::as_str) {
                Some("text") => b
                    .get("text")
                    .and_then(Value::as_str)
                    .map(|t| Block::Text(t.to_string())),
                Some("tool_use") => Some(Block::ToolUse {
                    id: b
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    name: b
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    input: b.get("input").cloned().unwrap_or(Value::Null),
                }),
                Some("tool_result") => Some(Block::ToolResult {
                    tool_use_id: b
                        .get("tool_use_id")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    content: match b.get("content") {
                        Some(c) => text_of(c),
                        None => String::new(),
                    },
                    is_error: b.get("is_error").and_then(Value::as_bool).unwrap_or(false),
                }),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn str_field(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}

/// Read a transcript file. Lines whose `type` is not `user` or `assistant`
/// (api request shapes, mode changes, cost state) are skipped, except
/// `ai-title`, which names the session.
pub fn parse(path: &Path) -> Result<Transcript, TranscriptError> {
    let file = fs::File::open(path).map_err(|e| TranscriptError::Io(path.to_path_buf(), e))?;
    let reader = BufReader::new(file);
    let mut entries = Vec::new();
    let mut results = HashMap::new();
    let mut title = None;
    let mut session_id = None;
    let mut unreadable_lines = 0;

    for line in reader.lines() {
        let line = line.map_err(|e| TranscriptError::Io(path.to_path_buf(), e))?;
        if line.trim().is_empty() {
            continue;
        }
        let v: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => {
                unreadable_lines += 1;
                continue;
            }
        };
        let kind = v.get("type").and_then(Value::as_str).unwrap_or("");
        if session_id.is_none() {
            session_id = str_field(&v, "sessionId");
        }
        match kind {
            "ai-title" => {
                if let Some(t) = str_field(&v, "aiTitle") {
                    title = Some(t);
                }
            }
            "user" | "assistant" => {
                let Some(message) = v.get("message") else {
                    continue;
                };
                let blocks = parse_blocks(message.get("content").unwrap_or(&Value::Null));
                let structured_result = v.get("toolUseResult").cloned();
                for b in &blocks {
                    if let Block::ToolResult {
                        tool_use_id,
                        content,
                        is_error,
                    } = b
                    {
                        results.insert(
                            tool_use_id.clone(),
                            ToolResult {
                                content: content.clone(),
                                is_error: *is_error,
                                structured: structured_result.clone(),
                            },
                        );
                    }
                }
                entries.push(Entry {
                    role: kind.to_string(),
                    uuid: str_field(&v, "uuid").unwrap_or_default(),
                    timestamp: str_field(&v, "timestamp").unwrap_or_default(),
                    git_branch: str_field(&v, "gitBranch"),
                    is_meta: v.get("isMeta").and_then(Value::as_bool).unwrap_or(false)
                        || v.get("isCompactSummary")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                    is_sidechain: v
                        .get("isSidechain")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    blocks,
                });
            }
            _ => {}
        }
    }

    let session_id = session_id
        .or_else(|| {
            path.file_stem()
                .and_then(|s| s.to_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| "unknown-session".to_string());

    Ok(Transcript {
        path: path.to_path_buf(),
        session_id,
        title,
        entries,
        results,
        unreadable_lines,
    })
}

// ---------------------------------------------------------------------------
// Turns
// ---------------------------------------------------------------------------

/// Something the assistant did inside a turn, in order.
#[derive(Debug, Clone)]
enum Step {
    Text {
        text: String,
        uuid: String,
        timestamp: String,
    },
    Tool {
        id: String,
        name: String,
        input: Value,
        uuid: String,
        timestamp: String,
        branch: Option<String>,
        result: Option<ToolResult>,
    },
}

/// One user prompt and everything the assistant did in answer to it.
#[derive(Debug, Clone)]
struct Turn {
    prompt: String,
    /// Which prompt of the session this is, from 1.
    number: usize,
    /// Prompts after this one that only said to go on ("ok do it", "sure"):
    /// what they set off is this prompt's work. Filled by [`joined`].
    follow_ups: Vec<String>,
    uuid: String,
    timestamp: String,
    branch: Option<String>,
    steps: Vec<Step>,
}

/// Text Claude Code puts in a `user` message that the user did not type.
fn is_synthetic_prompt(text: &str) -> bool {
    let t = text.trim_start();
    t.is_empty()
        || t.starts_with("<local-command-stdout>")
        || t.starts_with("<local-command-caveat>")
        || t.starts_with("<command-name>")
        || t.starts_with("<command-message>")
        || t.starts_with("<task-notification>")
        || t.starts_with("<bash-input>")
        || t.starts_with("<bash-stdout>")
        || t.starts_with("<bash-stderr>")
        || t.starts_with("<system-reminder>")
        || t.starts_with("[Request interrupted by user")
        || t.starts_with(
            "Caveat: The messages below were generated by the user while running local commands",
        )
}

fn human_prompt(entry: &Entry) -> Option<String> {
    if entry.role != "user" || entry.is_meta || entry.is_sidechain {
        return None;
    }
    if entry
        .blocks
        .iter()
        .any(|b| matches!(b, Block::ToolResult { .. }))
    {
        return None;
    }
    let text = entry
        .blocks
        .iter()
        .filter_map(|b| match b {
            Block::Text(t) => Some(t.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    if is_synthetic_prompt(&text) {
        return None;
    }
    Some(text.trim().to_string())
}

fn turns(t: &Transcript) -> Vec<Turn> {
    let mut out: Vec<Turn> = Vec::new();
    for e in &t.entries {
        if e.is_sidechain {
            continue;
        }
        if let Some(prompt) = human_prompt(e) {
            out.push(Turn {
                prompt,
                number: out.len() + 1,
                follow_ups: Vec::new(),
                uuid: e.uuid.clone(),
                timestamp: e.timestamp.clone(),
                branch: e.git_branch.clone(),
                steps: Vec::new(),
            });
            continue;
        }
        if e.role != "assistant" {
            continue;
        }
        let Some(turn) = out.last_mut() else {
            continue;
        };
        for b in &e.blocks {
            match b {
                Block::Text(text) => {
                    if !text.trim().is_empty() {
                        turn.steps.push(Step::Text {
                            text: text.clone(),
                            uuid: e.uuid.clone(),
                            timestamp: e.timestamp.clone(),
                        });
                    }
                }
                Block::ToolUse { id, name, input } => {
                    turn.steps.push(Step::Tool {
                        id: id.clone(),
                        name: name.clone(),
                        input: input.clone(),
                        uuid: e.uuid.clone(),
                        timestamp: e.timestamp.clone(),
                        branch: e.git_branch.clone(),
                        result: t.results.get(id).cloned(),
                    });
                }
                Block::ToolResult { .. } => {}
            }
        }
    }
    out
}

/// Words a prompt can be made of and still say nothing but "go on".
const GO_AHEAD_WORDS: &[&str] = &[
    "ok", "okay", "k", "yes", "yeah", "yep", "yup", "y", "sure", "please", "pls", "do", "it",
    "that", "this", "go", "ahead", "for", "lets", "let's", "proceed", "continue", "retry", "again",
    "try", "sounds", "good", "great", "fine", "lgtm", "ship", "then", "now", "and", "so",
    "alright", "cool", "perfect", "agreed", "approved", "right", "on", "with", "the", "plan",
    "both", "all", "of", "them",
];

/// "ok do it", "sure", "retry": a prompt of at most six words, every one of
/// them from [`GO_AHEAD_WORDS`]. It names nothing, so it cannot be a goal:
/// what it asks for is in the prompt before it.
///
/// A closed word list and not a length limit, because "add dark mode" is
/// three words and a goal. The price is that "go dor it" and "1, 4, 2 in
/// that order" are not recognised and stay prompts of their own.
fn is_go_ahead(prompt: &str) -> bool {
    let lower = prompt.to_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !(c.is_alphanumeric() || c == '\''))
        .filter(|w| !w.is_empty())
        .collect();
    (1..=6).contains(&words.len()) && words.iter().all(|w| GO_AHEAD_WORDS.contains(w))
}

/// Fold each go-ahead into the turn before it: its steps follow that turn's,
/// and its words are kept in `follow_ups`. A session that opens with one
/// keeps it as a turn, there being nothing before it.
fn joined(turns: Vec<Turn>, skipped: &mut Skipped) -> Vec<Turn> {
    let mut out: Vec<Turn> = Vec::new();
    for turn in turns {
        match out.last_mut() {
            Some(before) if is_go_ahead(&turn.prompt) => {
                before.follow_ups.push(turn.prompt);
                before.steps.extend(turn.steps);
                skipped.go_aheads += 1;
            }
            _ => out.push(turn),
        }
    }
    out
}

/// What the agent wrote to the graph itself during a turn, read from the
/// turn's own tool calls.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Logged {
    /// Local ids of goals it created, from `Created node 12 (type: goal, ...`
    /// in a command's output. An MCP write to a server does not give one.
    pub goal_ids: Vec<i32>,
    pub goal: bool,
    pub action: bool,
    pub outcome: bool,
}

impl Logged {
    fn saw(&mut self, kind: &str) {
        match kind {
            "goal" => self.goal = true,
            "action" => self.action = true,
            "outcome" => self.outcome = true,
            _ => {}
        }
    }
}

/// `Created node 12 (type: goal, title: ...` -> (12, "goal").
fn created_line(line: &str) -> Option<(i32, &str)> {
    let rest = &line[line.find("Created node ")? + "Created node ".len()..];
    let (id, rest) = rest.split_once(" (type: ")?;
    let kind = rest.split([',', ')']).next()?;
    Some((id.trim().parse().ok()?, kind))
}

fn logged_in(turn: &Turn) -> Logged {
    let mut logged = Logged::default();
    for step in &turn.steps {
        let Step::Tool {
            name,
            input,
            result,
            ..
        } = step
        else {
            continue;
        };
        let failed = result.as_ref().is_some_and(|r| r.is_error);
        if let Some(tool) = name.strip_prefix("mcp__deciduous__") {
            if failed {
                continue;
            }
            match tool {
                "add_node" => {
                    if let Some(kind) = input.get("node_type").and_then(Value::as_str) {
                        logged.saw(kind);
                    }
                }
                "capture_conversation_turn" => {
                    for kind in ["goal", "action", "outcome"] {
                        let has = |k: &str| input.get(k).is_some_and(|v| !v.is_null());
                        if has(kind) || has(&format!("{kind}s")) {
                            logged.saw(kind);
                        }
                    }
                }
                _ => {}
            }
            continue;
        }
        if name != "Bash" {
            continue;
        }
        let cmd = input.get("command").and_then(Value::as_str).unwrap_or("");
        if !cmd.contains("deciduous") {
            continue;
        }
        // The output is the proof, however the command was spelled (a
        // shell function, a path to the binary). A command whose output was
        // cut or thrown away is read from its words instead.
        let output = result.as_ref().map(bash_output).unwrap_or_default();
        let mut read_any = false;
        for (id, kind) in output.lines().filter_map(created_line) {
            read_any = true;
            logged.saw(kind);
            if kind == "goal" {
                logged.goal_ids.push(id);
            }
        }
        if !read_any && !failed {
            for kind in ["goal", "action", "outcome"] {
                if cmd.contains(&format!(" add {kind} ")) {
                    logged.saw(kind);
                }
            }
        }
    }
    logged
}

// ---------------------------------------------------------------------------
// The digest: a turn as a reader needs it
// ---------------------------------------------------------------------------

/// One thing that happened in a turn.
#[derive(Debug, Clone, PartialEq)]
pub enum DigestItem {
    /// The assistant said this.
    Said { timestamp: String, text: String },
    /// The assistant called a tool: its name and the path or command.
    Did(String),
}

/// A user prompt and what followed it, without tool results. This is what
/// `deciduous transcript decisions` shows a model: choices are made in
/// prose, and the results are most of a transcript's bytes.
#[derive(Debug, Clone)]
pub struct TurnDigest {
    /// The uuid of the prompt's entry: what the turn's goal is derived from.
    pub uuid: String,
    pub timestamp: String,
    pub prompt: String,
    pub items: Vec<DigestItem>,
}

pub fn digest(t: &Transcript) -> Vec<TurnDigest> {
    turns(t)
        .into_iter()
        .map(|turn| TurnDigest {
            uuid: turn.uuid,
            timestamp: turn.timestamp,
            prompt: turn.prompt,
            items: turn
                .steps
                .into_iter()
                .filter_map(|step| match step {
                    Step::Text {
                        text, timestamp, ..
                    } => (!text.trim().is_empty()).then_some(DigestItem::Said { timestamp, text }),
                    Step::Tool {
                        name,
                        input,
                        result,
                        ..
                    } => {
                        let what = [
                            "file_path",
                            "notebook_path",
                            "command",
                            "pattern",
                            "description",
                        ]
                        .iter()
                        .find_map(|k| input.get(*k).and_then(Value::as_str))
                        .unwrap_or("");
                        let what: String = what
                            .lines()
                            .next()
                            .unwrap_or("")
                            .chars()
                            .take(160)
                            .collect();
                        let failed = result.is_some_and(|r| r.is_error);
                        Some(DigestItem::Did(format!(
                            "{name} {what}{}",
                            if failed { " (failed)" } else { "" }
                        )))
                    }
                })
                .collect(),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Derivation
// ---------------------------------------------------------------------------

/// How a derived node is backed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Evidence {
    /// The transcript recorded it happening: a tool call and its result, or
    /// the user's own words.
    Proven,
    /// The assistant said so.
    Inferred,
}

/// A node the import will create.
#[derive(Debug, Clone, Serialize)]
pub struct DerivedNode {
    pub change_id: String,
    pub node_type: String,
    pub title: String,
    pub description: Option<String>,
    pub created_at: String,
    pub evidence: Evidence,
    pub metadata: Value,
    /// Index into the plan's nodes of the parent, if any.
    pub parent: Option<usize>,
    pub edge_rationale: Option<String>,
}

/// Everything an import would do, before it does it.
#[derive(Debug, Serialize)]
pub struct Plan {
    pub session_id: String,
    pub title: String,
    pub turns: usize,
    pub nodes: Vec<DerivedNode>,
    pub skipped: Skipped,
    /// For a goal the agent logged itself in that turn: its index in
    /// `nodes`, and what the transcript shows of the logging.
    #[serde(skip)]
    pub logged_goals: BTreeMap<usize, Logged>,
}

/// Tool calls the import saw and did not turn into nodes.
#[derive(Debug, Default, Serialize)]
pub struct Skipped {
    /// Subagent launches: their transcripts are separate files.
    pub subagents: usize,
    /// Writes the agent already made to this graph through MCP.
    pub deciduous_writes: usize,
    /// Reads, searches, and commands that are neither tests nor commits.
    pub other_tools: usize,
    /// Prompts whose turn changed nothing: no edit, no commit, no test or
    /// build run. A question and its answer are not a goal.
    pub idle_prompts: usize,
    /// Prompts that only said to go on: their work is under the prompt
    /// before them.
    pub go_aheads: usize,
    /// Actions and outcomes not derived because the agent logged its own
    /// in that turn.
    pub logged_by_agent: usize,
}

/// A change id that is a function of (session, kind, anchor): hyphenated
/// 128 bits of sha256 with the version nibble set to 8 (RFC 9562 custom).
pub fn derive_change_id(session_id: &str, kind: &str, anchor: &str) -> String {
    let mut h = Sha256::new();
    h.update(b"deciduous.transcript\0");
    h.update(session_id.as_bytes());
    h.update(b"\0");
    h.update(kind.as_bytes());
    h.update(b"\0");
    h.update(anchor.as_bytes());
    let d = h.finalize();
    let mut b = [0u8; 16];
    b.copy_from_slice(&d[..16]);
    b[6] = (b[6] & 0x0f) | 0x80;
    b[8] = (b[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7], b[8], b[9], b[10], b[11], b[12], b[13],
        b[14], b[15]
    )
}

/// First meaningful line of some text, without markdown furniture, cut to
/// `TITLE_LEN` on a word boundary.
pub fn title_from(text: &str) -> String {
    let line = text
        .lines()
        .map(|l| {
            l.trim()
                .trim_start_matches(['#', '*', '-', '>', ' '])
                .trim_end_matches(['*', ':', ' '])
        })
        .find(|l| !l.is_empty() && !l.starts_with('<') && !l.starts_with("```"))
        .unwrap_or("");
    let line = line.replace("**", "").replace('`', "");
    let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= TITLE_LEN {
        return line;
    }
    let mut cut = String::new();
    for word in line.split(' ') {
        if cut.chars().count() + word.chars().count() + 1 > TITLE_LEN - 1 {
            break;
        }
        if !cut.is_empty() {
            cut.push(' ');
        }
        cut.push_str(word);
    }
    if cut.is_empty() {
        cut = line.chars().take(TITLE_LEN - 1).collect();
    }
    cut.push('…');
    cut
}

/// A path as the graph should name it: relative to the project when it is
/// inside it, else as the tool saw it.
fn display_path(path: &str, root: Option<&Path>) -> String {
    if let Some(root) = root {
        if let Ok(rel) = Path::new(path).strip_prefix(root) {
            let s = rel.to_string_lossy();
            if !s.is_empty() {
                return s.to_string();
            }
        }
    }
    path.to_string()
}

fn edit_path(input: &Value) -> Option<String> {
    input
        .get("file_path")
        .or_else(|| input.get("notebook_path"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// Matches `[main abc1234] subject` at the start of a line of commit output.
fn commit_in_output(output: &str) -> Option<(String, String)> {
    for line in output.lines() {
        let line = line.trim();
        let rest = line.strip_prefix('[')?;
        let close = rest.find(']')?;
        let head = &rest[..close];
        let subject = rest[close + 1..].trim();
        let mut parts = head.split_whitespace().collect::<Vec<_>>();
        let sha = parts.pop()?;
        if sha.len() >= 7 && sha.chars().all(|c| c.is_ascii_hexdigit()) && !parts.is_empty() {
            return Some((sha.to_string(), subject.to_string()));
        }
    }
    None
}

/// A line of `git log --oneline` form: `233e07a subject`.
fn oneline_in_output(output: &str) -> Option<(String, String)> {
    for line in output.lines() {
        let Some((sha, subject)) = line.trim().split_once(' ') else {
            continue;
        };
        if (7..=40).contains(&sha.len())
            && sha.chars().all(|c| c.is_ascii_hexdigit())
            && !subject.trim().is_empty()
        {
            return Some((sha.to_string(), subject.trim().to_string()));
        }
    }
    None
}

/// The subject a `git commit -m "..."` or a `-F -` heredoc gave, when the
/// output shows nothing (`-q`).
fn commit_subject_in_command(cmd: &str) -> Option<String> {
    let idx = cmd.find(" commit")?;
    let rest = &cmd[idx..];
    if let Some(m) = rest.find(" -m ") {
        let after = rest[m + 4..].trim_start();
        let quote = after.chars().next()?;
        if quote == '"' || quote == '\'' {
            let body = &after[1..];
            let end = body.find(quote)?;
            return Some(title_from(&body[..end]));
        }
        return after.split_whitespace().next().map(str::to_string);
    }
    // `git commit -F - <<'EOF'`: the first line after the heredoc opener.
    let eof = rest.find("<<")?;
    let after = &rest[eof..];
    let mut lines = after.lines();
    lines.next()?;
    lines.map(str::trim).find(|l| !l.is_empty()).map(title_from)
}

fn is_commit_command(cmd: &str) -> bool {
    cmd.split(['|', ';', '&', '\n'])
        .any(|seg| seg.split_whitespace().any(|w| w == "git") && seg.contains(" commit"))
}

/// The build / test / lint invocation in a shell command, if there is one:
/// the phrase that names it, for the outcome's title.
fn check_command(cmd: &str) -> Option<String> {
    const PHRASES: &[&str] = &[
        "cargo test",
        "cargo nextest",
        "cargo build",
        "cargo clippy",
        "cargo check",
        "cargo fmt --check",
        "cargo fmt -- --check",
        "npm test",
        "npm run test",
        "npm run build",
        "npm run lint",
        "npm run check",
        "npm run typecheck",
        "pnpm test",
        "pnpm build",
        "yarn test",
        "yarn build",
        "bun test",
        "npx vitest",
        "npx jest",
        "npx tsc",
        "npx eslint",
        "npx playwright test",
        "pytest",
        "python -m pytest",
        "go test",
        "go build",
        "go vet",
        "mix test",
        "mix compile",
        "make test",
        "make check",
        "make build",
        "make",
        "gradle test",
        "gradlew test",
        "mvn test",
        "dotnet test",
        "dotnet build",
        "swift test",
        "zig build",
    ];
    for seg in cmd.split(['|', ';', '&', '\n']) {
        // `cd dir && cargo test` arrives as its own segment. `timeout 60
        // cargo test`, `RUST_LOG=x cargo test` and `env X=1 cargo test` do
        // not, so wrappers and assignments before the command are dropped.
        // The phrase must then start the segment: `grep cargo test src`
        // is not a test run.
        let words: Vec<&str> = seg
            .split_whitespace()
            .skip_while(|w| {
                w.contains('=')
                    || w.chars().all(|c| c.is_ascii_digit())
                    || matches!(*w, "timeout" | "env" | "time" | "nice" | "sudo" | "exec")
            })
            .collect();
        for phrase in PHRASES {
            let pw: Vec<&str> = phrase.split_whitespace().collect();
            if words.starts_with(&pw) {
                if *phrase == "make" && words.len() > 1 && !words[1].starts_with('-') {
                    return Some(format!("make {}", words[1]));
                }
                return Some(phrase.to_string());
            }
        }
    }
    None
}

/// `cargo test` prints one `test result:` line per binary; the sum is what
/// a reader wants. Other runners: the last few lines, which is where they
/// put their summary.
fn summarize_output(output: &str, failed: bool) -> Option<String> {
    let mut passed = 0u64;
    let mut fails = 0u64;
    let mut seen = false;
    for line in output.lines() {
        if let Some(rest) = line.trim().strip_prefix("test result:") {
            seen = true;
            for part in rest.split([';', '.']) {
                let mut it = part.split_whitespace();
                if let (Some(n), Some(what)) = (it.next(), it.next()) {
                    if let Ok(n) = n.parse::<u64>() {
                        match what {
                            "passed" => passed += n,
                            "failed" => fails += n,
                            _ => {}
                        }
                    }
                }
            }
        }
    }
    if seen {
        return Some(format!("{passed} passed, {fails} failed"));
    }
    let tail: Vec<&str> = output
        .lines()
        .map(str::trim_end)
        .filter(|l| !l.is_empty())
        .collect();
    if tail.is_empty() {
        return None;
    }
    let keep = if failed { 12 } else { 4 };
    let start = tail.len().saturating_sub(keep);
    Some(tail[start..].join("\n"))
}

/// The URL of a pull request in some output: `.../pull/<number>`, not the
/// `.../pull/new/<branch>` hint git prints after a push.
fn pr_url(output: &str) -> Option<String> {
    output
        .split_whitespace()
        .map(|w| w.trim_end_matches(['.', ',', ')']))
        .find(|w| {
            w.starts_with("https://")
                && w.rsplit_once("/pull/")
                    .is_some_and(|(_, n)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
        })
        .map(str::to_string)
}

/// Per-import state that is the same for every turn.
struct Ctx<'a> {
    session_id: &'a str,
    root: Option<&'a Path>,
    /// Absolute path of every file an action touched, for hashing.
    touched: BTreeSet<String>,
}

/// Build the plan: every node and edge an import of this transcript makes.
pub fn plan(t: &Transcript, root: Option<&Path>) -> Result<Plan, TranscriptError> {
    let turns = turns(t);
    if turns.is_empty() {
        return Err(TranscriptError::Empty(t.path.clone()));
    }
    let prompts = turns.len();
    let mut skipped = Skipped::default();
    let turns = joined(turns, &mut skipped);
    let mut logged_goals = BTreeMap::new();
    let mut ctx = Ctx {
        session_id: &t.session_id,
        root,
        touched: BTreeSet::new(),
    };
    let mut nodes: Vec<DerivedNode> = Vec::new();

    let session_title = t
        .title
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| title_from(&turns[0].prompt));

    let mut session_root: Option<usize> = None;
    for turn in &turns {
        let i = turn.number - 1;
        let start = nodes.len();
        let goal_title = if session_root.is_none() {
            session_title.clone()
        } else {
            title_from(&turn.prompt)
        };
        let goal_title = if goal_title.is_empty() {
            format!("Prompt {}", i + 1)
        } else {
            goal_title
        };
        let mut meta = json!({
            "prompt": turn.prompt,
            "evidence": evidence_json(Evidence::Proven, &ctx, &[turn.uuid.as_str()], &[]),
        });
        if let Some(b) = &turn.branch {
            meta["branch"] = json!(b);
        }
        if session_root.is_none() {
            meta["session"] = json!({
                "id": t.session_id,
                "turns": prompts,
                "transcript": t.path.to_string_lossy(),
            });
        }
        if !turn.follow_ups.is_empty() {
            meta["follow_ups"] = json!(turn.follow_ups);
        }
        let goal = push(
            &mut nodes,
            DerivedNode {
                change_id: derive_change_id(ctx.session_id, "goal", &turn.uuid),
                node_type: "goal".into(),
                title: goal_title,
                description: None,
                created_at: turn.timestamp.clone(),
                evidence: Evidence::Proven,
                metadata: meta,
                parent: session_root,
                edge_rationale: session_root.map(|_| format!("prompt {} of the session", i + 1)),
            },
        );
        let was_root = session_root.is_none();
        if was_root {
            session_root = Some(goal);
        }
        derive_turn(turn, goal, &mut ctx, &mut nodes, &mut skipped);
        // "why is my internet so slow" is a prompt and not a goal. A turn
        // is kept when the transcript proves it did something: an action
        // (edits, a commit) or a test or build run with its exit. The
        // assistant's closing words alone are not that.
        let worked = nodes[start + 1..]
            .iter()
            .any(|n| n.node_type == "action" || n.evidence == Evidence::Proven);
        if !worked {
            nodes.truncate(start);
            skipped.idle_prompts += 1;
            if was_root {
                session_root = None;
            }
            continue;
        }
        // The import is the backstop. Where the agent logged its own
        // actions in this turn, a derived "Edited src/db.rs" beside its
        // "Add the status column" is the same work twice, and the same for
        // outcomes. What it did not log is still derived.
        let logged = logged_in(turn);
        if logged.action || logged.outcome {
            skipped.logged_by_agent += drop_kinds(&mut nodes, start, &logged);
        }
        if logged.goal {
            logged_goals.insert(start, logged);
        }
    }

    Ok(Plan {
        session_id: t.session_id.clone(),
        title: session_title,
        turns: prompts,
        nodes,
        skipped,
        logged_goals,
    })
}

/// Remove the turn's derived actions and outcomes of the kinds the agent
/// logged. The turn's goal is `nodes[goal]`; what hung under a removed node
/// hangs under the goal. Returns how many were removed.
fn drop_kinds(nodes: &mut Vec<DerivedNode>, goal: usize, logged: &Logged) -> usize {
    let tail = nodes.split_off(goal + 1);
    let gone = |n: &DerivedNode| {
        (n.node_type == "action" && logged.action) || (n.node_type == "outcome" && logged.outcome)
    };
    // Old index -> new index, for the ones that stay.
    let mut moved: BTreeMap<usize, usize> = BTreeMap::new();
    let mut removed = 0;
    for (offset, mut n) in tail.into_iter().enumerate() {
        if gone(&n) {
            removed += 1;
            continue;
        }
        if let Some(parent) = n.parent.filter(|p| *p > goal) {
            n.parent = Some(moved.get(&parent).copied().unwrap_or(goal));
        }
        moved.insert(goal + 1 + offset, nodes.len());
        nodes.push(n);
    }
    removed
}

fn push(nodes: &mut Vec<DerivedNode>, n: DerivedNode) -> usize {
    nodes.push(n);
    nodes.len() - 1
}

fn evidence_json(kind: Evidence, ctx: &Ctx, uuids: &[&str], tool_use_ids: &[&str]) -> Value {
    let mut v = json!({
        "kind": kind,
        "source": "transcript",
        "session": ctx.session_id,
        "uuids": uuids,
    });
    if !tool_use_ids.is_empty() {
        v["tool_use_ids"] = json!(tool_use_ids);
    }
    v
}

/// An edit tool call waiting to be folded into an action.
struct PendingEdit {
    tool_use_id: String,
    uuid: String,
    tool: String,
    path: String,
    timestamp: String,
    branch: Option<String>,
    error: Option<String>,
}

fn derive_turn(
    turn: &Turn,
    goal: usize,
    ctx: &mut Ctx,
    nodes: &mut Vec<DerivedNode>,
    skipped: &mut Skipped,
) {
    let mut narration: Vec<String> = Vec::new();
    let mut consulted: Vec<String> = Vec::new();
    let mut edits: Vec<PendingEdit> = Vec::new();
    let mut last_action: Option<usize> = None;
    let mut derived_any = false;
    let mut had_tools = false;

    for step in &turn.steps {
        match step {
            Step::Text { text, .. } => {
                // Text after an edit run ends the run: what follows is about
                // something else.
                if !edits.is_empty() {
                    last_action = Some(flush_edits(
                        &mut edits,
                        &mut narration,
                        &mut consulted,
                        goal,
                        ctx,
                        nodes,
                    ));
                    derived_any = true;
                }
                narration.push(text.trim().to_string());
            }
            Step::Tool {
                id,
                name,
                input,
                uuid,
                timestamp,
                branch,
                result,
            } => {
                had_tools = true;
                if EDIT_TOOLS.contains(&name.as_str()) {
                    if let Some(path) = edit_path(input) {
                        edits.push(PendingEdit {
                            tool_use_id: id.clone(),
                            uuid: uuid.clone(),
                            tool: name.clone(),
                            path,
                            timestamp: timestamp.clone(),
                            branch: branch.clone(),
                            error: result
                                .as_ref()
                                .filter(|r| r.is_error)
                                .map(|r| r.content.clone()),
                        });
                    }
                    continue;
                }
                if name == "Read" {
                    if let Some(p) = input.get("file_path").and_then(Value::as_str) {
                        let shown = display_path(p, ctx.root);
                        if !consulted.contains(&shown) {
                            consulted.push(shown);
                        }
                    }
                    continue;
                }
                // Any other tool ends an edit run.
                if !edits.is_empty() {
                    last_action = Some(flush_edits(
                        &mut edits,
                        &mut narration,
                        &mut consulted,
                        goal,
                        ctx,
                        nodes,
                    ));
                    derived_any = true;
                }
                if name == "Agent" || name == "Task" {
                    skipped.subagents += 1;
                    continue;
                }
                if name.starts_with("mcp__deciduous__") {
                    skipped.deciduous_writes += 1;
                    continue;
                }
                if name != "Bash" {
                    skipped.other_tools += 1;
                    continue;
                }
                let cmd = input
                    .get("command")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let result = result.clone().unwrap_or_default();
                let output = bash_output(&result);

                if is_deciduous_command(&cmd) {
                    skipped.deciduous_writes += 1;
                    continue;
                }

                // Claude Code diffs the working tree around a shell command
                // and records which files it changed (`bashEditDiff`): a
                // heredoc, a sed -i, a script. Those are edits too.
                let bash_edits = files_changed_by_bash(&result);
                if !bash_edits.is_empty() {
                    for path in bash_edits {
                        edits.push(PendingEdit {
                            tool_use_id: id.clone(),
                            uuid: uuid.clone(),
                            tool: "Bash".to_string(),
                            path,
                            timestamp: timestamp.clone(),
                            branch: branch.clone(),
                            error: None,
                        });
                    }
                }

                let is_commit = is_commit_command(&cmd);
                let check = check_command(&cmd);
                let is_pr = cmd.contains("gh pr create");
                if (is_commit || check.is_some() || is_pr) && !edits.is_empty() {
                    last_action = Some(flush_edits(
                        &mut edits,
                        &mut narration,
                        &mut consulted,
                        goal,
                        ctx,
                        nodes,
                    ));
                    derived_any = true;
                } else if !edits.is_empty() {
                    // The command edited files and was nothing else: the
                    // run goes on.
                    continue;
                }

                if is_commit {
                    let found = commit_in_output(&output)
                        .or_else(|| oneline_in_output(&output))
                        .map(|(sha, subject)| (Some(sha), subject))
                        .or_else(|| {
                            (!result.is_error)
                                .then(|| commit_subject_in_command(&cmd))
                                .flatten()
                                .map(|subject| (None, subject))
                        });
                    if let Some((sha, subject)) = found {
                        let mut meta = json!({
                            "evidence": evidence_json(Evidence::Proven, ctx, &[uuid], &[id]),
                        });
                        if let Some(sha) = sha {
                            let full = resolve_commit(ctx.root, &sha).unwrap_or(sha);
                            meta["commit"] = json!(full);
                        }
                        if let Some(b) = branch {
                            meta["branch"] = json!(b);
                        }
                        let action = push(
                            nodes,
                            DerivedNode {
                                change_id: derive_change_id(ctx.session_id, "commit", id),
                                node_type: "action".into(),
                                title: format!("Committed: {}", title_from(&subject)),
                                description: Some(cmd.trim().to_string()),
                                created_at: timestamp.clone(),
                                evidence: Evidence::Proven,
                                metadata: meta,
                                parent: Some(goal),
                                edge_rationale: Some("commit made in this turn".into()),
                            },
                        );
                        last_action = Some(action);
                        derived_any = true;
                        narration.clear();
                        continue;
                    }
                    if result.is_error {
                        push_outcome(
                            nodes,
                            ctx,
                            id,
                            uuid,
                            timestamp,
                            branch,
                            "git commit failed".to_string(),
                            summarize_output(&output, true),
                            last_action.unwrap_or(goal),
                            true,
                        );
                        derived_any = true;
                    }
                    continue;
                }

                if let Some(check) = check {
                    let failed = result.is_error;
                    let title = format!("{} {}", check, if failed { "failed" } else { "passed" });
                    let summary = summarize_output(&output, failed);
                    push_outcome(
                        nodes,
                        ctx,
                        id,
                        uuid,
                        timestamp,
                        branch,
                        title,
                        summary,
                        last_action.unwrap_or(goal),
                        failed,
                    );
                    derived_any = true;
                    narration.clear();
                    continue;
                }

                if is_pr {
                    if let Some(url) = pr_url(&output) {
                        let number = url.rsplit('/').next().unwrap_or("");
                        push_outcome(
                            nodes,
                            ctx,
                            id,
                            uuid,
                            timestamp,
                            branch,
                            format!("Opened PR #{number}"),
                            Some(url),
                            last_action.unwrap_or(goal),
                            false,
                        );
                        derived_any = true;
                        narration.clear();
                        continue;
                    }
                }

                skipped.other_tools += 1;
            }
        }
    }

    if !edits.is_empty() {
        flush_edits(&mut edits, &mut narration, &mut consulted, goal, ctx, nodes);
        derived_any = true;
    }

    // The assistant's closing words: what it says happened. Inferred, and
    // linked from the goal so it sits beside the proven outcomes rather
    // than under one of them.
    let closing = trailing_text(&turn.steps);
    if let Some((text, uuid, timestamp)) = closing {
        let title = title_from(&text);
        if !title.is_empty() {
            let node_type = if derived_any || had_tools {
                "outcome"
            } else {
                "observation"
            };
            let mut meta = json!({
                "evidence": evidence_json(Evidence::Inferred, ctx, &[uuid.as_str()], &[]),
            });
            if let Some(b) = &turn.branch {
                meta["branch"] = json!(b);
            }
            push(
                nodes,
                DerivedNode {
                    change_id: derive_change_id(ctx.session_id, "closing", &uuid),
                    node_type: node_type.into(),
                    title,
                    description: Some(text),
                    created_at: timestamp,
                    evidence: Evidence::Inferred,
                    metadata: meta,
                    parent: Some(goal),
                    edge_rationale: Some("the assistant's report at the end of the turn".into()),
                },
            );
        }
    }
}

/// Text blocks after the last tool call of a turn, joined. `None` when the
/// turn ended on a tool call.
fn trailing_text(steps: &[Step]) -> Option<(String, String, String)> {
    let last_tool = steps
        .iter()
        .rposition(|s| matches!(s, Step::Tool { .. }))
        .map(|i| i + 1)
        .unwrap_or(0);
    let tail = &steps[last_tool..];
    let mut text = Vec::new();
    let mut uuid = None;
    let mut ts = None;
    for s in tail {
        if let Step::Text {
            text: t,
            uuid: u,
            timestamp,
        } = s
        {
            text.push(t.trim().to_string());
            uuid = Some(u.clone());
            ts = Some(timestamp.clone());
        }
    }
    let text = text.join("\n\n");
    if text.trim().is_empty() {
        return None;
    }
    Some((text, uuid?, ts?))
}

/// A shell command that writes to this graph: the agent logged something,
/// which is already in the graph and is not an edit to the project.
fn is_deciduous_command(cmd: &str) -> bool {
    cmd.split(['|', ';', '&', '\n']).any(|seg| {
        let mut words = seg.split_whitespace().skip_while(|w| w.contains('='));
        matches!(words.next(), Some("deciduous"))
    })
}

/// Files Claude Code saw a shell command change (`toolUseResult.bashEditDiff`),
/// leaving out the graph's own files and git's.
fn files_changed_by_bash(r: &ToolResult) -> Vec<String> {
    let Some(files) = r
        .structured
        .as_ref()
        .and_then(|s| s.get("bashEditDiff"))
        .and_then(|d| d.get("files"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    files
        .iter()
        .filter_map(|f| f.get("filePath").and_then(Value::as_str))
        .filter(|p| !p.contains("/.deciduous/") && !p.contains("/.git/"))
        .map(str::to_string)
        .collect()
}

fn bash_output(r: &ToolResult) -> String {
    if let Some(s) = &r.structured {
        let out = s.get("stdout").and_then(Value::as_str).unwrap_or("");
        let err = s.get("stderr").and_then(Value::as_str).unwrap_or("");
        if !out.is_empty() || !err.is_empty() {
            return format!("{out}\n{err}");
        }
    }
    r.content.clone()
}

fn resolve_commit(root: Option<&Path>, short: &str) -> Option<String> {
    let root = root?;
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{short}^{{commit}}"),
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (s.len() == 40).then_some(s)
}

#[allow(clippy::too_many_arguments)]
fn push_outcome(
    nodes: &mut Vec<DerivedNode>,
    ctx: &Ctx,
    tool_use_id: &str,
    uuid: &str,
    timestamp: &str,
    branch: &Option<String>,
    title: String,
    description: Option<String>,
    parent: usize,
    failed: bool,
) -> usize {
    // `cargo build` run eight times in a row, each passing, is one fact
    // repeated: fold it into the node before it, counting the runs and
    // keeping the latest output.
    if let Some(last) = nodes.last_mut() {
        if last.node_type == "outcome"
            && last.evidence == Evidence::Proven
            && last.parent == Some(parent)
            && last.title == title
        {
            let runs = last.metadata["runs"].as_u64().unwrap_or(1) + 1;
            last.metadata["runs"] = json!(runs);
            if let Some(Value::Array(ids)) = last.metadata["evidence"].get_mut("tool_use_ids") {
                ids.push(json!(tool_use_id));
            }
            if let Some(Value::Array(us)) = last.metadata["evidence"].get_mut("uuids") {
                us.push(json!(uuid));
            }
            if description.is_some() {
                last.description = description;
            }
            return nodes.len() - 1;
        }
    }
    let mut meta = json!({
        "evidence": evidence_json(Evidence::Proven, ctx, &[uuid], &[tool_use_id]),
        "exit": if failed { "error" } else { "ok" },
    });
    if let Some(b) = branch {
        meta["branch"] = json!(b);
    }
    push(
        nodes,
        DerivedNode {
            change_id: derive_change_id(ctx.session_id, "outcome", tool_use_id),
            node_type: "outcome".into(),
            title,
            description,
            created_at: timestamp.to_string(),
            evidence: Evidence::Proven,
            metadata: meta,
            parent: Some(parent),
            edge_rationale: Some("command run in this turn".into()),
        },
    )
}

fn flush_edits(
    edits: &mut Vec<PendingEdit>,
    narration: &mut Vec<String>,
    consulted: &mut Vec<String>,
    goal: usize,
    ctx: &mut Ctx,
    nodes: &mut Vec<DerivedNode>,
) -> usize {
    let run: Vec<PendingEdit> = std::mem::take(edits);
    let first = &run[0];

    let mut files: Vec<String> = Vec::new();
    let mut all_write = true;
    for e in &run {
        ctx.touched.insert(e.path.clone());
        let shown = display_path(&e.path, ctx.root);
        if !files.contains(&shown) {
            files.push(shown);
        }
        if e.tool != "Write" {
            all_write = false;
        }
    }
    let verb = if all_write { "Write" } else { "Edit" };
    let title = match files.len() {
        1 => format!("{verb} {}", files[0]),
        2 | 3 => format!("{verb} {}", files.join(", ")),
        n => format!("{verb} {}, {} and {} more", files[0], files[1], n - 2),
    };

    let intent = narration
        .iter()
        .filter(|s| !s.is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join("\n\n");
    narration.clear();

    let uuids: Vec<&str> = run.iter().map(|e| e.uuid.as_str()).collect();
    let ids: Vec<&str> = run.iter().map(|e| e.tool_use_id.as_str()).collect();
    let mut meta = json!({
        "files": files,
        "evidence": evidence_json(Evidence::Proven, ctx, &uuids, &ids),
        "edits": run.len(),
    });
    if !consulted.is_empty() {
        meta["consulted"] = json!(consulted);
    }
    consulted.clear();
    if let Some(b) = &first.branch {
        meta["branch"] = json!(b);
    }
    let failed: Vec<&PendingEdit> = run.iter().filter(|e| e.error.is_some()).collect();

    let action = push(
        nodes,
        DerivedNode {
            change_id: derive_change_id(ctx.session_id, "edit", &first.tool_use_id),
            node_type: "action".into(),
            title,
            description: (!intent.is_empty()).then_some(intent),
            created_at: first.timestamp.clone(),
            evidence: Evidence::Proven,
            metadata: meta,
            parent: Some(goal),
            edge_rationale: Some("files changed in this turn".into()),
        },
    );

    for e in failed {
        let err = e.error.as_deref().unwrap_or("");
        let shown = display_path(&e.path, ctx.root);
        let mut meta = json!({
            "evidence": evidence_json(Evidence::Proven, ctx, &[e.uuid.as_str()], &[e.tool_use_id.as_str()]),
            "exit": "error",
            "files": [shown],
        });
        if let Some(b) = &e.branch {
            meta["branch"] = json!(b);
        }
        push(
            nodes,
            DerivedNode {
                change_id: derive_change_id(ctx.session_id, "edit-failed", &e.tool_use_id),
                node_type: "outcome".into(),
                title: format!("{} of {} failed", e.tool, display_path(&e.path, ctx.root)),
                description: (!err.trim().is_empty()).then(|| title_from(err)),
                created_at: e.timestamp.clone(),
                evidence: Evidence::Proven,
                metadata: meta,
                parent: Some(action),
                edge_rationale: Some("the tool refused the edit".into()),
            },
        );
    }
    action
}

// ---------------------------------------------------------------------------
// Hashing
// ---------------------------------------------------------------------------

/// Hash every file the plan's actions touched, as it stands now. Git's blob
/// id when git is available (the id `git log --find-object` understands),
/// sha256 otherwise. Files that no longer exist are left out.
pub fn hash_touched(plan: &mut Plan, root: Option<&Path>) -> usize {
    let mut paths: BTreeSet<String> = BTreeSet::new();
    for n in &plan.nodes {
        if n.node_type != "action" {
            continue;
        }
        if let Some(files) = n.metadata.get("files").and_then(Value::as_array) {
            for f in files.iter().filter_map(Value::as_str) {
                paths.insert(f.to_string());
            }
        }
    }
    if paths.is_empty() {
        return 0;
    }
    let abs = |p: &str| -> PathBuf {
        let path = Path::new(p);
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            root.map(|r| r.join(path))
                .unwrap_or_else(|| path.to_path_buf())
        }
    };
    let existing: Vec<(String, PathBuf)> = paths
        .into_iter()
        .map(|p| {
            let a = abs(&p);
            (p, a)
        })
        .filter(|(_, a)| a.is_file())
        .collect();
    if existing.is_empty() {
        return 0;
    }

    let mut hashes: BTreeMap<String, String> = BTreeMap::new();
    if let Some(blobs) = git_blob_ids(root, &existing) {
        hashes = blobs;
    }
    for (shown, a) in &existing {
        if hashes.contains_key(shown) {
            continue;
        }
        if let Ok(bytes) = fs::read(a) {
            let d = Sha256::digest(&bytes);
            hashes.insert(shown.clone(), format!("sha256:{:x}", d));
        }
    }

    let stamp = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    for n in &mut plan.nodes {
        if n.node_type != "action" {
            continue;
        }
        let Some(files) = n.metadata.get("files").and_then(Value::as_array).cloned() else {
            continue;
        };
        let mut own = serde_json::Map::new();
        for f in files.iter().filter_map(Value::as_str) {
            if let Some(h) = hashes.get(f) {
                own.insert(f.to_string(), json!(h));
            }
        }
        if !own.is_empty() {
            n.metadata["file_hashes"] = Value::Object(own);
            n.metadata["hashed_at"] = json!(stamp);
        }
    }
    hashes.len()
}

fn git_blob_ids(
    root: Option<&Path>,
    files: &[(String, PathBuf)],
) -> Option<BTreeMap<String, String>> {
    use std::io::Write;
    let mut cmd = Command::new("git");
    if let Some(r) = root {
        cmd.arg("-C").arg(r);
    }
    let mut child = cmd
        .args(["hash-object", "--stdin-paths"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    {
        let mut stdin = child.stdin.take()?;
        for (_, a) in files {
            writeln!(stdin, "{}", a.display()).ok()?;
        }
    }
    let out = child.wait_with_output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let ids: Vec<&str> = text.lines().map(str::trim).collect();
    if ids.len() != files.len() {
        return None;
    }
    Some(
        files
            .iter()
            .zip(ids)
            .filter(|(_, id)| id.len() == 40)
            .map(|((shown, _), id)| (shown.clone(), format!("git:{id}")))
            .collect(),
    )
}

// ---------------------------------------------------------------------------
// Applying
// ---------------------------------------------------------------------------

/// What an import did.
#[derive(Debug, Default, Serialize)]
pub struct ImportReport {
    pub session_id: String,
    pub title: String,
    pub transcript: String,
    pub turns: usize,
    pub created: Counts,
    /// Nodes whose change_id was already in the graph: an earlier import.
    pub already_present: usize,
    pub edges_created: usize,
    /// Prompts whose goal the agent had already logged during the session:
    /// what the import derived hangs under that goal, and no second one is
    /// made. See [`LiveGoals`].
    pub attached_to_logged_goals: usize,
    pub files_hashed: usize,
    pub skipped: Skipped,
    pub unreadable_lines: usize,
    /// Local id of the session's root goal.
    pub root_id: Option<i32>,
    pub dry_run: bool,
}

#[derive(Debug, Default, Serialize)]
pub struct Counts {
    pub goals: usize,
    pub actions: usize,
    pub outcomes_proven: usize,
    pub outcomes_inferred: usize,
    pub observations: usize,
}

impl Counts {
    fn add(&mut self, n: &DerivedNode) {
        match (n.node_type.as_str(), n.evidence) {
            ("goal", _) => self.goals += 1,
            ("action", _) => self.actions += 1,
            ("outcome", Evidence::Proven) => self.outcomes_proven += 1,
            ("outcome", Evidence::Inferred) => self.outcomes_inferred += 1,
            ("observation", _) => self.observations += 1,
            _ => {}
        }
    }
}

pub struct ImportOptions {
    pub dry_run: bool,
    pub hash_files: bool,
}

/// Goals already in the graph that an agent logged, for recognising the
/// goal it logged for a prompt while the session ran.
///
/// The import derives a goal per prompt, with a change_id made from the
/// session and the prompt's entry. An agent that followed its instructions
/// logged that goal too (`add goal -p "<the prompt>"`, or `add_node` with
/// `prompt`), under a change_id of its own, so importing the session it
/// logged gave every prompt two goals: one with the agent's options and
/// decisions under it, one with the transcript's actions and outcomes.
///
/// Matching on the words and the time alone let a second session claim the
/// goal: two sessions were sent "fix the tests" in the same hour, one logged
/// a goal, and both imports hung their work under it. So a goal is a
/// prompt's only when the transcript shows this session logging a goal in
/// that turn ([`Logged`]), and then:
///
/// - by id, when the command's output gave one: that node, if it is a logged
///   goal created inside the turn;
/// - otherwise by words: `metadata.prompt` is the prompt (or one of its
///   go-aheads), verbatim after trimming, created inside the turn.
///
/// Inside the turn is after the prompt was sent and before the next goal's
/// (or within a day, for the last). Either way a goal that already has
/// another session's derived work under it is not taken: two sessions that
/// both logged the same words each keep their own.
struct LiveGoals {
    goals: Vec<LiveGoal>,
    claimed: BTreeSet<i32>,
}

struct LiveGoal {
    id: i32,
    /// Trimmed; empty when the goal was logged without one.
    prompt: String,
    created_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    /// Sessions whose derived nodes hang under it.
    sessions: BTreeSet<String>,
}

fn parse_time(s: &str) -> Option<chrono::DateTime<chrono::FixedOffset>> {
    chrono::DateTime::parse_from_rfc3339(s).ok()
}

/// The session a node was derived from, if it was derived.
fn derived_session(metadata_json: Option<&str>) -> Option<String> {
    let meta: Value = serde_json::from_str(metadata_json?).ok()?;
    Some(meta.get("evidence")?.get("session")?.as_str()?.to_string())
}

impl LiveGoals {
    fn load(db: &Database) -> Result<Self, TranscriptError> {
        let nodes = db.get_all_nodes()?;
        let session_of: BTreeMap<i32, String> = nodes
            .iter()
            .filter_map(|n| Some((n.id, derived_session(n.metadata_json.as_deref())?)))
            .collect();
        let mut under: BTreeMap<i32, BTreeSet<String>> = BTreeMap::new();
        for e in db.get_all_edges()? {
            if let Some(session) = session_of.get(&e.to_node_id) {
                under
                    .entry(e.from_node_id)
                    .or_default()
                    .insert(session.clone());
            }
        }
        let goals = nodes
            .into_iter()
            .filter(|n| n.node_type == "goal")
            // A goal an import derived is found by its change_id, never here.
            .filter(|n| !session_of.contains_key(&n.id))
            .map(|n| {
                let prompt = n
                    .metadata_json
                    .as_deref()
                    .and_then(|m| serde_json::from_str::<Value>(m).ok())
                    .and_then(|m| Some(m.get("prompt")?.as_str()?.trim().to_string()))
                    .unwrap_or_default();
                LiveGoal {
                    id: n.id,
                    prompt,
                    created_at: parse_time(&n.created_at),
                    sessions: under.remove(&n.id).unwrap_or_default(),
                }
            })
            .collect();
        Ok(Self {
            goals,
            claimed: BTreeSet::new(),
        })
    }

    /// The goal this session logged for a turn whose prompt was sent at
    /// `sent`, the next goal's at `next`. `prompts` are the turn's own words
    /// and its go-aheads. Claims it.
    fn claim(
        &mut self,
        session: &str,
        logged: &Logged,
        prompts: &[&str],
        sent: &str,
        next: Option<&str>,
    ) -> Option<i32> {
        let sent = parse_time(sent)?;
        let day = sent + chrono::Duration::hours(24);
        let until = next.and_then(parse_time).map_or(day, |n| n.min(day));
        let free = |g: &&LiveGoal| {
            !self.claimed.contains(&g.id)
                && g.created_at.is_some_and(|at| at >= sent && at < until)
                && g.sessions.iter().all(|s| s == session)
        };
        let by_id = self
            .goals
            .iter()
            .filter(free)
            .find(|g| logged.goal_ids.contains(&g.id))
            .map(|g| g.id);
        let id = by_id.or_else(|| {
            self.goals
                .iter()
                .filter(free)
                .filter(|g| !g.prompt.is_empty() && prompts.iter().any(|p| p.trim() == g.prompt))
                .map(|g| (g.created_at, g.id))
                .min()
                .map(|(_, id)| id)
        })?;
        self.claimed.insert(id);
        Some(id)
    }

    /// [`claim`](Self::claim) for the goal at `plan.nodes[i]`.
    fn claim_for(&mut self, plan: &Plan, i: usize, next: Option<&str>) -> Option<i32> {
        let logged = plan.logged_goals.get(&i)?;
        let n = &plan.nodes[i];
        let mut prompts: Vec<&str> = Vec::new();
        prompts.extend(n.metadata.get("prompt").and_then(Value::as_str));
        if let Some(more) = n.metadata.get("follow_ups").and_then(Value::as_array) {
            prompts.extend(more.iter().filter_map(Value::as_str));
        }
        self.claim(&plan.session_id, logged, &prompts, &n.created_at, next)
    }

    /// True when the plan is nothing but goals the agent logged: the agent
    /// logged its own actions and outcomes too, and an import adds nothing.
    fn covers(&mut self, plan: &Plan) -> bool {
        self.claimed.clear();
        let times: Vec<&str> = plan.nodes.iter().map(|n| n.created_at.as_str()).collect();
        !plan.nodes.is_empty()
            && (0..plan.nodes.len()).all(|i| {
                plan.nodes[i].node_type == "goal"
                    && self.claim_for(plan, i, times.get(i + 1).copied()).is_some()
            })
    }
}

/// Import one transcript into the database. Idempotent: nodes whose derived
/// change_id already exists are reused, edges that exist are left alone, and
/// a prompt whose goal was logged during the session is attached to that goal
/// again rather than given a new one.
pub fn import(
    db: &Database,
    path: &Path,
    opts: &ImportOptions,
) -> Result<ImportReport, TranscriptError> {
    let t = parse(path)?;
    let root = db.project_root().map(Path::to_path_buf);
    let mut plan = plan(&t, root.as_deref())?;
    let files_hashed = if opts.hash_files {
        hash_touched(&mut plan, root.as_deref())
    } else {
        0
    };

    let mut report = ImportReport {
        session_id: plan.session_id.clone(),
        title: plan.title.clone(),
        transcript: path.to_string_lossy().to_string(),
        turns: plan.turns,
        files_hashed,
        skipped: std::mem::take(&mut plan.skipped),
        unreadable_lines: t.unreadable_lines,
        dry_run: opts.dry_run,
        ..Default::default()
    };

    let mut live = LiveGoals::load(db)?;
    let goal_times: Vec<&str> = plan
        .nodes
        .iter()
        .filter(|n| n.node_type == "goal")
        .map(|n| n.created_at.as_str())
        .collect();
    let mut goal_index = 0;

    let mut ids: Vec<Option<i32>> = Vec::with_capacity(plan.nodes.len());
    for (i, n) in plan.nodes.iter().enumerate() {
        let existing = db.node_id_by_change_id(&n.change_id)?;
        let logged = if n.node_type == "goal" {
            let next = goal_times.get(goal_index + 1).copied();
            goal_index += 1;
            match existing {
                None => live.claim_for(&plan, i, next),
                Some(_) => None,
            }
        } else {
            None
        };
        let id = match (existing, logged) {
            (Some(id), _) => {
                report.already_present += 1;
                Some(id)
            }
            (None, Some(id)) => {
                report.attached_to_logged_goals += 1;
                Some(id)
            }
            (None, None) => {
                report.created.add(n);
                if opts.dry_run {
                    None
                } else {
                    Some(db.create_derived_node(
                        &n.change_id,
                        &n.node_type,
                        &n.title,
                        n.description.as_deref(),
                        Some(&n.metadata),
                        &n.created_at,
                    )?)
                }
            }
        };
        if i == 0 {
            report.root_id = id;
        }
        ids.push(id);
    }

    for (i, n) in plan.nodes.iter().enumerate() {
        let Some(parent) = n.parent else {
            continue;
        };
        if opts.dry_run {
            match (ids[parent], ids[i]) {
                (Some(from), Some(to)) => {
                    if from != to && !db.edge_exists(from, to, "leads_to")? {
                        report.edges_created += 1;
                    }
                }
                _ => report.edges_created += 1,
            }
            continue;
        }
        let (Some(from), Some(to)) = (ids[parent], ids[i]) else {
            continue;
        };
        if from == to || db.edge_exists(from, to, "leads_to")? {
            continue;
        }
        db.create_edge(from, to, "leads_to", n.edge_rationale.as_deref())?;
        report.edges_created += 1;
    }

    Ok(report)
}

// ---------------------------------------------------------------------------
// Finding transcripts
// ---------------------------------------------------------------------------

/// Claude Code keeps a project's transcripts under
/// `~/.claude/projects/<slug>/`, the slug being the project's absolute path
/// with every character outside `[A-Za-z0-9]` replaced by `-`.
pub fn claude_project_dir(project_root: &Path) -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    let slug: String = project_root
        .to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    Some(home.join(".claude").join("projects").join(slug))
}

/// One transcript as `transcript list` describes it.
#[derive(Debug, Serialize)]
pub struct Listed {
    pub path: PathBuf,
    pub session_id: String,
    pub title: String,
    pub prompts: usize,
    pub first_prompt_at: String,
    pub imported: bool,
    /// False when no prompt of the session changed anything: an import
    /// would add nothing.
    pub work: bool,
}

/// Every top-level transcript in a directory, newest first, with whether
/// its root goal is already in the graph.
pub fn list(db: &Database, dir: &Path) -> Result<Vec<Listed>, TranscriptError> {
    let mut out = Vec::new();
    let mut live = LiveGoals::load(db)?;
    if !dir.is_dir() {
        return Err(TranscriptError::NoDirectory(dir.to_path_buf()));
    }
    let entries = fs::read_dir(dir).map_err(|e| TranscriptError::Io(dir.to_path_buf(), e))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        let Ok(t) = parse(&path) else {
            continue;
        };
        let turns = turns(&t);
        let Some(first) = turns.first() else {
            continue;
        };
        // The root may be a goal the agent logged, which has its own
        // change_id; then anything else the import derived says it ran.
        // And when the agent logged everything itself there is nothing
        // derived to find: the session is in because nothing is left out.
        let planned = plan(&t, None).ok();
        let work = planned.as_ref().is_some_and(|p| !p.nodes.is_empty());
        let imported = planned.as_ref().is_some_and(|p| {
            p.nodes
                .iter()
                .any(|n| matches!(db.node_id_by_change_id(&n.change_id), Ok(Some(_))))
                || live.covers(p)
        });
        out.push(Listed {
            path: path.clone(),
            session_id: t.session_id.clone(),
            title: t
                .title
                .clone()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| title_from(&first.prompt)),
            prompts: turns.len(),
            first_prompt_at: first.timestamp.clone(),
            imported,
            work,
        });
    }
    out.sort_by(|a, b| b.first_prompt_at.cmp(&a.first_prompt_at));
    Ok(out)
}

/// `import` takes a path or a session id; a session id is looked up in the
/// project's transcript directory.
pub fn locate(db: &Database, arg: &str) -> Result<PathBuf, String> {
    let p = Path::new(arg);
    if p.is_file() {
        return Ok(p.to_path_buf());
    }
    let looks_like_session =
        arg.len() >= 8 && arg.chars().all(|c| c.is_ascii_hexdigit() || c == '-');
    if !looks_like_session {
        return Err(format!("{arg}: no such file"));
    }
    let root = db
        .project_root()
        .ok_or_else(|| "no project around this database to look up a session in".to_string())?;
    let dir = claude_project_dir(root).ok_or_else(|| "HOME is not set".to_string())?;
    let entries = fs::read_dir(&dir).map_err(|e| {
        format!(
            "{arg}: not a file, and no transcript directory to look it up in ({}: {})",
            dir.display(),
            e
        )
    })?;
    let mut matches: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension().and_then(|e| e.to_str()) == Some("jsonl")
                && p.file_stem()
                    .and_then(|s| s.to_str())
                    .is_some_and(|s| s.starts_with(arg))
        })
        .collect();
    match matches.len() {
        0 => Err(format!(
            "no transcript starting with {arg} in {}; `deciduous transcript list` shows them",
            dir.display()
        )),
        1 => Ok(matches.remove(0)),
        n => Err(format!(
            "{arg} matches {n} transcripts in {}; give more of the id",
            dir.display()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn change_ids_are_stable_and_uuid_shaped() {
        let a = derive_change_id("s", "goal", "u1");
        let b = derive_change_id("s", "goal", "u1");
        assert_eq!(a, b);
        assert_ne!(a, derive_change_id("s", "goal", "u2"));
        assert_ne!(a, derive_change_id("s", "edit", "u1"));
        assert_eq!(a.len(), 36);
        assert_eq!(&a[14..15], "8");
        assert!(matches!(&a[19..20], "8" | "9" | "a" | "b"));
    }

    #[test]
    fn titles_drop_markdown_and_cut_on_words() {
        assert_eq!(
            title_from("## **Done.** Tests pass\nmore"),
            "Done. Tests pass"
        );
        assert_eq!(title_from("\n\n  - fix the thing:  \n"), "fix the thing");
        let long = "word ".repeat(40);
        let t = title_from(&long);
        assert!(t.chars().count() <= TITLE_LEN);
        assert!(t.ends_with('…'));
        assert_eq!(
            title_from("<system-reminder>x</system-reminder>\nreal line"),
            "real line"
        );
    }

    #[test]
    fn commit_line_is_found() {
        let out = "[main abc1234] feat: add thing\n 2 files changed";
        assert_eq!(
            commit_in_output(out),
            Some(("abc1234".into(), "feat: add thing".into()))
        );
        assert_eq!(
            commit_in_output("[68-the-transcript-is-the-log 0123abcd] x"),
            Some(("0123abcd".into(), "x".into()))
        );
        assert_eq!(commit_in_output("nothing to commit"), None);
        assert!(is_commit_command("git add a && git commit -m 'x'"));
        assert!(!is_commit_command("git log --oneline"));
    }

    #[test]
    fn quiet_commits_are_still_found() {
        assert_eq!(
            oneline_in_output("0\n233e07a docs: the server assumes 5432"),
            Some(("233e07a".into(), "docs: the server assumes 5432".into()))
        );
        assert_eq!(oneline_in_output("nothing here"), None);
        assert_eq!(
            commit_subject_in_command("git add a && git commit -q -m \"feat: x\" && git log"),
            Some("feat: x".into())
        );
        assert_eq!(
            commit_subject_in_command("git commit -q -F - <<'EOF'\nfix: the thing\n\nbody\nEOF\n"),
            Some("fix: the thing".into())
        );
        assert!(is_deciduous_command("deciduous add goal x"));
        assert!(is_deciduous_command(
            "cd x && DECIDUOUS_DB_PATH=y deciduous link 1 2"
        ));
        assert!(!is_deciduous_command("cargo run -- deciduous"));
        assert_eq!(
            pr_url(
                "remote: https://github.com/o/r/pull/new/branch\nhttps://github.com/o/r/pull/42"
            ),
            Some("https://github.com/o/r/pull/42".into())
        );
        assert_eq!(
            pr_url("remote: https://github.com/o/r/pull/new/branch"),
            None
        );
    }

    #[test]
    fn check_commands_are_recognised() {
        assert_eq!(
            check_command("cd x && cargo test 2>&1 | tail"),
            Some("cargo test".into())
        );
        assert_eq!(
            check_command("timeout 60 cargo clippy -- -D warnings"),
            Some("cargo clippy".into())
        );
        assert_eq!(check_command("npm run build"), Some("npm run build".into()));
        assert_eq!(check_command("make lint"), Some("make lint".into()));
        assert_eq!(check_command("ls -la"), None);
        assert_eq!(check_command("grep cargo test src"), None);
    }

    #[test]
    fn cargo_results_are_summed() {
        let out = "test result: ok. 10 passed; 0 failed; 1 ignored\n\
                   running 3 tests\ntest result: FAILED. 2 passed; 1 failed; 0 ignored";
        assert_eq!(
            summarize_output(out, true),
            Some("12 passed, 1 failed".into())
        );
        assert_eq!(summarize_output("", false), None);
        assert_eq!(
            summarize_output("a\nb\nc\nd\ne\n", false),
            Some("b\nc\nd\ne".into())
        );
    }

    fn line(v: Value) -> String {
        format!("{}\n", v)
    }

    /// A session: one prompt, a read, two edits, a test run, a commit, and
    /// a closing report.
    fn fixture(dir: &Path) -> PathBuf {
        let p = dir.join("sess.jsonl");
        let mut s = String::new();
        s.push_str(&line(
            json!({"type":"ai-title","aiTitle":"Add the thing","sessionId":"s1"}),
        ));
        s.push_str("not json at all\n");
        s.push_str(&line(json!({"type":"user","uuid":"u1","timestamp":"2026-10-01T10:00:00Z","cwd":dir,"gitBranch":"feat","sessionId":"s1",
            "message":{"role":"user","content":"please add the thing\nwith two lines"}})));
        s.push_str(&line(json!({"type":"assistant","uuid":"a1","timestamp":"2026-10-01T10:00:05Z","cwd":dir,"gitBranch":"feat",
            "message":{"role":"assistant","content":[{"type":"text","text":"I'll read the module first."}]}})));
        s.push_str(&line(json!({"type":"assistant","uuid":"a2","timestamp":"2026-10-01T10:00:06Z","cwd":dir,"gitBranch":"feat",
            "message":{"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"Read","input":{"file_path":dir.join("src/lib.rs")}}]}})));
        s.push_str(&line(json!({"type":"user","uuid":"r1","timestamp":"2026-10-01T10:00:07Z","cwd":dir,
            "message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"fn x(){}"}]}})));
        s.push_str(&line(json!({"type":"assistant","uuid":"a3","timestamp":"2026-10-01T10:00:08Z","cwd":dir,"gitBranch":"feat",
            "message":{"role":"assistant","content":[{"type":"text","text":"Adding the function and its test."}]}})));
        s.push_str(&line(json!({"type":"assistant","uuid":"a4","timestamp":"2026-10-01T10:00:09Z","cwd":dir,"gitBranch":"feat",
            "message":{"role":"assistant","content":[{"type":"tool_use","id":"t2","name":"Edit","input":{"file_path":dir.join("src/lib.rs"),"old_string":"a","new_string":"b"}}]}})));
        s.push_str(&line(json!({"type":"user","uuid":"r2","timestamp":"2026-10-01T10:00:10Z","cwd":dir,
            "message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t2","content":"ok"}]}})));
        s.push_str(&line(json!({"type":"assistant","uuid":"a5","timestamp":"2026-10-01T10:00:11Z","cwd":dir,"gitBranch":"feat",
            "message":{"role":"assistant","content":[{"type":"tool_use","id":"t3","name":"Write","input":{"file_path":dir.join("tests/thing.rs"),"content":"#[test] fn t(){}"}}]}})));
        s.push_str(&line(json!({"type":"user","uuid":"r3","timestamp":"2026-10-01T10:00:12Z","cwd":dir,
            "message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t3","content":"ok"}]}})));
        s.push_str(&line(json!({"type":"assistant","uuid":"a6","timestamp":"2026-10-01T10:00:13Z","cwd":dir,"gitBranch":"feat",
            "message":{"role":"assistant","content":[{"type":"tool_use","id":"t4","name":"Bash","input":{"command":"cargo test"}}]}})));
        s.push_str(&line(json!({"type":"user","uuid":"r4","timestamp":"2026-10-01T10:00:20Z","cwd":dir,
            "toolUseResult":{"stdout":"running 2 tests\ntest result: ok. 2 passed; 0 failed; 0 ignored","stderr":""},
            "message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t4","content":"..."}]}})));
        s.push_str(&line(json!({"type":"assistant","uuid":"a7","timestamp":"2026-10-01T10:00:21Z","cwd":dir,"gitBranch":"feat",
            "message":{"role":"assistant","content":[{"type":"tool_use","id":"t5","name":"Bash","input":{"command":"git add src/lib.rs && git commit -m 'feat: thing'"}}]}})));
        s.push_str(&line(json!({"type":"user","uuid":"r5","timestamp":"2026-10-01T10:00:22Z","cwd":dir,
            "toolUseResult":{"stdout":"[feat 1234567] feat: thing\n 1 file changed","stderr":""},
            "message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t5","content":"..."}]}})));
        s.push_str(&line(json!({"type":"assistant","uuid":"a8","timestamp":"2026-10-01T10:00:23Z","cwd":dir,"gitBranch":"feat",
            "message":{"role":"assistant","content":[{"type":"text","text":"**Done.** The thing is added and both tests pass."}]}})));
        // A second prompt with no tools: a question answered in prose.
        s.push_str(&line(json!({"type":"user","uuid":"u2","timestamp":"2026-10-01T10:05:00Z","cwd":dir,"gitBranch":"feat","sessionId":"s1",
            "message":{"role":"user","content":[{"type":"text","text":"why did you pick that name?"}]}})));
        s.push_str(&line(json!({"type":"assistant","uuid":"a9","timestamp":"2026-10-01T10:05:03Z","cwd":dir,"gitBranch":"feat",
            "message":{"role":"assistant","content":[{"type":"text","text":"Because the module already used it."}]}})));
        // Synthetic user lines that are not prompts.
        s.push_str(&line(
            json!({"type":"user","uuid":"u3","timestamp":"2026-10-01T10:06:00Z","isMeta":true,
            "message":{"role":"user","content":"<command-name>/foo</command-name>"}}),
        ));
        s.push_str(&line(
            json!({"type":"user","uuid":"u4","timestamp":"2026-10-01T10:06:01Z",
            "message":{"role":"user","content":"<task-notification>done</task-notification>"}}),
        ));
        fs::write(&p, s).unwrap();
        p
    }

    #[test]
    fn plan_from_fixture() {
        let dir = tempfile::tempdir().unwrap();
        let p = fixture(dir.path());
        let t = parse(&p).unwrap();
        assert_eq!(t.session_id, "s1");
        assert_eq!(t.unreadable_lines, 1);
        let plan = plan(&t, Some(dir.path())).unwrap();
        assert_eq!(plan.turns, 2);
        assert_eq!(plan.title, "Add the thing");

        let kinds: Vec<(&str, &str)> = plan
            .nodes
            .iter()
            .map(|n| (n.node_type.as_str(), n.title.as_str()))
            .collect();
        assert_eq!(
            kinds,
            vec![
                ("goal", "Add the thing"),
                ("action", "Edit src/lib.rs, tests/thing.rs"),
                ("outcome", "cargo test passed"),
                ("action", "Committed: feat: thing"),
                ("outcome", "Done. The thing is added and both tests pass."),
            ],
            "the second prompt is a question answered in prose: not a goal"
        );
        assert_eq!(plan.skipped.idle_prompts, 1);
        let root = &plan.nodes[0];
        assert_eq!(
            root.metadata["prompt"],
            "please add the thing\nwith two lines"
        );
        assert_eq!(root.metadata["evidence"]["kind"], "proven");
        assert_eq!(root.metadata["session"]["turns"], 2);
        assert_eq!(root.parent, None);

        let edit = &plan.nodes[1];
        assert_eq!(edit.parent, Some(0));
        assert_eq!(
            edit.description.as_deref(),
            Some("I'll read the module first.\n\nAdding the function and its test.")
        );
        assert_eq!(edit.metadata["consulted"], json!(["src/lib.rs"]));
        assert_eq!(edit.metadata["edits"], 2);
        assert_eq!(
            edit.metadata["evidence"]["tool_use_ids"],
            json!(["t2", "t3"])
        );
        assert_eq!(edit.metadata["branch"], "feat");

        let test = &plan.nodes[2];
        assert_eq!(
            test.parent,
            Some(1),
            "the test run hangs under the edit that preceded it"
        );
        assert_eq!(test.description.as_deref(), Some("2 passed, 0 failed"));
        assert_eq!(test.metadata["exit"], "ok");

        let commit = &plan.nodes[3];
        assert_eq!(commit.metadata["commit"], "1234567");
        assert_eq!(commit.parent, Some(0));

        let closing = &plan.nodes[4];
        assert_eq!(closing.evidence, Evidence::Inferred);
        assert_eq!(closing.metadata["evidence"]["kind"], "inferred");
        assert_eq!(closing.parent, Some(0));

        // Same transcript, same ids.
        let again = super::plan(&parse(&p).unwrap(), Some(dir.path())).unwrap();
        let a: Vec<&str> = plan.nodes.iter().map(|n| n.change_id.as_str()).collect();
        let b: Vec<&str> = again.nodes.iter().map(|n| n.change_id.as_str()).collect();
        assert_eq!(a, b);
    }

    /// A session that opens with a question and then does work: the root is
    /// the prompt that did the work, and a session of questions is empty.
    #[test]
    fn a_prompt_that_changed_nothing_is_not_a_goal() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("s.jsonl");
        let user = |uuid: &str, at: &str, text: &str| {
            line(
                json!({"type":"user","uuid":uuid,"timestamp":at,"cwd":dir.path(),"sessionId":"s2",
                "message":{"role":"user","content":text}}),
            )
        };
        let says = |uuid: &str, at: &str, text: &str| {
            line(
                json!({"type":"assistant","uuid":uuid,"timestamp":at,"cwd":dir.path(),
                "message":{"role":"assistant","content":[{"type":"text","text":text}]}}),
            )
        };
        let questions = user("u1", "2026-10-01T10:00:00Z", "why is my internet so slow")
            + &says("a1", "2026-10-01T10:00:02Z", "I cannot tell from here.")
            + &user("u2", "2026-10-01T10:01:00Z", "ok try again")
            + &says("a2", "2026-10-01T10:01:02Z", "Still nothing.");
        fs::write(&p, &questions).unwrap();
        let plan = super::plan(&parse(&p).unwrap(), Some(dir.path())).unwrap();
        assert!(plan.nodes.is_empty(), "{:?}", plan.nodes);
        // "ok try again" names nothing: it is part of the prompt before it.
        assert_eq!(plan.skipped.idle_prompts, 1);
        assert_eq!(plan.skipped.go_aheads, 1);

        let work = user("u3", "2026-10-01T10:02:00Z", "rename x to y")
            + &line(
                json!({"type":"assistant","uuid":"a3","timestamp":"2026-10-01T10:02:02Z","cwd":dir.path(),
                "message":{"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"Edit",
                    "input":{"file_path":dir.path().join("src/lib.rs"),"old_string":"x","new_string":"y"}}]}}),
            )
            + &line(
                json!({"type":"user","uuid":"r1","timestamp":"2026-10-01T10:02:03Z","cwd":dir.path(),
                "message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"ok"}]}}),
            );
        fs::write(&p, questions + &work).unwrap();
        let plan = super::plan(&parse(&p).unwrap(), Some(dir.path())).unwrap();
        assert_eq!(plan.skipped.idle_prompts, 1);
        let root = &plan.nodes[0];
        assert_eq!(root.metadata["prompt"], "rename x to y");
        assert_eq!(root.parent, None);
        assert_eq!(root.metadata["session"]["id"], "s2");
        assert!(plan
            .nodes
            .iter()
            .all(|n| n.metadata["prompt"] != "ok try again"));
    }

    fn user(uuid: &str, at: &str, text: &str) -> String {
        line(
            json!({"type":"user","uuid":uuid,"timestamp":at,"cwd":"/p","sessionId":"s3",
            "message":{"role":"user","content":text}}),
        )
    }

    fn says(uuid: &str, at: &str, text: &str) -> String {
        line(
            json!({"type":"assistant","uuid":uuid,"timestamp":at,"cwd":"/p",
            "message":{"role":"assistant","content":[{"type":"text","text":text}]}}),
        )
    }

    /// A tool call and its result.
    fn tool(id: &str, at: &str, name: &str, input: Value, stdout: &str) -> String {
        line(
            json!({"type":"assistant","uuid":format!("a-{id}"),"timestamp":at,"cwd":"/p",
            "message":{"role":"assistant","content":[{"type":"tool_use","id":id,"name":name,"input":input}]}}),
        ) + &line(
            json!({"type":"user","uuid":format!("r-{id}"),"timestamp":at,"cwd":"/p",
                "toolUseResult":{"stdout":stdout,"stderr":""},
                "message":{"role":"user","content":[{"type":"tool_result","tool_use_id":id,"content":stdout}]}}),
        )
    }

    fn plan_of(text: &str) -> Plan {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("s.jsonl");
        fs::write(&p, text).unwrap();
        super::plan(&parse(&p).unwrap(), None).unwrap()
    }

    #[test]
    fn a_go_ahead_is_made_of_nothing_but_go_ahead_words() {
        for yes in [
            "ok do it",
            "sure",
            "retry",
            "Ok, do it?",
            "yes please",
            "go for it",
            "ok",
        ] {
            assert!(is_go_ahead(yes), "{yes}");
        }
        for no in [
            "add dark mode",
            "ok do it but in rust",
            "go dor it",
            "1, 4, 2, 5, 7 in that order",
            "no",
            "",
            "ok ok ok ok ok ok ok",
        ] {
            assert!(!is_go_ahead(no), "{no}");
        }
    }

    /// The ask, a proposal, "ok do it", the work: one goal, and it is the ask.
    #[test]
    fn work_set_off_by_a_go_ahead_belongs_to_the_prompt_before_it() {
        let plan = plan_of(
            &(user(
                "u1",
                "2026-10-01T10:00:00Z",
                "should the cache be per user?",
            ) + &says(
                "a1",
                "2026-10-01T10:00:02Z",
                "Yes: here is how I would do it.",
            ) + &user("u2", "2026-10-01T10:01:00Z", "ok do it")
                + &tool(
                    "t1",
                    "2026-10-01T10:01:02Z",
                    "Edit",
                    json!({"file_path":"/p/src/cache.rs","old_string":"a","new_string":"b"}),
                    "ok",
                )
                + &says("a2", "2026-10-01T10:01:05Z", "Done.")),
        );
        let goals: Vec<&DerivedNode> = plan
            .nodes
            .iter()
            .filter(|n| n.node_type == "goal")
            .collect();
        assert_eq!(goals.len(), 1, "{:?}", plan.nodes);
        assert_eq!(goals[0].metadata["prompt"], "should the cache be per user?");
        assert_eq!(goals[0].metadata["follow_ups"], json!(["ok do it"]));
        assert_eq!(goals[0].change_id, derive_change_id("s3", "goal", "u1"));
        assert_eq!(plan.skipped.go_aheads, 1);
        assert_eq!(plan.skipped.idle_prompts, 0);
        assert_eq!(plan.turns, 2);
        assert!(plan.nodes.iter().any(|n| n.node_type == "action"));
    }

    #[test]
    fn what_the_agent_logged_is_read_from_the_output_however_the_command_was_spelled() {
        let turn = |steps: String| {
            let dir = tempfile::tempdir().unwrap();
            let p = dir.path().join("s.jsonl");
            fs::write(
                &p,
                user("u1", "2026-10-01T10:00:00Z", "add the thing") + &steps,
            )
            .unwrap();
            logged_in(&turns(&parse(&p).unwrap())[0])
        };
        let l = turn(tool(
            "t1",
            "2026-10-01T10:00:01Z",
            "Bash",
            json!({"command":"Q() { \"$@\" | grep Created; }; Q ./target/release/deciduous add goal \"Thing\" -p \"add the thing\""}),
            "Created node 12 (type: goal, title: Thing) [prompt: 13 chars]",
        ));
        assert_eq!(l.goal_ids, vec![12]);
        assert!(l.goal && !l.action && !l.outcome);

        // Output thrown away: the words of the command are what is left.
        let l = turn(tool(
            "t1",
            "2026-10-01T10:00:01Z",
            "Bash",
            json!({"command":"deciduous add action \"Add it\" >/dev/null"}),
            "",
        ));
        assert!(l.action && l.goal_ids.is_empty());

        let l = turn(tool(
            "t1",
            "2026-10-01T10:00:01Z",
            "mcp__deciduous__add_node",
            json!({"node_type":"outcome","title":"It works"}),
            "{}",
        ));
        assert!(l.outcome && !l.action);

        // A command that only mentions the line is not deciduous writing.
        let l = turn(tool(
            "t1",
            "2026-10-01T10:00:01Z",
            "Bash",
            json!({"command":"cat notes.txt"}),
            "Created node 12 (type: goal, title: Thing)",
        ));
        assert_eq!(l, Logged::default());
    }

    /// The agent logged an action for its edits: the import does not add
    /// "Edited src/x.rs" beside it, and still records the test run nobody
    /// logged.
    #[test]
    fn a_kind_the_agent_logged_in_the_turn_is_not_derived_again() {
        let edit = tool(
            "t1",
            "2026-10-01T10:00:01Z",
            "Edit",
            json!({"file_path":"/p/src/x.rs","old_string":"a","new_string":"b"}),
            "ok",
        );
        let tests = tool(
            "t2",
            "2026-10-01T10:00:05Z",
            "Bash",
            json!({"command":"cargo test"}),
            "test result: ok. 3 passed; 0 failed; 0 ignored",
        );
        let logs_action = tool(
            "t3",
            "2026-10-01T10:00:06Z",
            "Bash",
            json!({"command":"deciduous add action \"Rename a to b\""}),
            "Created node 7 (type: action, title: Rename a to b)",
        );
        let base = user("u1", "2026-10-01T10:00:00Z", "rename a to b") + &edit + &tests;

        let without = plan_of(&(base.clone() + &says("a9", "2026-10-01T10:00:09Z", "Done.")));
        assert!(without.nodes.iter().any(|n| n.node_type == "action"));
        assert_eq!(without.skipped.logged_by_agent, 0);

        let with =
            plan_of(&(base.clone() + &logs_action + &says("a9", "2026-10-01T10:00:09Z", "Done.")));
        assert!(
            with.nodes.iter().all(|n| n.node_type != "action"),
            "{:?}",
            with.nodes
        );
        assert_eq!(with.skipped.logged_by_agent, 1);
        // The test run hung under the edit; it hangs under the goal now.
        let run = with
            .nodes
            .iter()
            .find(|n| n.title == "cargo test passed")
            .unwrap();
        assert_eq!(run.parent, Some(0));
        // And every parent still points at a node that is there.
        assert!(with
            .nodes
            .iter()
            .all(|n| n.parent.is_none_or(|p| p < with.nodes.len())));

        let logs_outcome = tool(
            "t4",
            "2026-10-01T10:00:07Z",
            "Bash",
            json!({"command":"deciduous add outcome \"3 tests pass\""}),
            "Created node 8 (type: outcome, title: 3 tests pass)",
        );
        let both = plan_of(
            &(base + &logs_action + &logs_outcome + &says("a9", "2026-10-01T10:00:09Z", "Done.")),
        );
        assert_eq!(both.nodes.len(), 1, "{:?}", both.nodes);
        assert_eq!(both.nodes[0].node_type, "goal");
    }

    #[test]
    fn hashing_covers_files_that_exist() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("src/lib.rs"), "fn x(){}\n").unwrap();
        let p = fixture(dir.path());
        let t = parse(&p).unwrap();
        let mut plan = plan(&t, Some(dir.path())).unwrap();
        let n = hash_touched(&mut plan, Some(dir.path()));
        assert_eq!(n, 1, "tests/thing.rs does not exist, src/lib.rs does");
        let h = plan.nodes[1].metadata["file_hashes"]["src/lib.rs"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(h.starts_with("git:") || h.starts_with("sha256:"), "{h}");
        if h.starts_with("git:") {
            // The blob id of "fn x(){}\n", as `git hash-object` computes it.
            assert_eq!(h, "git:3c193313a5e77e0d9374be39542d3e56c1c81d91");
        }
    }
}
