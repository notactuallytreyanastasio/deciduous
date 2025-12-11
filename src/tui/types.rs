//! TUI Type Definitions
//!
//! These types mirror the React/TypeScript types in web/src/types/graph.ts
//! and the Rust backend types in src/db.rs.
//!
//! Type Hierarchy:
//!   - src/db.rs: Database models (Diesel ORM)
//!   - src/tui/types.rs: TUI view models (this file)
//!   - web/src/types/graph.ts: Web UI models (TypeScript)
//!
//! All three MUST stay in sync for consistent behavior.

use crate::{DecisionNode, DecisionEdge};
use serde_json::Value;

// =============================================================================
// Node Types - matches schema CHECK constraint
// =============================================================================

/// Valid node types in the decision graph
pub const NODE_TYPES: &[&str] = &["goal", "decision", "option", "action", "outcome", "observation"];

/// Valid node statuses
pub const NODE_STATUSES: &[&str] = &["pending", "active", "completed", "rejected"];

// =============================================================================
// Edge Types - matches schema CHECK constraint
// =============================================================================

/// Valid edge types connecting nodes
pub const EDGE_TYPES: &[&str] = &["leads_to", "requires", "chosen", "rejected", "blocks", "enables"];

// =============================================================================
// Metadata - stored as JSON string in metadata_json field
// =============================================================================

/// Parsed node metadata from metadata_json field
#[derive(Debug, Clone, Default)]
pub struct NodeMetadata {
    /// Confidence score 0-100
    pub confidence: Option<i32>,
    /// Git commit hash (full 40 chars)
    pub commit: Option<String>,
    /// User prompt that triggered this decision
    pub prompt: Option<String>,
    /// Associated files
    pub files: Vec<String>,
    /// Git branch this node was created on
    pub branch: Option<String>,
}

impl NodeMetadata {
    /// Parse metadata from JSON string
    pub fn from_json(json: &str) -> Self {
        serde_json::from_str::<Value>(json)
            .ok()
            .map(|v| Self {
                confidence: v.get("confidence").and_then(|c| c.as_i64()).map(|c| c as i32),
                commit: v.get("commit").and_then(|c| c.as_str()).map(|s| s.to_string()),
                prompt: v.get("prompt").and_then(|p| p.as_str()).map(|s| s.to_string()),
                files: v.get("files")
                    .and_then(|f| f.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default(),
                branch: v.get("branch").and_then(|b| b.as_str()).map(|s| s.to_string()),
            })
            .unwrap_or_default()
    }

    /// Parse metadata from Option<String>
    pub fn from_option(json: Option<&String>) -> Self {
        json.map(|s| Self::from_json(s)).unwrap_or_default()
    }
}

// =============================================================================
// Helper Functions - Mirror web/src/types/graph.ts functions
// =============================================================================

/// Extract confidence from a node (mirrors getConfidence in TypeScript)
pub fn get_confidence(node: &DecisionNode) -> Option<i32> {
    NodeMetadata::from_option(node.metadata_json.as_ref()).confidence
}

/// Extract commit hash from a node (mirrors getCommit in TypeScript)
pub fn get_commit(node: &DecisionNode) -> Option<String> {
    NodeMetadata::from_option(node.metadata_json.as_ref()).commit
}

/// Extract branch from a node (mirrors getBranch in TypeScript)
pub fn get_branch(node: &DecisionNode) -> Option<String> {
    NodeMetadata::from_option(node.metadata_json.as_ref()).branch
}

/// Extract files from a node (mirrors getFiles in TypeScript)
pub fn get_files(node: &DecisionNode) -> Vec<String> {
    NodeMetadata::from_option(node.metadata_json.as_ref()).files
}

/// Extract prompt from a node (mirrors getPrompt in TypeScript)
pub fn get_prompt(node: &DecisionNode) -> Option<String> {
    NodeMetadata::from_option(node.metadata_json.as_ref()).prompt
}

/// Get short commit hash (7 chars) (mirrors shortCommit in TypeScript)
pub fn short_commit(commit: &str) -> &str {
    &commit[..7.min(commit.len())]
}

/// Get confidence level category (mirrors getConfidenceLevel in TypeScript)
pub fn get_confidence_level(confidence: Option<i32>) -> Option<&'static str> {
    confidence.map(|c| {
        if c >= 70 { "high" }
        else if c >= 40 { "med" }
        else { "low" }
    })
}

/// Truncate string with ellipsis (mirrors truncate in TypeScript)
/// Uses char indices to handle Unicode safely
pub fn truncate(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        let char_len = max_len.saturating_sub(3);
        let truncated: String = s.chars().take(char_len).collect();
        format!("{}...", truncated)
    }
}

/// Type guard for valid node type
pub fn is_node_type(value: &str) -> bool {
    NODE_TYPES.contains(&value)
}

/// Type guard for valid edge type
pub fn is_edge_type(value: &str) -> bool {
    EDGE_TYPES.contains(&value)
}

/// Get all unique branches from a list of nodes
pub fn get_unique_branches(nodes: &[DecisionNode]) -> Vec<String> {
    let mut branches: Vec<String> = nodes
        .iter()
        .filter_map(|n| get_branch(n))
        .collect();
    branches.sort();
    branches.dedup();
    branches
}

/// Get incoming edges for a node
pub fn get_incoming_edges<'a>(node_id: i32, edges: &'a [DecisionEdge]) -> Vec<&'a DecisionEdge> {
    edges.iter().filter(|e| e.to_node_id == node_id).collect()
}

/// Get outgoing edges from a node
pub fn get_outgoing_edges<'a>(node_id: i32, edges: &'a [DecisionEdge]) -> Vec<&'a DecisionEdge> {
    edges.iter().filter(|e| e.from_node_id == node_id).collect()
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_node(id: i32, node_type: &str, title: &str, metadata_json: Option<&str>) -> DecisionNode {
        DecisionNode {
            id,
            change_id: format!("test-change-{}", id),
            node_type: node_type.to_string(),
            title: title.to_string(),
            description: None,
            status: "pending".to_string(),
            created_at: "2024-12-10T12:00:00Z".to_string(),
            updated_at: "2024-12-10T12:00:00Z".to_string(),
            metadata_json: metadata_json.map(|s| s.to_string()),
        }
    }

    fn make_test_edge(id: i32, from_id: i32, to_id: i32, edge_type: &str) -> DecisionEdge {
        DecisionEdge {
            id,
            from_node_id: from_id,
            to_node_id: to_id,
            from_change_id: None,
            to_change_id: None,
            edge_type: edge_type.to_string(),
            weight: Some(1.0),
            rationale: None,
            created_at: "2024-12-10T12:00:00Z".to_string(),
        }
    }

    #[test]
    fn test_node_types_valid() {
        assert!(is_node_type("goal"));
        assert!(is_node_type("decision"));
        assert!(is_node_type("option"));
        assert!(is_node_type("action"));
        assert!(is_node_type("outcome"));
        assert!(is_node_type("observation"));
        assert!(!is_node_type("invalid"));
        assert!(!is_node_type(""));
    }

    #[test]
    fn test_edge_types_valid() {
        assert!(is_edge_type("leads_to"));
        assert!(is_edge_type("requires"));
        assert!(is_edge_type("chosen"));
        assert!(is_edge_type("rejected"));
        assert!(is_edge_type("blocks"));
        assert!(is_edge_type("enables"));
        assert!(!is_edge_type("invalid"));
        assert!(!is_edge_type(""));
    }

    #[test]
    fn test_metadata_parsing() {
        let json = r#"{"confidence": 85, "commit": "abc1234", "branch": "main", "files": ["src/foo.rs", "src/bar.rs"]}"#;
        let meta = NodeMetadata::from_json(json);

        assert_eq!(meta.confidence, Some(85));
        assert_eq!(meta.commit, Some("abc1234".to_string()));
        assert_eq!(meta.branch, Some("main".to_string()));
        assert_eq!(meta.files, vec!["src/foo.rs", "src/bar.rs"]);
    }

    #[test]
    fn test_metadata_parsing_empty() {
        let meta = NodeMetadata::from_json("{}");
        assert_eq!(meta.confidence, None);
        assert_eq!(meta.commit, None);
        assert!(meta.files.is_empty());
    }

    #[test]
    fn test_metadata_parsing_invalid() {
        let meta = NodeMetadata::from_json("not json");
        assert_eq!(meta.confidence, None);
    }

    #[test]
    fn test_get_confidence() {
        let node = make_test_node(1, "goal", "Test", Some(r#"{"confidence": 90}"#));
        assert_eq!(get_confidence(&node), Some(90));

        let node_no_meta = make_test_node(2, "goal", "Test", None);
        assert_eq!(get_confidence(&node_no_meta), None);
    }

    #[test]
    fn test_get_commit() {
        let node = make_test_node(1, "action", "Test", Some(r#"{"commit": "abc1234567890"}"#));
        assert_eq!(get_commit(&node), Some("abc1234567890".to_string()));
    }

    #[test]
    fn test_get_branch() {
        let node = make_test_node(1, "goal", "Test", Some(r#"{"branch": "feature/test"}"#));
        assert_eq!(get_branch(&node), Some("feature/test".to_string()));
    }

    #[test]
    fn test_get_files() {
        let node = make_test_node(1, "action", "Test", Some(r#"{"files": ["a.rs", "b.rs"]}"#));
        assert_eq!(get_files(&node), vec!["a.rs", "b.rs"]);

        let node_no_files = make_test_node(2, "action", "Test", Some(r#"{}"#));
        assert!(get_files(&node_no_files).is_empty());
    }

    #[test]
    fn test_short_commit() {
        assert_eq!(short_commit("abc1234567890"), "abc1234");
        assert_eq!(short_commit("abc"), "abc");
        assert_eq!(short_commit(""), "");
    }

    #[test]
    fn test_confidence_level() {
        assert_eq!(get_confidence_level(Some(90)), Some("high"));
        assert_eq!(get_confidence_level(Some(70)), Some("high"));
        assert_eq!(get_confidence_level(Some(69)), Some("med"));
        assert_eq!(get_confidence_level(Some(40)), Some("med"));
        assert_eq!(get_confidence_level(Some(39)), Some("low"));
        assert_eq!(get_confidence_level(Some(0)), Some("low"));
        assert_eq!(get_confidence_level(None), None);
    }

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 8), "hello...");
        assert_eq!(truncate("", 10), "");
    }

    #[test]
    fn test_truncate_unicode() {
        // Test with emoji - should not panic on multi-byte chars
        assert_eq!(truncate("✅ Done", 10), "✅ Done");
        // "✅ This is a long message" = 25 chars, truncate to 10 = 7 chars + "..."
        // chars: ✅, ' ', T, h, i, s, ' ' = 7 chars -> "✅ This ..."
        assert_eq!(truncate("✅ This is a long message", 10), "✅ This ...");
        // Test with checkmark
        assert_eq!(truncate("✓ Complete", 15), "✓ Complete");
        // Test truncating emoji string (6 emoji = 6 chars, truncate to 5 = 2 chars + "...")
        assert_eq!(truncate("🎉🎊🎁🎄🎅🎆", 5), "🎉🎊...");
        // String exactly at limit should not be truncated
        assert_eq!(truncate("🎉🎊🎁🎄🎅", 5), "🎉🎊🎁🎄🎅");
    }

    #[test]
    fn test_get_unique_branches() {
        let nodes = vec![
            make_test_node(1, "goal", "A", Some(r#"{"branch": "main"}"#)),
            make_test_node(2, "goal", "B", Some(r#"{"branch": "feature"}"#)),
            make_test_node(3, "goal", "C", Some(r#"{"branch": "main"}"#)),
            make_test_node(4, "goal", "D", None),
        ];
        let branches = get_unique_branches(&nodes);
        assert_eq!(branches, vec!["feature", "main"]);
    }

    #[test]
    fn test_get_incoming_edges() {
        let edges = vec![
            make_test_edge(1, 1, 2, "leads_to"),
            make_test_edge(2, 1, 3, "leads_to"),
            make_test_edge(3, 2, 3, "chosen"),
        ];

        let incoming_to_3 = get_incoming_edges(3, &edges);
        assert_eq!(incoming_to_3.len(), 2);
        assert!(incoming_to_3.iter().any(|e| e.from_node_id == 1));
        assert!(incoming_to_3.iter().any(|e| e.from_node_id == 2));

        let incoming_to_1 = get_incoming_edges(1, &edges);
        assert!(incoming_to_1.is_empty());
    }

    #[test]
    fn test_get_outgoing_edges() {
        let edges = vec![
            make_test_edge(1, 1, 2, "leads_to"),
            make_test_edge(2, 1, 3, "leads_to"),
            make_test_edge(3, 2, 3, "chosen"),
        ];

        let outgoing_from_1 = get_outgoing_edges(1, &edges);
        assert_eq!(outgoing_from_1.len(), 2);

        let outgoing_from_3 = get_outgoing_edges(3, &edges);
        assert!(outgoing_from_3.is_empty());
    }
}
