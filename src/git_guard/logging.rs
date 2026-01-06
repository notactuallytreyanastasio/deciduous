//! Logging for git operations
//!
//! All git commands are logged to git.log for audit purposes.

use crate::git_guard::config::GitGuardConfig;
use chrono::Local;
use std::fs::OpenOptions;
use std::io::Write;

/// Log status for a git command
#[derive(Debug, Clone, Copy)]
pub enum LogStatus {
    Allowed,
    Blocked,
    Checked,
}

impl LogStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            LogStatus::Allowed => "ALLOWED",
            LogStatus::Blocked => "BLOCKED",
            LogStatus::Checked => "CHECKED",
        }
    }
}

/// Log a git command to git.log
pub fn log_command(command: &str, status: LogStatus, config: &GitGuardConfig) {
    if !config.logging.log_all_commands && !matches!(status, LogStatus::Blocked) {
        return;
    }

    let log_file = &config.logging.log_file;
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

    let entry = config
        .logging
        .format
        .replace("{timestamp}", &timestamp)
        .replace("{status}", status.as_str())
        .replace("{command}", command);

    // Append to log file
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(log_file) {
        let _ = writeln!(file, "{}", entry);
    }
}

/// Log a blocked command with reason
pub fn log_blocked(command: &str, reason: &str, config: &GitGuardConfig) {
    let log_file = &config.logging.log_file;
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

    let entry = format!("{} | BLOCKED | {} | Reason: {}", timestamp, command, reason);

    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(log_file) {
        let _ = writeln!(file, "{}", entry);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_log_command() {
        let dir = tempdir().unwrap();
        let log_path = dir.path().join("test.log");

        let config = GitGuardConfig {
            logging: crate::git_guard::config::LoggingConfig {
                log_all_commands: true,
                log_file: log_path.to_string_lossy().to_string(),
                format: "{timestamp} | {status} | {command}".to_string(),
            },
            ..Default::default()
        };

        log_command("git status", LogStatus::Allowed, &config);
        log_command("git push --force", LogStatus::Blocked, &config);

        let contents = fs::read_to_string(&log_path).unwrap();
        assert!(contents.contains("ALLOWED"));
        assert!(contents.contains("BLOCKED"));
        assert!(contents.contains("git status"));
        assert!(contents.contains("git push --force"));
    }
}
