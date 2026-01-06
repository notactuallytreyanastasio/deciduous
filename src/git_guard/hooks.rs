//! Git native hook handlers
//!
//! Handles pre-commit, pre-push, and pre-rebase hooks.

use crate::git_guard::config::GitGuardConfig;
use crate::git_guard::logging::{log_blocked, log_command, LogStatus};
use crate::git_guard::rules;
use std::io::{self, BufRead};
use std::process::Command;

/// Type of git hook being handled
#[derive(Debug, Clone, Copy)]
pub enum HookType {
    PreCommit,
    PrePush,
    PreRebase,
}

impl HookType {
    /// Parse hook type from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pre-commit" => Some(HookType::PreCommit),
            "pre-push" => Some(HookType::PrePush),
            "pre-rebase" => Some(HookType::PreRebase),
            _ => None,
        }
    }
}

/// Handle a git hook event
///
/// Returns Ok(()) to allow the operation, Err(message) to block it.
pub fn handle_hook(hook_type: HookType, config: &GitGuardConfig) -> Result<(), String> {
    if !config.is_enabled() {
        return Ok(());
    }

    match hook_type {
        HookType::PreCommit => handle_pre_commit(config),
        HookType::PrePush => handle_pre_push(config),
        HookType::PreRebase => handle_pre_rebase(config),
    }
}

/// Handle pre-commit hook
///
/// Checks for:
/// - Sensitive files in staged changes
/// - AI attribution in commit message (via prepare-commit-msg, not here)
fn handle_pre_commit(config: &GitGuardConfig) -> Result<(), String> {
    log_command("git commit (pre-commit hook)", LogStatus::Checked, config);

    // Get list of staged files
    let staged_files = get_staged_files()?;

    if staged_files.is_empty() {
        return Ok(());
    }

    // Check for sensitive files
    let file_refs: Vec<&str> = staged_files.iter().map(|s| s.as_str()).collect();
    let sensitive = rules::find_sensitive_files(&file_refs, config);

    if !sensitive.is_empty() {
        log_blocked(
            "git commit",
            &format!("Sensitive files: {:?}", sensitive),
            config,
        );

        let files_list = sensitive
            .iter()
            .map(|f| format!("  - {}", f))
            .collect::<Vec<_>>()
            .join("\n");

        let message = config
            .sensitive_files
            .block_message
            .replace("{files}", &files_list);

        return Err(message);
    }

    Ok(())
}

/// Handle pre-push hook
///
/// Checks for:
/// - Force push to protected branches
fn handle_pre_push(config: &GitGuardConfig) -> Result<(), String> {
    log_command("git push (pre-push hook)", LogStatus::Checked, config);

    // Read push info from stdin
    // Format per line: <local ref> <local sha> <remote ref> <remote sha>
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let line = line.map_err(|e| format!("Failed to read stdin: {}", e))?;
        let parts: Vec<&str> = line.split_whitespace().collect();

        if parts.len() >= 3 {
            let remote_ref = parts[2];
            // Extract branch name from refs/heads/main
            let branch = remote_ref.strip_prefix("refs/heads/").unwrap_or(remote_ref);

            // Check if pushing to protected branch
            if config
                .protected_branches
                .names
                .contains(&branch.to_string())
            {
                // Check if this is a force push by examining local and remote shas
                let local_sha = parts[1];
                let remote_sha = parts[3];

                // If remote sha is all zeros, it's a new branch (OK)
                // If local sha is all zeros, it's a delete (check separately)
                if remote_sha != "0000000000000000000000000000000000000000"
                    && local_sha != "0000000000000000000000000000000000000000"
                {
                    // Could check if local is descendant of remote here
                    // For now, just log the push
                    log_command(
                        &format!("git push to protected branch: {}", branch),
                        LogStatus::Checked,
                        config,
                    );
                }
            }
        }
    }

    Ok(())
}

/// Handle pre-rebase hook
///
/// ALWAYS blocks rebase and shows guidance.
fn handle_pre_rebase(config: &GitGuardConfig) -> Result<(), String> {
    log_blocked("git rebase", "All rebases blocked by git-guard", config);

    if config.rebase.blocked {
        return Err(config.rebase.guidance_message.clone());
    }

    Ok(())
}

/// Get list of staged files
fn get_staged_files() -> Result<Vec<String>, String> {
    let output = Command::new("git")
        .args(["diff", "--cached", "--name-only"])
        .output()
        .map_err(|e| format!("Failed to run git diff: {}", e))?;

    if !output.status.success() {
        return Err("git diff --cached failed".to_string());
    }

    let files = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| l.to_string())
        .collect();

    Ok(files)
}

/// Check commit message for AI attribution and length
///
/// Called from commit-msg hook (not pre-commit).
pub fn check_commit_message(message: &str, config: &GitGuardConfig) -> Result<(), String> {
    // Check AI attribution if enabled
    if config.commit_message.block_ai_attribution {
        if let Some(pattern) = rules::contains_ai_attribution(message, config) {
            return Err(format!(
                "🤖 BLOCKED: Commit message contains AI attribution\n\n\
                 Found pattern: '{}'\n\n\
                 Remove AI-generated attribution from your commit message.\n\
                 This is configured in .deciduous/git-guard.toml",
                pattern
            ));
        }
    }

    // Always check message length
    if rules::is_message_too_short(message, config) {
        return Err(format!(
            "📝 BLOCKED: Commit message too short\n\n\
             Minimum length: {} characters\n\
             Your message: {} characters",
            config.commit_message.min_length,
            message.trim().len()
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hook_type_from_str() {
        assert!(matches!(
            HookType::from_str("pre-commit"),
            Some(HookType::PreCommit)
        ));
        assert!(matches!(
            HookType::from_str("pre-push"),
            Some(HookType::PrePush)
        ));
        assert!(matches!(
            HookType::from_str("pre-rebase"),
            Some(HookType::PreRebase)
        ));
        assert!(HookType::from_str("invalid").is_none());
    }

    #[test]
    fn test_check_commit_message_ai_attribution() {
        let config = GitGuardConfig::default();

        assert!(check_commit_message("feat: add auth", &config).is_ok());
        assert!(check_commit_message("feat: add auth\n\nGenerated with Claude", &config).is_err());
    }

    #[test]
    fn test_check_commit_message_length() {
        let config = GitGuardConfig {
            commit_message: crate::git_guard::config::CommitMessageConfig {
                min_length: 10,
                block_ai_attribution: false,
                ..Default::default()
            },
            ..Default::default()
        };

        assert!(check_commit_message("fix", &config).is_err());
        assert!(check_commit_message("fix: proper message", &config).is_ok());
    }
}
