//! Roadmap parsing and GitHub issue sync
//!
//! Parses ROADMAP.md files and syncs items with GitHub issues.

use crate::db::{Database, RoadmapItem, RoadmapSummary};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

/// A parsed roadmap item from markdown
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedRoadmapItem {
    pub section: String,
    pub title: String,
    pub description: Option<String>,
    pub status: String,
    pub scope: Option<String>,
    pub tags: Vec<String>,
    pub priority: Option<i32>,
    pub line_number: i32,
    pub children: Vec<ParsedRoadmapItem>,
}

/// Result of parsing a roadmap file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedRoadmap {
    pub items: Vec<ParsedRoadmapItem>,
    pub sections: Vec<String>,
    pub total_items: i32,
    pub completed_items: i32,
}

/// GitHub issue info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubIssue {
    pub number: i32,
    pub title: String,
    pub state: String,
    pub url: String,
    pub labels: Vec<String>,
}

/// Result of syncing with GitHub
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResult {
    pub created: i32,
    pub updated: i32,
    pub unchanged: i32,
    pub errors: Vec<String>,
}

/// Parse a ROADMAP.md file into structured items
pub fn parse_roadmap<P: AsRef<Path>>(path: P) -> Result<ParsedRoadmap, String> {
    let content = std::fs::read_to_string(path.as_ref())
        .map_err(|e| format!("Failed to read roadmap file: {}", e))?;

    parse_roadmap_content(&content)
}

/// Parse roadmap content (for testing)
pub fn parse_roadmap_content(content: &str) -> Result<ParsedRoadmap, String> {
    let mut items: Vec<ParsedRoadmapItem> = Vec::new();
    let mut sections: Vec<String> = Vec::new();
    let mut current_section = String::new();
    let mut total_items = 0;
    let mut completed_items = 0;

    for (line_num, line) in content.lines().enumerate() {
        let line_number = (line_num + 1) as i32;

        // Section headers (### Something)
        if line.starts_with("### ") {
            current_section = line.trim_start_matches("### ").trim().to_string();
            if !sections.contains(&current_section) {
                sections.push(current_section.clone());
            }
            continue;
        }

        // Checklist items (- [ ] or - [x])
        if let Some(item) = parse_checklist_item(line, &current_section, line_number) {
            total_items += 1;
            if item.status == "completed" {
                completed_items += 1;
            }
            items.push(item);
        }
    }

    Ok(ParsedRoadmap {
        items,
        sections,
        total_items,
        completed_items,
    })
}

/// Parse a single checklist item line
fn parse_checklist_item(line: &str, section: &str, line_number: i32) -> Option<ParsedRoadmapItem> {
    let trimmed = line.trim();

    // Check for checkbox pattern
    let (status, rest) = if trimmed.starts_with("- [ ] ") {
        ("pending", trimmed.trim_start_matches("- [ ] "))
    } else if trimmed.starts_with("- [x] ") || trimmed.starts_with("- [X] ") {
        ("completed", trimmed.trim_start_matches("- [x] ").trim_start_matches("- [X] "))
    } else {
        return None;
    };

    // Parse the title - handle **bold** titles
    let (title, description) = if let Some(stripped) = rest.strip_prefix("**") {
        // Bold title with potential description
        if let Some(end) = stripped.find("**") {
            let title = stripped[..end].to_string();
            let desc = stripped[end + 2..].trim();
            let description = if desc.is_empty() { None } else { Some(desc.to_string()) };
            (title, description)
        } else {
            (rest.to_string(), None)
        }
    } else {
        // Plain title
        (rest.to_string(), None)
    };

    // Extract scope from title if present (e.g., "Title (Critical)")
    let (clean_title, scope) = extract_scope(&title);

    // Extract tags from description or title
    let tags = extract_tags(&clean_title, description.as_deref());

    Some(ParsedRoadmapItem {
        section: section.to_string(),
        title: clean_title,
        description,
        status: status.to_string(),
        scope,
        tags,
        priority: None,
        line_number,
        children: Vec::new(),
    })
}

/// Extract scope annotation like "(Critical)" from title
fn extract_scope(title: &str) -> (String, Option<String>) {
    let scope_patterns = ["(Critical)", "(Extended)", "(Experimental)", "(future)"];

    for pattern in &scope_patterns {
        if title.contains(pattern) {
            let clean = title.replace(pattern, "").trim().to_string();
            let scope = pattern.trim_matches(|c| c == '(' || c == ')').to_string();
            return (clean, Some(scope));
        }
    }

    (title.to_string(), None)
}

/// Extract tags from title and description
fn extract_tags(title: &str, description: Option<&str>) -> Vec<String> {
    let mut tags = Vec::new();

    // Common tag keywords to look for
    let tag_keywords = [
        "TUI", "Web", "CLI", "GitHub", "API", "Database", "Sync",
        "AI", "LLM", "Claude", "MCP", "Export", "Import", "View",
    ];

    let text = format!("{} {}", title, description.unwrap_or(""));

    for keyword in &tag_keywords {
        if text.to_lowercase().contains(&keyword.to_lowercase()) {
            tags.push(keyword.to_string());
        }
    }

    tags
}

/// Import parsed roadmap items into the database
pub fn import_roadmap_to_db(db: &Database, roadmap: &ParsedRoadmap, replace: bool) -> Result<(i32, i32), String> {
    if replace {
        db.clear_roadmap_items()
            .map_err(|e| format!("Failed to clear existing items: {}", e))?;
    }

    let mut created = 0;
    let mut updated = 0;

    for (idx, item) in roadmap.items.iter().enumerate() {
        let priority = Some(idx as i32);
        let tags_str = if item.tags.is_empty() {
            None
        } else {
            Some(item.tags.join(","))
        };

        let result = db.upsert_roadmap_item(
            &item.section,
            &item.title,
            item.description.as_deref(),
            &item.status,
            item.scope.as_deref(),
            tags_str.as_deref(),
            priority,
            Some(item.line_number),
        ).map_err(|e| format!("Failed to upsert item '{}': {}", item.title, e))?;

        // Check if this was a create or update by seeing if the ID is new
        if replace || result > 0 {
            created += 1;
        } else {
            updated += 1;
        }
    }

    Ok((created, updated))
}

/// Get GitHub repo info from git remote
pub fn get_github_repo() -> Option<String> {
    let output = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();

    // Parse GitHub URL: git@github.com:owner/repo.git or https://github.com/owner/repo.git
    if url.contains("github.com") {
        let repo = url
            .trim_end_matches(".git")
            .split("github.com")
            .last()
            .map(|s| s.trim_start_matches(':').trim_start_matches('/'))
            .map(|s| s.to_string());
        return repo;
    }

    None
}

/// Create a GitHub issue for a roadmap item using `gh` CLI
pub fn create_github_issue(repo: &str, item: &RoadmapItem, labels: &[&str]) -> Result<GitHubIssue, String> {
    // Build issue body
    let body = build_issue_body(item);

    // Build labels arg
    let labels_str = labels.join(",");

    // Create issue using gh CLI
    let mut args = vec![
        "issue", "create",
        "-R", repo,
        "--title", &item.title,
        "--body", &body,
    ];

    if !labels_str.is_empty() {
        args.push("--label");
        args.push(&labels_str);
    }

    let output = Command::new("gh")
        .args(&args)
        .output()
        .map_err(|e| format!("Failed to run gh CLI: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("gh issue create failed: {}", stderr));
    }

    // Parse the issue URL from output
    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let number = url
        .rsplit('/')
        .next()
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(0);

    Ok(GitHubIssue {
        number,
        title: item.title.clone(),
        state: "open".to_string(),
        url,
        labels: labels.iter().map(|s| s.to_string()).collect(),
    })
}

/// Build issue body from roadmap item
fn build_issue_body(item: &RoadmapItem) -> String {
    let mut body = String::new();

    body.push_str(&format!("**Section:** {}\n\n", item.section));

    if let Some(ref desc) = item.description {
        body.push_str(&format!("{}\n\n", desc));
    }

    if let Some(ref scope) = item.scope {
        body.push_str(&format!("**Scope:** {}\n", scope));
    }

    if let Some(ref tags) = item.tags {
        body.push_str(&format!("**Tags:** {}\n", tags));
    }

    body.push_str("\n---\n");
    body.push_str("*This issue was created from ROADMAP.md by deciduous*\n");

    body
}

/// Fetch existing GitHub issues for the repo
pub fn fetch_github_issues(repo: &str, label: Option<&str>) -> Result<Vec<GitHubIssue>, String> {
    let mut args = vec![
        "issue", "list",
        "-R", repo,
        "--json", "number,title,state,url,labels",
        "--limit", "500",
    ];

    if let Some(lbl) = label {
        args.push("--label");
        args.push(lbl);
    }

    let output = Command::new("gh")
        .args(&args)
        .output()
        .map_err(|e| format!("Failed to run gh CLI: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("gh issue list failed: {}", stderr));
    }

    let json = String::from_utf8_lossy(&output.stdout);

    #[derive(Deserialize)]
    struct GhIssue {
        number: i32,
        title: String,
        state: String,
        url: String,
        labels: Vec<GhLabel>,
    }

    #[derive(Deserialize)]
    struct GhLabel {
        name: String,
    }

    let gh_issues: Vec<GhIssue> = serde_json::from_str(&json)
        .map_err(|e| format!("Failed to parse gh output: {}", e))?;

    let issues = gh_issues
        .into_iter()
        .map(|i| GitHubIssue {
            number: i.number,
            title: i.title,
            state: i.state,
            url: i.url,
            labels: i.labels.into_iter().map(|l| l.name).collect(),
        })
        .collect();

    Ok(issues)
}

/// Sync roadmap items with GitHub issues
pub fn sync_with_github(
    db: &Database,
    repo: &str,
    label: &str,
    create_missing: bool,
    dry_run: bool,
) -> Result<SyncResult, String> {
    let mut result = SyncResult {
        created: 0,
        updated: 0,
        unchanged: 0,
        errors: Vec::new(),
    };

    // Get all roadmap items from DB
    let items = db.get_all_roadmap_items()
        .map_err(|e| format!("Failed to get roadmap items: {}", e))?;

    // Fetch existing issues
    let issues = fetch_github_issues(repo, Some(label))?;

    // Build a map of title -> issue for matching
    let issue_map: HashMap<String, &GitHubIssue> = issues
        .iter()
        .map(|i| (i.title.to_lowercase(), i))
        .collect();

    for item in &items {
        // Try to find matching issue by title
        if let Some(issue) = issue_map.get(&item.title.to_lowercase()) {
            // Update DB with issue info if changed
            let needs_update = item.github_issue_number != Some(issue.number)
                || item.github_issue_state.as_deref() != Some(&issue.state);

            if needs_update {
                if !dry_run {
                    if let Err(e) = db.update_roadmap_github_issue(
                        item.id,
                        issue.number,
                        &issue.url,
                        &issue.state,
                    ) {
                        result.errors.push(format!("Failed to update item {}: {}", item.id, e));
                        continue;
                    }
                }
                result.updated += 1;
            } else {
                result.unchanged += 1;
            }
        } else if create_missing && item.github_issue_number.is_none() {
            // Create new issue
            if dry_run {
                result.created += 1;
                continue;
            }

            match create_github_issue(repo, item, &[label]) {
                Ok(issue) => {
                    if let Err(e) = db.update_roadmap_github_issue(
                        item.id,
                        issue.number,
                        &issue.url,
                        &issue.state,
                    ) {
                        result.errors.push(format!("Created issue but failed to update DB: {}", e));
                    }
                    result.created += 1;
                }
                Err(e) => {
                    result.errors.push(format!("Failed to create issue for '{}': {}", item.title, e));
                }
            }
        } else {
            result.unchanged += 1;
        }
    }

    Ok(result)
}

/// Export roadmap items to JSON for web viewer
pub fn export_roadmap_json(db: &Database) -> Result<String, String> {
    let items = db.get_all_roadmap_items()
        .map_err(|e| format!("Failed to get roadmap items: {}", e))?;

    let summary = db.get_roadmap_summary()
        .map_err(|e| format!("Failed to get summary: {}", e))?;

    // Group items by section
    let mut sections: HashMap<String, Vec<&RoadmapItem>> = HashMap::new();
    for item in &items {
        sections.entry(item.section.clone())
            .or_default()
            .push(item);
    }

    #[derive(Serialize)]
    struct RoadmapExport<'a> {
        items: &'a [RoadmapItem],
        sections: HashMap<String, Vec<&'a RoadmapItem>>,
        summary: RoadmapSummary,
    }

    let export = RoadmapExport {
        items: &items,
        sections,
        summary,
    };

    serde_json::to_string_pretty(&export)
        .map_err(|e| format!("Failed to serialize: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_checklist_item_pending() {
        let item = parse_checklist_item("- [ ] Add dark mode", "Features", 1);
        assert!(item.is_some());
        let item = item.unwrap();
        assert_eq!(item.title, "Add dark mode");
        assert_eq!(item.status, "pending");
        assert_eq!(item.section, "Features");
    }

    #[test]
    fn test_parse_checklist_item_completed() {
        let item = parse_checklist_item("- [x] Fix bug", "Fixes", 5);
        assert!(item.is_some());
        let item = item.unwrap();
        assert_eq!(item.title, "Fix bug");
        assert_eq!(item.status, "completed");
    }

    #[test]
    fn test_parse_checklist_item_bold_title() {
        let item = parse_checklist_item("- [ ] **Bold Title** - Description here", "Section", 10);
        assert!(item.is_some());
        let item = item.unwrap();
        assert_eq!(item.title, "Bold Title");
        assert_eq!(item.description, Some("- Description here".to_string()));
    }

    #[test]
    fn test_parse_checklist_item_not_checklist() {
        let item = parse_checklist_item("Regular line", "Section", 1);
        assert!(item.is_none());

        let item = parse_checklist_item("- Regular bullet", "Section", 1);
        assert!(item.is_none());
    }

    #[test]
    fn test_extract_scope() {
        let (title, scope) = extract_scope("Context Recovery (Critical)");
        assert_eq!(title, "Context Recovery");
        assert_eq!(scope, Some("Critical".to_string()));

        let (title, scope) = extract_scope("Normal Title");
        assert_eq!(title, "Normal Title");
        assert_eq!(scope, None);
    }

    #[test]
    fn test_parse_roadmap_content() {
        let content = r#"
# Deciduous Roadmap

### Core Features
- [ ] Add dark mode
- [x] Fix navigation

### Future
- [ ] **AI Integration** - Use LLM for analysis
"#;

        let roadmap = parse_roadmap_content(content).unwrap();
        assert_eq!(roadmap.total_items, 3);
        assert_eq!(roadmap.completed_items, 1);
        assert_eq!(roadmap.sections.len(), 2);
        assert!(roadmap.sections.contains(&"Core Features".to_string()));
        assert!(roadmap.sections.contains(&"Future".to_string()));
    }
}
