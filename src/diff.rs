//! Diff/patch functionality for multi-user graph sync
//!
//! Implements jj-inspired change_id based syncing between local databases
//! and version-controlled patch files.

use crate::db::{Database, DecisionEdge, DecisionNode};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

/// A patch file containing nodes and edges to sync
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphPatch {
    /// Patch format version
    pub version: String,
    /// Author who created this patch
    pub author: Option<String>,
    /// Git branch this patch was created from
    pub branch: Option<String>,
    /// Timestamp when patch was created
    pub created_at: String,
    /// Git commit hash at time of patch creation
    pub base_commit: Option<String>,
    /// Nodes included in this patch
    pub nodes: Vec<PatchNode>,
    /// Edges included in this patch
    pub edges: Vec<PatchEdge>,
}

/// A node in a patch file (uses change_id, not integer id)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchNode {
    /// Globally unique change ID
    pub change_id: String,
    /// Node type: goal, decision, option, action, outcome, observation
    pub node_type: String,
    /// Node title
    pub title: String,
    /// Optional description
    pub description: Option<String>,
    /// Node status
    pub status: String,
    /// Metadata JSON (confidence, branch, prompt, files, etc.)
    pub metadata_json: Option<String>,
    /// Created timestamp
    pub created_at: String,
}

/// An edge in a patch file (uses change_ids for references)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchEdge {
    /// Source node change_id
    pub from_change_id: String,
    /// Target node change_id
    pub to_change_id: String,
    /// Edge type: leads_to, chosen, etc.
    pub edge_type: String,
    /// Optional rationale for the edge
    pub rationale: Option<String>,
}

impl GraphPatch {
    /// Create a new empty patch
    pub fn new(
        author: Option<String>,
        branch: Option<String>,
        base_commit: Option<String>,
    ) -> Self {
        Self {
            version: "1.0".to_string(),
            author,
            branch,
            created_at: chrono::Local::now().to_rfc3339(),
            base_commit,
            nodes: Vec::new(),
            edges: Vec::new(),
        }
    }

    /// Load a patch from a JSON file
    pub fn load(path: &Path) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read patch file: {}", e))?;
        serde_json::from_str(&content).map_err(|e| format!("Failed to parse patch JSON: {}", e))
    }

    /// Save the patch to a JSON file
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize patch: {}", e))?;

        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create directory: {}", e))?;
        }

        std::fs::write(path, content).map_err(|e| format!("Failed to write patch file: {}", e))
    }

    /// Add a node to the patch
    pub fn add_node(&mut self, node: &DecisionNode) {
        self.nodes.push(PatchNode {
            change_id: node.change_id.clone(),
            node_type: node.node_type.clone(),
            title: node.title.clone(),
            description: node.description.clone(),
            status: node.status.clone(),
            metadata_json: node.metadata_json.clone(),
            created_at: node.created_at.clone(),
        });
    }

    /// Add an edge to the patch
    pub fn add_edge(&mut self, edge: &DecisionEdge) {
        if let (Some(from_cid), Some(to_cid)) = (&edge.from_change_id, &edge.to_change_id) {
            self.edges.push(PatchEdge {
                from_change_id: from_cid.clone(),
                to_change_id: to_cid.clone(),
                edge_type: edge.edge_type.clone(),
                rationale: edge.rationale.clone(),
            });
        }
    }
}

/// Result of applying a patch
#[derive(Debug, Default)]
pub struct ApplyResult {
    /// Number of nodes added
    pub nodes_added: usize,
    /// Number of nodes skipped (already existed)
    pub nodes_skipped: usize,
    /// Number of edges added
    pub edges_added: usize,
    /// Number of edges skipped (already existed)
    pub edges_skipped: usize,
    /// Edges that couldn't be created (missing nodes)
    pub edges_failed: Vec<String>,
}

impl Database {
    /// Export nodes and edges as a patch
    pub fn export_patch(
        &self,
        node_ids: Option<Vec<i32>>,
        branch_filter: Option<&str>,
        author: Option<String>,
        base_commit: Option<String>,
    ) -> Result<GraphPatch, crate::db::DbError> {
        let all_nodes = self.get_all_nodes()?;
        let all_edges = self.get_all_edges()?;

        // Get current branch for patch metadata
        let current_branch = crate::db::get_current_git_branch();
        let mut patch = GraphPatch::new(author, current_branch, base_commit);

        // Filter nodes
        let nodes: Vec<&DecisionNode> = all_nodes
            .iter()
            .filter(|n| {
                // Filter by node IDs if specified
                if let Some(ref ids) = node_ids {
                    if !ids.contains(&n.id) {
                        return false;
                    }
                }

                // Filter by branch if specified
                if let Some(branch) = branch_filter {
                    if let Some(ref meta) = n.metadata_json {
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(meta) {
                            if let Some(node_branch) = json.get("branch").and_then(|b| b.as_str()) {
                                return node_branch == branch;
                            }
                        }
                    }
                    return false; // No branch metadata and branch filter specified
                }

                true
            })
            .collect();

        // Collect change_ids of nodes being exported
        let change_ids: HashSet<&str> = nodes.iter().map(|n| n.change_id.as_str()).collect();

        // Add nodes to patch
        for node in &nodes {
            patch.add_node(node);
        }

        // Add edges where BOTH endpoints are in the patch
        // Note: We use AND, not OR, because applying a patch requires both nodes to exist
        for edge in &all_edges {
            if let (Some(ref from_cid), Some(ref to_cid)) =
                (&edge.from_change_id, &edge.to_change_id)
            {
                if change_ids.contains(from_cid.as_str()) && change_ids.contains(to_cid.as_str()) {
                    patch.add_edge(edge);
                }
            }
        }

        Ok(patch)
    }

    /// Apply a patch to the database
    pub fn apply_patch(
        &self,
        patch: &GraphPatch,
        dry_run: bool,
    ) -> Result<ApplyResult, crate::db::DbError> {
        let mut result = ApplyResult::default();

        // Get existing change_ids
        let existing_nodes = self.get_all_nodes()?;
        let existing_change_ids: HashSet<String> =
            existing_nodes.iter().map(|n| n.change_id.clone()).collect();

        // Track newly added change_ids -> local ids
        let mut change_id_to_local_id: std::collections::HashMap<String, i32> = existing_nodes
            .iter()
            .map(|n| (n.change_id.clone(), n.id))
            .collect();

        // Apply nodes
        for patch_node in &patch.nodes {
            if existing_change_ids.contains(&patch_node.change_id) {
                result.nodes_skipped += 1;
                continue;
            }

            if !dry_run {
                // Get branch from metadata
                let branch = patch_node
                    .metadata_json
                    .as_ref()
                    .and_then(|m| serde_json::from_str::<serde_json::Value>(m).ok())
                    .and_then(|j| {
                        j.get("branch")
                            .and_then(|b| b.as_str())
                            .map(|s| s.to_string())
                    });

                let confidence = patch_node
                    .metadata_json
                    .as_ref()
                    .and_then(|m| serde_json::from_str::<serde_json::Value>(m).ok())
                    .and_then(|j| {
                        j.get("confidence")
                            .and_then(|c| c.as_u64())
                            .map(|c| c as u8)
                    });

                let prompt = patch_node
                    .metadata_json
                    .as_ref()
                    .and_then(|m| serde_json::from_str::<serde_json::Value>(m).ok())
                    .and_then(|j| {
                        j.get("prompt")
                            .and_then(|p| p.as_str())
                            .map(|s| s.to_string())
                    });

                let files = patch_node
                    .metadata_json
                    .as_ref()
                    .and_then(|m| serde_json::from_str::<serde_json::Value>(m).ok())
                    .and_then(|j| {
                        j.get("files").and_then(|f| {
                            if let Some(arr) = f.as_array() {
                                Some(
                                    arr.iter()
                                        .filter_map(|v| v.as_str())
                                        .collect::<Vec<_>>()
                                        .join(","),
                                )
                            } else {
                                None
                            }
                        })
                    });

                // Create node with explicit change_id
                let local_id = self.create_node_with_change_id(
                    &patch_node.change_id,
                    &patch_node.node_type,
                    &patch_node.title,
                    patch_node.description.as_deref(),
                    confidence,
                    None, // commit
                    prompt.as_deref(),
                    files.as_deref(),
                    branch.as_deref(),
                )?;

                change_id_to_local_id.insert(patch_node.change_id.clone(), local_id);
            }

            result.nodes_added += 1;
        }

        // Get existing edges (by change_id pairs)
        let existing_edges = self.get_all_edges()?;
        let existing_edge_keys: HashSet<(String, String, String)> = existing_edges
            .iter()
            .filter_map(|e| match (&e.from_change_id, &e.to_change_id) {
                (Some(from), Some(to)) => Some((from.clone(), to.clone(), e.edge_type.clone())),
                _ => None,
            })
            .collect();

        // Apply edges
        for patch_edge in &patch.edges {
            let edge_key = (
                patch_edge.from_change_id.clone(),
                patch_edge.to_change_id.clone(),
                patch_edge.edge_type.clone(),
            );

            if existing_edge_keys.contains(&edge_key) {
                result.edges_skipped += 1;
                continue;
            }

            // Look up local IDs
            let from_id = change_id_to_local_id.get(&patch_edge.from_change_id);
            let to_id = change_id_to_local_id.get(&patch_edge.to_change_id);

            match (from_id, to_id) {
                (Some(&from), Some(&to)) => {
                    if !dry_run {
                        self.create_edge(
                            from,
                            to,
                            &patch_edge.edge_type,
                            patch_edge.rationale.as_deref(),
                        )?;
                    }
                    result.edges_added += 1;
                }
                _ => {
                    let msg = format!(
                        "Edge {} -> {} ({}): missing node(s)",
                        patch_edge.from_change_id, patch_edge.to_change_id, patch_edge.edge_type
                    );
                    result.edges_failed.push(msg);
                }
            }
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_node(id: i32, change_id: &str, node_type: &str, title: &str) -> DecisionNode {
        DecisionNode {
            id,
            change_id: change_id.to_string(),
            node_type: node_type.to_string(),
            title: title.to_string(),
            description: None,
            status: "pending".to_string(),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
            metadata_json: Some(r#"{"branch": "main", "confidence": 90}"#.to_string()),
        }
    }

    fn sample_edge(id: i32, from: i32, to: i32, from_cid: &str, to_cid: &str) -> DecisionEdge {
        DecisionEdge {
            id,
            from_node_id: from,
            to_node_id: to,
            from_change_id: Some(from_cid.to_string()),
            to_change_id: Some(to_cid.to_string()),
            edge_type: "leads_to".to_string(),
            weight: Some(1.0),
            rationale: Some("test rationale".to_string()),
            created_at: "2024-01-01T00:00:00Z".to_string(),
        }
    }

    // === GraphPatch Tests ===

    #[test]
    fn test_patch_new() {
        let patch = GraphPatch::new(
            Some("alice".to_string()),
            Some("feature-x".to_string()),
            Some("abc123".to_string()),
        );

        assert_eq!(patch.version, "1.0");
        assert_eq!(patch.author, Some("alice".to_string()));
        assert_eq!(patch.branch, Some("feature-x".to_string()));
        assert_eq!(patch.base_commit, Some("abc123".to_string()));
        assert!(patch.nodes.is_empty());
        assert!(patch.edges.is_empty());
    }

    #[test]
    fn test_patch_add_node() {
        let mut patch = GraphPatch::new(None, None, None);
        let node = sample_node(1, "change-abc", "goal", "Test Goal");

        patch.add_node(&node);

        assert_eq!(patch.nodes.len(), 1);
        assert_eq!(patch.nodes[0].change_id, "change-abc");
        assert_eq!(patch.nodes[0].node_type, "goal");
        assert_eq!(patch.nodes[0].title, "Test Goal");
    }

    #[test]
    fn test_patch_add_edge() {
        let mut patch = GraphPatch::new(None, None, None);
        let edge = sample_edge(1, 1, 2, "cid-from", "cid-to");

        patch.add_edge(&edge);

        assert_eq!(patch.edges.len(), 1);
        assert_eq!(patch.edges[0].from_change_id, "cid-from");
        assert_eq!(patch.edges[0].to_change_id, "cid-to");
        assert_eq!(patch.edges[0].edge_type, "leads_to");
        assert_eq!(patch.edges[0].rationale, Some("test rationale".to_string()));
    }

    #[test]
    fn test_patch_add_edge_without_change_ids() {
        let mut patch = GraphPatch::new(None, None, None);
        let mut edge = sample_edge(1, 1, 2, "cid-from", "cid-to");
        edge.from_change_id = None; // Edge missing change_id

        patch.add_edge(&edge);

        // Should not add edge without change_ids
        assert!(patch.edges.is_empty());
    }

    // === Serialization Tests ===

    #[test]
    fn test_patch_serialization_roundtrip() {
        let mut patch = GraphPatch::new(Some("bob".to_string()), Some("main".to_string()), None);

        patch.add_node(&sample_node(1, "cid-1", "goal", "Goal 1"));
        patch.add_node(&sample_node(2, "cid-2", "action", "Action 1"));
        patch.add_edge(&sample_edge(1, 1, 2, "cid-1", "cid-2"));

        // Serialize to JSON
        let json = serde_json::to_string(&patch).expect("serialize");

        // Deserialize back
        let restored: GraphPatch = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(restored.version, patch.version);
        assert_eq!(restored.author, patch.author);
        assert_eq!(restored.nodes.len(), 2);
        assert_eq!(restored.edges.len(), 1);
        assert_eq!(restored.nodes[0].title, "Goal 1");
        assert_eq!(restored.edges[0].from_change_id, "cid-1");
    }

    #[test]
    fn test_patch_json_format() {
        let patch = GraphPatch::new(Some("alice".to_string()), None, None);

        let json = serde_json::to_string_pretty(&patch).expect("serialize");

        // Verify it contains expected fields
        assert!(json.contains("\"version\": \"1.0\""));
        assert!(json.contains("\"author\": \"alice\""));
        assert!(json.contains("\"nodes\": []"));
        assert!(json.contains("\"edges\": []"));
    }

    // === PatchNode Tests ===

    #[test]
    fn test_patch_node_from_decision_node() {
        let node = DecisionNode {
            id: 42,
            change_id: "unique-uuid".to_string(),
            node_type: "decision".to_string(),
            title: "Choose framework".to_string(),
            description: Some("Evaluate options".to_string()),
            status: "completed".to_string(),
            created_at: "2024-06-01T12:00:00Z".to_string(),
            updated_at: "2024-06-01T12:00:00Z".to_string(),
            metadata_json: Some(r#"{"confidence": 85}"#.to_string()),
        };

        let mut patch = GraphPatch::new(None, None, None);
        patch.add_node(&node);

        let pn = &patch.nodes[0];
        assert_eq!(pn.change_id, "unique-uuid");
        assert_eq!(pn.node_type, "decision");
        assert_eq!(pn.title, "Choose framework");
        assert_eq!(pn.description, Some("Evaluate options".to_string()));
        assert_eq!(pn.status, "completed");
        // Note: local id (42) is NOT included in patch
    }

    // === ApplyResult Tests ===

    #[test]
    fn test_apply_result_default() {
        let result = ApplyResult::default();

        assert_eq!(result.nodes_added, 0);
        assert_eq!(result.nodes_skipped, 0);
        assert_eq!(result.edges_added, 0);
        assert_eq!(result.edges_skipped, 0);
        assert!(result.edges_failed.is_empty());
    }

    // === Edge Cases ===

    #[test]
    fn test_empty_patch() {
        let patch = GraphPatch::new(None, None, None);

        assert!(patch.nodes.is_empty());
        assert!(patch.edges.is_empty());
        assert!(patch.author.is_none());
        assert!(patch.branch.is_none());
        assert!(patch.base_commit.is_none());
    }

    #[test]
    fn test_patch_with_special_characters() {
        let mut patch = GraphPatch::new(None, None, None);

        let node = DecisionNode {
            id: 1,
            change_id: "cid-special".to_string(),
            node_type: "goal".to_string(),
            title: "Handle \"quotes\" and 'apostrophes'".to_string(),
            description: Some("Line1\nLine2\tTabbed".to_string()),
            status: "pending".to_string(),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
            metadata_json: None,
        };

        patch.add_node(&node);

        // Should serialize without error
        let json = serde_json::to_string(&patch).expect("serialize with special chars");
        let restored: GraphPatch = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(
            restored.nodes[0].title,
            "Handle \"quotes\" and 'apostrophes'"
        );
        assert_eq!(
            restored.nodes[0].description,
            Some("Line1\nLine2\tTabbed".to_string())
        );
    }
}
