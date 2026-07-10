//! Small shared utilities

/// Truncate a string to max length (Unicode-safe)
pub fn truncate(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        let char_len = max_len.saturating_sub(3);
        let truncated: String = s.chars().take(char_len).collect();
        format!("{}...", truncated)
    }
}

/// Git commit info for timeline view (matches web/src/types/graph.ts GitCommit)
#[derive(serde::Serialize)]
pub struct GitCommit {
    pub hash: String,
    pub short_hash: String,
    pub author: String,
    pub date: String,
    pub message: String,
    pub files_changed: Option<u32>,
}

/// Get commit info from git for a given hash.
///
/// If `repo_root` is provided, git commands run in that directory;
/// otherwise they run in the current working directory.
pub fn get_git_commit_info(hash: &str, repo_root: Option<&std::path::Path>) -> Option<GitCommit> {
    use std::process::Command;

    // Get commit info: hash, author, date (ISO), full message body
    // Use %x00 (null byte) as separator since message can have newlines
    let mut cmd = Command::new("git");
    if let Some(root) = repo_root {
        cmd.current_dir(root);
    }
    let output = cmd
        .args(["log", "-1", "--format=%H%x00%an%x00%aI%x00%B", hash])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = stdout.trim().split('\x00').collect();
    if parts.len() < 4 {
        return None;
    }

    // Clean up the message - trim whitespace
    let message = parts[3].trim().to_string();

    // Get files changed count
    let mut files_cmd = Command::new("git");
    if let Some(root) = repo_root {
        files_cmd.current_dir(root);
    }
    let files_output = files_cmd
        .args(["diff-tree", "--no-commit-id", "--name-only", "-r", hash])
        .output()
        .ok();

    let files_changed = files_output.and_then(|o| {
        if o.status.success() {
            let count = String::from_utf8_lossy(&o.stdout).trim().lines().count();
            Some(count as u32)
        } else {
            None
        }
    });

    Some(GitCommit {
        hash: parts[0].to_string(),
        short_hash: parts[0].chars().take(7).collect(),
        author: parts[1].to_string(),
        date: parts[2].to_string(),
        message,
        files_changed,
    })
}
