//! ROADMAP.md Parser and Sync Engine
//!
//! Parses ROADMAP.md format into structured roadmap items,
//! handles metadata comments for sync, and provides utilities
//! for bidirectional synchronization with GitHub Issues.

use regex::Regex;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use uuid::Uuid;

/// Represents a parsed roadmap section (## or ### header)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RoadmapSection {
    pub change_id: String,
    pub title: String,
    pub level: u8, // 2 for ##, 3 for ###
    pub description: Option<String>,
    pub items: Vec<RoadmapCheckItem>,
    pub github_issue_number: Option<i32>,
    pub github_issue_state: Option<String>,
    pub line_start: usize,
    pub line_end: usize,
    pub content_hash: String,
}

/// Represents a checkbox item (- [ ] or - [x])
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RoadmapCheckItem {
    pub change_id: String,
    pub text: String,
    pub checked: bool,
    pub outcome_change_id: Option<String>,
    pub line_number: usize,
}

/// Metadata embedded in HTML comments
#[derive(Debug, Clone, Default)]
pub struct SectionMetadata {
    pub id: Option<String>,
    pub issue: Option<i32>,
    pub status: Option<String>,
    pub last_sync: Option<String>,
}

/// Metadata for checkbox items
#[derive(Debug, Clone, Default)]
pub struct ItemMetadata {
    pub id: Option<String>,
    pub outcome_change_id: Option<String>,
}

/// Result of parsing ROADMAP.md
#[derive(Debug, Clone, serde::Serialize)]
pub struct ParsedRoadmap {
    pub path: String,
    pub sections: Vec<RoadmapSection>,
    pub content_hash: String,
}

/// Error type for roadmap operations
#[derive(Debug)]
pub enum RoadmapError {
    Io(std::io::Error),
    Parse(String),
    Regex(regex::Error),
}

impl std::fmt::Display for RoadmapError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RoadmapError::Io(e) => write!(f, "IO error: {}", e),
            RoadmapError::Parse(msg) => write!(f, "Parse error: {}", msg),
            RoadmapError::Regex(e) => write!(f, "Regex error: {}", e),
        }
    }
}

impl std::error::Error for RoadmapError {}

impl From<std::io::Error> for RoadmapError {
    fn from(e: std::io::Error) -> Self {
        RoadmapError::Io(e)
    }
}

impl From<regex::Error> for RoadmapError {
    fn from(e: regex::Error) -> Self {
        RoadmapError::Regex(e)
    }
}

pub type Result<T> = std::result::Result<T, RoadmapError>;

/// Compute SHA256 hash of content
pub fn compute_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Parse section metadata from HTML comment
/// Format: <!-- roadmap:section id="UUID" issue="42" status="open" last_sync="2025-01-15T10:30:00Z" -->
pub fn parse_section_metadata(comment: &str) -> SectionMetadata {
    let mut meta = SectionMetadata::default();

    // Extract id
    if let Some(caps) = Regex::new(r#"id="([^"]+)""#)
        .ok()
        .and_then(|re| re.captures(comment))
    {
        meta.id = caps.get(1).map(|m| m.as_str().to_string());
    }

    // Extract issue number
    if let Some(caps) = Regex::new(r#"issue="(\d+)""#)
        .ok()
        .and_then(|re| re.captures(comment))
    {
        meta.issue = caps.get(1).and_then(|m| m.as_str().parse().ok());
    }

    // Extract status
    if let Some(caps) = Regex::new(r#"status="([^"]+)""#)
        .ok()
        .and_then(|re| re.captures(comment))
    {
        meta.status = caps.get(1).map(|m| m.as_str().to_string());
    }

    // Extract last_sync
    if let Some(caps) = Regex::new(r#"last_sync="([^"]+)""#)
        .ok()
        .and_then(|re| re.captures(comment))
    {
        meta.last_sync = caps.get(1).map(|m| m.as_str().to_string());
    }

    meta
}

/// Parse item metadata from HTML comment
/// Format: <!-- roadmap:item id="UUID" outcome_change_id="UUID" -->
pub fn parse_item_metadata(comment: &str) -> ItemMetadata {
    let mut meta = ItemMetadata::default();

    // Extract id
    if let Some(caps) = Regex::new(r#"id="([^"]+)""#)
        .ok()
        .and_then(|re| re.captures(comment))
    {
        meta.id = caps.get(1).map(|m| m.as_str().to_string());
    }

    // Extract outcome_change_id
    if let Some(caps) = Regex::new(r#"outcome_change_id="([^"]+)""#)
        .ok()
        .and_then(|re| re.captures(comment))
    {
        let value = caps.get(1).map(|m| m.as_str().to_string());
        // Only set if not empty
        if value.as_ref().map(|s| !s.is_empty()).unwrap_or(false) {
            meta.outcome_change_id = value;
        }
    }

    meta
}

/// Generate section metadata comment
pub fn generate_section_metadata(
    change_id: &str,
    issue_number: Option<i32>,
    issue_state: Option<&str>,
    last_sync: Option<&str>,
) -> String {
    let mut parts = vec![format!(r#"id="{}""#, change_id)];

    if let Some(num) = issue_number {
        parts.push(format!(r#"issue="{}""#, num));
    }

    if let Some(state) = issue_state {
        parts.push(format!(r#"status="{}""#, state));
    }

    if let Some(sync) = last_sync {
        parts.push(format!(r#"last_sync="{}""#, sync));
    }

    format!("<!-- roadmap:section {} -->", parts.join(" "))
}

/// Generate item metadata comment
pub fn generate_item_metadata(change_id: &str, outcome_change_id: Option<&str>) -> String {
    let outcome = outcome_change_id.unwrap_or("");
    format!(
        r#"<!-- roadmap:item id="{}" outcome_change_id="{}" -->"#,
        change_id, outcome
    )
}

/// Parse ROADMAP.md file into structured sections
pub fn parse_roadmap<P: AsRef<Path>>(path: P) -> Result<ParsedRoadmap> {
    let content = fs::read_to_string(path.as_ref())?;
    let path_str = path.as_ref().to_string_lossy().to_string();
    let content_hash = compute_hash(&content);

    let lines: Vec<&str> = content.lines().collect();
    let mut sections: Vec<RoadmapSection> = Vec::new();

    // Regex patterns
    let header_re = Regex::new(r"^(#{2,3})\s+(.+)$")?;
    let checkbox_re = Regex::new(r"^-\s+\[([ xX])\]\s+(.+)$")?;
    let section_meta_re = Regex::new(r"<!--\s*roadmap:section\s+(.+?)\s*-->")?;
    let item_meta_re = Regex::new(r"<!--\s*roadmap:item\s+(.+?)\s*-->")?;

    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];

        // Check for header
        if let Some(caps) = header_re.captures(line) {
            let level = caps.get(1).unwrap().as_str().len() as u8;
            let title = caps.get(2).unwrap().as_str().trim().to_string();

            let line_start = i + 1; // 1-indexed

            // Look for metadata comment on next line
            let mut section_meta = SectionMetadata::default();
            let mut description_start = i + 1;

            if i + 1 < lines.len() {
                if let Some(meta_caps) = section_meta_re.captures(lines[i + 1]) {
                    section_meta = parse_section_metadata(meta_caps.get(1).unwrap().as_str());
                    description_start = i + 2;
                }
            }

            // Generate change_id if not present
            let change_id = section_meta
                .id
                .unwrap_or_else(|| Uuid::new_v4().to_string());

            // Collect description lines until next header or checkbox
            let mut description_lines: Vec<&str> = Vec::new();
            let mut items: Vec<RoadmapCheckItem> = Vec::new();
            let mut j = description_start;

            while j < lines.len() {
                let next_line = lines[j];

                // Stop at next header
                if header_re.is_match(next_line) {
                    break;
                }

                // Check for checkbox item
                if let Some(check_caps) = checkbox_re.captures(next_line) {
                    let checked = check_caps.get(1).unwrap().as_str().to_lowercase() == "x";
                    let text = check_caps.get(2).unwrap().as_str().trim().to_string();

                    // Look for item metadata on next line
                    let mut item_meta = ItemMetadata::default();
                    if j + 1 < lines.len() {
                        if let Some(item_meta_caps) = item_meta_re.captures(lines[j + 1]) {
                            item_meta =
                                parse_item_metadata(item_meta_caps.get(1).unwrap().as_str());
                            j += 1; // Skip metadata line
                        }
                    }

                    let item_change_id = item_meta.id.unwrap_or_else(|| Uuid::new_v4().to_string());

                    items.push(RoadmapCheckItem {
                        change_id: item_change_id,
                        text,
                        checked,
                        outcome_change_id: item_meta.outcome_change_id,
                        line_number: j + 1, // 1-indexed
                    });
                } else if !next_line.trim().is_empty()
                    && !item_meta_re.is_match(next_line)
                    && !section_meta_re.is_match(next_line)
                {
                    // Non-empty, non-metadata line is description
                    if items.is_empty() {
                        description_lines.push(next_line);
                    }
                }

                j += 1;
            }

            let line_end = j;
            let description = if description_lines.is_empty() {
                None
            } else {
                Some(description_lines.join("\n").trim().to_string())
            };

            // Compute content hash for this section
            let section_content: Vec<&str> = lines[i..j].to_vec();
            let section_hash = compute_hash(&section_content.join("\n"));

            sections.push(RoadmapSection {
                change_id,
                title,
                level,
                description,
                items,
                github_issue_number: section_meta.issue,
                github_issue_state: section_meta.status,
                line_start,
                line_end,
                content_hash: section_hash,
            });

            i = j;
        } else {
            i += 1;
        }
    }

    Ok(ParsedRoadmap {
        path: path_str,
        sections,
        content_hash,
    })
}

/// Rewrite ROADMAP.md with updated metadata
pub fn write_roadmap_with_metadata<P: AsRef<Path>>(
    path: P,
    sections: &[RoadmapSection],
    original_content: &str,
) -> Result<String> {
    let lines: Vec<&str> = original_content.lines().collect();
    let mut output_lines: Vec<String> = Vec::new();

    let header_re = Regex::new(r"^(#{2,3})\s+(.+)$")?;
    let checkbox_re = Regex::new(r"^-\s+\[([ xX])\]\s+(.+)$")?;
    let section_meta_re = Regex::new(r"<!--\s*roadmap:section\s+(.+?)\s*-->")?;
    let item_meta_re = Regex::new(r"<!--\s*roadmap:item\s+(.+?)\s*-->")?;

    // Build lookup maps
    let section_map: HashMap<String, &RoadmapSection> =
        sections.iter().map(|s| (s.title.clone(), s)).collect();

    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];

        // Check for header
        if let Some(caps) = header_re.captures(line) {
            let title = caps.get(2).unwrap().as_str().trim().to_string();

            output_lines.push(line.to_string());

            // Check if we have metadata for this section
            if let Some(section) = section_map.get(&title) {
                // Skip existing metadata comment if present
                if i + 1 < lines.len() && section_meta_re.is_match(lines[i + 1]) {
                    i += 1; // Skip old metadata
                }

                // Add updated metadata
                let meta_comment = generate_section_metadata(
                    &section.change_id,
                    section.github_issue_number,
                    section.github_issue_state.as_deref(),
                    None, // last_sync will be set by sync operation
                );
                output_lines.push(meta_comment);
            }

            i += 1;
            continue;
        }

        // Check for checkbox item
        if let Some(check_caps) = checkbox_re.captures(line) {
            let text = check_caps.get(2).unwrap().as_str().trim();

            output_lines.push(line.to_string());

            // Find matching item in sections
            let mut found_item: Option<&RoadmapCheckItem> = None;
            for section in sections {
                for item in &section.items {
                    if item.text.contains(text) || text.contains(&item.text) {
                        found_item = Some(item);
                        break;
                    }
                }
            }

            // Skip existing item metadata if present
            if i + 1 < lines.len() && item_meta_re.is_match(lines[i + 1]) {
                i += 1;
            }

            // Add updated metadata
            if let Some(item) = found_item {
                let meta_comment =
                    generate_item_metadata(&item.change_id, item.outcome_change_id.as_deref());
                output_lines.push(format!("  {}", meta_comment));
            }

            i += 1;
            continue;
        }

        // Skip existing metadata comments (they'll be regenerated)
        if section_meta_re.is_match(line) || item_meta_re.is_match(line) {
            i += 1;
            continue;
        }

        output_lines.push(line.to_string());
        i += 1;
    }

    let new_content = output_lines.join("\n");

    // Write to file
    fs::write(path.as_ref(), &new_content)?;

    Ok(new_content)
}

/// Generate GitHub issue body from a roadmap section
pub fn generate_issue_body(section: &RoadmapSection) -> String {
    let mut body = String::new();

    // Add description if present
    if let Some(desc) = &section.description {
        body.push_str(desc);
        body.push_str("\n\n");
    }

    // Add checkbox items
    if !section.items.is_empty() {
        body.push_str("## Tasks\n\n");
        for item in &section.items {
            let checkbox = if item.checked { "[x]" } else { "[ ]" };
            body.push_str(&format!("- {} {}\n", checkbox, item.text));
        }
    }

    // Add metadata footer
    body.push_str("\n---\n");
    body.push_str(&format!(
        "_Synced from ROADMAP.md (change_id: {})_\n",
        section.change_id
    ));

    body
}

/// Parse checkbox state from issue body
pub fn parse_issue_body_checkboxes(body: &str) -> Vec<(String, bool)> {
    let checkbox_re = Regex::new(r"-\s+\[([ xX])\]\s+(.+)").unwrap();
    let mut items = Vec::new();

    for line in body.lines() {
        if let Some(caps) = checkbox_re.captures(line) {
            let checked = caps.get(1).unwrap().as_str().to_lowercase() == "x";
            let text = caps.get(2).unwrap().as_str().trim().to_string();
            items.push((text, checked));
        }
    }

    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_compute_hash() {
        let hash = compute_hash("test content");
        assert!(!hash.is_empty());
        assert_eq!(hash.len(), 64); // SHA256 produces 64 hex chars

        // Same input should produce same hash
        assert_eq!(hash, compute_hash("test content"));

        // Different input should produce different hash
        assert_ne!(hash, compute_hash("different content"));
    }

    #[test]
    fn test_parse_section_metadata() {
        let comment = r#"id="abc-123" issue="42" status="open" last_sync="2025-01-15T10:30:00Z""#;
        let meta = parse_section_metadata(comment);

        assert_eq!(meta.id, Some("abc-123".to_string()));
        assert_eq!(meta.issue, Some(42));
        assert_eq!(meta.status, Some("open".to_string()));
        assert_eq!(meta.last_sync, Some("2025-01-15T10:30:00Z".to_string()));
    }

    #[test]
    fn test_parse_section_metadata_partial() {
        let comment = r#"id="xyz-789""#;
        let meta = parse_section_metadata(comment);

        assert_eq!(meta.id, Some("xyz-789".to_string()));
        assert_eq!(meta.issue, None);
        assert_eq!(meta.status, None);
    }

    #[test]
    fn test_parse_item_metadata() {
        let comment = r#"id="item-123" outcome_change_id="outcome-456""#;
        let meta = parse_item_metadata(comment);

        assert_eq!(meta.id, Some("item-123".to_string()));
        assert_eq!(meta.outcome_change_id, Some("outcome-456".to_string()));
    }

    #[test]
    fn test_parse_item_metadata_empty_outcome() {
        let comment = r#"id="item-123" outcome_change_id="""#;
        let meta = parse_item_metadata(comment);

        assert_eq!(meta.id, Some("item-123".to_string()));
        assert_eq!(meta.outcome_change_id, None); // Empty string treated as None
    }

    #[test]
    fn test_generate_section_metadata() {
        let comment = generate_section_metadata(
            "abc-123",
            Some(42),
            Some("open"),
            Some("2025-01-15T10:30:00Z"),
        );

        assert!(comment.contains("roadmap:section"));
        assert!(comment.contains(r#"id="abc-123""#));
        assert!(comment.contains(r#"issue="42""#));
        assert!(comment.contains(r#"status="open""#));
    }

    #[test]
    fn test_generate_item_metadata() {
        let comment = generate_item_metadata("item-123", Some("outcome-456"));

        assert!(comment.contains("roadmap:item"));
        assert!(comment.contains(r#"id="item-123""#));
        assert!(comment.contains(r#"outcome_change_id="outcome-456""#));
    }

    #[test]
    fn test_parse_roadmap_basic() {
        let content = r#"# Deciduous Roadmap

## In Progress

### Feature One
This is a description.
- [ ] Task 1
- [x] Task 2 (completed)

### Feature Two
- [ ] Another task
"#;

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let result = parse_roadmap(file.path()).unwrap();

        assert!(!result.sections.is_empty());
        assert!(!result.content_hash.is_empty());

        // Find Feature One
        let feature_one = result.sections.iter().find(|s| s.title == "Feature One");
        assert!(feature_one.is_some());

        let section = feature_one.unwrap();
        assert_eq!(section.level, 3);
        assert!(section.description.is_some());
        assert_eq!(section.items.len(), 2);
        assert!(!section.items[0].checked);
        assert!(section.items[1].checked);
    }

    #[test]
    fn test_parse_roadmap_with_metadata() {
        let content = r#"# Deciduous Roadmap

### Feature With Metadata
<!-- roadmap:section id="existing-uuid" issue="42" status="open" -->
Description here.
- [ ] Task 1
  <!-- roadmap:item id="item-uuid" outcome_change_id="" -->
- [x] Task 2
  <!-- roadmap:item id="item-uuid-2" outcome_change_id="outcome-uuid" -->
"#;

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let result = parse_roadmap(file.path()).unwrap();

        let section = result
            .sections
            .iter()
            .find(|s| s.title == "Feature With Metadata")
            .unwrap();
        assert_eq!(section.change_id, "existing-uuid");
        assert_eq!(section.github_issue_number, Some(42));
        assert_eq!(section.github_issue_state, Some("open".to_string()));

        assert_eq!(section.items[0].change_id, "item-uuid");
        assert_eq!(section.items[0].outcome_change_id, None);

        assert_eq!(section.items[1].change_id, "item-uuid-2");
        assert_eq!(
            section.items[1].outcome_change_id,
            Some("outcome-uuid".to_string())
        );
    }

    #[test]
    fn test_generate_issue_body() {
        let section = RoadmapSection {
            change_id: "test-uuid".to_string(),
            title: "Test Feature".to_string(),
            level: 3,
            description: Some("This is a test feature.".to_string()),
            items: vec![
                RoadmapCheckItem {
                    change_id: "item-1".to_string(),
                    text: "First task".to_string(),
                    checked: false,
                    outcome_change_id: None,
                    line_number: 1,
                },
                RoadmapCheckItem {
                    change_id: "item-2".to_string(),
                    text: "Second task".to_string(),
                    checked: true,
                    outcome_change_id: Some("outcome-123".to_string()),
                    line_number: 2,
                },
            ],
            github_issue_number: None,
            github_issue_state: None,
            line_start: 1,
            line_end: 5,
            content_hash: "hash".to_string(),
        };

        let body = generate_issue_body(&section);

        assert!(body.contains("This is a test feature."));
        assert!(body.contains("- [ ] First task"));
        assert!(body.contains("- [x] Second task"));
        assert!(body.contains("test-uuid"));
    }

    #[test]
    fn test_parse_issue_body_checkboxes() {
        let body = r#"Some description.

## Tasks

- [ ] Unchecked task
- [x] Checked task
- [X] Also checked (uppercase)

More text.
"#;

        let items = parse_issue_body_checkboxes(body);

        assert_eq!(items.len(), 3);
        assert_eq!(items[0], ("Unchecked task".to_string(), false));
        assert_eq!(items[1], ("Checked task".to_string(), true));
        assert_eq!(items[2], ("Also checked (uppercase)".to_string(), true));
    }
}
