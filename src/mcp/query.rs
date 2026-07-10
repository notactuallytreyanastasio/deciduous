//! Graph analysis and reporting — pure functions on DecisionGraph.
//!
//! These functions operate on in-memory graph data (nodes + edges) and return
//! structured results. They are the "reporting layer" that gives AI assistants
//! enough context to generate natural language reports about what/why/how
//! decisions were made.
//!
//! **Functional core**: every function takes data in and returns data out.
//! No IO, no database access, no side effects.

use crate::db::{DecisionEdge, DecisionGraph, DecisionNode};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet, VecDeque};

// ---------------------------------------------------------------------------
// Trace chain — BFS traversal from a starting node
// ---------------------------------------------------------------------------

/// Direction for graph traversal.
#[derive(Debug, Clone, PartialEq)]
pub enum TraceDirection {
    Both,
    Outgoing,
    Incoming,
}

impl TraceDirection {
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "outgoing" => Self::Outgoing,
            "incoming" => Self::Incoming,
            _ => Self::Both,
        }
    }
}

/// Result of a chain trace — a connected subgraph.
#[derive(Debug, Clone)]
pub struct TraceResult {
    pub start_node_id: i32,
    pub nodes: Vec<DecisionNode>,
    pub edges: Vec<DecisionEdge>,
    pub depth_map: HashMap<i32, usize>,
}

/// BFS traversal from a starting node. Returns all reachable nodes and edges.
pub fn trace_chain(
    graph: &DecisionGraph,
    start_id: i32,
    max_depth: usize,
    direction: &TraceDirection,
) -> TraceResult {
    let mut visited: HashSet<i32> = HashSet::new();
    let mut queue: VecDeque<(i32, usize)> = VecDeque::new();
    let mut depth_map: HashMap<i32, usize> = HashMap::new();

    // Build adjacency for fast lookup
    let outgoing = build_adjacency_out(&graph.edges);
    let incoming = build_adjacency_in(&graph.edges);

    queue.push_back((start_id, 0));
    visited.insert(start_id);
    depth_map.insert(start_id, 0);

    while let Some((node_id, depth)) = queue.pop_front() {
        if max_depth > 0 && depth >= max_depth {
            continue;
        }

        let neighbors: Vec<i32> = match direction {
            TraceDirection::Outgoing => outgoing.get(&node_id).cloned().unwrap_or_default(),
            TraceDirection::Incoming => incoming.get(&node_id).cloned().unwrap_or_default(),
            TraceDirection::Both => {
                let mut all = outgoing.get(&node_id).cloned().unwrap_or_default();
                all.extend(incoming.get(&node_id).cloned().unwrap_or_default());
                all
            }
        };

        for neighbor_id in neighbors {
            if visited.insert(neighbor_id) {
                depth_map.insert(neighbor_id, depth + 1);
                queue.push_back((neighbor_id, depth + 1));
            }
        }
    }

    // Collect nodes and edges that belong to this subgraph
    let node_set: HashSet<i32> = visited;
    let nodes: Vec<DecisionNode> = graph
        .nodes
        .iter()
        .filter(|n| node_set.contains(&n.id))
        .cloned()
        .collect();

    let edges: Vec<DecisionEdge> = graph
        .edges
        .iter()
        .filter(|e| node_set.contains(&e.from_node_id) && node_set.contains(&e.to_node_id))
        .cloned()
        .collect();

    TraceResult {
        start_node_id: start_id,
        nodes,
        edges,
        depth_map,
    }
}

// ---------------------------------------------------------------------------
// Node context — neighborhood of a single node
// ---------------------------------------------------------------------------

/// Full context around a single node.
#[derive(Debug, Clone)]
pub struct NodeContext {
    pub node: Option<DecisionNode>,
    pub parents: Vec<DecisionNode>,
    pub children: Vec<DecisionNode>,
    pub siblings: Vec<DecisionNode>,
    pub incoming_edges: Vec<DecisionEdge>,
    pub outgoing_edges: Vec<DecisionEdge>,
}

/// Get the full context around a node: parents, children, siblings, and edges.
pub fn get_node_context(graph: &DecisionGraph, node_id: i32) -> NodeContext {
    let node = graph.nodes.iter().find(|n| n.id == node_id).cloned();

    let incoming_edges: Vec<DecisionEdge> = graph
        .edges
        .iter()
        .filter(|e| e.to_node_id == node_id)
        .cloned()
        .collect();

    let outgoing_edges: Vec<DecisionEdge> = graph
        .edges
        .iter()
        .filter(|e| e.from_node_id == node_id)
        .cloned()
        .collect();

    let parent_ids: HashSet<i32> = incoming_edges.iter().map(|e| e.from_node_id).collect();
    let child_ids: HashSet<i32> = outgoing_edges.iter().map(|e| e.to_node_id).collect();

    let parents: Vec<DecisionNode> = graph
        .nodes
        .iter()
        .filter(|n| parent_ids.contains(&n.id))
        .cloned()
        .collect();

    let children: Vec<DecisionNode> = graph
        .nodes
        .iter()
        .filter(|n| child_ids.contains(&n.id))
        .cloned()
        .collect();

    // Siblings: nodes that share a parent with this node
    let sibling_ids: HashSet<i32> = graph
        .edges
        .iter()
        .filter(|e| parent_ids.contains(&e.from_node_id) && e.to_node_id != node_id)
        .map(|e| e.to_node_id)
        .collect();

    let siblings: Vec<DecisionNode> = graph
        .nodes
        .iter()
        .filter(|n| sibling_ids.contains(&n.id))
        .cloned()
        .collect();

    NodeContext {
        node,
        parents,
        children,
        siblings,
        incoming_edges,
        outgoing_edges,
    }
}

// ---------------------------------------------------------------------------
// Timeline — chronological view of nodes
// ---------------------------------------------------------------------------

/// Get nodes sorted by creation time (newest first), with optional filters.
pub fn get_timeline(
    graph: &DecisionGraph,
    limit: usize,
    node_type: Option<&str>,
    branch: Option<&str>,
    since: Option<&str>,
) -> Vec<DecisionNode> {
    let mut nodes: Vec<DecisionNode> = graph
        .nodes
        .iter()
        .filter(|n| {
            if let Some(t) = node_type {
                if n.node_type != t {
                    return false;
                }
            }
            if let Some(b) = branch {
                if !node_has_branch(n, b) {
                    return false;
                }
            }
            if let Some(since_date) = since {
                if n.created_at.as_str() < since_date {
                    return false;
                }
            }
            true
        })
        .cloned()
        .collect();

    // Sort newest first
    nodes.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    if limit > 0 {
        nodes.truncate(limit);
    }

    nodes
}

// ---------------------------------------------------------------------------
// Pulse — graph health and statistics
// ---------------------------------------------------------------------------

/// Graph health metrics and statistics.
#[derive(Debug, Clone)]
pub struct PulseReport {
    pub total_nodes: usize,
    pub total_edges: usize,
    pub nodes_by_type: HashMap<String, usize>,
    pub nodes_by_status: HashMap<String, usize>,
    pub orphan_count: usize,
    pub orphan_nodes: Vec<OrphanNode>,
    pub active_goals: Vec<DecisionNode>,
    pub recent_nodes: Vec<DecisionNode>,
}

/// Get comprehensive graph health metrics.
pub fn get_pulse(graph: &DecisionGraph, branch: Option<&str>, recent_count: usize) -> PulseReport {
    let filtered_nodes: Vec<&DecisionNode> = graph
        .nodes
        .iter()
        .filter(|n| {
            if let Some(b) = branch {
                node_has_branch(n, b)
            } else {
                true
            }
        })
        .collect();

    let mut nodes_by_type: HashMap<String, usize> = HashMap::new();
    let mut nodes_by_status: HashMap<String, usize> = HashMap::new();

    for n in &filtered_nodes {
        *nodes_by_type.entry(n.node_type.clone()).or_insert(0) += 1;
        *nodes_by_status.entry(n.status.clone()).or_insert(0) += 1;
    }

    let orphan_nodes = find_orphans(graph);
    let orphan_count = orphan_nodes.len();

    let active_goals: Vec<DecisionNode> = filtered_nodes
        .iter()
        .filter(|n| n.node_type == "goal" && (n.status == "active" || n.status == "pending"))
        .cloned()
        .cloned()
        .collect();

    let mut recent: Vec<DecisionNode> = filtered_nodes.iter().cloned().cloned().collect();
    recent.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    recent.truncate(recent_count);

    PulseReport {
        total_nodes: filtered_nodes.len(),
        total_edges: graph.edges.len(),
        nodes_by_type,
        nodes_by_status,
        orphan_count,
        orphan_nodes,
        active_goals,
        recent_nodes: recent,
    }
}

// ---------------------------------------------------------------------------
// Orphan detection
// ---------------------------------------------------------------------------

/// A node that violates connection rules.
#[derive(Debug, Clone)]
pub struct OrphanNode {
    pub node: DecisionNode,
    pub reason: String,
}

/// Find nodes that violate the connection rules.
/// Goals are valid orphans (root nodes). Other types should have parents.
pub fn find_orphans(graph: &DecisionGraph) -> Vec<OrphanNode> {
    let nodes_with_incoming: HashSet<i32> = graph.edges.iter().map(|e| e.to_node_id).collect();
    let nodes_with_outgoing: HashSet<i32> = graph.edges.iter().map(|e| e.from_node_id).collect();

    graph
        .nodes
        .iter()
        .filter_map(|n| {
            let has_incoming = nodes_with_incoming.contains(&n.id);
            let has_outgoing = nodes_with_outgoing.contains(&n.id);

            // Goals are valid root nodes — skip them
            if n.node_type == "goal" {
                return None;
            }

            if !has_incoming {
                Some(OrphanNode {
                    node: n.clone(),
                    reason: match n.node_type.as_str() {
                        "outcome" => "Outcome without parent action".to_string(),
                        "action" => "Action without parent decision".to_string(),
                        "decision" => "Decision without parent option".to_string(),
                        "option" => "Option without parent goal".to_string(),
                        "observation" => "Observation not linked to any node".to_string(),
                        _ => format!("{} without incoming edges", n.node_type),
                    },
                })
            } else if !has_outgoing && n.node_type != "outcome" && n.node_type != "observation" {
                // Leaf nodes: outcomes and observations are fine without outgoing edges
                None
            } else {
                None
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Branch summary
// ---------------------------------------------------------------------------

/// Summary of activity on a specific branch.
#[derive(Debug, Clone)]
pub struct BranchSummary {
    pub branch: String,
    pub total_nodes: usize,
    pub goals: Vec<DecisionNode>,
    pub decisions: Vec<DecisionNode>,
    pub actions: Vec<DecisionNode>,
    pub outcomes: Vec<DecisionNode>,
    pub observations: Vec<DecisionNode>,
}

/// Get a summary of all nodes on a specific branch.
pub fn get_branch_summary(graph: &DecisionGraph, branch: &str) -> BranchSummary {
    let branch_nodes: Vec<&DecisionNode> = graph
        .nodes
        .iter()
        .filter(|n| node_has_branch(n, branch))
        .collect();

    let collect_type = |t: &str| -> Vec<DecisionNode> {
        branch_nodes
            .iter()
            .filter(|n| n.node_type == t)
            .cloned()
            .cloned()
            .collect()
    };

    BranchSummary {
        branch: branch.to_string(),
        total_nodes: branch_nodes.len(),
        goals: collect_type("goal"),
        decisions: collect_type("decision"),
        actions: collect_type("action"),
        outcomes: collect_type("outcome"),
        observations: collect_type("observation"),
    }
}

// ---------------------------------------------------------------------------
// JSON serialization helpers (for MCP tool results)
// ---------------------------------------------------------------------------

/// Convert a TraceResult into a JSON value for MCP response.
pub fn trace_result_to_json(result: &TraceResult) -> Value {
    let nodes: Vec<Value> = result
        .nodes
        .iter()
        .map(|n| {
            let mut j = node_summary_json(n);
            if let Some(depth) = result.depth_map.get(&n.id) {
                j["depth"] = json!(depth);
            }
            j
        })
        .collect();

    let edges: Vec<Value> = result.edges.iter().map(edge_summary_json).collect();

    json!({
        "start_node_id": result.start_node_id,
        "node_count": result.nodes.len(),
        "edge_count": result.edges.len(),
        "nodes": nodes,
        "edges": edges,
    })
}

/// Convert a NodeContext into a JSON value for MCP response.
pub fn node_context_to_json(ctx: &NodeContext) -> Value {
    json!({
        "node": ctx.node.as_ref().map(node_summary_json),
        "parents": ctx.parents.iter().map(node_summary_json).collect::<Vec<_>>(),
        "children": ctx.children.iter().map(node_summary_json).collect::<Vec<_>>(),
        "siblings": ctx.siblings.iter().map(node_summary_json).collect::<Vec<_>>(),
        "incoming_edges": ctx.incoming_edges.iter().map(edge_summary_json).collect::<Vec<_>>(),
        "outgoing_edges": ctx.outgoing_edges.iter().map(edge_summary_json).collect::<Vec<_>>(),
    })
}

/// Convert a PulseReport into a JSON value for MCP response.
pub fn pulse_report_to_json(report: &PulseReport) -> Value {
    json!({
        "total_nodes": report.total_nodes,
        "total_edges": report.total_edges,
        "nodes_by_type": report.nodes_by_type,
        "nodes_by_status": report.nodes_by_status,
        "orphan_count": report.orphan_count,
        "orphans": report.orphan_nodes.iter().map(|o| json!({
            "id": o.node.id,
            "node_type": o.node.node_type,
            "title": o.node.title,
            "reason": o.reason,
        })).collect::<Vec<_>>(),
        "active_goals": report.active_goals.iter().map(node_summary_json).collect::<Vec<_>>(),
        "recent_nodes": report.recent_nodes.iter().map(node_summary_json).collect::<Vec<_>>(),
    })
}

/// Convert a BranchSummary into a JSON value for MCP response.
pub fn branch_summary_to_json(summary: &BranchSummary) -> Value {
    json!({
        "branch": summary.branch,
        "total_nodes": summary.total_nodes,
        "goals": summary.goals.iter().map(node_summary_json).collect::<Vec<_>>(),
        "decisions": summary.decisions.iter().map(node_summary_json).collect::<Vec<_>>(),
        "actions": summary.actions.iter().map(node_summary_json).collect::<Vec<_>>(),
        "outcomes": summary.outcomes.iter().map(node_summary_json).collect::<Vec<_>>(),
        "observations": summary.observations.iter().map(node_summary_json).collect::<Vec<_>>(),
    })
}

/// Convert orphan list into a JSON value.
pub fn orphans_to_json(orphans: &[OrphanNode]) -> Value {
    json!({
        "count": orphans.len(),
        "orphans": orphans.iter().map(|o| json!({
            "id": o.node.id,
            "node_type": o.node.node_type,
            "title": o.node.title,
            "status": o.node.status,
            "reason": o.reason,
        })).collect::<Vec<_>>(),
    })
}

/// Convert timeline nodes into a JSON value.
pub fn timeline_to_json(nodes: &[DecisionNode]) -> Value {
    json!({
        "count": nodes.len(),
        "nodes": nodes.iter().map(node_summary_json).collect::<Vec<_>>(),
    })
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn node_summary_json(node: &DecisionNode) -> Value {
    let mut j = json!({
        "id": node.id,
        "node_type": node.node_type,
        "title": node.title,
        "status": node.status,
        "created_at": node.created_at,
    });

    if let Some(ref desc) = node.description {
        j["description"] = json!(desc);
    }

    // Unpack key metadata fields
    if let Some(ref meta_str) = node.metadata_json {
        if let Ok(meta) = serde_json::from_str::<Value>(meta_str) {
            if let Some(c) = meta.get("confidence").and_then(Value::as_u64) {
                j["confidence"] = json!(c);
            }
            if let Some(b) = meta.get("branch").and_then(Value::as_str) {
                j["branch"] = json!(b);
            }
            if let Some(commit) = meta.get("commit").and_then(Value::as_str) {
                j["commit"] = json!(commit);
            }
            if let Some(prompt) = meta.get("prompt").and_then(Value::as_str) {
                j["prompt"] = json!(prompt);
            }
        }
    }

    j
}

fn edge_summary_json(edge: &DecisionEdge) -> Value {
    let mut j = json!({
        "id": edge.id,
        "from_node_id": edge.from_node_id,
        "to_node_id": edge.to_node_id,
        "edge_type": edge.edge_type,
    });
    if let Some(ref r) = edge.rationale {
        j["rationale"] = json!(r);
    }
    j
}

fn node_has_branch(node: &DecisionNode, branch: &str) -> bool {
    node.metadata_json
        .as_ref()
        .and_then(|m| serde_json::from_str::<Value>(m).ok())
        .and_then(|v| v.get("branch").and_then(Value::as_str).map(|b| b == branch))
        .unwrap_or(false)
}

fn build_adjacency_out(edges: &[DecisionEdge]) -> HashMap<i32, Vec<i32>> {
    let mut map: HashMap<i32, Vec<i32>> = HashMap::new();
    for e in edges {
        map.entry(e.from_node_id).or_default().push(e.to_node_id);
    }
    map
}

fn build_adjacency_in(edges: &[DecisionEdge]) -> HashMap<i32, Vec<i32>> {
    let mut map: HashMap<i32, Vec<i32>> = HashMap::new();
    for e in edges {
        map.entry(e.to_node_id).or_default().push(e.from_node_id);
    }
    map
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{DecisionEdge, DecisionGraph, DecisionNode};

    fn make_node(id: i32, node_type: &str, title: &str, branch: Option<&str>) -> DecisionNode {
        let metadata = branch.map(|b| format!(r#"{{"branch":"{}"}}"#, b));
        DecisionNode {
            id,
            change_id: format!("uuid-{id}"),
            node_type: node_type.to_string(),
            title: title.to_string(),
            description: None,
            status: "pending".to_string(),
            created_at: format!("2024-01-{:02}T00:00:00Z", id.min(28)),
            updated_at: format!("2024-01-{:02}T00:00:00Z", id.min(28)),
            metadata_json: metadata,
        }
    }

    fn make_edge(id: i32, from: i32, to: i32, edge_type: &str) -> DecisionEdge {
        DecisionEdge {
            id,
            from_node_id: from,
            to_node_id: to,
            from_change_id: None,
            to_change_id: None,
            edge_type: edge_type.to_string(),
            weight: Some(1.0),
            rationale: Some("test".to_string()),
            created_at: "2024-01-01T00:00:00Z".to_string(),
        }
    }

    fn sample_graph() -> DecisionGraph {
        // goal(1) -> option(2) -> decision(3) -> action(4) -> outcome(5)
        //                      -> option(6) [rejected]
        DecisionGraph {
            nodes: vec![
                make_node(1, "goal", "Build auth", Some("main")),
                make_node(2, "option", "Use JWT", Some("main")),
                make_node(3, "decision", "Choose JWT", Some("main")),
                make_node(4, "action", "Implement JWT", Some("main")),
                make_node(5, "outcome", "JWT works", Some("main")),
                make_node(6, "option", "Use sessions", Some("main")),
            ],
            edges: vec![
                make_edge(1, 1, 2, "leads_to"),
                make_edge(2, 1, 6, "leads_to"),
                make_edge(3, 2, 3, "leads_to"),
                make_edge(4, 3, 4, "leads_to"),
                make_edge(5, 4, 5, "leads_to"),
                make_edge(6, 6, 3, "rejected"),
            ],
            config: None,
            themes: vec![],
            node_themes: vec![],
            documents: vec![],
        }
    }

    // -- trace_chain tests --

    #[test]
    fn test_trace_chain_both_directions() {
        let graph = sample_graph();
        let result = trace_chain(&graph, 3, 0, &TraceDirection::Both);
        // Should find all 6 nodes (fully connected)
        assert_eq!(result.nodes.len(), 6);
        assert_eq!(result.start_node_id, 3);
    }

    #[test]
    fn test_trace_chain_outgoing_only() {
        let graph = sample_graph();
        let result = trace_chain(&graph, 3, 0, &TraceDirection::Outgoing);
        // From decision(3): action(4) -> outcome(5)
        assert_eq!(result.nodes.len(), 3); // 3, 4, 5
    }

    #[test]
    fn test_trace_chain_incoming_only() {
        let graph = sample_graph();
        let result = trace_chain(&graph, 5, 0, &TraceDirection::Incoming);
        // outcome(5) <- action(4) <- decision(3) <- option(2) <- goal(1)
        // Also: decision(3) <- option(6)
        assert_eq!(result.nodes.len(), 6);
    }

    #[test]
    fn test_trace_chain_with_max_depth() {
        let graph = sample_graph();
        let result = trace_chain(&graph, 1, 1, &TraceDirection::Outgoing);
        // depth 0: goal(1), depth 1: option(2), option(6) — stop
        assert_eq!(result.nodes.len(), 3);
    }

    #[test]
    fn test_trace_chain_nonexistent_start() {
        let graph = sample_graph();
        let result = trace_chain(&graph, 999, 0, &TraceDirection::Both);
        assert!(result.nodes.is_empty());
        assert_eq!(result.depth_map.len(), 1); // just the start ID
    }

    #[test]
    fn test_trace_chain_depth_map() {
        let graph = sample_graph();
        let result = trace_chain(&graph, 1, 0, &TraceDirection::Outgoing);
        assert_eq!(result.depth_map[&1], 0);
        assert_eq!(result.depth_map[&2], 1);
        assert_eq!(result.depth_map[&6], 1);
    }

    // -- node context tests --

    #[test]
    fn test_node_context_decision() {
        let graph = sample_graph();
        let ctx = get_node_context(&graph, 3);
        assert!(ctx.node.is_some());
        assert_eq!(ctx.node.as_ref().unwrap().title, "Choose JWT");
        // Parents: option(2), option(6)
        assert_eq!(ctx.parents.len(), 2);
        // Children: action(4)
        assert_eq!(ctx.children.len(), 1);
        assert_eq!(ctx.children[0].title, "Implement JWT");
    }

    #[test]
    fn test_node_context_siblings() {
        let graph = sample_graph();
        let ctx = get_node_context(&graph, 2);
        // option(2) has parent goal(1). goal(1) also connects to option(6).
        // So option(6) is a sibling of option(2).
        assert_eq!(ctx.siblings.len(), 1);
        assert_eq!(ctx.siblings[0].title, "Use sessions");
    }

    #[test]
    fn test_node_context_nonexistent() {
        let graph = sample_graph();
        let ctx = get_node_context(&graph, 999);
        assert!(ctx.node.is_none());
        assert!(ctx.parents.is_empty());
        assert!(ctx.children.is_empty());
    }

    // -- timeline tests --

    #[test]
    fn test_timeline_all() {
        let graph = sample_graph();
        let timeline = get_timeline(&graph, 0, None, None, None);
        assert_eq!(timeline.len(), 6);
        // Newest first
        assert!(timeline[0].created_at >= timeline[1].created_at);
    }

    #[test]
    fn test_timeline_with_limit() {
        let graph = sample_graph();
        let timeline = get_timeline(&graph, 3, None, None, None);
        assert_eq!(timeline.len(), 3);
    }

    #[test]
    fn test_timeline_type_filter() {
        let graph = sample_graph();
        let timeline = get_timeline(&graph, 0, Some("option"), None, None);
        assert_eq!(timeline.len(), 2);
    }

    #[test]
    fn test_timeline_branch_filter() {
        let graph = sample_graph();
        let timeline = get_timeline(&graph, 0, None, Some("main"), None);
        assert_eq!(timeline.len(), 6);
        let timeline = get_timeline(&graph, 0, None, Some("feature-x"), None);
        assert_eq!(timeline.len(), 0);
    }

    // -- pulse tests --

    #[test]
    fn test_pulse_basic() {
        let graph = sample_graph();
        let report = get_pulse(&graph, None, 3);
        assert_eq!(report.total_nodes, 6);
        assert_eq!(report.total_edges, 6);
        assert_eq!(report.nodes_by_type["goal"], 1);
        assert_eq!(report.nodes_by_type["option"], 2);
        assert_eq!(report.active_goals.len(), 1);
        assert_eq!(report.recent_nodes.len(), 3);
    }

    #[test]
    fn test_pulse_branch_filter() {
        let graph = sample_graph();
        let report = get_pulse(&graph, Some("nonexistent"), 10);
        assert_eq!(report.total_nodes, 0);
    }

    // -- orphan tests --

    #[test]
    fn test_find_orphans_none() {
        let graph = sample_graph();
        let orphans = find_orphans(&graph);
        // All non-goal nodes have incoming edges, so no orphans
        assert_eq!(orphans.len(), 0);
    }

    #[test]
    fn test_find_orphans_disconnected_action() {
        let mut graph = sample_graph();
        graph
            .nodes
            .push(make_node(7, "action", "Dangling action", Some("main")));

        let orphans = find_orphans(&graph);
        assert_eq!(orphans.len(), 1);
        assert_eq!(orphans[0].node.id, 7);
        assert!(orphans[0].reason.contains("Action"));
    }

    #[test]
    fn test_find_orphans_goal_is_not_orphan() {
        let graph = DecisionGraph {
            nodes: vec![make_node(1, "goal", "Standalone goal", None)],
            edges: vec![],
            config: None,
            themes: vec![],
            node_themes: vec![],
            documents: vec![],
        };

        let orphans = find_orphans(&graph);
        assert_eq!(orphans.len(), 0);
    }

    // -- branch summary tests --

    #[test]
    fn test_branch_summary() {
        let graph = sample_graph();
        let summary = get_branch_summary(&graph, "main");
        assert_eq!(summary.total_nodes, 6);
        assert_eq!(summary.goals.len(), 1);
        assert_eq!(summary.decisions.len(), 1);
        assert_eq!(summary.actions.len(), 1);
        assert_eq!(summary.outcomes.len(), 1);
    }

    #[test]
    fn test_branch_summary_empty() {
        let graph = sample_graph();
        let summary = get_branch_summary(&graph, "nonexistent");
        assert_eq!(summary.total_nodes, 0);
    }

    // -- JSON serialization tests --

    #[test]
    fn test_trace_result_to_json() {
        let graph = sample_graph();
        let result = trace_chain(&graph, 1, 0, &TraceDirection::Outgoing);
        let json = trace_result_to_json(&result);
        assert_eq!(json["start_node_id"], 1);
        assert!(json["node_count"].as_u64().unwrap() > 0);
    }

    #[test]
    fn test_node_context_to_json() {
        let graph = sample_graph();
        let ctx = get_node_context(&graph, 3);
        let json = node_context_to_json(&ctx);
        assert!(json["node"].is_object());
        assert!(json["parents"].is_array());
        assert!(json["children"].is_array());
    }

    #[test]
    fn test_pulse_report_to_json() {
        let graph = sample_graph();
        let report = get_pulse(&graph, None, 3);
        let json = pulse_report_to_json(&report);
        assert_eq!(json["total_nodes"], 6);
        assert!(json["nodes_by_type"].is_object());
    }

    #[test]
    fn test_branch_summary_to_json() {
        let graph = sample_graph();
        let summary = get_branch_summary(&graph, "main");
        let json = branch_summary_to_json(&summary);
        assert_eq!(json["branch"], "main");
        assert!(json["goals"].is_array());
    }

    #[test]
    fn test_orphans_to_json() {
        let mut graph = sample_graph();
        graph
            .nodes
            .push(make_node(7, "outcome", "Lost outcome", None));
        let orphans = find_orphans(&graph);
        let json = orphans_to_json(&orphans);
        assert!(json["count"].as_u64().unwrap() > 0);
    }

    #[test]
    fn test_timeline_to_json() {
        let graph = sample_graph();
        let tl = get_timeline(&graph, 2, None, None, None);
        let json = timeline_to_json(&tl);
        assert_eq!(json["count"], 2);
    }

    // -- helper tests --

    #[test]
    fn test_node_has_branch() {
        let node = make_node(1, "goal", "Test", Some("feature-x"));
        assert!(node_has_branch(&node, "feature-x"));
        assert!(!node_has_branch(&node, "main"));
    }

    #[test]
    fn test_node_has_branch_no_metadata() {
        let node = make_node(1, "goal", "Test", None);
        assert!(!node_has_branch(&node, "main"));
    }

    #[test]
    fn test_trace_direction_parse() {
        assert_eq!(TraceDirection::parse("outgoing"), TraceDirection::Outgoing);
        assert_eq!(TraceDirection::parse("incoming"), TraceDirection::Incoming);
        assert_eq!(TraceDirection::parse("both"), TraceDirection::Both);
        assert_eq!(TraceDirection::parse("unknown"), TraceDirection::Both);
    }

    #[test]
    fn test_node_summary_json_unpacks_metadata() {
        let mut node = make_node(1, "goal", "Test", Some("main"));
        node.metadata_json =
            Some(r#"{"confidence":85,"branch":"main","commit":"abc123"}"#.to_string());
        let j = node_summary_json(&node);
        assert_eq!(j["confidence"], 85);
        assert_eq!(j["branch"], "main");
        assert_eq!(j["commit"], "abc123");
    }
}
