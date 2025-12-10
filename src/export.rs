//! Export utilities for decision graphs
//!
//! Provides DOT graph export and PR writeup generation.

use crate::db::{DecisionEdge, DecisionGraph, DecisionNode};
use std::collections::{HashMap, HashSet};
use std::fmt::Write;

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
        "goal" => "#FFE4B5",      // Moccasin (warm yellow)
        "decision" => "#E6E6FA",   // Lavender
        "option" => "#E0FFFF",     // Light cyan
        "action" => "#90EE90",     // Light green
        "outcome" => "#87CEEB",    // Sky blue
        "observation" => "#DDA0DD", // Plum
        _ => "#F5F5F5",            // White smoke
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
        "chosen" => "#228B22",     // Forest green
        "rejected" => "#DC143C",   // Crimson
        "blocks" => "#FF4500",     // Orange red
        "enables" => "#4169E1",    // Royal blue
        _ => "#333333",            // Dark gray
    }
}

/// Escape a string for DOT labels
fn escape_dot(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

/// Truncate a string to max length
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
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
            .and_then(|v| v.get("commit").and_then(|c| c.as_str().map(|s| s.to_string())))
    })
}

/// Convert a decision graph to DOT format
pub fn graph_to_dot(graph: &DecisionGraph, config: &DotConfig) -> String {
    let mut dot = String::new();

    // Graph header
    writeln!(dot, "digraph DecisionGraph {{").unwrap();
    writeln!(dot, "  rankdir={};", config.rankdir).unwrap();
    writeln!(dot, "  node [fontname=\"Arial\" fontsize=10];").unwrap();
    writeln!(dot, "  edge [fontname=\"Arial\" fontsize=9];").unwrap();

    if let Some(title) = &config.title {
        writeln!(dot, "  label=\"{}\";", escape_dot(title)).unwrap();
        writeln!(dot, "  labelloc=t;").unwrap();
        writeln!(dot, "  fontsize=14;").unwrap();
    }
    writeln!(dot).unwrap();

    // Nodes
    for node in &graph.nodes {
        let mut label = String::new();

        if config.show_ids {
            write!(label, "[{}] ", node.id).unwrap();
        }

        label.push_str(&truncate(&node.title, 40));

        if config.show_confidence {
            if let Some(conf) = extract_confidence(&node.metadata_json) {
                write!(label, "\\n({}%)", conf).unwrap();
            }
        }

        writeln!(
            dot,
            "  {} [label=\"{}\" shape=\"{}\" fillcolor=\"{}\" style=\"filled\"];",
            node.id,
            escape_dot(&label),
            node_shape(&node.node_type),
            node_color(&node.node_type)
        )
        .unwrap();
    }

    writeln!(dot).unwrap();

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

        writeln!(
            dot,
            "  {} -> {} [{}];",
            edge.from_node_id,
            edge.to_node_id,
            attrs.join(" ")
        )
        .unwrap();
    }

    writeln!(dot, "}}").unwrap();

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
                if let (Ok(start), Ok(end)) = (parts[0].trim().parse::<i32>(), parts[1].trim().parse::<i32>()) {
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
    writeln!(writeup, "## Summary\n").unwrap();

    // Goals section
    let goals: Vec<&DecisionNode> = filtered
        .nodes
        .iter()
        .filter(|n| n.node_type == "goal")
        .collect();

    if !goals.is_empty() {
        for goal in &goals {
            writeln!(writeup, "**Goal:** {}", goal.title).unwrap();
            if let Some(desc) = &goal.description {
                writeln!(writeup, "\n{}\n", desc).unwrap();
            }
        }
        writeln!(writeup).unwrap();
    }

    // Decisions section
    let decisions: Vec<&DecisionNode> = filtered
        .nodes
        .iter()
        .filter(|n| n.node_type == "decision")
        .collect();

    if !decisions.is_empty() {
        writeln!(writeup, "## Key Decisions\n").unwrap();

        for decision in &decisions {
            writeln!(writeup, "### {}\n", decision.title).unwrap();

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
                writeln!(writeup, "**Options considered:**\n").unwrap();
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
                    writeln!(writeup, "- {} {}", marker, opt.title).unwrap();
                }
                writeln!(writeup).unwrap();
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
                writeln!(writeup, "**Observations:**\n").unwrap();
                for obs in &observations {
                    writeln!(writeup, "- {}", obs.title).unwrap();
                }
                writeln!(writeup).unwrap();
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
        writeln!(writeup, "## Implementation\n").unwrap();

        for action in &actions {
            let commit = extract_commit(&action.metadata_json);
            let commit_badge = commit
                .as_ref()
                .map(|c| format!(" `{}`", &c[..7.min(c.len())]))
                .unwrap_or_default();

            writeln!(writeup, "- {}{}", action.title, commit_badge).unwrap();
        }
        writeln!(writeup).unwrap();
    }

    // Outcomes section
    let outcomes: Vec<&DecisionNode> = filtered
        .nodes
        .iter()
        .filter(|n| n.node_type == "outcome")
        .collect();

    if !outcomes.is_empty() {
        writeln!(writeup, "## Outcomes\n").unwrap();

        for outcome in &outcomes {
            let confidence = extract_confidence(&outcome.metadata_json);
            let conf_badge = confidence
                .map(|c| format!(" ({}% confidence)", c))
                .unwrap_or_default();

            writeln!(writeup, "- {}{}", outcome.title, conf_badge).unwrap();
        }
        writeln!(writeup).unwrap();
    }

    // DOT graph section
    if config.include_dot {
        writeln!(writeup, "## Decision Graph\n").unwrap();

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
            writeln!(writeup, "![Decision Graph]({})\n", url).unwrap();

            // Put DOT source in collapsible details
            writeln!(writeup, "<details>").unwrap();
            writeln!(writeup, "<summary>DOT source (click to expand)</summary>\n").unwrap();
        }

        writeln!(writeup, "```dot").unwrap();
        let dot_config = DotConfig {
            title: Some(config.title.clone()),
            show_ids: true,
            show_rationale: false, // Keep DOT compact in writeup
            show_confidence: true,
            rankdir: "TB".to_string(),
        };
        write!(writeup, "{}", graph_to_dot(&filtered, &dot_config)).unwrap();
        writeln!(writeup, "```\n").unwrap();

        if image_url.is_some() {
            writeln!(writeup, "</details>\n").unwrap();
        } else {
            writeln!(writeup, "*Render with: `dot -Tpng graph.dot -o graph.png`*\n").unwrap();
        }
    }

    // Test plan section
    if config.include_test_plan {
        writeln!(writeup, "## Test Plan\n").unwrap();

        // Generate test plan from outcomes
        let test_items: Vec<String> = outcomes
            .iter()
            .filter(|o| o.status == "completed")
            .map(|o| format!("- [x] {}", o.title))
            .collect();

        if test_items.is_empty() {
            writeln!(writeup, "- [ ] Verify implementation").unwrap();
            writeln!(writeup, "- [ ] Run test suite").unwrap();
        } else {
            for item in test_items {
                writeln!(writeup, "{}", item).unwrap();
            }
        }
        writeln!(writeup).unwrap();
    }

    // Decision graph reference
    if !filtered.nodes.is_empty() {
        let node_ids: Vec<String> = filtered.nodes.iter().map(|n| n.id.to_string()).collect();
        writeln!(writeup, "## Decision Graph Reference\n").unwrap();
        writeln!(
            writeup,
            "This PR corresponds to deciduous nodes: {}\n",
            node_ids.join(", ")
        )
        .unwrap();
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
                    edge_type: "leads_to".to_string(),
                    weight: Some(1.0),
                    rationale: Some("Goal requires decision".to_string()),
                    created_at: "2025-01-01T00:00:00Z".to_string(),
                },
                DecisionEdge {
                    id: 2,
                    from_node_id: 2,
                    to_node_id: 3,
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
}
