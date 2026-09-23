//! `deciduous log-loop <event>`: the Claude Code hook that makes an agent log
//! to the graph as it works instead of afterwards.
//!
//! The hook that shipped before this (`require-action-node.sh`) asked the
//! local SQLite for a recent node. Since 0.19 agents write through the MCP
//! server, so that database never saw their writes; and it grepped `deciduous
//! nodes` output for `[goal]` and a timestamp that output has never contained,
//! so it never blocked anything at all.
//!
//! This one never asks a database. It counts what the agent does between
//! graph writes, per Claude Code session, in a small JSON file:
//!
//! - `pre` (PreToolUse `Edit|Write|NotebookEdit|Bash`): count the call; deny it
//!   once more than [`DEFAULT_LIMIT`] have gone unlogged, or at once after an
//!   unlogged git commit or merge. The deciduous MCP tools are not matched,
//!   so logging is always possible.
//! - `post-log` (PostToolUse on the deciduous MCP write tools): reset.
//! - `post-bash` (PostToolUse `Bash`): a `deciduous add|link|...` command
//!   counts as logging; a `git commit` or `git merge` demands a node.
//! - `stop` (Stop): refuse to end a turn with [`DEFAULT_STOP_MIN`] or more
//!   unlogged actions, unless Claude Code says the stop hook already fired
//!   (`stop_hook_active`), so it can never loop.
//!
//! `DECIDUOUS_LOG_LOOP=off` disables it; `DECIDUOUS_LOG_EVERY` and
//! `DECIDUOUS_LOG_STOP_MIN` tune the thresholds.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::Read;
use std::path::{Path, PathBuf};

pub const DEFAULT_LIMIT: u32 = 10;
pub const DEFAULT_STOP_MIN: u32 = 3;

#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct State {
    #[serde(default)]
    pub count: u32,
    #[serde(default)]
    pub commit: Option<String>,
    #[serde(default)]
    pub recent: Vec<String>,
}

/// What the hook tells Claude Code: nothing (allow), or a JSON object on
/// stdout. Kept separate from I/O so the rules are testable.
#[derive(Debug, PartialEq)]
pub enum Verdict {
    Allow,
    Say(Value),
}

pub struct Limits {
    pub every: u32,
    pub stop_min: u32,
}

impl Limits {
    pub fn from_env() -> Self {
        let num = |k: &str, d: u32| {
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

/// Where to log, for the message: the workspace the server would derive for
/// this directory and the current branch.
pub struct Place {
    pub workspace: String,
    pub branch: String,
}

impl Place {
    pub fn of(dir: &Path) -> Self {
        let branch = std::process::Command::new("git")
            .args([
                "-C",
                &dir.display().to_string(),
                "rev-parse",
                "--abbrev-ref",
                "HEAD",
            ])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "main".to_string());

        Place {
            workspace: crate::remote::workspace_for(dir),
            branch,
        }
    }

    fn how(&self) -> String {
        format!(
            "Call mcp__deciduous__add_node with workspace: \"{}\", branch: \"{}\" (node_type action for \
             what you did, outcome for a result, observation for a finding; the why goes in description), \
             then mcp__deciduous__add_edge to link it to its parent. Without the MCP server: \
             `deciduous add action \"...\"` and `deciduous link <parent> <id>`.",
            self.workspace, self.branch
        )
    }
}

fn is_git_commit(cmd: &str) -> bool {
    // A commit or merge anywhere in a compound command (`cd x && git commit`),
    // but not one that was only aborted or dry-run.
    let re = regex::Regex::new(r"\bgit\b[^|;&]*\b(commit|merge)\b").unwrap();
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

/// The rules. Mutates `state`; returns what to tell Claude Code.
pub fn decide(
    event: &str,
    input: &Value,
    state: &mut State,
    limits: &Limits,
    place: &dyn Fn() -> Place,
) -> Verdict {
    let tool = input["tool_name"].as_str().unwrap_or("");
    let tool_input = &input["tool_input"];

    match event {
        "post-log" => {
            *state = State::default();
            Verdict::Allow
        }
        "post-bash" => {
            let cmd = tool_input["command"].as_str().unwrap_or("");
            if is_cli_log(cmd) {
                *state = State::default();
            } else if is_git_commit(cmd) {
                state.commit = Some(
                    commit_sha(&input["tool_response"])
                        .unwrap_or_else(|| "the commit you just made".into()),
                );
            }
            Verdict::Allow
        }
        "pre" => {
            if let Some(sha) = &state.commit {
                let p = place();
                return Verdict::Say(json!({"hookSpecificOutput": {
                    "hookEventName": "PreToolUse",
                    "permissionDecision": "deny",
                    "permissionDecisionReason": format!(
                        "DECIDUOUS: you committed ({sha}) and have not logged it. Log it now with commit set to that sha. {}",
                        p.how()
                    ),
                }}));
            }
            state.count += 1;
            let label = tool_input["command"]
                .as_str()
                .or_else(|| tool_input["file_path"].as_str())
                .unwrap_or(tool);
            let label: String = label.chars().take(80).collect();
            state.recent.push(format!("{tool}: {label}"));
            let keep = limits.every.max(1) as usize;
            if state.recent.len() > keep {
                let cut = state.recent.len() - keep;
                state.recent.drain(..cut);
            }
            if state.count > limits.every {
                let p = place();
                return Verdict::Say(json!({"hookSpecificOutput": {
                    "hookEventName": "PreToolUse",
                    "permissionDecision": "deny",
                    "permissionDecisionReason": format!(
                        "DECIDUOUS: {} actions since your last graph write. Log what they did before doing more:\n  - {}\n{}",
                        state.count - 1,
                        state.recent.join("\n  - "),
                        p.how()
                    ),
                }}));
            }
            Verdict::Allow
        }
        "stop" => {
            if input["stop_hook_active"].as_bool() == Some(true) {
                return Verdict::Allow;
            }
            let what = match (&state.commit, state.count) {
                (Some(sha), _) => format!("an unlogged commit ({sha})"),
                (None, n) if n >= limits.stop_min => format!("{n} unlogged actions"),
                _ => return Verdict::Allow,
            };
            let p = place();
            Verdict::Say(json!({
                "decision": "block",
                "reason": format!(
                    "DECIDUOUS: this turn has {what}. Before finishing, log the outcome (what works, what does not). {}",
                    p.how()
                ),
            }))
        }
        _ => Verdict::Allow,
    }
}

fn state_path(session: &str) -> Option<PathBuf> {
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
            .join(format!("{safe}.json")),
    )
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
    let session = input["session_id"].as_str().unwrap_or("nosession");
    let Some(path) = state_path(session) else {
        return;
    };

    let mut state: State = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();

    let dir = input["cwd"]
        .as_str()
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let verdict = decide(event, &input, &mut state, &Limits::from_env(), &|| {
        Place::of(&dir)
    });

    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(body) = serde_json::to_string(&state) {
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, body).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }

    if let Verdict::Say(v) = verdict {
        println!("{v}");
    }
}

/// The MCP tools whose success counts as logging.
pub const MCP_WRITE_MATCHER: &str = "mcp__deciduous__(add_node|add_edge|update_node|log_decision|log_observation|capture_conversation_turn|close_thread|delete_node|delete_edge)";

const PRE_MATCHER: &str = "Edit|Write|NotebookEdit|Bash";

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
                "hooks": [{"type": "command", "command": "deciduous log-loop pre"}]}));
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
                "hooks": [{"type": "command", "command": "deciduous log-loop post-bash"}]}));
            changed = true;
        }
        if !entries.iter().any(|e| runs(e, "log-loop post-log")) {
            entries.push(json!({"matcher": MCP_WRITE_MATCHER,
                "hooks": [{"type": "command", "command": "deciduous log-loop post-log"}]}));
            changed = true;
        }
    }

    let stop = hooks.entry("Stop").or_insert_with(|| json!([]));
    if let Some(entries) = stop.as_array_mut() {
        if !entries.iter().any(|e| runs(e, "log-loop stop")) {
            entries.push(
                json!({"hooks": [{"type": "command", "command": "deciduous log-loop stop"}]}),
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

    #[test]
    fn denies_the_call_after_the_limit_and_names_what_went_unlogged() {
        let mut s = State::default();
        for i in 0..3 {
            assert_eq!(
                decide("pre", &bash(&format!("ls {i}")), &mut s, &lim(), &place),
                Verdict::Allow
            );
        }
        let v = decide("pre", &bash("ls 3"), &mut s, &lim(), &place);
        assert!(denied(&v));
        let Verdict::Say(j) = v else { unreachable!() };
        let reason = j["hookSpecificOutput"]["permissionDecisionReason"]
            .as_str()
            .unwrap();
        assert!(reason.contains("3 actions since your last graph write"));
        assert!(reason.contains("Bash: ls 3"));
        assert!(reason.contains("workspace: \"ws\", branch: \"br\""));
    }

    #[test]
    fn an_mcp_write_or_a_cli_write_resets_the_count() {
        let mut s = State {
            count: 9,
            ..Default::default()
        };
        decide("post-log", &json!({}), &mut s, &lim(), &place);
        assert_eq!(s, State::default());

        let mut s = State {
            count: 9,
            ..Default::default()
        };
        decide(
            "post-bash",
            &bash("cd repo && deciduous add action \"x\" -c 90"),
            &mut s,
            &lim(),
            &place,
        );
        assert_eq!(s.count, 0);
    }

    #[test]
    fn a_commit_blocks_everything_until_it_is_logged() {
        let mut s = State::default();
        let input = json!({"tool_name": "Bash", "tool_input": {"command": "cd x && git commit -m hi"},
                           "tool_response": {"stdout": "[main abc1234] hi"}});
        decide("post-bash", &input, &mut s, &lim(), &place);
        assert_eq!(s.commit.as_deref(), Some("abc1234"));
        assert!(denied(&decide("pre", &bash("ls"), &mut s, &lim(), &place)));
        decide("post-log", &json!({}), &mut s, &lim(), &place);
        assert_eq!(
            decide("pre", &bash("ls"), &mut s, &lim(), &place),
            Verdict::Allow
        );
    }

    #[test]
    fn an_aborted_merge_is_not_a_commit() {
        let mut s = State::default();
        decide(
            "post-bash",
            &bash("git merge --abort"),
            &mut s,
            &lim(),
            &place,
        );
        assert_eq!(s.commit, None);
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
            "deciduous log-loop stop"
        );

        // Idempotent.
        assert!(!merge_claude_settings(&mut v));
    }

    #[test]
    fn the_shipped_settings_template_is_already_merged() {
        let mut v: Value =
            serde_json::from_str(crate::init::templates::CLAUDE_SETTINGS_JSON).unwrap();
        assert!(!merge_claude_settings(&mut v));
    }

    #[test]
    fn stop_blocks_unlogged_work_but_never_twice_in_a_row() {
        let mut s = State {
            count: 2,
            ..Default::default()
        };
        let v = decide("stop", &json!({}), &mut s, &lim(), &place);
        assert!(matches!(v, Verdict::Say(ref j) if j["decision"] == "block"));
        assert_eq!(
            decide(
                "stop",
                &json!({"stop_hook_active": true}),
                &mut s,
                &lim(),
                &place
            ),
            Verdict::Allow
        );

        let mut quiet = State {
            count: 1,
            ..Default::default()
        };
        assert_eq!(
            decide("stop", &json!({}), &mut quiet, &lim(), &place),
            Verdict::Allow
        );
    }
}
