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

use crate::{DecisionNode, DecisionEdge};
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
    let mut branches: Vec<String> = nodes
        .iter()
        .filter_map(super::types::get_branch)
        .collect();
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
    const TYPES: &[&str] = &["goal", "decision", "option", "action", "outcome", "observation"];
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
    if delta >= 0 {
        (current + delta as usize).min(max_scroll)
    } else {
        current.saturating_sub((-delta) as usize)
    }
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
        let edges = vec![
            make_edge(1, 1, 2),
            make_edge(2, 2, 3),
        ];

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
        let edges = vec![
            make_edge(1, 1, 2),
            make_edge(2, 2, 3),
        ];

        let descendants = get_descendants(1, &nodes, &edges);
        assert_eq!(descendants.len(), 3);
        assert!(descendants.iter().any(|(id, depth)| *id == 1 && *depth == 0));
        assert!(descendants.iter().any(|(id, depth)| *id == 2 && *depth == 1));
        assert!(descendants.iter().any(|(id, depth)| *id == 3 && *depth == 2));
    }

    // --- Branch/Type Cycling Tests ---

    #[test]
    fn test_cycle_type_filter() {
        assert_eq!(cycle_type_filter(None), Some("goal".to_string()));
        assert_eq!(cycle_type_filter(Some("goal")), Some("decision".to_string()));
        assert_eq!(cycle_type_filter(Some("observation")), None);
    }

    #[test]
    fn test_cycle_branch_filter() {
        let branches = vec!["main".to_string(), "feature".to_string()];

        assert_eq!(cycle_branch_filter(None, &branches), Some("main".to_string()));
        assert_eq!(cycle_branch_filter(Some("main"), &branches), Some("feature".to_string()));
        assert_eq!(cycle_branch_filter(Some("feature"), &branches), None);
        assert_eq!(cycle_branch_filter(None, &[]), None);
    }

    #[test]
    fn test_filter_branch_matches() {
        let branches = vec!["main".to_string(), "feature-auth".to_string(), "feature-ui".to_string()];

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
