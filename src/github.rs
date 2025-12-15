//! GitHub CLI (`gh`) Integration
//!
//! Wrapper around the GitHub CLI for issue operations.
//! Uses `gh` instead of direct API to avoid token management complexity.

use serde::{Deserialize, Serialize};
use std::process::Command;

/// GitHub Issue representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubIssue {
    pub number: i32,
    pub title: String,
    pub body: String,
    pub state: String,  // "open" or "closed"
    pub html_url: String,
    pub created_at: String,
    pub updated_at: String,
}

/// GitHub Issue Comment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubComment {
    pub id: i64,
    pub body: String,
    pub author: CommentAuthor,
    pub created_at: String,
    #[serde(rename = "createdAt")]
    pub created_at_alt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentAuthor {
    pub login: String,
}

/// Error type for GitHub operations
#[derive(Debug)]
pub enum GitHubError {
    CommandFailed { command: String, stderr: String },
    NotAuthenticated,
    RateLimited,
    IssueNotFound { number: i32 },
    ParseError { message: String },
    IoError(std::io::Error),
}

impl std::fmt::Display for GitHubError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GitHubError::CommandFailed { command, stderr } => {
                write!(f, "Command '{}' failed: {}", command, stderr)
            }
            GitHubError::NotAuthenticated => {
                write!(f, "Not authenticated with GitHub. Run 'gh auth login' first.")
            }
            GitHubError::RateLimited => {
                write!(f, "GitHub API rate limit exceeded. Try again later.")
            }
            GitHubError::IssueNotFound { number } => {
                write!(f, "Issue #{} not found", number)
            }
            GitHubError::ParseError { message } => {
                write!(f, "Failed to parse GitHub response: {}", message)
            }
            GitHubError::IoError(e) => write!(f, "IO error: {}", e),
        }
    }
}

impl std::error::Error for GitHubError {}

impl From<std::io::Error> for GitHubError {
    fn from(e: std::io::Error) -> Self {
        GitHubError::IoError(e)
    }
}

pub type Result<T> = std::result::Result<T, GitHubError>;

/// GitHub client using `gh` CLI
pub struct GitHubClient {
    repo: Option<String>,  // "owner/repo" format
}

impl GitHubClient {
    /// Create a new client, optionally with explicit repo
    pub fn new(repo: Option<String>) -> Self {
        Self { repo }
    }

    /// Auto-detect repo from git remote
    pub fn auto_detect() -> Result<Self> {
        let output = Command::new("gh")
            .args(["repo", "view", "--json", "nameWithOwner", "-q", ".nameWithOwner"])
            .output()?;

        if output.status.success() {
            let repo = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !repo.is_empty() {
                return Ok(Self { repo: Some(repo) });
            }
        }

        Ok(Self { repo: None })
    }

    /// Check if gh CLI is authenticated
    pub fn check_auth() -> Result<bool> {
        let output = Command::new("gh")
            .args(["auth", "status"])
            .output()?;

        Ok(output.status.success())
    }

    /// Get repo string for gh commands
    fn repo_args(&self) -> Vec<String> {
        match &self.repo {
            Some(repo) => vec!["-R".to_string(), repo.clone()],
            None => vec![],
        }
    }

    /// Create a new issue
    pub fn create_issue(
        &self,
        title: &str,
        body: &str,
        labels: &[&str],
    ) -> Result<GitHubIssue> {
        let mut args = vec!["issue", "create", "--title", title, "--body", body];

        // Add labels
        for label in labels {
            args.push("--label");
            args.push(label);
        }

        let mut cmd = Command::new("gh");
        cmd.args(&args);

        for arg in self.repo_args() {
            cmd.arg(&arg);
        }

        let output = cmd.output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            if stderr.contains("not logged") || stderr.contains("auth") {
                return Err(GitHubError::NotAuthenticated);
            }
            return Err(GitHubError::CommandFailed {
                command: "gh issue create".to_string(),
                stderr,
            });
        }

        // Parse the output URL to get issue number
        let stdout = String::from_utf8_lossy(&output.stdout);
        let url = stdout.trim();

        // Extract issue number from URL like "https://github.com/owner/repo/issues/42"
        let number: i32 = url
            .rsplit('/')
            .next()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| GitHubError::ParseError {
                message: format!("Could not parse issue number from URL: {}", url),
            })?;

        // Fetch the full issue details
        self.get_issue(number)
    }

    /// Get an issue by number
    pub fn get_issue(&self, number: i32) -> Result<GitHubIssue> {
        let mut cmd = Command::new("gh");
        cmd.args([
            "issue", "view",
            &number.to_string(),
            "--json", "number,title,body,state,url,createdAt,updatedAt",
        ]);

        for arg in self.repo_args() {
            cmd.arg(&arg);
        }

        let output = cmd.output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            if stderr.contains("not found") || stderr.contains("Could not resolve") {
                return Err(GitHubError::IssueNotFound { number });
            }
            if stderr.contains("rate limit") {
                return Err(GitHubError::RateLimited);
            }
            return Err(GitHubError::CommandFailed {
                command: format!("gh issue view {}", number),
                stderr,
            });
        }

        let json_str = String::from_utf8_lossy(&output.stdout);

        // Parse the JSON response
        #[derive(Deserialize)]
        struct IssueResponse {
            number: i32,
            title: String,
            body: String,
            state: String,
            url: String,
            #[serde(rename = "createdAt")]
            created_at: String,
            #[serde(rename = "updatedAt")]
            updated_at: String,
        }

        let resp: IssueResponse = serde_json::from_str(&json_str)
            .map_err(|e| GitHubError::ParseError {
                message: format!("JSON parse error: {} - Raw: {}", e, json_str),
            })?;

        Ok(GitHubIssue {
            number: resp.number,
            title: resp.title,
            body: resp.body,
            state: resp.state.to_lowercase(),
            html_url: resp.url,
            created_at: resp.created_at,
            updated_at: resp.updated_at,
        })
    }

    /// Update an issue's body
    pub fn update_issue_body(&self, number: i32, body: &str) -> Result<()> {
        let mut cmd = Command::new("gh");
        cmd.args([
            "issue", "edit",
            &number.to_string(),
            "--body", body,
        ]);

        for arg in self.repo_args() {
            cmd.arg(&arg);
        }

        let output = cmd.output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(GitHubError::CommandFailed {
                command: format!("gh issue edit {}", number),
                stderr,
            });
        }

        Ok(())
    }

    /// Update an issue's title
    pub fn update_issue_title(&self, number: i32, title: &str) -> Result<()> {
        let mut cmd = Command::new("gh");
        cmd.args([
            "issue", "edit",
            &number.to_string(),
            "--title", title,
        ]);

        for arg in self.repo_args() {
            cmd.arg(&arg);
        }

        let output = cmd.output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(GitHubError::CommandFailed {
                command: format!("gh issue edit {} --title", number),
                stderr,
            });
        }

        Ok(())
    }

    /// Close an issue
    pub fn close_issue(&self, number: i32) -> Result<()> {
        let mut cmd = Command::new("gh");
        cmd.args([
            "issue", "close",
            &number.to_string(),
        ]);

        for arg in self.repo_args() {
            cmd.arg(&arg);
        }

        let output = cmd.output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(GitHubError::CommandFailed {
                command: format!("gh issue close {}", number),
                stderr,
            });
        }

        Ok(())
    }

    /// Reopen an issue
    pub fn reopen_issue(&self, number: i32) -> Result<()> {
        let mut cmd = Command::new("gh");
        cmd.args([
            "issue", "reopen",
            &number.to_string(),
        ]);

        for arg in self.repo_args() {
            cmd.arg(&arg);
        }

        let output = cmd.output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(GitHubError::CommandFailed {
                command: format!("gh issue reopen {}", number),
                stderr,
            });
        }

        Ok(())
    }

    /// Get comments on an issue
    pub fn get_issue_comments(&self, number: i32) -> Result<Vec<GitHubComment>> {
        let mut cmd = Command::new("gh");
        cmd.args([
            "issue", "view",
            &number.to_string(),
            "--json", "comments",
            "-q", ".comments",
        ]);

        for arg in self.repo_args() {
            cmd.arg(&arg);
        }

        let output = cmd.output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(GitHubError::CommandFailed {
                command: format!("gh issue view {} --json comments", number),
                stderr,
            });
        }

        let json_str = String::from_utf8_lossy(&output.stdout);

        // Handle empty comments (returns "null" or "[]")
        if json_str.trim() == "null" || json_str.trim().is_empty() {
            return Ok(vec![]);
        }

        let comments: Vec<GitHubComment> = serde_json::from_str(&json_str)
            .map_err(|e| GitHubError::ParseError {
                message: format!("JSON parse error for comments: {} - Raw: {}", e, json_str),
            })?;

        Ok(comments)
    }

    /// Add a comment to an issue
    pub fn add_comment(&self, number: i32, body: &str) -> Result<()> {
        let mut cmd = Command::new("gh");
        cmd.args([
            "issue", "comment",
            &number.to_string(),
            "--body", body,
        ]);

        for arg in self.repo_args() {
            cmd.arg(&arg);
        }

        let output = cmd.output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(GitHubError::CommandFailed {
                command: format!("gh issue comment {}", number),
                stderr,
            });
        }

        Ok(())
    }

    /// List issues with a specific label
    pub fn list_issues_with_label(&self, label: &str) -> Result<Vec<GitHubIssue>> {
        let mut cmd = Command::new("gh");
        cmd.args([
            "issue", "list",
            "--label", label,
            "--state", "all",
            "--json", "number,title,body,state,url,createdAt,updatedAt",
        ]);

        for arg in self.repo_args() {
            cmd.arg(&arg);
        }

        let output = cmd.output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(GitHubError::CommandFailed {
                command: format!("gh issue list --label {}", label),
                stderr,
            });
        }

        let json_str = String::from_utf8_lossy(&output.stdout);

        #[derive(Deserialize)]
        struct IssueListItem {
            number: i32,
            title: String,
            body: String,
            state: String,
            url: String,
            #[serde(rename = "createdAt")]
            created_at: String,
            #[serde(rename = "updatedAt")]
            updated_at: String,
        }

        let items: Vec<IssueListItem> = serde_json::from_str(&json_str)
            .map_err(|e| GitHubError::ParseError {
                message: format!("JSON parse error: {}", e),
            })?;

        Ok(items.into_iter().map(|item| GitHubIssue {
            number: item.number,
            title: item.title,
            body: item.body,
            state: item.state.to_lowercase(),
            html_url: item.url,
            created_at: item.created_at,
            updated_at: item.updated_at,
        }).collect())
    }

    /// Search for an issue by title
    pub fn find_issue_by_title(&self, title: &str) -> Result<Option<GitHubIssue>> {
        let mut cmd = Command::new("gh");
        cmd.args([
            "issue", "list",
            "--search", &format!("\"{}\" in:title", title),
            "--state", "all",
            "--json", "number,title,body,state,url,createdAt,updatedAt",
            "--limit", "1",
        ]);

        for arg in self.repo_args() {
            cmd.arg(&arg);
        }

        let output = cmd.output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(GitHubError::CommandFailed {
                command: "gh issue list --search".to_string(),
                stderr,
            });
        }

        let json_str = String::from_utf8_lossy(&output.stdout);

        #[derive(Deserialize)]
        struct IssueListItem {
            number: i32,
            title: String,
            body: String,
            state: String,
            url: String,
            #[serde(rename = "createdAt")]
            created_at: String,
            #[serde(rename = "updatedAt")]
            updated_at: String,
        }

        let items: Vec<IssueListItem> = serde_json::from_str(&json_str)
            .map_err(|e| GitHubError::ParseError {
                message: format!("JSON parse error: {}", e),
            })?;

        if items.is_empty() {
            return Ok(None);
        }

        let item = &items[0];
        // Check if title matches exactly (search is fuzzy)
        if item.title.to_lowercase() == title.to_lowercase() {
            Ok(Some(GitHubIssue {
                number: item.number,
                title: item.title.clone(),
                body: item.body.clone(),
                state: item.state.to_lowercase(),
                html_url: item.url.clone(),
                created_at: item.created_at.clone(),
                updated_at: item.updated_at.clone(),
            }))
        } else {
            Ok(None)
        }
    }

    /// Get the repo name
    pub fn repo_name(&self) -> Option<&str> {
        self.repo.as_deref()
    }

    /// Check if a label exists
    pub fn label_exists(&self, name: &str) -> Result<bool> {
        let mut cmd = Command::new("gh");
        cmd.args([
            "label", "list",
            "--search", name,
            "--json", "name",
            "-q", &format!(".[] | select(.name == \"{}\")", name),
        ]);

        for arg in self.repo_args() {
            cmd.arg(&arg);
        }

        let output = cmd.output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(GitHubError::CommandFailed {
                command: "gh label list".to_string(),
                stderr,
            });
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(!stdout.trim().is_empty())
    }

    /// Create a label
    pub fn create_label(&self, name: &str, description: &str, color: &str) -> Result<()> {
        let mut cmd = Command::new("gh");
        cmd.args([
            "label", "create", name,
            "--description", description,
            "--color", color,
            "--force",  // Update if exists
        ]);

        for arg in self.repo_args() {
            cmd.arg(&arg);
        }

        let output = cmd.output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(GitHubError::CommandFailed {
                command: format!("gh label create {}", name),
                stderr,
            });
        }

        Ok(())
    }
}

/// Ensure the 'roadmap' label exists, creating it if needed
/// Returns Ok(true) if label was created, Ok(false) if it already existed
pub fn ensure_roadmap_label(client: &GitHubClient) -> Result<bool> {
    match client.label_exists("roadmap") {
        Ok(true) => Ok(false),  // Already exists
        Ok(false) => {
            client.create_label(
                "roadmap",
                "Roadmap item synced from ROADMAP.md by deciduous",
                "0e8a16"  // Green color
            )?;
            Ok(true)  // Created
        }
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let client = GitHubClient::new(Some("owner/repo".to_string()));
        assert_eq!(client.repo_name(), Some("owner/repo"));
    }

    #[test]
    fn test_client_no_repo() {
        let client = GitHubClient::new(None);
        assert_eq!(client.repo_name(), None);
    }

    #[test]
    fn test_repo_args() {
        let client_with_repo = GitHubClient::new(Some("owner/repo".to_string()));
        assert_eq!(client_with_repo.repo_args(), vec!["-R", "owner/repo"]);

        let client_without_repo = GitHubClient::new(None);
        assert!(client_without_repo.repo_args().is_empty());
    }

    // Note: Integration tests would require actual gh CLI and authentication
    // These are covered by manual testing
}
