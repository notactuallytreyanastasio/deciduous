//! Command checking logic
//!
//! Checks git commands against configured rules and returns allow/block decisions.

use crate::git_guard::config::GitGuardConfig;
use crate::git_guard::logging::{log_blocked, log_command, LogStatus};
use crate::git_guard::rules;
use serde_json::Value;
use std::io::{self, Read};

/// Mode for checking commands based on input source
#[derive(Debug)]
pub enum CheckMode {
    /// Raw command string passed as argument
    Direct(String),
    /// Claude Code: JSON on stdin with tool_input.command
    Claude,
    /// Windsurf: JSON on stdin with command_line
    Windsurf,
    /// OpenCode: JSON on stdin
    OpenCode,
}

/// Result of checking a command
#[derive(Debug)]
pub enum CheckResult {
    /// Command is allowed
    Allow,
    /// Command is blocked
    Block {
        reason: String,
        guidance: Option<String>,
    },
    /// Not a git command, skip checking
    Skip,
}

/// Check a command against git-guard rules
pub fn check_command(mode: CheckMode, config: &GitGuardConfig) -> CheckResult {
    // Extract command based on mode
    let command = match mode {
        CheckMode::Direct(cmd) => cmd,
        CheckMode::Claude => parse_claude_stdin(),
        CheckMode::Windsurf => parse_windsurf_stdin(),
        CheckMode::OpenCode => parse_opencode_stdin(),
    };

    // Empty command or not a git command - skip
    if command.is_empty() || !command.starts_with("git ") {
        return CheckResult::Skip;
    }

    // Check if git-guard is enabled
    if !config.is_enabled() {
        log_command(&command, LogStatus::Allowed, config);
        return CheckResult::Allow;
    }

    // Check banned commands
    if let Some(pattern) = rules::matches_banned(&command, config) {
        log_blocked(&command, &format!("Banned pattern: {}", pattern), config);

        // Special handling for rebase - include guidance
        if rules::is_rebase_command(&command) && config.rebase.blocked {
            return CheckResult::Block {
                reason: format!("Matches banned pattern: {}", pattern),
                guidance: Some(config.rebase.guidance_message.clone()),
            };
        }

        return CheckResult::Block {
            reason: format!("Matches banned pattern: {}", pattern),
            guidance: None,
        };
    }

    // Check force push to protected branches
    if rules::is_force_push(&command) {
        if let Some(branch) = rules::targets_protected_branch(&command, config) {
            if !config.protected_branches.allow_force_push {
                log_blocked(
                    &command,
                    &format!("Force push to protected branch: {}", branch),
                    config,
                );
                return CheckResult::Block {
                    reason: format!("Force push to protected branch '{}' is not allowed", branch),
                    guidance: None,
                };
            }
        }
    }

    // Command is allowed
    log_command(&command, LogStatus::Allowed, config);
    CheckResult::Allow
}

/// Parse Claude Code PreToolUse JSON from stdin
///
/// Expected format:
/// ```json
/// {
///   "tool_name": "Bash",
///   "tool_input": {
///     "command": "git push --force origin main"
///   }
/// }
/// ```
fn parse_claude_stdin() -> String {
    let mut input = String::new();
    if io::stdin().read_to_string(&mut input).is_err() {
        return String::new();
    }

    let parsed: Value = match serde_json::from_str(&input) {
        Ok(v) => v,
        Err(_) => return String::new(),
    };

    // Check if it's a Bash tool call
    let tool_name = parsed["tool_name"].as_str().unwrap_or("");
    if tool_name != "Bash" {
        return String::new();
    }

    parsed["tool_input"]["command"]
        .as_str()
        .unwrap_or("")
        .to_string()
}

/// Parse Windsurf Cascade pre_run_command JSON from stdin
///
/// Expected format:
/// ```json
/// {
///   "command_line": "git push --force origin main",
///   "cwd": "/path/to/project"
/// }
/// ```
fn parse_windsurf_stdin() -> String {
    let mut input = String::new();
    if io::stdin().read_to_string(&mut input).is_err() {
        return String::new();
    }

    let parsed: Value = match serde_json::from_str(&input) {
        Ok(v) => v,
        Err(_) => return String::new(),
    };

    parsed["command_line"].as_str().unwrap_or("").to_string()
}

/// Parse OpenCode JSON from stdin
fn parse_opencode_stdin() -> String {
    let mut input = String::new();
    if io::stdin().read_to_string(&mut input).is_err() {
        return String::new();
    }

    let parsed: Value = match serde_json::from_str(&input) {
        Ok(v) => v,
        Err(_) => return String::new(),
    };

    // Try common field names
    parsed["command"]
        .as_str()
        .or_else(|| parsed["cmd"].as_str())
        .or_else(|| parsed["command_line"].as_str())
        .unwrap_or("")
        .to_string()
}

/// Format check result as JSON for Claude Code hooks
pub fn format_claude_response(result: &CheckResult) -> String {
    match result {
        CheckResult::Allow | CheckResult::Skip => {
            // Allow - exit with code 0, no special output needed
            String::new()
        }
        CheckResult::Block { reason, guidance } => {
            let mut message = format!("Git Guard: {}", reason);
            if let Some(guide) = guidance {
                message.push_str("\n\n");
                message.push_str(guide);
            }

            serde_json::json!({
                "hookSpecificOutput": {
                    "hookEventName": "PreToolUse",
                    "permissionDecision": "deny",
                    "permissionDecisionReason": message
                }
            })
            .to_string()
        }
    }
}

/// Format check result as JSON for Windsurf hooks
pub fn format_windsurf_response(result: &CheckResult) -> String {
    match result {
        CheckResult::Allow | CheckResult::Skip => String::new(),
        CheckResult::Block { reason, guidance } => {
            let mut message = format!("Git Guard: {}", reason);
            if let Some(guide) = guidance {
                message.push_str("\n\n");
                message.push_str(guide);
            }
            message
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> GitGuardConfig {
        GitGuardConfig {
            general: crate::git_guard::config::GeneralConfig {
                enabled: true,
                log_file: "/dev/null".to_string(),
            },
            banned: crate::git_guard::config::BannedConfig {
                commands: vec!["git reset --hard".to_string(), "git rebase".to_string()],
                block_message: String::new(),
            },
            rebase: crate::git_guard::config::RebaseConfig {
                blocked: true,
                guidance_message: "Do rebase manually".to_string(),
            },
            protected_branches: crate::git_guard::config::ProtectedBranchesConfig {
                names: vec!["main".to_string()],
                on_push: "allow".to_string(),
                allow_force_push: false,
            },
            ..Default::default()
        }
    }

    #[test]
    fn test_check_allowed() {
        let config = test_config();
        let result = check_command(CheckMode::Direct("git status".to_string()), &config);
        assert!(matches!(result, CheckResult::Allow));
    }

    #[test]
    fn test_check_banned() {
        let config = test_config();
        let result = check_command(
            CheckMode::Direct("git reset --hard HEAD~3".to_string()),
            &config,
        );
        assert!(matches!(result, CheckResult::Block { .. }));
    }

    #[test]
    fn test_check_rebase_with_guidance() {
        let config = test_config();
        let result = check_command(CheckMode::Direct("git rebase main".to_string()), &config);

        match result {
            CheckResult::Block { guidance, .. } => {
                assert!(guidance.is_some());
                assert!(guidance.unwrap().contains("manually"));
            }
            _ => panic!("Expected Block result"),
        }
    }

    #[test]
    fn test_check_force_push_protected() {
        let config = test_config();
        let result = check_command(
            CheckMode::Direct("git push --force origin main".to_string()),
            &config,
        );
        assert!(matches!(result, CheckResult::Block { .. }));
    }

    #[test]
    fn test_check_non_git() {
        let config = test_config();
        let result = check_command(CheckMode::Direct("npm install".to_string()), &config);
        assert!(matches!(result, CheckResult::Skip));
    }
}
