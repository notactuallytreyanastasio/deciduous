//! `deciduous log-loop <event>`: the Claude Code hook that makes an agent log
//! to the graph as it works instead of afterwards.
//!
//! The hook that shipped before this (`require-action-node.sh`) asked the
//! local SQLite for a recent node. Since 0.19 agents write through the MCP
//! server, so that database never saw their writes; and it grepped `deciduous
//! nodes` output for `[goal]` and a timestamp that output has never contained,
//! so it never blocked anything at all.
//!
//! This one never asks a database. Per Claude Code session it keeps
//! `<session>.events`, append-only, one JSON line per counted action or
//! commit, plus `<session>.reset` and `<session>.turn`, timestamps written by
//! atomic rename. Nothing is read, modified and written back, so hooks that
//! run in parallel (Claude Code fires them for parallel tool calls) cannot
//! lose each other's updates. What counts is every event newer than the last
//! graph write, and for Stop, newer than the start of the turn too.
//!
//! - `pre` (PreToolUse `Edit|Write|NotebookEdit|Bash`): record the action;
//!   deny it once more than [`DEFAULT_LIMIT`] are unlogged, or at once after
//!   an unlogged git commit or merge. Not counted, never denied: read-only
//!   Bash (every segment of the command is a read and nothing is redirected
//!   into a file) and `deciduous add|link|...`, which is itself logging.
//! - `post-log` (PostToolUse on any `mcp__*deciduous*__` write tool): reset.
//! - `post-bash` (PostToolUse `Bash`): `deciduous add|link|...` resets; `git
//!   commit` / `git merge` in subcommand position records a commit.
//! - `turn` (UserPromptSubmit): a new turn starts.
//! - `stop` (Stop): refuse to end a turn with [`DEFAULT_STOP_MIN`] or more
//!   actions unlogged in that turn, or an unlogged commit; allowed on the
//!   retry (`stop_hook_active`), so it can never loop.
//!
//! `DECIDUOUS_LOG_LOOP=off` disables it; `DECIDUOUS_LOG_EVERY` and
//! `DECIDUOUS_LOG_STOP_MIN` tune the thresholds. A project whose
//! `.deciduous/config.toml` disables hooks or `require-action-node` does not
//! get these hooks installed by `update` or `hooks install`.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub const DEFAULT_LIMIT: usize = 10;
pub const DEFAULT_STOP_MIN: usize = 3;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Event {
    /// Seconds since the epoch.
    pub t: f64,
    /// "act" or "commit".
    pub k: String,
    /// What it was, for the denial message.
    pub l: String,
}

/// What the rules see: the session's events and its two timestamps.
#[derive(Debug, Default)]
pub struct Snapshot {
    pub events: Vec<Event>,
    pub reset: f64,
    pub turn: f64,
}

/// What the rules want written. Applied by [`run`], never inside [`decide`].
#[derive(Debug, PartialEq)]
pub enum Effect {
    Append(Event),
    Reset,
    Turn,
}

/// What the hook tells Claude Code: nothing (allow), or a JSON object on
/// stdout.
#[derive(Debug, PartialEq)]
pub enum Verdict {
    Allow,
    Say(Value),
}

pub struct Limits {
    pub every: usize,
    pub stop_min: usize,
}

impl Limits {
    pub fn from_env() -> Self {
        let num = |k: &str, d: usize| {
            std::env::var(k)
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(d)
        };
        Limits {
            every: num("DECIDUOUS_LOG_EVERY", DEFAULT_LIMIT),
            stop_min: num("DECIDUOUS_LOG_STOP_MIN", DEFAULT_STOP_MIN),
        }
    }
}

/// Where to log, for the message.
pub struct Place {
    pub workspace: String,
    pub branch: String,
}

fn git_out(dir: &Path, args: &[&str]) -> Option<String> {
    std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

impl Place {
    /// The repository's name, not the worktree directory's: an agent in
    /// `agents/agent-3/` of the tetris-arena repo writes to "tetris-arena".
    /// `[remote] workspace` in the project config wins when set.
    pub fn of(dir: &Path) -> Self {
        let configured = crate::config::Config::load().remote.workspace;
        let from_git = git_out(
            dir,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        )
        .map(|common| {
            let common = PathBuf::from(common);
            let root = if common.file_name().map(|n| n == ".git").unwrap_or(false) {
                common.parent().map(Path::to_path_buf).unwrap_or(common)
            } else {
                common
            };
            root.file_name()
                .map(|n| n.to_string_lossy().to_lowercase())
                .unwrap_or_else(|| crate::remote::FALLBACK_WORKSPACE.to_string())
        });
        Place {
            workspace: configured
                .or(from_git)
                .unwrap_or_else(|| crate::remote::FALLBACK_WORKSPACE.to_string()),
            branch: git_out(dir, &["rev-parse", "--abbrev-ref", "HEAD"])
                .unwrap_or_else(|| "main".into()),
        }
    }

    fn how(&self) -> String {
        format!(
            "Log it: mcp__deciduous__add_node with workspace: \"{}\", branch: \"{}\", and parent_id set to \
             the node it belongs under, so it is created and linked in one call (node_type action for what \
             you did, outcome for a result, observation for a finding; the why goes in description). Never \
             send add_edge in the same batch as the add_node whose id it needs. Without MCP: \
             `deciduous add action \"...\"`.",
            self.workspace, self.branch
        )
    }
}

const READ_CMDS: &[&str] = &[
    "ls",
    "cat",
    "head",
    "tail",
    "grep",
    "rg",
    "find",
    "wc",
    "jq",
    "echo",
    "printf",
    "pwd",
    "which",
    "awk",
    "sort",
    "uniq",
    "diff",
    "cut",
    "tr",
    "stat",
    "file",
    "du",
    "df",
    "date",
    "env",
    "cd",
    "true",
    "test",
    "[",
    "less",
    "more",
    "tree",
    "basename",
    "dirname",
    "realpath",
    "readlink",
    "cksum",
    "shasum",
    "sha256sum",
    "md5",
    "nl",
    "column",
    "xxd",
    "od",
    "type",
];
const GIT_READ: &[&str] = &[
    "status",
    "log",
    "diff",
    "show",
    "rev-parse",
    "ls-files",
    "ls-remote",
    "blame",
    "grep",
    "describe",
    "shortlog",
    "reflog",
    "cat-file",
    "merge-base",
    "for-each-ref",
    "name-rev",
];

/// True when every segment of a shell command only reads. Deliberately
/// narrow: anything it does not recognise counts as work.
pub fn is_read_only(cmd: &str) -> bool {
    let redirect = regex::Regex::new(r"(^|[^0-9&])>{1,2}\s*([^/&\s]|/[^d]|/d[^e])").unwrap();
    if redirect.is_match(cmd) {
        return false;
    }
    let seps = regex::Regex::new(r"&&|\|\||;|\||\n").unwrap();
    for seg in seps.split(cmd) {
        let mut words = seg
            .split_whitespace()
            .map(|w| w.trim_matches(|c| c == '"' || c == '\'' || c == '(' || c == ')'))
            .skip_while(|w| {
                w.contains('=')
                    && w.chars()
                        .next()
                        .map(|c| c.is_ascii_alphabetic() || c == '_')
                        .unwrap_or(false)
            });
        let Some(first) = words.next() else { continue };
        let first = first.rsplit('/').next().unwrap_or(first);
        let rest: Vec<&str> = words.collect();
        if first == "sed" {
            if rest.iter().any(|a| a.starts_with("-i")) {
                return false;
            }
            continue;
        }
        if first == "git" {
            let mut r = rest.as_slice();
            while r.len() >= 2 && (r[0] == "-C" || r[0] == "-c") {
                r = &r[2..];
            }
            let ok = match r {
                [sub, ..] if GIT_READ.contains(sub) => true,
                [sub] if ["branch", "tag", "remote", "stash", "worktree"].contains(sub) => true,
                [sub, flag, ..]
                    if ["branch", "tag", "remote", "stash", "worktree"].contains(sub) =>
                {
                    ["-v", "-vv", "-a", "-l", "--list", "list", "show"].contains(flag)
                }
                _ => false,
            };
            if !ok {
                return false;
            }
            continue;
        }
        if !READ_CMDS.contains(&first) {
            return false;
        }
    }
    true
}

fn is_git_commit(cmd: &str) -> bool {
    // `commit` or `merge` as git's subcommand, after any -C/-c options:
    // not `git merge-base`, `git log --grep=merge`, a path or a branch name.
    let re = regex::Regex::new(r"(^|[;&|(\s])git(\s+-[cC]\s+\S+)*\s+(commit|merge)(\s|$)").unwrap();
    re.is_match(cmd) && !cmd.contains("--abort") && !cmd.contains("--dry-run")
}

fn is_cli_log(cmd: &str) -> bool {
    let re =
        regex::Regex::new(r"(^|[;&|(]\s*|\s)deciduous\s+(add|link|status|prompt|delete|unlink)\b")
            .unwrap();
    re.is_match(cmd)
}

fn commit_sha(response: &Value) -> Option<String> {
    let text = response.to_string();
    let re = regex::Regex::new(r"\[[^\]\s]+ ([0-9a-f]{7,40})\]").unwrap();
    re.captures(&text).map(|c| c[1].to_string())
}

/// The rules. Pure: reads a snapshot, returns what to say and what to write.
pub fn decide(
    event: &str,
    input: &Value,
    snap: &Snapshot,
    limits: &Limits,
    now: f64,
    place: &dyn Fn() -> Place,
) -> (Verdict, Vec<Effect>) {
    let tool = input["tool_name"].as_str().unwrap_or("");
    let tool_input = &input["tool_input"];
    let cmd = tool_input["command"].as_str().unwrap_or("");
    let since = |t: f64| snap.events.iter().filter(move |e| e.t > t);

    match event {
        "post-log" => (Verdict::Allow, vec![Effect::Reset]),
        "turn" => (Verdict::Allow, vec![Effect::Turn]),
        "post-bash" => {
            if is_cli_log(cmd) {
                (Verdict::Allow, vec![Effect::Reset])
            } else if is_git_commit(cmd) {
                let l = commit_sha(&input["tool_response"])
                    .unwrap_or_else(|| "the commit you just made".into());
                (
                    Verdict::Allow,
                    vec![Effect::Append(Event {
                        t: now,
                        k: "commit".into(),
                        l,
                    })],
                )
            } else {
                (Verdict::Allow, vec![])
            }
        }
        "pre" => {
            if tool == "Bash" && (is_cli_log(cmd) || is_read_only(cmd)) {
                return (Verdict::Allow, vec![]);
            }
            if let Some(c) = since(snap.reset).rfind(|e| e.k == "commit") {
                let p = place();
                return (
                    Verdict::Say(json!({"hookSpecificOutput": {
                        "hookEventName": "PreToolUse",
                        "permissionDecision": "deny",
                        "permissionDecisionReason": format!(
                            "DECIDUOUS: you committed ({}) and have not logged it; set commit to that sha. {}",
                            c.l,
                            p.how()
                        ),
                    }})),
                    vec![],
                );
            }
            let label = if cmd.is_empty() {
                tool_input["file_path"].as_str().unwrap_or(tool)
            } else {
                cmd
            };
            let label: String = label.chars().take(80).collect();
            let this = Event {
                t: now,
                k: "act".into(),
                l: format!("{tool}: {label}"),
            };
            let mut acts: Vec<&Event> = since(snap.reset).filter(|e| e.k == "act").collect();
            acts.push(&this);
            let verdict = if acts.len() > limits.every {
                let p = place();
                let listing: Vec<&str> = acts
                    .iter()
                    .rev()
                    .take(limits.every)
                    .rev()
                    .map(|e| e.l.as_str())
                    .collect();
                Verdict::Say(json!({"hookSpecificOutput": {
                    "hookEventName": "PreToolUse",
                    "permissionDecision": "deny",
                    "permissionDecisionReason": format!(
                        "DECIDUOUS: {} actions since your last graph write. Log what they did before doing more:\n  - {}\n{}",
                        acts.len() - 1,
                        listing.join("\n  - "),
                        p.how()
                    ),
                }}))
            } else {
                Verdict::Allow
            };
            (verdict, vec![Effect::Append(this)])
        }
        "stop" => {
            if input["stop_hook_active"].as_bool() == Some(true) {
                return (Verdict::Allow, vec![]);
            }
            let from = snap.reset.max(snap.turn);
            let commit = since(from).rfind(|e| e.k == "commit");
            let acts = since(from).filter(|e| e.k == "act").count();
            let what = match (commit, acts) {
                (Some(c), _) => format!("an unlogged commit ({})", c.l),
                (None, n) if n >= limits.stop_min => format!("{n} unlogged actions"),
                _ => return (Verdict::Allow, vec![]),
            };
            let p = place();
            (
                Verdict::Say(json!({
                    "decision": "block",
                    "reason": format!(
                        "DECIDUOUS: this turn has {what}. Before finishing, log the outcome (what works, what does not). {}",
                        p.how()
                    ),
                })),
                vec![],
            )
        }
        _ => (Verdict::Allow, vec![]),
    }
}

fn state_base(session: &str) -> Option<PathBuf> {
    let safe: String = session
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let home = std::env::var_os("HOME")?;
    Some(
        PathBuf::from(home)
            .join(".claude")
            .join("deciduous-log-loop")
            .join(safe),
    )
}

fn with_ext(base: &Path, ext: &str) -> PathBuf {
    let mut s = base.as_os_str().to_owned();
    s.push(ext);
    PathBuf::from(s)
}

fn read_ts(path: &Path) -> f64 {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0.0)
}

fn write_ts(path: &Path, now: f64) {
    // A tmp name per process, so two hooks never publish each other's write.
    let tmp = with_ext(path, &format!(".{}.tmp", std::process::id()));
    if std::fs::write(&tmp, now.to_string()).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

/// Entry point for the subcommand. Never fails the tool call on its own
/// errors: a hook that cannot read its input or state allows.
pub fn run(event: &str) {
    if matches!(
        std::env::var("DECIDUOUS_LOG_LOOP")
            .unwrap_or_default()
            .to_lowercase()
            .as_str(),
        "off" | "0" | "false"
    ) {
        return;
    }
    let mut raw = String::new();
    if std::io::stdin().read_to_string(&mut raw).is_err() {
        return;
    }
    let Ok(input) = serde_json::from_str::<Value>(&raw) else {
        return;
    };
    let Some(base) = state_base(input["session_id"].as_str().unwrap_or("nosession")) else {
        return;
    };
    if let Some(dir) = base.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let (events_path, reset_path, turn_path) = (
        with_ext(&base, ".events"),
        with_ext(&base, ".reset"),
        with_ext(&base, ".turn"),
    );

    let snap = Snapshot {
        events: std::fs::read_to_string(&events_path)
            .unwrap_or_default()
            .lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect(),
        reset: read_ts(&reset_path),
        turn: read_ts(&turn_path),
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    let dir = input["cwd"]
        .as_str()
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    let (verdict, effects) = decide(event, &input, &snap, &Limits::from_env(), now, &|| {
        Place::of(&dir)
    });

    for effect in effects {
        match effect {
            Effect::Reset => write_ts(&reset_path, now),
            Effect::Turn => write_ts(&turn_path, now),
            Effect::Append(ev) => {
                // One short line per O_APPEND write: appends do not interleave.
                if let (Ok(line), Ok(mut f)) = (
                    serde_json::to_string(&ev),
                    std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&events_path),
                ) {
                    let _ = f.write_all(format!("{line}\n").as_bytes());
                }
            }
        }
    }

    if let Verdict::Say(v) = verdict {
        println!("{v}");
    }
}

/// The MCP tools whose success counts as logging.
pub const MCP_WRITE_MATCHER: &str = "mcp__.*deciduous.*__(add_node|add_edge|update_node|log_decision|log_observation|capture_conversation_turn|close_thread|delete_node|delete_edge)";

const PRE_MATCHER: &str = "Edit|Write|NotebookEdit|Bash";

/// Adds one `{matcher, hooks: [{command}]}` entry under `event` unless some
/// entry there already runs exactly `command`. Returns whether it added one.
pub fn ensure_entry(settings: &mut Value, event: &str, matcher: &str, command: &str) -> bool {
    let Some(obj) = settings.as_object_mut() else {
        return false;
    };
    let hooks = obj.entry("hooks").or_insert_with(|| json!({}));
    let Some(hooks) = hooks.as_object_mut() else {
        return false;
    };
    let entries = hooks.entry(event).or_insert_with(|| json!([]));
    let Some(entries) = entries.as_array_mut() else {
        return false;
    };
    let present = entries.iter().any(|e| {
        e["hooks"]
            .as_array()
            .is_some_and(|hs| hs.iter().any(|h| h["command"] == command))
    });
    if !present {
        entries
            .push(json!({"matcher": matcher, "hooks": [{"type": "command", "command": command}]}));
    }
    !present
}

/// Brings a project's `.claude/settings.json` up to the log-loop hooks
/// without touching anything the user added. Returns whether it changed.
///
/// - the entry running `require-action-node.sh` gets the wider matcher (it
///   used to be `Edit|Write`, so agents editing through Bash were never seen);
/// - a PostToolUse entry for the deciduous MCP write tools and a Stop entry
///   are added once, recognised by their `deciduous log-loop` command.
pub fn merge_claude_settings(settings: &mut Value) -> bool {
    let mut changed = false;
    if !settings.is_object() {
        *settings = json!({});
        changed = true;
    }
    let hooks = settings
        .as_object_mut()
        .unwrap()
        .entry("hooks")
        .or_insert_with(|| json!({}));
    if !hooks.is_object() {
        *hooks = json!({});
        changed = true;
    }
    let hooks = hooks.as_object_mut().unwrap();

    let runs = |entry: &Value, needle: &str| {
        entry["hooks"]
            .as_array()
            .map(|hs| {
                hs.iter()
                    .any(|h| h["command"].as_str().unwrap_or("").contains(needle))
            })
            .unwrap_or(false)
    };

    let pre = hooks.entry("PreToolUse").or_insert_with(|| json!([]));
    if let Some(entries) = pre.as_array_mut() {
        let mut found = false;
        for e in entries.iter_mut() {
            if runs(e, "require-action-node") || runs(e, "log-loop pre") {
                found = true;
                if e["matcher"] != PRE_MATCHER {
                    e["matcher"] = json!(PRE_MATCHER);
                    changed = true;
                }
            }
        }
        if !found {
            entries.push(json!({"matcher": PRE_MATCHER,
                "hooks": [{"type": "command", "command": "deciduous log-loop pre || true"}]}));
            changed = true;
        }
    }

    let post = hooks.entry("PostToolUse").or_insert_with(|| json!([]));
    if let Some(entries) = post.as_array_mut() {
        if !entries
            .iter()
            .any(|e| runs(e, "post-commit-reminder") || runs(e, "log-loop post-bash"))
        {
            entries.push(json!({"matcher": "Bash",
                "hooks": [{"type": "command", "command": "deciduous log-loop post-bash || true"}]}));
            changed = true;
        }
        if !entries.iter().any(|e| runs(e, "log-loop post-log")) {
            entries.push(json!({"matcher": MCP_WRITE_MATCHER,
                "hooks": [{"type": "command", "command": "deciduous log-loop post-log || true"}]}));
            changed = true;
        }
    }

    if let Some(entries) = hooks.get_mut("PostToolUse").and_then(Value::as_array_mut) {
        for e in entries.iter_mut() {
            if runs(e, "log-loop post-log") && e["matcher"] != MCP_WRITE_MATCHER {
                e["matcher"] = json!(MCP_WRITE_MATCHER);
                changed = true;
            }
        }
    }

    let turn = hooks.entry("UserPromptSubmit").or_insert_with(|| json!([]));
    if let Some(entries) = turn.as_array_mut() {
        if !entries.iter().any(|e| runs(e, "log-loop turn")) {
            entries.push(json!({"hooks": [{"type": "command", "command": "deciduous log-loop turn || true"}]}));
            changed = true;
        }
    }

    // Every log-loop command must tolerate a binary without `log-loop` (exit
    // 2 is Claude Code's "block"); repair entries written before that rule.
    for entries in hooks.values_mut().filter_map(Value::as_array_mut) {
        for entry in entries.iter_mut() {
            if let Some(hs) = entry["hooks"].as_array_mut() {
                for h in hs.iter_mut() {
                    if let Some(cmd) = h["command"].as_str() {
                        if cmd.starts_with("deciduous log-loop ") && !cmd.contains("||") {
                            h["command"] = json!(format!("{cmd} || true"));
                            changed = true;
                        }
                    }
                }
            }
        }
    }

    let stop = hooks.entry("Stop").or_insert_with(|| json!([]));
    if let Some(entries) = stop.as_array_mut() {
        if !entries.iter().any(|e| runs(e, "log-loop stop")) {
            entries.push(
                json!({"hooks": [{"type": "command", "command": "deciduous log-loop stop || true"}]}),
            );
            changed = true;
        }
    }

    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place() -> Place {
        Place {
            workspace: "ws".into(),
            branch: "br".into(),
        }
    }
    fn lim() -> Limits {
        Limits {
            every: 3,
            stop_min: 2,
        }
    }
    fn bash(cmd: &str) -> Value {
        json!({"tool_name": "Bash", "tool_input": {"command": cmd}})
    }
    fn denied(v: &Verdict) -> bool {
        matches!(v, Verdict::Say(j) if j["hookSpecificOutput"]["permissionDecision"] == "deny")
    }
    /// Runs one hook call against a snapshot and applies its effects, the way
    /// `run` does with files.
    fn step(snap: &mut Snapshot, event: &str, input: &Value, now: f64) -> Verdict {
        let (v, effects) = decide(event, input, snap, &lim(), now, &place);
        for e in effects {
            match e {
                Effect::Append(ev) => snap.events.push(ev),
                Effect::Reset => snap.reset = now,
                Effect::Turn => snap.turn = now,
            }
        }
        v
    }

    #[test]
    fn denies_work_past_the_limit_and_names_it() {
        let mut s = Snapshot::default();
        for i in 0..3 {
            assert_eq!(
                step(&mut s, "pre", &bash(&format!("touch {i}")), i as f64 + 1.0),
                Verdict::Allow
            );
        }
        let v = step(&mut s, "pre", &bash("touch 3"), 4.0);
        assert!(denied(&v));
        let Verdict::Say(j) = v else { unreachable!() };
        let reason = j["hookSpecificOutput"]["permissionDecisionReason"]
            .as_str()
            .unwrap();
        assert!(reason.contains("3 actions since your last graph write"));
        assert!(reason.contains("Bash: touch 3"));
        assert!(reason.contains("workspace: \"ws\", branch: \"br\""));
    }

    #[test]
    fn reads_are_not_work() {
        let mut s = Snapshot::default();
        for cmd in [
            "git diff HEAD~1",
            "cat a | grep b",
            "git merge-base HEAD main",
            "git log --grep=merge",
            "ls > /dev/null 2>&1",
            "sed -n 1,5p x",
            "cd repo && git status",
            "git -C x branch -a",
        ] {
            for _ in 0..5 {
                assert_eq!(
                    step(&mut s, "pre", &bash(cmd), 1.0),
                    Verdict::Allow,
                    "{cmd}"
                );
            }
        }
        assert!(s.events.is_empty());
        for cmd in [
            "ls > out.txt",
            "sed -i s/a/b/ f",
            "git checkout main",
            "cargo build",
            "rm x",
        ] {
            assert!(!is_read_only(cmd), "{cmd}");
        }
    }

    #[test]
    fn logging_through_the_cli_is_never_denied_and_resets() {
        let mut s = Snapshot::default();
        for i in 0..5 {
            step(&mut s, "pre", &bash("touch x"), i as f64 + 1.0);
        }
        assert_eq!(
            step(
                &mut s,
                "pre",
                &bash("deciduous add action \"x\" -c 90"),
                6.0
            ),
            Verdict::Allow
        );
        step(
            &mut s,
            "post-bash",
            &bash("deciduous add action \"x\" -c 90"),
            7.0,
        );
        assert_eq!(step(&mut s, "pre", &bash("touch y"), 8.0), Verdict::Allow);
    }

    #[test]
    fn only_a_real_commit_or_merge_is_a_commit() {
        for cmd in [
            "git merge-base HEAD main",
            "git log --grep=merge",
            "git diff -- src/commit.rs",
            "git checkout -b fix/commit-hook",
            "git show HEAD:lib/merge.ex",
            "git merge --abort",
        ] {
            assert!(!is_git_commit(cmd), "{cmd}");
        }
        for cmd in [
            "git commit -m x",
            "cd a && git -C b commit -qm x",
            "git merge origin/main",
            "git -c user.name=x commit",
        ] {
            assert!(is_git_commit(cmd), "{cmd}");
        }
    }

    #[test]
    fn a_commit_blocks_everything_until_it_is_logged() {
        let mut s = Snapshot::default();
        let input = json!({"tool_name": "Bash", "tool_input": {"command": "git commit -m hi"},
                           "tool_response": {"stdout": "[main abc1234] hi"}});
        step(&mut s, "post-bash", &input, 1.0);
        assert!(denied(&step(
            &mut s,
            "pre",
            &json!({"tool_name": "Edit", "tool_input": {"file_path": "/x"}}),
            2.0
        )));
        step(&mut s, "post-log", &json!({}), 3.0);
        assert_eq!(step(&mut s, "pre", &bash("touch y"), 4.0), Verdict::Allow);
    }

    #[test]
    fn a_reset_is_not_lost_to_a_parallel_pre() {
        // pre and post-log race: pre read the snapshot before the reset, then
        // appends. With an event log the reset still covers everything older.
        let mut s = Snapshot::default();
        for i in 0..3 {
            step(&mut s, "pre", &bash("touch x"), i as f64 + 1.0);
        }
        let stale = Snapshot {
            events: s.events.clone(),
            reset: s.reset,
            turn: s.turn,
        };
        let (_, effects) = decide("pre", &bash("touch y"), &stale, &lim(), 4.0, &place);
        step(&mut s, "post-log", &json!({}), 4.5);
        for e in effects {
            if let Effect::Append(ev) = e {
                s.events.push(ev);
            }
        }
        assert_eq!(step(&mut s, "pre", &bash("touch z"), 5.0), Verdict::Allow);
    }

    #[test]
    fn stop_counts_this_turn_only_and_never_blocks_twice() {
        let mut s = Snapshot::default();
        step(&mut s, "turn", &json!({}), 1.0);
        step(&mut s, "pre", &bash("touch a"), 2.0);
        step(&mut s, "pre", &bash("touch b"), 3.0);
        assert!(
            matches!(step(&mut s, "stop", &json!({}), 4.0), Verdict::Say(ref j) if j["decision"] == "block")
        );
        assert_eq!(
            step(&mut s, "stop", &json!({"stop_hook_active": true}), 5.0),
            Verdict::Allow
        );
        // Next turn, a plain question: nothing new, so it may end.
        step(&mut s, "turn", &json!({}), 6.0);
        assert_eq!(step(&mut s, "stop", &json!({}), 7.0), Verdict::Allow);
    }

    #[test]
    fn merging_an_old_install_widens_the_matcher_adds_post_log_and_stop_and_keeps_user_hooks() {
        let mut v: Value = serde_json::from_str(r#"{"model": "x", "hooks": {
            "PreToolUse": [
              {"matcher": "Edit|Write", "hooks": [{"type": "command", "command": "\"$CLAUDE_PROJECT_DIR/.claude/hooks/require-action-node.sh\""}]},
              {"matcher": "Bash", "hooks": [{"type": "command", "command": "mine.sh"}]}],
            "PostToolUse": [
              {"matcher": "Bash", "hooks": [{"type": "command", "command": "\"$CLAUDE_PROJECT_DIR/.claude/hooks/post-commit-reminder.sh\""}]}]
        }}"#).unwrap();
        assert!(merge_claude_settings(&mut v));
        assert_eq!(v["model"], "x");
        assert_eq!(v["hooks"]["PreToolUse"][0]["matcher"], PRE_MATCHER);
        assert_eq!(
            v["hooks"]["PreToolUse"][1]["hooks"][0]["command"],
            "mine.sh"
        );
        assert_eq!(v["hooks"]["PostToolUse"].as_array().unwrap().len(), 2);
        assert_eq!(v["hooks"]["PostToolUse"][1]["matcher"], MCP_WRITE_MATCHER);
        assert_eq!(
            v["hooks"]["Stop"][0]["hooks"][0]["command"],
            "deciduous log-loop stop || true"
        );

        // Idempotent.
        assert!(!merge_claude_settings(&mut v));

        // An entry written without the old-binary guard is repaired.
        v["hooks"]["Stop"][0]["hooks"][0]["command"] = json!("deciduous log-loop stop");
        assert!(merge_claude_settings(&mut v));
        assert_eq!(
            v["hooks"]["Stop"][0]["hooks"][0]["command"],
            "deciduous log-loop stop || true"
        );
    }

    #[test]
    fn the_shipped_settings_template_is_already_merged() {
        let mut v: Value =
            serde_json::from_str(crate::init::templates::CLAUDE_SETTINGS_JSON).unwrap();
        assert!(!merge_claude_settings(&mut v));
    }
}
