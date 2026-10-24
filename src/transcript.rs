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
    let mut ctx = Ctx {
        session_id: &t.session_id,
        root,
        touched: BTreeSet::new(),
    };
    let mut nodes: Vec<DerivedNode> = Vec::new();
    let mut skipped = Skipped::default();

    let session_title = t
        .title
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| title_from(&turns[0].prompt));

    let mut session_root: Option<usize> = None;
    for (i, turn) in turns.iter().enumerate() {
        let goal_title = if i == 0 {
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
        if i == 0 {
            meta["session"] = json!({
                "id": t.session_id,
                "turns": turns.len(),
                "transcript": t.path.to_string_lossy(),
            });
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
        if session_root.is_none() {
            session_root = Some(goal);
        }
        derive_turn(turn, goal, &mut ctx, &mut nodes, &mut skipped);
    }

    Ok(Plan {
        session_id: t.session_id.clone(),
        title: session_title,
        turns: turns.len(),
        nodes,
        skipped,
    })
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

/// Import one transcript into the database. Idempotent: nodes whose derived
/// change_id already exists are reused, edges that exist are left alone.
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
        skipped: plan.skipped,
        unreadable_lines: t.unreadable_lines,
        dry_run: opts.dry_run,
        ..Default::default()
    };

    let mut ids: Vec<Option<i32>> = Vec::with_capacity(plan.nodes.len());
    for (i, n) in plan.nodes.iter().enumerate() {
        let existing = db.node_id_by_change_id(&n.change_id)?;
        let id = match existing {
            Some(id) => {
                report.already_present += 1;
                Some(id)
            }
            None => {
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
            if db.node_id_by_change_id(&n.change_id)?.is_none()
                || db
                    .node_id_by_change_id(&plan.nodes[parent].change_id)?
                    .is_none()
            {
                report.edges_created += 1;
            }
            continue;
        }
        let (Some(from), Some(to)) = (ids[parent], ids[i]) else {
            continue;
        };
        if db.edge_exists(from, to, "leads_to")? {
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
}

/// Every top-level transcript in a directory, newest first, with whether
/// its root goal is already in the graph.
pub fn list(db: &Database, dir: &Path) -> Result<Vec<Listed>, TranscriptError> {
    let mut out = Vec::new();
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
        let root_cid = derive_change_id(&t.session_id, "goal", &first.uuid);
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
            imported: db.node_id_by_change_id(&root_cid)?.is_some(),
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
                ("goal", "why did you pick that name?"),
                ("observation", "Because the module already used it."),
            ]
        );
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

        let second = &plan.nodes[5];
        assert_eq!(second.parent, Some(0));
        assert_eq!(
            second.edge_rationale.as_deref(),
            Some("prompt 2 of the session")
        );
        assert_eq!(plan.nodes[6].node_type, "observation");

        // Same transcript, same ids.
        let again = super::plan(&parse(&p).unwrap(), Some(dir.path())).unwrap();
        let a: Vec<&str> = plan.nodes.iter().map(|n| n.change_id.as_str()).collect();
        let b: Vec<&str> = again.nodes.iter().map(|n| n.change_id.as_str()).collect();
        assert_eq!(a, b);
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
