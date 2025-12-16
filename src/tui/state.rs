//! Pure state transformations for the TUI (Functional Core)
//!
//! This module contains ONLY pure functions with no I/O.
//! All functions here:
//! - Take immutable inputs
//! - Return new values (no mutation)
//! - Have no side effects
//! - Are easy to test in isolation
//!
//! The "imperative shell" (app.rs, events.rs) handles I/O and calls these pure functions.

use crate::{DecisionEdge, DecisionNode};
use std::collections::{HashSet, VecDeque};

// =============================================================================
// Filter Functions - Pure transformations on node lists
// =============================================================================

/// Filter nodes by type
pub fn filter_by_type(nodes: &[DecisionNode], type_filter: Option<&str>) -> Vec<DecisionNode> {
    match type_filter {
        Some(t) => nodes.iter().filter(|n| n.node_type == t).cloned().collect(),
        None => nodes.to_vec(),
    }
}

/// Filter nodes by branch (extracted from metadata_json)
pub fn filter_by_branch(nodes: &[DecisionNode], branch: Option<&str>) -> Vec<DecisionNode> {
    match branch {
        Some(b) => nodes
            .iter()
            .filter(|n| {
                super::types::get_branch(n)
                    .map(|node_branch| node_branch == b)
                    .unwrap_or(false)
            })
            .cloned()
            .collect(),
        None => nodes.to_vec(),
    }
}

/// Filter nodes by search query (searches title and description)
pub fn filter_by_search(nodes: &[DecisionNode], query: &str) -> Vec<DecisionNode> {
    if query.is_empty() {
        return nodes.to_vec();
    }
    let query_lower = query.to_lowercase();
    nodes
        .iter()
        .filter(|n| {
            n.title.to_lowercase().contains(&query_lower)
                || n.description
                    .as_ref()
                    .map(|d| d.to_lowercase().contains(&query_lower))
                    .unwrap_or(false)
        })
        .cloned()
        .collect()
}

/// Sort nodes by created_at timestamp
/// If `reverse` is true, sorts oldest first (chronological)
/// If `reverse` is false, sorts newest first (reverse-chronological)
pub fn sort_by_time(nodes: &[DecisionNode], reverse: bool) -> Vec<DecisionNode> {
    let mut sorted = nodes.to_vec();
    if reverse {
        sorted.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    } else {
        sorted.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    }
    sorted
}

/// Apply all filters and sorting in one pass
pub fn apply_all_filters(
    nodes: &[DecisionNode],
    type_filter: Option<&str>,
    branch_filter: Option<&str>,
    search_query: &str,
    reverse_order: bool,
) -> Vec<DecisionNode> {
    let filtered = filter_by_type(nodes, type_filter);
    let filtered = filter_by_branch(&filtered, branch_filter);
    let filtered = filter_by_search(&filtered, search_query);
    sort_by_time(&filtered, reverse_order)
}

// =============================================================================
// Navigation - Pure index calculations
// =============================================================================

/// Calculate new selected index after moving up
pub fn move_selection_up(current: usize) -> usize {
    current.saturating_sub(1)
}

/// Calculate new selected index after moving down
pub fn move_selection_down(current: usize, max: usize) -> usize {
    if max == 0 {
        0
    } else {
        (current + 1).min(max - 1)
    }
}

/// Calculate new selected index after page down
pub fn page_down(current: usize, page_size: usize, max: usize) -> usize {
    if max == 0 {
        0
    } else {
        (current + page_size).min(max - 1)
    }
}

/// Calculate new selected index after page up
pub fn page_up(current: usize, page_size: usize) -> usize {
    current.saturating_sub(page_size)
}

/// Calculate scroll offset to keep selection visible
pub fn calculate_scroll_offset(
    selected: usize,
    current_offset: usize,
    visible_items: usize,
) -> usize {
    if visible_items == 0 {
        return 0;
    }
    if selected < current_offset {
        selected
    } else if selected >= current_offset + visible_items {
        selected.saturating_sub(visible_items - 1)
    } else {
        current_offset
    }
}

/// Clamp selection index to valid range
pub fn clamp_selection(selected: usize, max: usize) -> usize {
    if max == 0 {
        0
    } else {
        selected.min(max - 1)
    }
}

// =============================================================================
// Chain Types and Recency - For DAG view filtering
// =============================================================================

/// A chain is a goal node with all its descendants
#[derive(Debug, Clone)]
pub struct Chain {
    /// The root goal node
    pub root: DecisionNode,
    /// All nodes in this chain (including root)
    pub nodes: Vec<DecisionNode>,
}

impl Chain {
    /// Get the most recent update time across all nodes in the chain
    pub fn last_updated(&self) -> chrono::DateTime<chrono::Utc> {
        self.nodes
            .iter()
            .filter_map(|n| chrono::DateTime::parse_from_rfc3339(&n.updated_at).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .max()
            .unwrap_or_else(chrono::Utc::now)
    }

    /// Get the most recent update timestamp as milliseconds (for sorting)
    pub fn last_updated_millis(&self) -> i64 {
        self.last_updated().timestamp_millis()
    }
}

/// Build chains from graph data (goal roots with all descendants)
pub fn build_chains(nodes: &[DecisionNode], edges: &[DecisionEdge]) -> Vec<Chain> {
    // Find all goal nodes (these are chain roots)
    let goal_nodes: Vec<_> = nodes.iter().filter(|n| n.node_type == "goal").collect();

    goal_nodes
        .into_iter()
        .map(|root| {
            let descendants = get_descendants(root.id, nodes, edges);
            let chain_nodes: Vec<DecisionNode> = descendants
                .iter()
                .filter_map(|(id, _depth)| nodes.iter().find(|n| n.id == *id))
                .cloned()
                .collect();

            Chain {
                root: root.clone(),
                nodes: chain_nodes,
            }
        })
        .collect()
}

/// Sort chains by recency (most recently updated first)
pub fn sort_chains_by_recency(chains: &[Chain]) -> Vec<Chain> {
    let mut sorted = chains.to_vec();
    sorted.sort_by_key(|c| std::cmp::Reverse(c.last_updated_millis()));
    sorted
}

/// Get the N most recent chains
pub fn get_recent_chains(chains: &[Chain], count: usize) -> Vec<Chain> {
    let sorted = sort_chains_by_recency(chains);
    sorted.into_iter().take(count).collect()
}

/// Filter nodes to only those in the given chains
pub fn filter_nodes_by_chains(chains: &[Chain]) -> HashSet<i32> {
    let mut node_ids = HashSet::new();
    for chain in chains {
        for node in &chain.nodes {
            node_ids.insert(node.id);
        }
    }
    node_ids
}

// =============================================================================
// Graph Traversal - Pure graph algorithms
// =============================================================================

/// Find root goal by traversing incoming edges
/// Returns None if cycle detected or no goal found
pub fn find_root_goal(
    start_id: i32,
    nodes: &[DecisionNode],
    edges: &[DecisionEdge],
) -> Option<i32> {
    let mut visited = HashSet::new();
    let mut current = start_id;

    loop {
        if visited.contains(&current) {
            return None; // Cycle detected
        }
        visited.insert(current);

        // Check if current node is a goal
        if let Some(node) = nodes.iter().find(|n| n.id == current) {
            if node.node_type == "goal" {
                return Some(current);
            }
        }

        // Find incoming edges to traverse up
        let incoming: Vec<_> = edges.iter().filter(|e| e.to_node_id == current).collect();

        if incoming.is_empty() {
            return None; // No parent
        }

        // Take the first incoming edge
        current = incoming[0].from_node_id;
    }
}

/// Get all descendant nodes from a root (BFS traversal)
/// Returns Vec of (node_id, depth)
pub fn get_descendants(
    root_id: i32,
    nodes: &[DecisionNode],
    edges: &[DecisionEdge],
) -> Vec<(i32, usize)> {
    let mut result = Vec::new();
    let mut queue = VecDeque::new();
    let mut visited = HashSet::new();

    queue.push_back((root_id, 0usize));

    while let Some((node_id, depth)) = queue.pop_front() {
        if visited.contains(&node_id) {
            continue;
        }
        visited.insert(node_id);

        // Only include if node exists
        if nodes.iter().any(|n| n.id == node_id) {
            result.push((node_id, depth));

            // Add all children (outgoing edges)
            for edge in edges {
                if edge.from_node_id == node_id && !visited.contains(&edge.to_node_id) {
                    queue.push_back((edge.to_node_id, depth + 1));
                }
            }
        }
    }

    result
}

/// Get unique branches from nodes
pub fn get_unique_branches(nodes: &[DecisionNode]) -> Vec<String> {
    let mut branches: Vec<String> = nodes.iter().filter_map(super::types::get_branch).collect();
    branches.sort();
    branches.dedup();
    branches
}

/// Filter branch search matches
pub fn filter_branch_matches(branches: &[String], query: &str) -> Vec<String> {
    if query.is_empty() {
        return branches.to_vec();
    }
    let query_lower = query.to_lowercase();
    branches
        .iter()
        .filter(|b| b.to_lowercase().contains(&query_lower))
        .cloned()
        .collect()
}

/// Cycle through type filters
pub fn cycle_type_filter(current: Option<&str>) -> Option<String> {
    const TYPES: &[&str] = &[
        "goal",
        "decision",
        "option",
        "action",
        "outcome",
        "observation",
    ];
    match current {
        None => Some(TYPES[0].to_string()),
        Some(c) => {
            let idx = TYPES.iter().position(|t| *t == c);
            match idx {
                Some(i) if i + 1 < TYPES.len() => Some(TYPES[i + 1].to_string()),
                _ => None,
            }
        }
    }
}

/// Cycle through branch filters
pub fn cycle_branch_filter(current: Option<&str>, branches: &[String]) -> Option<String> {
    if branches.is_empty() {
        return None;
    }
    match current {
        None => Some(branches[0].clone()),
        Some(c) => {
            let idx = branches.iter().position(|b| b == c);
            match idx {
                Some(i) if i + 1 < branches.len() => Some(branches[i + 1].clone()),
                _ => None,
            }
        }
    }
}

// =============================================================================
// Modal Scroll Calculations
// =============================================================================

/// Calculate scroll offset for modal (clamped to valid range)
pub fn scroll_modal(current: usize, delta: isize, total_lines: usize, visible: usize) -> usize {
    let max_scroll = total_lines.saturating_sub(visible);
    let new_offset = if delta >= 0 {
        current.saturating_add(delta as usize)
    } else {
        current.saturating_sub((-delta) as usize)
    };
    // Always clamp to valid range
    new_offset.min(max_scroll)
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_node(id: i32, node_type: &str, title: &str, metadata: Option<&str>) -> DecisionNode {
        DecisionNode {
            id,
            change_id: format!("change-{}", id),
            node_type: node_type.to_string(),
            title: title.to_string(),
            description: None,
            status: "pending".to_string(),
            created_at: format!("2024-12-10T12:00:0{}Z", id),
            updated_at: format!("2024-12-10T12:00:0{}Z", id),
            metadata_json: metadata.map(|s| s.to_string()),
        }
    }

    fn make_edge(id: i32, from: i32, to: i32) -> DecisionEdge {
        DecisionEdge {
            id,
            from_node_id: from,
            to_node_id: to,
            from_change_id: None,
            to_change_id: None,
            edge_type: "leads_to".to_string(),
            weight: Some(1.0),
            rationale: None,
            created_at: "2024-12-10T12:00:00Z".to_string(),
        }
    }

    // --- Filter Tests ---

    #[test]
    fn test_filter_by_type() {
        let nodes = vec![
            make_node(1, "goal", "Goal 1", None),
            make_node(2, "action", "Action 1", None),
            make_node(3, "goal", "Goal 2", None),
        ];

        let filtered = filter_by_type(&nodes, Some("goal"));
        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().all(|n| n.node_type == "goal"));

        let all = filter_by_type(&nodes, None);
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn test_filter_by_branch() {
        let nodes = vec![
            make_node(1, "goal", "G1", Some(r#"{"branch": "main"}"#)),
            make_node(2, "goal", "G2", Some(r#"{"branch": "feature"}"#)),
            make_node(3, "goal", "G3", None),
        ];

        let main_nodes = filter_by_branch(&nodes, Some("main"));
        assert_eq!(main_nodes.len(), 1);
        assert_eq!(main_nodes[0].id, 1);

        let all = filter_by_branch(&nodes, None);
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn test_filter_by_search() {
        let nodes = vec![
            make_node(1, "goal", "Add authentication", None),
            make_node(2, "action", "Fix bug", None),
            make_node(3, "goal", "Auth cleanup", None),
        ];

        let auth_nodes = filter_by_search(&nodes, "auth");
        assert_eq!(auth_nodes.len(), 2);

        let empty = filter_by_search(&nodes, "");
        assert_eq!(empty.len(), 3);

        let none = filter_by_search(&nodes, "xyz");
        assert_eq!(none.len(), 0);
    }

    #[test]
    fn test_sort_by_time() {
        let nodes = vec![
            make_node(3, "goal", "Third", None),
            make_node(1, "goal", "First", None),
            make_node(2, "goal", "Second", None),
        ];

        let oldest_first = sort_by_time(&nodes, true);
        assert_eq!(oldest_first[0].id, 1);
        assert_eq!(oldest_first[2].id, 3);

        let newest_first = sort_by_time(&nodes, false);
        assert_eq!(newest_first[0].id, 3);
        assert_eq!(newest_first[2].id, 1);
    }

    // --- Navigation Tests ---

    #[test]
    fn test_move_selection() {
        assert_eq!(move_selection_up(5), 4);
        assert_eq!(move_selection_up(0), 0);

        assert_eq!(move_selection_down(5, 10), 6);
        assert_eq!(move_selection_down(9, 10), 9);
        assert_eq!(move_selection_down(0, 0), 0);
    }

    #[test]
    fn test_page_navigation() {
        assert_eq!(page_down(0, 10, 100), 10);
        assert_eq!(page_down(95, 10, 100), 99);

        assert_eq!(page_up(15, 10), 5);
        assert_eq!(page_up(5, 10), 0);
    }

    #[test]
    fn test_calculate_scroll_offset() {
        // Selection visible - no change
        assert_eq!(calculate_scroll_offset(5, 0, 10), 0);

        // Selection above viewport - scroll up
        assert_eq!(calculate_scroll_offset(2, 5, 10), 2);

        // Selection below viewport - scroll down
        assert_eq!(calculate_scroll_offset(15, 0, 10), 6);
    }

    #[test]
    fn test_clamp_selection() {
        assert_eq!(clamp_selection(5, 10), 5);
        assert_eq!(clamp_selection(15, 10), 9);
        assert_eq!(clamp_selection(5, 0), 0);
    }

    // --- Graph Traversal Tests ---

    #[test]
    fn test_find_root_goal() {
        let nodes = vec![
            make_node(1, "goal", "Root Goal", None),
            make_node(2, "decision", "Decision", None),
            make_node(3, "action", "Action", None),
        ];
        let edges = vec![make_edge(1, 1, 2), make_edge(2, 2, 3)];

        // From action, should find root goal
        assert_eq!(find_root_goal(3, &nodes, &edges), Some(1));

        // From goal, should return itself
        assert_eq!(find_root_goal(1, &nodes, &edges), Some(1));

        // Orphan node
        let orphan = make_node(99, "action", "Orphan", None);
        let nodes_with_orphan = vec![nodes[0].clone(), orphan];
        assert_eq!(find_root_goal(99, &nodes_with_orphan, &edges), None);
    }

    #[test]
    fn test_get_descendants() {
        let nodes = vec![
            make_node(1, "goal", "Goal", None),
            make_node(2, "decision", "Decision", None),
            make_node(3, "action", "Action", None),
        ];
        let edges = vec![make_edge(1, 1, 2), make_edge(2, 2, 3)];

        let descendants = get_descendants(1, &nodes, &edges);
        assert_eq!(descendants.len(), 3);
        assert!(descendants
            .iter()
            .any(|(id, depth)| *id == 1 && *depth == 0));
        assert!(descendants
            .iter()
            .any(|(id, depth)| *id == 2 && *depth == 1));
        assert!(descendants
            .iter()
            .any(|(id, depth)| *id == 3 && *depth == 2));
    }

    // --- Chain Recency Tests ---

    fn make_node_with_updated(
        id: i32,
        node_type: &str,
        title: &str,
        updated_at: &str,
    ) -> DecisionNode {
        DecisionNode {
            id,
            change_id: format!("change-{}", id),
            node_type: node_type.to_string(),
            title: title.to_string(),
            description: None,
            status: "pending".to_string(),
            created_at: "2024-12-10T12:00:00Z".to_string(),
            updated_at: updated_at.to_string(),
            metadata_json: None,
        }
    }

    #[test]
    fn test_build_chains() {
        let nodes = vec![
            make_node(1, "goal", "Goal 1", None),
            make_node(2, "decision", "Decision", None),
            make_node(3, "goal", "Goal 2", None),
        ];
        let edges = vec![make_edge(1, 1, 2)];

        let chains = build_chains(&nodes, &edges);
        assert_eq!(chains.len(), 2); // Two goals = two chains
        assert!(chains.iter().any(|c| c.root.id == 1));
        assert!(chains.iter().any(|c| c.root.id == 3));

        // Chain 1 should have 2 nodes (goal + decision)
        let chain1 = chains.iter().find(|c| c.root.id == 1).unwrap();
        assert_eq!(chain1.nodes.len(), 2);

        // Chain 2 should have 1 node (just the goal)
        let chain2 = chains.iter().find(|c| c.root.id == 3).unwrap();
        assert_eq!(chain2.nodes.len(), 1);
    }

    #[test]
    fn test_chain_last_updated() {
        let nodes = vec![
            make_node_with_updated(1, "goal", "Goal", "2024-12-10T12:00:00Z"),
            make_node_with_updated(2, "action", "Action", "2024-12-12T14:00:00Z"), // More recent
        ];
        let edges = vec![make_edge(1, 1, 2)];

        let chains = build_chains(&nodes, &edges);
        let chain = &chains[0];

        // Chain's last_updated should be the most recent node
        let last = chain.last_updated();
        assert!(last.to_rfc3339().contains("2024-12-12"));
    }

    #[test]
    fn test_sort_chains_by_recency() {
        let nodes = vec![
            make_node_with_updated(1, "goal", "Old Goal", "2024-12-01T12:00:00Z"),
            make_node_with_updated(2, "goal", "New Goal", "2024-12-15T12:00:00Z"),
            make_node_with_updated(3, "goal", "Mid Goal", "2024-12-10T12:00:00Z"),
        ];
        let edges: Vec<DecisionEdge> = vec![];

        let chains = build_chains(&nodes, &edges);
        let sorted = sort_chains_by_recency(&chains);

        // Most recent first
        assert_eq!(sorted[0].root.id, 2); // New Goal
        assert_eq!(sorted[1].root.id, 3); // Mid Goal
        assert_eq!(sorted[2].root.id, 1); // Old Goal
    }

    #[test]
    fn test_get_recent_chains() {
        let nodes = vec![
            make_node_with_updated(1, "goal", "G1", "2024-12-01T12:00:00Z"),
            make_node_with_updated(2, "goal", "G2", "2024-12-15T12:00:00Z"),
            make_node_with_updated(3, "goal", "G3", "2024-12-10T12:00:00Z"),
            make_node_with_updated(4, "goal", "G4", "2024-12-12T12:00:00Z"),
        ];
        let edges: Vec<DecisionEdge> = vec![];

        let chains = build_chains(&nodes, &edges);
        let recent = get_recent_chains(&chains, 2);

        assert_eq!(recent.len(), 2);
        // Should be the two most recent
        assert!(recent.iter().any(|c| c.root.id == 2));
        assert!(recent.iter().any(|c| c.root.id == 4));
    }

    #[test]
    fn test_filter_nodes_by_chains() {
        let nodes = vec![
            make_node(1, "goal", "Goal", None),
            make_node(2, "decision", "Dec", None),
            make_node(3, "action", "Act", None),
        ];
        let edges = vec![make_edge(1, 1, 2), make_edge(2, 2, 3)];

        let chains = build_chains(&nodes, &edges);
        let visible = filter_nodes_by_chains(&chains);

        assert_eq!(visible.len(), 3);
        assert!(visible.contains(&1));
        assert!(visible.contains(&2));
        assert!(visible.contains(&3));
    }

    // --- Branch/Type Cycling Tests ---

    #[test]
    fn test_cycle_type_filter() {
        assert_eq!(cycle_type_filter(None), Some("goal".to_string()));
        assert_eq!(
            cycle_type_filter(Some("goal")),
            Some("decision".to_string())
        );
        assert_eq!(cycle_type_filter(Some("observation")), None);
    }

    #[test]
    fn test_cycle_branch_filter() {
        let branches = vec!["main".to_string(), "feature".to_string()];

        assert_eq!(
            cycle_branch_filter(None, &branches),
            Some("main".to_string())
        );
        assert_eq!(
            cycle_branch_filter(Some("main"), &branches),
            Some("feature".to_string())
        );
        assert_eq!(cycle_branch_filter(Some("feature"), &branches), None);
        assert_eq!(cycle_branch_filter(None, &[]), None);
    }

    #[test]
    fn test_filter_branch_matches() {
        let branches = vec![
            "main".to_string(),
            "feature-auth".to_string(),
            "feature-ui".to_string(),
        ];

        let matches = filter_branch_matches(&branches, "feature");
        assert_eq!(matches.len(), 2);

        let all = filter_branch_matches(&branches, "");
        assert_eq!(all.len(), 3);
    }

    // --- Modal Scroll Tests ---

    #[test]
    fn test_scroll_modal() {
        // Scroll down
        assert_eq!(scroll_modal(0, 5, 100, 20), 5);
        // Scroll up
        assert_eq!(scroll_modal(10, -5, 100, 20), 5);
        // Clamp to max
        assert_eq!(scroll_modal(75, 10, 100, 20), 80);
        // Clamp to 0
        assert_eq!(scroll_modal(3, -10, 100, 20), 0);
    }
}

// =============================================================================
// Property-Based Tests (Proptest)
// =============================================================================

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    // Strategy for selection index and max value
    fn index_and_max() -> impl Strategy<Value = (usize, usize)> {
        (0..=1000usize).prop_flat_map(|max| (0..=max, Just(max)))
    }

    proptest! {
        // === Navigation Property Tests ===

        #[test]
        fn prop_move_up_never_underflows(current in 0..=1000usize) {
            let result = move_selection_up(current);
            // Result should always be <= current
            prop_assert!(result <= current);
            // Result should never be negative (implicit due to usize)
        }

        #[test]
        fn prop_move_down_never_overflows((current, max) in index_and_max()) {
            let result = move_selection_down(current, max);
            // Result should always be <= max
            prop_assert!(result <= max);
            // Result should always be >= current (or equal if at max)
            prop_assert!(result >= current || current >= max);
        }

        #[test]
        fn prop_clamp_selection_always_valid(current in 0..=1000usize, len in 0..=1000usize) {
            let result = clamp_selection(current, len);
            if len == 0 {
                prop_assert_eq!(result, 0);
            } else {
                prop_assert!(result < len, "result {} should be < len {}", result, len);
            }
        }

        #[test]
        fn prop_page_navigation_bounded(
            current in 0..=1000usize,
            visible in 1..=100usize,
            total in 0..=1000usize
        ) {
            let down = page_down(current, visible, total);
            let up = page_up(current, visible);

            // Page down should not exceed total - 1 (if total > 0)
            if total > 0 {
                prop_assert!(down < total, "page_down {} should be < total {}", down, total);
            }

            // Page up should always be <= current
            prop_assert!(up <= current, "page_up {} should be <= current {}", up, current);
        }

        // === Modal Scroll Property Tests ===

        #[test]
        fn prop_scroll_modal_bounded(
            current in 0..=1000usize,
            delta in -100..=100isize,
            total_lines in 1..=1000usize,
            viewport_height in 1..=100usize
        ) {
            let result = scroll_modal(current, delta, total_lines, viewport_height);

            // Result should never be negative (implicit due to usize)
            // Result should not exceed max_scroll
            let max_scroll = total_lines.saturating_sub(viewport_height);
            prop_assert!(result <= max_scroll,
                "scroll result {} should be <= max_scroll {}", result, max_scroll);
        }

        // === Filter Property Tests ===

        #[test]
        fn prop_filter_by_type_subset(node_count in 0..=50usize) {
            // Create test nodes
            let nodes: Vec<DecisionNode> = (0..node_count)
                .map(|i| DecisionNode {
                    id: i as i32,
                    node_type: if i % 3 == 0 { "goal" } else { "action" }.to_string(),
                    title: format!("Node {}", i),
                    description: None,
                    status: "pending".to_string(),
                    created_at: "2024-01-01".to_string(),
                    updated_at: "2024-01-01".to_string(),
                    metadata_json: None,
                    change_id: format!("change-{}", i),
                })
                .collect();

            let filtered = filter_by_type(&nodes, Some("goal"));

            // Filtered should be a subset
            prop_assert!(filtered.len() <= nodes.len());

            // All filtered nodes should match the type
            for node in &filtered {
                prop_assert_eq!(&node.node_type, "goal");
            }
        }

        #[test]
        fn prop_filter_none_returns_all(node_count in 0..=50usize) {
            let nodes: Vec<DecisionNode> = (0..node_count)
                .map(|i| DecisionNode {
                    id: i as i32,
                    node_type: "action".to_string(),
                    title: format!("Node {}", i),
                    description: None,
                    status: "pending".to_string(),
                    created_at: "2024-01-01".to_string(),
                    updated_at: "2024-01-01".to_string(),
                    metadata_json: None,
                    change_id: format!("change-{}", i),
                })
                .collect();

            let filtered = filter_by_type(&nodes, None);
            prop_assert_eq!(filtered.len(), nodes.len());
        }

        #[test]
        fn prop_sort_preserves_length(node_count in 0..=50usize, reverse in proptest::bool::ANY) {
            let nodes: Vec<DecisionNode> = (0..node_count)
                .map(|i| DecisionNode {
                    id: i as i32,
                    node_type: "action".to_string(),
                    title: format!("Node {}", i),
                    description: None,
                    status: "pending".to_string(),
                    created_at: format!("2024-01-{:02}", (i % 28) + 1),
                    updated_at: "2024-01-01".to_string(),
                    metadata_json: None,
                    change_id: format!("change-{}", i),
                })
                .collect();

            let sorted = sort_by_time(&nodes, reverse);
            prop_assert_eq!(sorted.len(), nodes.len());
        }

        // === Type Cycling Property Tests ===

        #[test]
        fn prop_cycle_type_filter_cycles(iterations in 1..=20usize) {
            let mut current: Option<String> = None;
            let mut seen_none = false;

            for _ in 0..iterations {
                current = cycle_type_filter(current.as_deref());
                if current.is_none() {
                    seen_none = true;
                }
            }

            // After enough iterations, we should cycle back to None
            prop_assert!(seen_none || iterations < 7,
                "Should cycle through all types and back to None");
        }
    }
}
