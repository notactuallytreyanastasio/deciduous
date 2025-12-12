//! Import decisions from Claude Code session files
//!
//! Claude Code stores session transcripts as JSONL files in ~/.claude/projects/.
//! This module discovers those files and invokes Claude to extract decisions.

use std::path::PathBuf;
use std::process::Command;

/// Get the Claude projects directory for the current platform
pub fn claude_projects_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".claude").join("projects"))
}

/// Convert a working directory path to Claude's project folder name format
/// e.g., "C:\Users\Nat\source\deciduous" -> "C--Users-Nat-source-deciduous"
/// e.g., "/home/user/project" -> "-home-user-project"
pub fn path_to_project_name(path: &std::path::Path) -> String {
    let path_str = path.to_string_lossy();

    // Replace path separators and colons with dashes
    path_str
        .replace('\\', "-")
        .replace('/', "-")
        .replace(':', "-")
}

/// Find Claude session files for a given project directory
pub fn find_session_files(project_path: &std::path::Path) -> Result<Vec<SessionFile>, String> {
    let projects_dir = claude_projects_dir()
        .ok_or_else(|| "Could not determine home directory".to_string())?;

    let project_name = path_to_project_name(project_path);
    let project_sessions_dir = projects_dir.join(&project_name);

    if !project_sessions_dir.exists() {
        return Err(format!(
            "No Claude sessions found for this project.\nExpected: {}\nProject name: {}",
            project_sessions_dir.display(),
            project_name
        ));
    }

    let mut sessions = Vec::new();

    let entries = std::fs::read_dir(&project_sessions_dir)
        .map_err(|e| format!("Failed to read sessions directory: {}", e))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().map(|e| e == "jsonl").unwrap_or(false) {
            // Skip agent files (subagent sessions) - focus on main sessions
            let filename = path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");

            if filename.starts_with("agent-") {
                continue;
            }

            // Get file metadata for sorting
            let metadata = std::fs::metadata(&path).ok();
            let modified = metadata.as_ref().and_then(|m| m.modified().ok());
            let size = metadata.map(|m| m.len()).unwrap_or(0);

            sessions.push(SessionFile {
                path,
                modified,
                size,
            });
        }
    }

    // Sort by modification time, newest first
    sessions.sort_by(|a, b| b.modified.cmp(&a.modified));

    Ok(sessions)
}

/// A Claude Code session file
#[derive(Debug)]
pub struct SessionFile {
    pub path: PathBuf,
    pub modified: Option<std::time::SystemTime>,
    pub size: u64,
}

impl SessionFile {
    /// Format the modification time as a human-readable string
    pub fn modified_str(&self) -> String {
        self.modified
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| {
                let secs = d.as_secs();
                let dt = chrono::DateTime::from_timestamp(secs as i64, 0)
                    .unwrap_or_default();
                dt.format("%Y-%m-%d %H:%M").to_string()
            })
            .unwrap_or_else(|| "unknown".to_string())
    }

    /// Get the session ID (filename without extension)
    pub fn session_id(&self) -> String {
        self.path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string()
    }
}

/// Import decisions from a session file by invoking Claude
pub fn import_session(session_path: &std::path::Path, dry_run: bool) -> Result<(), String> {
    if !session_path.exists() {
        return Err(format!("Session file not found: {}", session_path.display()));
    }

    // Build the prompt for Claude
    let prompt = format!(
        r#"Analyze the Claude Code session transcript at: {}

Read the JSONL file and extract all implicit decisions, goals, actions, and outcomes. For each one:

1. Identify the node type (goal, decision, action, outcome, observation)
2. Determine an appropriate title
3. Estimate a confidence score (0-100)
4. Identify relationships between nodes

Then execute the appropriate deciduous commands:
- `deciduous add <type> "title" -c <confidence>`
- `deciduous link <from> <to> -r "reason"`

Start with root goals (user requests), then work through decisions and actions that flowed from them.

Focus on substantive decisions, not routine operations. A "decision" is a choice between alternatives. An "action" is implementation work. An "outcome" is a result."#,
        session_path.display()
    );

    if dry_run {
        println!("Would run claude with prompt:");
        println!("---");
        println!("{}", prompt);
        println!("---");
        return Ok(());
    }

    // Invoke claude CLI
    let status = Command::new("claude")
        .arg("-p")
        .arg(&prompt)
        .status()
        .map_err(|e| format!("Failed to run claude: {}\nIs claude CLI installed and in PATH?", e))?;

    if !status.success() {
        return Err(format!("Claude exited with status: {}", status));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_to_project_name_windows() {
        let path = std::path::Path::new("C:\\Users\\Nat\\source\\deciduous");
        let name = path_to_project_name(path);
        assert_eq!(name, "C--Users-Nat-source-deciduous");
    }

    #[test]
    fn test_path_to_project_name_unix() {
        let path = std::path::Path::new("/home/user/projects/myapp");
        let name = path_to_project_name(path);
        assert_eq!(name, "-home-user-projects-myapp");
    }
}
