//! Rule matching for git commands
//!
//! Matches commands against banned patterns and sensitive file patterns.

use crate::git_guard::config::GitGuardConfig;

/// Check if a command matches any banned pattern
///
/// Returns the matched pattern if found, None otherwise.
pub fn matches_banned(command: &str, config: &GitGuardConfig) -> Option<String> {
    for pattern in &config.banned.commands {
        if matches_pattern(command, pattern) {
            return Some(pattern.clone());
        }
    }
    None
}

/// Check if a command is a rebase command
pub fn is_rebase_command(command: &str) -> bool {
    command.starts_with("git rebase") || command.contains(" rebase ")
}

/// Check if a command is a force push
pub fn is_force_push(command: &str) -> bool {
    if !command.starts_with("git push") {
        return false;
    }

    command.contains("--force")
        || command.contains("-f ")
        || command.contains("-f\t")
        || command.ends_with("-f")
        || command.contains(" +") // git push origin +main
}

/// Check if a command targets a protected branch
pub fn targets_protected_branch(command: &str, config: &GitGuardConfig) -> Option<String> {
    for branch in &config.protected_branches.names {
        // Handle wildcard patterns like "release/*"
        if branch.ends_with('*') {
            let prefix = &branch[..branch.len() - 1];
            if command.contains(prefix) {
                return Some(branch.clone());
            }
        } else if command.contains(branch) {
            return Some(branch.clone());
        }
    }
    None
}

/// Find sensitive files in a list of staged files
///
/// Returns list of files that match sensitive patterns.
pub fn find_sensitive_files(files: &[&str], config: &GitGuardConfig) -> Vec<String> {
    let mut matches = Vec::new();

    for file in files {
        for pattern in &config.sensitive_files.patterns {
            if matches_file_pattern(file, pattern) {
                matches.push((*file).to_string());
                break; // Don't add same file multiple times
            }
        }
    }

    matches
}

/// Check if a commit message contains AI attribution
pub fn contains_ai_attribution(message: &str, config: &GitGuardConfig) -> Option<String> {
    let message_lower = message.to_lowercase();

    for pattern in &config.commit_message.ai_attribution_patterns {
        if message_lower.contains(&pattern.to_lowercase()) {
            return Some(pattern.clone());
        }
    }

    None
}

/// Check if a commit message is too short
pub fn is_message_too_short(message: &str, config: &GitGuardConfig) -> bool {
    message.trim().len() < config.commit_message.min_length
}

/// Match a command against a pattern
///
/// Supports:
/// - Exact substring match: "git reset --hard"
/// - Prefix match with wildcard: "git rebase*" (not currently used)
fn matches_pattern(command: &str, pattern: &str) -> bool {
    // Simple substring match - if pattern is in command, it matches
    command.contains(pattern)
}

/// Match a filename against a glob-like pattern
///
/// Supports:
/// - Exact match: ".env"
/// - Extension match: "*.pem"
/// - Prefix match: "id_rsa*"
/// - Contains match: "credentials.*"
fn matches_file_pattern(filename: &str, pattern: &str) -> bool {
    // Handle wildcard patterns
    if pattern.starts_with('*') && pattern.ends_with('*') {
        // *foo* - contains
        let inner = &pattern[1..pattern.len() - 1];
        return filename.contains(inner);
    } else if pattern.starts_with('*') {
        // *.pem - extension match
        let suffix = &pattern[1..];
        return filename.ends_with(suffix);
    } else if pattern.ends_with('*') {
        // id_rsa* - prefix match
        let prefix = &pattern[..pattern.len() - 1];
        return filename.starts_with(prefix);
    } else if pattern.contains('*') {
        // credentials.* - middle wildcard
        let parts: Vec<&str> = pattern.split('*').collect();
        if parts.len() == 2 {
            return filename.starts_with(parts[0]) && filename.ends_with(parts[1]);
        }
    }

    // Exact match or substring
    filename == pattern || filename.ends_with(&format!("/{}", pattern))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> GitGuardConfig {
        GitGuardConfig {
            banned: crate::git_guard::config::BannedConfig {
                commands: vec![
                    "git reset --hard".to_string(),
                    "git push --force origin main".to_string(),
                    "git rebase".to_string(),
                ],
                block_message: String::new(),
            },
            sensitive_files: crate::git_guard::config::SensitiveFilesConfig {
                patterns: vec![
                    ".env".to_string(),
                    "*.pem".to_string(),
                    "*.key".to_string(),
                    "id_rsa*".to_string(),
                ],
                block_message: String::new(),
            },
            protected_branches: crate::git_guard::config::ProtectedBranchesConfig {
                names: vec!["main".to_string(), "master".to_string()],
                on_push: "allow".to_string(),
                allow_force_push: false,
            },
            commit_message: crate::git_guard::config::CommitMessageConfig {
                block_ai_attribution: true,
                ai_attribution_patterns: vec![
                    "Generated with Claude".to_string(),
                    "Co-Authored-By: Claude".to_string(),
                ],
                min_length: 10,
            },
            ..Default::default()
        }
    }

    #[test]
    fn test_matches_banned() {
        let config = test_config();

        assert!(matches_banned("git reset --hard HEAD~3", &config).is_some());
        assert!(matches_banned("git push --force origin main", &config).is_some());
        assert!(matches_banned("git rebase main", &config).is_some());
        assert!(matches_banned("git commit -m 'test'", &config).is_none());
        assert!(matches_banned("git push origin feature", &config).is_none());
    }

    #[test]
    fn test_is_rebase_command() {
        assert!(is_rebase_command("git rebase main"));
        assert!(is_rebase_command("git rebase -i HEAD~3"));
        assert!(!is_rebase_command("git commit -m 'rebase'"));
    }

    #[test]
    fn test_is_force_push() {
        assert!(is_force_push("git push --force origin main"));
        assert!(is_force_push("git push -f origin main"));
        assert!(is_force_push("git push origin +main"));
        assert!(!is_force_push("git push origin main"));
    }

    #[test]
    fn test_find_sensitive_files() {
        let config = test_config();
        let files = vec![".env", "src/main.rs", "secrets.pem", "id_rsa"];

        let matches = find_sensitive_files(&files, &config);
        assert_eq!(matches.len(), 3);
        assert!(matches.contains(&".env".to_string()));
        assert!(matches.contains(&"secrets.pem".to_string()));
        assert!(matches.contains(&"id_rsa".to_string()));
    }

    #[test]
    fn test_contains_ai_attribution() {
        let config = test_config();

        assert!(
            contains_ai_attribution("feat: add auth\n\nGenerated with Claude", &config).is_some()
        );
        assert!(contains_ai_attribution("fix: bug\n\nCo-Authored-By: Claude", &config).is_some());
        assert!(contains_ai_attribution("feat: add auth", &config).is_none());
    }

    #[test]
    fn test_is_message_too_short() {
        let config = test_config();

        assert!(is_message_too_short("fix", &config));
        assert!(is_message_too_short("short", &config));
        assert!(!is_message_too_short("fix: proper commit message", &config));
    }
}
