//! Export utilities for decision graphs
//!
//! Provides DOT graph export and PR writeup generation.

use crate::db::{DecisionEdge, DecisionGraph, DecisionNode};
use std::collections::{HashMap, HashSet};
use std::fmt::Write;

// Helper macro for infallible String writes
// Writing to String never fails, but write! returns Result
// This macro makes intent clear and silences the warning
macro_rules! w {
    ($dst:expr, $($arg:tt)*) => {
        let _ = write!($dst, $($arg)*);
    };
}

macro_rules! wln {
    ($dst:expr) => {
        let _ = writeln!($dst);
    };
    ($dst:expr, $($arg:tt)*) => {
        let _ = writeln!($dst, $($arg)*);
    };
}

/// Configuration for DOT export
#[derive(Debug, Clone)]
pub struct DotConfig {
    /// Title for the graph
    pub title: Option<String>,
    /// Include rationale text on edges
    pub show_rationale: bool,
    /// Include confidence values
    pub show_confidence: bool,
    /// Include node IDs in labels
    pub show_ids: bool,
    /// Orientation: "TB" (top-bottom), "LR" (left-right)
    pub rankdir: String,
}

impl Default for DotConfig {
    fn default() -> Self {
        Self {
            title: None,
            show_rationale: true,
            show_confidence: true,
            show_ids: true,
            rankdir: "TB".to_string(),
        }
    }
}

/// Get the shape for a node type
fn node_shape(node_type: &str) -> &'static str {
    match node_type {
        "goal" => "house",
        "decision" => "diamond",
        "option" => "parallelogram",
        "action" => "box",
        "outcome" => "ellipse",
        "observation" => "note",
        _ => "box",
    }
}

/// Get the fill color for a node type
fn node_color(node_type: &str) -> &'static str {
    match node_type {
        "goal" => "#FFE4B5",        // Moccasin (warm yellow)
        "decision" => "#E6E6FA",    // Lavender
        "option" => "#E0FFFF",      // Light cyan
        "action" => "#90EE90",      // Light green
        "outcome" => "#87CEEB",     // Sky blue
        "observation" => "#DDA0DD", // Plum
        _ => "#F5F5F5",             // White smoke
    }
}

/// Get the edge style based on edge type
fn edge_style(edge_type: &str) -> &'static str {
    match edge_type {
        "chosen" => "bold",
        "rejected" => "dashed",
        "blocks" => "dotted",
        _ => "solid",
    }
}

/// Get the edge color based on edge type
fn edge_color(edge_type: &str) -> &'static str {
    match edge_type {
        "chosen" => "#228B22",   // Forest green
        "rejected" => "#DC143C", // Crimson
        "blocks" => "#FF4500",   // Orange red
        "enables" => "#4169E1",  // Royal blue
        _ => "#333333",          // Dark gray
    }
}

/// Escape a string for DOT labels
fn escape_dot(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

/// Truncate a string to max length (Unicode-safe)
fn truncate(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        let char_len = max_len.saturating_sub(3);
        let truncated: String = s.chars().take(char_len).collect();
        format!("{}...", truncated)
    }
}

/// Extract confidence from metadata_json
fn extract_confidence(metadata: &Option<String>) -> Option<u8> {
    metadata.as_ref().and_then(|m| {
        serde_json::from_str::<serde_json::Value>(m)
            .ok()
            .and_then(|v| v.get("confidence").and_then(|c| c.as_u64()))
            .map(|c| c as u8)
    })
}

/// Extract commit hash from metadata_json
fn extract_commit(metadata: &Option<String>) -> Option<String> {
    metadata.as_ref().and_then(|m| {
        serde_json::from_str::<serde_json::Value>(m)
            .ok()
            .and_then(|v| {
                v.get("commit")
                    .and_then(|c| c.as_str().map(|s| s.to_string()))
            })
    })
}

/// Convert a decision graph to DOT format
pub fn graph_to_dot(graph: &DecisionGraph, config: &DotConfig) -> String {
    let mut dot = String::new();

    // Graph header
    wln!(dot, "digraph DecisionGraph {{");
    wln!(dot, "  rankdir={};", config.rankdir);
    wln!(dot, "  node [fontname=\"Arial\" fontsize=10];");
    wln!(dot, "  edge [fontname=\"Arial\" fontsize=9];");

    if let Some(title) = &config.title {
        wln!(dot, "  label=\"{}\";", escape_dot(title));
        wln!(dot, "  labelloc=t;");
        wln!(dot, "  fontsize=14;");
    }
    wln!(dot);

    // Nodes
    for node in &graph.nodes {
        let mut label = String::new();

        if config.show_ids {
            w!(label, "[{}] ", node.id);
        }

        label.push_str(&truncate(&node.title, 40));

        if config.show_confidence {
            if let Some(conf) = extract_confidence(&node.metadata_json) {
                w!(label, "\\n({}%)", conf);
            }
        }

        wln!(
            dot,
            "  {} [label=\"{}\" shape=\"{}\" fillcolor=\"{}\" style=\"filled\"];",
            node.id,
            escape_dot(&label),
            node_shape(&node.node_type),
            node_color(&node.node_type)
        );
    }

    wln!(dot);

    // Edges
    for edge in &graph.edges {
        let mut attrs = vec![
            format!("style=\"{}\"", edge_style(&edge.edge_type)),
            format!("color=\"{}\"", edge_color(&edge.edge_type)),
        ];

        if config.show_rationale {
            if let Some(rationale) = &edge.rationale {
                let truncated = truncate(rationale, 30);
                attrs.push(format!("label=\"{}\"", escape_dot(&truncated)));
            }
        }

        wln!(
            dot,
            "  {} -> {} [{}];",
            edge.from_node_id,
            edge.to_node_id,
            attrs.join(" ")
        );
    }

    wln!(dot, "}}");

    dot
}

/// Filter a graph to only include nodes reachable from given root IDs
pub fn filter_graph_from_roots(graph: &DecisionGraph, root_ids: &[i32]) -> DecisionGraph {
    let mut reachable: HashSet<i32> = HashSet::new();
    let mut to_visit: Vec<i32> = root_ids.to_vec();

    // Build adjacency map
    let mut children: HashMap<i32, Vec<i32>> = HashMap::new();
    for edge in &graph.edges {
        children
            .entry(edge.from_node_id)
            .or_default()
            .push(edge.to_node_id);
    }

    // BFS to find all reachable nodes
    while let Some(node_id) = to_visit.pop() {
        if reachable.insert(node_id) {
            if let Some(kids) = children.get(&node_id) {
                to_visit.extend(kids);
            }
        }
    }

    // Filter nodes and edges
    let nodes: Vec<DecisionNode> = graph
        .nodes
        .iter()
        .filter(|n| reachable.contains(&n.id))
        .cloned()
        .collect();

    let edges: Vec<DecisionEdge> = graph
        .edges
        .iter()
        .filter(|e| reachable.contains(&e.from_node_id) && reachable.contains(&e.to_node_id))
        .cloned()
        .collect();

    DecisionGraph { nodes, edges }
}

/// Filter a graph to only include specific node IDs (no traversal)
pub fn filter_graph_by_ids(graph: &DecisionGraph, node_ids: &[i32]) -> DecisionGraph {
    let id_set: HashSet<i32> = node_ids.iter().cloned().collect();

    let nodes: Vec<DecisionNode> = graph
        .nodes
        .iter()
        .filter(|n| id_set.contains(&n.id))
        .cloned()
        .collect();

    let edges: Vec<DecisionEdge> = graph
        .edges
        .iter()
        .filter(|e| id_set.contains(&e.from_node_id) && id_set.contains(&e.to_node_id))
        .cloned()
        .collect();

    DecisionGraph { nodes, edges }
}

/// Parse a node range specification (e.g., "1-11" or "1,2,5-10,15")
pub fn parse_node_range(spec: &str) -> Vec<i32> {
    let mut ids = Vec::new();

    for part in spec.split(',') {
        let part = part.trim();
        if part.contains('-') {
            let parts: Vec<&str> = part.split('-').collect();
            if parts.len() == 2 {
                if let (Ok(start), Ok(end)) = (
                    parts[0].trim().parse::<i32>(),
                    parts[1].trim().parse::<i32>(),
                ) {
                    for id in start..=end {
                        ids.push(id);
                    }
                }
            }
        } else if let Ok(id) = part.parse::<i32>() {
            ids.push(id);
        }
    }

    ids
}

/// Configuration for PR writeup generation
#[derive(Debug, Clone)]
pub struct WriteupConfig {
    /// PR title
    pub title: String,
    /// Root node IDs to include in writeup
    pub root_ids: Vec<i32>,
    /// Include DOT graph section
    pub include_dot: bool,
    /// Include test plan section
    pub include_test_plan: bool,
    /// PNG filename (will auto-detect GitHub repo/branch for URL)
    pub png_filename: Option<String>,
    /// GitHub repo in format "owner/repo" (auto-detected if not provided)
    pub github_repo: Option<String>,
    /// Git branch name (auto-detected if not provided)
    pub git_branch: Option<String>,
}

/// Generate a PR writeup from a decision graph
pub fn generate_pr_writeup(graph: &DecisionGraph, config: &WriteupConfig) -> String {
    let filtered = if config.root_ids.is_empty() {
        graph.clone()
    } else {
        filter_graph_from_roots(graph, &config.root_ids)
    };

    let mut writeup = String::new();

    // Title
    wln!(writeup, "## Summary\n");

    // Goals section
    let goals: Vec<&DecisionNode> = filtered
        .nodes
        .iter()
        .filter(|n| n.node_type == "goal")
        .collect();

    if !goals.is_empty() {
        for goal in &goals {
            wln!(writeup, "**Goal:** {}", goal.title);
            if let Some(desc) = &goal.description {
                wln!(writeup, "\n{}\n", desc);
            }
        }
        wln!(writeup);
    }

    // Decisions section
    let decisions: Vec<&DecisionNode> = filtered
        .nodes
        .iter()
        .filter(|n| n.node_type == "decision")
        .collect();

    if !decisions.is_empty() {
        wln!(writeup, "## Key Decisions\n");

        for decision in &decisions {
            wln!(writeup, "### {}\n", decision.title);

            // Find options for this decision
            let decision_options: Vec<&DecisionNode> = filtered
                .nodes
                .iter()
                .filter(|n| {
                    n.node_type == "option"
                        && filtered
                            .edges
                            .iter()
                            .any(|e| e.from_node_id == decision.id && e.to_node_id == n.id)
                })
                .collect();

            if !decision_options.is_empty() {
                wln!(writeup, "**Options considered:**\n");
                for opt in &decision_options {
                    let marker = if filtered.edges.iter().any(|e| {
                        e.from_node_id == decision.id
                            && e.to_node_id == opt.id
                            && e.edge_type == "chosen"
                    }) {
                        "[x]"
                    } else {
                        "[ ]"
                    };
                    wln!(writeup, "- {} {}", marker, opt.title);
                }
                wln!(writeup);
            }

            // Find observations related to this decision
            let observations: Vec<&DecisionNode> = filtered
                .nodes
                .iter()
                .filter(|n| {
                    n.node_type == "observation"
                        && filtered.edges.iter().any(|e| {
                            (e.from_node_id == decision.id && e.to_node_id == n.id)
                                || (e.from_node_id == n.id && e.to_node_id == decision.id)
                        })
                })
                .collect();

            if !observations.is_empty() {
                wln!(writeup, "**Observations:**\n");
                for obs in &observations {
                    wln!(writeup, "- {}", obs.title);
                }
                wln!(writeup);
            }
        }
    }

    // Actions section
    let actions: Vec<&DecisionNode> = filtered
        .nodes
        .iter()
        .filter(|n| n.node_type == "action")
        .collect();

    if !actions.is_empty() {
        wln!(writeup, "## Implementation\n");

        for action in &actions {
            let commit = extract_commit(&action.metadata_json);
            let commit_badge = commit
                .as_ref()
                .map(|c| format!(" `{}`", &c[..7.min(c.len())]))
                .unwrap_or_default();

            wln!(writeup, "- {}{}", action.title, commit_badge);
        }
        wln!(writeup);
    }

    // Outcomes section
    let outcomes: Vec<&DecisionNode> = filtered
        .nodes
        .iter()
        .filter(|n| n.node_type == "outcome")
        .collect();

    if !outcomes.is_empty() {
        wln!(writeup, "## Outcomes\n");

        for outcome in &outcomes {
            let confidence = extract_confidence(&outcome.metadata_json);
            let conf_badge = confidence
                .map(|c| format!(" ({}% confidence)", c))
                .unwrap_or_default();

            wln!(writeup, "- {}{}", outcome.title, conf_badge);
        }
        wln!(writeup);
    }

    // DOT graph section
    if config.include_dot {
        wln!(writeup, "## Decision Graph\n");

        // Build image URL if PNG filename provided
        let image_url = config.png_filename.as_ref().map(|filename| {
            if let (Some(repo), Some(branch)) = (&config.github_repo, &config.git_branch) {
                format!(
                    "https://raw.githubusercontent.com/{}/{}/{}",
                    repo, branch, filename
                )
            } else {
                // Fallback to relative path (won't work in PR descriptions but OK for files)
                filename.clone()
            }
        });

        // If image URL available, show the PNG image
        if let Some(url) = &image_url {
            wln!(writeup, "![Decision Graph]({})\n", url);

            // Put DOT source in collapsible details
            wln!(writeup, "<details>");
            wln!(writeup, "<summary>DOT source (click to expand)</summary>\n");
        }

        wln!(writeup, "```dot");
        let dot_config = DotConfig {
            title: Some(config.title.clone()),
            show_ids: true,
            show_rationale: false, // Keep DOT compact in writeup
            show_confidence: true,
            rankdir: "TB".to_string(),
        };
        w!(writeup, "{}", graph_to_dot(&filtered, &dot_config));
        wln!(writeup, "```\n");

        if image_url.is_some() {
            wln!(writeup, "</details>\n");
        } else {
            wln!(
                writeup,
                "*Render with: `dot -Tpng graph.dot -o graph.png`*\n"
            );
        }
    }

    // Test plan section
    if config.include_test_plan {
        wln!(writeup, "## Test Plan\n");

        // Generate test plan from outcomes
        let test_items: Vec<String> = outcomes
            .iter()
            .filter(|o| o.status == "completed")
            .map(|o| format!("- [x] {}", o.title))
            .collect();

        if test_items.is_empty() {
            wln!(writeup, "- [ ] Verify implementation");
            wln!(writeup, "- [ ] Run test suite");
        } else {
            for item in test_items {
                wln!(writeup, "{}", item);
            }
        }
        wln!(writeup);
    }

    // Decision graph reference
    if !filtered.nodes.is_empty() {
        let node_ids: Vec<String> = filtered.nodes.iter().map(|n| n.id.to_string()).collect();
        wln!(writeup, "## Decision Graph Reference\n");
        wln!(
            writeup,
            "This PR corresponds to deciduous nodes: {}\n",
            node_ids.join(", ")
        );
    }

    writeup
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_graph() -> DecisionGraph {
        DecisionGraph {
            nodes: vec![
                DecisionNode {
                    id: 1,
                    change_id: "change-id-1".to_string(),
                    node_type: "goal".to_string(),
                    title: "Build feature X".to_string(),
                    description: None,
                    status: "pending".to_string(),
                    created_at: "2025-01-01T00:00:00Z".to_string(),
                    updated_at: "2025-01-01T00:00:00Z".to_string(),
                    metadata_json: Some(r#"{"confidence":90}"#.to_string()),
                },
                DecisionNode {
                    id: 2,
                    change_id: "change-id-2".to_string(),
                    node_type: "decision".to_string(),
                    title: "Choose approach".to_string(),
                    description: None,
                    status: "pending".to_string(),
                    created_at: "2025-01-01T00:00:00Z".to_string(),
                    updated_at: "2025-01-01T00:00:00Z".to_string(),
                    metadata_json: None,
                },
                DecisionNode {
                    id: 3,
                    change_id: "change-id-3".to_string(),
                    node_type: "action".to_string(),
                    title: "Implement solution".to_string(),
                    description: None,
                    status: "completed".to_string(),
                    created_at: "2025-01-01T00:00:00Z".to_string(),
                    updated_at: "2025-01-01T00:00:00Z".to_string(),
                    metadata_json: Some(r#"{"commit":"abc1234"}"#.to_string()),
                },
            ],
            edges: vec![
                DecisionEdge {
                    id: 1,
                    from_node_id: 1,
                    to_node_id: 2,
                    from_change_id: Some("change-id-1".to_string()),
                    to_change_id: Some("change-id-2".to_string()),
                    edge_type: "leads_to".to_string(),
                    weight: Some(1.0),
                    rationale: Some("Goal requires decision".to_string()),
                    created_at: "2025-01-01T00:00:00Z".to_string(),
                },
                DecisionEdge {
                    id: 2,
                    from_node_id: 2,
                    to_node_id: 3,
                    from_change_id: Some("change-id-2".to_string()),
                    to_change_id: Some("change-id-3".to_string()),
                    edge_type: "leads_to".to_string(),
                    weight: Some(1.0),
                    rationale: None,
                    created_at: "2025-01-01T00:00:00Z".to_string(),
                },
            ],
        }
    }

    #[test]
    fn test_graph_to_dot() {
        let graph = sample_graph();
        let config = DotConfig::default();
        let dot = graph_to_dot(&graph, &config);

        assert!(dot.contains("digraph DecisionGraph"));
        assert!(dot.contains("1 [label="));
        assert!(dot.contains("1 -> 2"));
        assert!(dot.contains("shape=\"house\"")); // goal shape
        assert!(dot.contains("shape=\"diamond\"")); // decision shape
    }

    #[test]
    fn test_filter_graph() {
        let graph = sample_graph();
        let filtered = filter_graph_from_roots(&graph, &[1]);

        assert_eq!(filtered.nodes.len(), 3);
        assert_eq!(filtered.edges.len(), 2);
    }

    #[test]
    fn test_generate_writeup() {
        let graph = sample_graph();
        let config = WriteupConfig {
            title: "Test PR".to_string(),
            root_ids: vec![],
            include_dot: true,
            include_test_plan: true,
            png_filename: None,
            github_repo: None,
            git_branch: None,
        };
        let writeup = generate_pr_writeup(&graph, &config);

        assert!(writeup.contains("## Summary"));
        assert!(writeup.contains("Build feature X"));
        assert!(writeup.contains("## Decision Graph"));
        assert!(writeup.contains("```dot"));
    }

    #[test]
    fn test_extract_confidence() {
        let meta = Some(r#"{"confidence":85}"#.to_string());
        assert_eq!(extract_confidence(&meta), Some(85));

        let no_meta: Option<String> = None;
        assert_eq!(extract_confidence(&no_meta), None);
    }

    #[test]
    fn test_extract_commit() {
        let meta = Some(r#"{"commit":"abc1234"}"#.to_string());
        assert_eq!(extract_commit(&meta), Some("abc1234".to_string()));
    }

    // === Additional Helper Function Tests ===

    #[test]
    fn test_node_shape() {
        assert_eq!(node_shape("goal"), "house");
        assert_eq!(node_shape("decision"), "diamond");
        assert_eq!(node_shape("option"), "parallelogram");
        assert_eq!(node_shape("action"), "box");
        assert_eq!(node_shape("outcome"), "ellipse");
        assert_eq!(node_shape("observation"), "note");
        assert_eq!(node_shape("unknown"), "box"); // default
    }

    #[test]
    fn test_node_color() {
        assert_eq!(node_color("goal"), "#FFE4B5");
        assert_eq!(node_color("decision"), "#E6E6FA");
        assert_eq!(node_color("option"), "#E0FFFF");
        assert_eq!(node_color("action"), "#90EE90");
        assert_eq!(node_color("outcome"), "#87CEEB");
        assert_eq!(node_color("observation"), "#DDA0DD");
        assert_eq!(node_color("unknown"), "#F5F5F5"); // default: white smoke
    }

    #[test]
    fn test_edge_style() {
        assert_eq!(edge_style("leads_to"), "solid"); // default
        assert_eq!(edge_style("chosen"), "bold");
        assert_eq!(edge_style("rejected"), "dashed");
        assert_eq!(edge_style("blocks"), "dotted");
        assert_eq!(edge_style("unknown"), "solid"); // default
    }

    #[test]
    fn test_edge_color() {
        assert_eq!(edge_color("leads_to"), "#333333"); // default
        assert_eq!(edge_color("chosen"), "#228B22"); // forest green
        assert_eq!(edge_color("rejected"), "#DC143C"); // crimson
        assert_eq!(edge_color("blocks"), "#FF4500"); // orange red
        assert_eq!(edge_color("enables"), "#4169E1"); // royal blue
        assert_eq!(edge_color("unknown"), "#333333"); // default
    }

    #[test]
    fn test_escape_dot() {
        assert_eq!(escape_dot("hello"), "hello");
        assert_eq!(escape_dot("hello \"world\""), "hello \\\"world\\\"");
        assert_eq!(escape_dot("line1\nline2"), "line1\\nline2");
        assert_eq!(escape_dot("back\\slash"), "back\\\\slash");
    }

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 8), "hello...");
        assert_eq!(truncate("hi", 2), "hi");
        assert_eq!(truncate("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_unicode() {
        // Unicode-safe truncation
        assert_eq!(truncate("🎉🎊🎁", 10), "🎉🎊🎁");
        let result = truncate("🎉🎊🎁🎄🎅🎆", 5);
        assert!(result.ends_with("...") || result.chars().count() <= 5);
    }

    // === DOT Config Tests ===

    #[test]
    fn test_dot_config_default() {
        let config = DotConfig::default();
        assert!(config.show_rationale);
        assert!(config.show_confidence);
        assert!(config.show_ids);
        assert_eq!(config.rankdir, "TB");
        assert!(config.title.is_none());
    }

    #[test]
    fn test_dot_with_title() {
        let graph = sample_graph();
        let config = DotConfig {
            title: Some("My Graph".to_string()),
            ..Default::default()
        };
        let dot = graph_to_dot(&graph, &config);

        assert!(dot.contains("label=\"My Graph\""));
        assert!(dot.contains("labelloc=t"));
    }

    #[test]
    fn test_dot_with_custom_rankdir() {
        let graph = sample_graph();
        let config = DotConfig {
            rankdir: "LR".to_string(),
            ..Default::default()
        };
        let dot = graph_to_dot(&graph, &config);

        assert!(dot.contains("rankdir=LR"));
    }

    // === Filter Tests ===

    #[test]
    fn test_filter_graph_empty_roots() {
        let graph = sample_graph();
        let filtered = filter_graph_from_roots(&graph, &[]);

        // Empty roots should return empty graph
        assert!(filtered.nodes.is_empty());
        assert!(filtered.edges.is_empty());
    }

    #[test]
    fn test_filter_graph_single_node() {
        let graph = sample_graph();
        // Filter starting from node 3 (leaf)
        let filtered = filter_graph_from_roots(&graph, &[3]);

        assert_eq!(filtered.nodes.len(), 1);
        assert_eq!(filtered.edges.len(), 0);
    }

    #[test]
    fn test_filter_graph_nonexistent_root() {
        let graph = sample_graph();
        let filtered = filter_graph_from_roots(&graph, &[999]);

        assert!(filtered.nodes.is_empty());
    }

    // === Extract Tests ===

    #[test]
    fn test_extract_confidence_invalid_json() {
        let meta = Some("not json".to_string());
        assert_eq!(extract_confidence(&meta), None);
    }

    #[test]
    fn test_extract_confidence_missing_field() {
        let meta = Some(r#"{"branch":"main"}"#.to_string());
        assert_eq!(extract_confidence(&meta), None);
    }

    #[test]
    fn test_extract_commit_invalid_json() {
        let meta = Some("not json".to_string());
        assert_eq!(extract_commit(&meta), None);
    }

    // === Writeup Config Tests ===

    #[test]
    fn test_writeup_without_dot() {
        let graph = sample_graph();
        let config = WriteupConfig {
            title: "No DOT".to_string(),
            root_ids: vec![],
            include_dot: false,
            include_test_plan: true,
            png_filename: None,
            github_repo: None,
            git_branch: None,
        };
        let writeup = generate_pr_writeup(&graph, &config);

        assert!(!writeup.contains("```dot"));
        // Note: "## Decision Graph Reference" is always present, but "## Decision Graph\n" is not
        assert!(!writeup.contains("## Decision Graph\n"));
    }

    #[test]
    fn test_writeup_without_test_plan() {
        let graph = sample_graph();
        let config = WriteupConfig {
            title: "No Test Plan".to_string(),
            root_ids: vec![],
            include_dot: false,
            include_test_plan: false,
            png_filename: None,
            github_repo: None,
            git_branch: None,
        };
        let writeup = generate_pr_writeup(&graph, &config);

        assert!(!writeup.contains("## Test Plan"));
    }

    #[test]
    fn test_writeup_with_png() {
        let graph = sample_graph();
        let config = WriteupConfig {
            title: "With PNG".to_string(),
            root_ids: vec![],
            include_dot: true,
            include_test_plan: false,
            png_filename: Some("docs/graph.png".to_string()),
            github_repo: Some("owner/repo".to_string()),
            git_branch: Some("main".to_string()),
        };
        let writeup = generate_pr_writeup(&graph, &config);

        assert!(writeup.contains("![Decision Graph]"));
        assert!(writeup.contains("raw.githubusercontent.com"));
        assert!(writeup.contains("<details>")); // DOT in collapsible
    }

    // === Empty Graph Tests ===

    #[test]
    fn test_dot_empty_graph() {
        let graph = DecisionGraph {
            nodes: vec![],
            edges: vec![],
        };
        let config = DotConfig::default();
        let dot = graph_to_dot(&graph, &config);

        assert!(dot.contains("digraph DecisionGraph"));
        assert!(dot.contains("}"));
    }

    #[test]
    fn test_writeup_empty_graph() {
        let graph = DecisionGraph {
            nodes: vec![],
            edges: vec![],
        };
        let config = WriteupConfig {
            title: "Empty".to_string(),
            root_ids: vec![],
            include_dot: false,
            include_test_plan: false,
            png_filename: None,
            github_repo: None,
            git_branch: None,
        };
        let writeup = generate_pr_writeup(&graph, &config);

        // Should still produce valid output
        assert!(writeup.contains("## Summary"));
    }
}
