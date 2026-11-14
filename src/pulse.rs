//! Pulse - current state health report for the decision graph
//!
//! Shows open goals, confidence distribution, orphan nodes,
//! recent activity, and coverage gaps.
//!
//! "Open" is `pending` or `active` ([`OPEN_STATUSES`]). A goal that is
//! completed, rejected, superseded or abandoned is not listed, and under an
//! open goal only the open descendants are: the closed ones are counted. The
//! obvious version, every goal with its whole tree, is a list of everything
//! ever done, which is the one thing this report is not for.

use crate::db::{Database, DecisionEdge, DecisionNode, OPEN_STATUSES};
use colored::Colorize;
use serde::Serialize;
use std::collections::{HashMap, HashSet};

/// Full pulse report
#[derive(Debug, Serialize)]
pub struct PulseReport {
    pub summary: PulseSummary,
    pub active_goals: Vec<GoalTree>,
    pub orphan_nodes: Vec<NodeRef>,
    pub recent_nodes: Vec<NodeRef>,
    pub coverage_gaps: Vec<CoverageGap>,
}

/// High-level summary statistics
#[derive(Debug, Serialize)]
pub struct PulseSummary {
    pub total_nodes: usize,
    pub total_edges: usize,
    pub by_type: HashMap<String, usize>,
    pub by_status: HashMap<String, usize>,
    pub confidence: ConfidenceDistribution,
}

/// Confidence level buckets
#[derive(Debug, Serialize)]
pub struct ConfidenceDistribution {
    pub high: usize,   // 80-100
    pub medium: usize, // 50-79
    pub low: usize,    // 1-49
    pub unset: usize,  // no confidence
}

/// An open goal with the descendants that are still open
#[derive(Debug, Serialize)]
pub struct GoalTree {
    pub goal: NodeRef,
    /// Open descendants, breadth first.
    pub children: Vec<NodeRef>,
    /// How many descendants are closed and so not listed.
    pub closed_children: usize,
    /// The newest `created_at` in the tree: goals are ordered by it.
    pub last_activity: String,
}

/// How much of the report [`print_pulse`] prints. The report itself is
/// never cut: `--json` carries every open goal.
#[derive(Debug, Clone, Copy)]
pub struct PulseLimits {
    pub goals: usize,
    pub children: usize,
    pub list: usize,
}

impl Default for PulseLimits {
    fn default() -> Self {
        Self {
            goals: 10,
            children: 8,
            list: 15,
        }
    }
}

impl PulseLimits {
    pub fn unlimited() -> Self {
        Self {
            goals: usize::MAX,
            children: usize::MAX,
            list: usize::MAX,
        }
    }
}

fn is_open(status: &str) -> bool {
    OPEN_STATUSES.contains(&status)
}

/// Lightweight node reference
#[derive(Debug, Serialize)]
pub struct NodeRef {
    pub id: i32,
    pub node_type: String,
    pub title: String,
    pub status: String,
    pub confidence: Option<u8>,
    pub created_at: String,
}

/// A gap in graph coverage
#[derive(Debug, Serialize)]
pub struct CoverageGap {
    pub node_id: i32,
    pub node_type: String,
    pub title: String,
    pub gap_type: String,
}

fn node_to_ref(node: &DecisionNode) -> NodeRef {
    let confidence = node
        .metadata_json
        .as_ref()
        .and_then(|m| serde_json::from_str::<serde_json::Value>(m).ok())
        .and_then(|v| v.get("confidence").and_then(|c| c.as_u64()))
        .map(|c| c.min(100) as u8);
    NodeRef {
        id: node.id,
        node_type: node.node_type.clone(),
        title: node.title.clone(),
        status: node.status.clone(),
        confidence,
        created_at: node.created_at.clone(),
    }
}

fn get_branch(node: &DecisionNode) -> Option<String> {
    node.metadata_json
        .as_ref()
        .and_then(|m| serde_json::from_str::<serde_json::Value>(m).ok())
        .and_then(|v| {
            v.get("branch")
                .and_then(|b| b.as_str())
                .map(|s| s.to_string())
        })
}

/// Generate the full pulse report
pub fn generate_pulse(
    db: &Database,
    branch: Option<&str>,
    recent_count: usize,
) -> Result<PulseReport, String> {
    let all_nodes = db.get_all_nodes().map_err(|e| e.to_string())?;
    let all_edges = db.get_all_edges().map_err(|e| e.to_string())?;

    // Apply branch filter
    let nodes: Vec<&DecisionNode> = if let Some(br) = branch {
        all_nodes
            .iter()
            .filter(|n| get_branch(n).as_deref() == Some(br))
            .collect()
    } else {
        all_nodes.iter().collect()
    };

    let node_ids: HashSet<i32> = nodes.iter().map(|n| n.id).collect();

    // Filter edges to only those connecting filtered nodes
    let edges: Vec<&DecisionEdge> = all_edges
        .iter()
        .filter(|e| node_ids.contains(&e.from_node_id) && node_ids.contains(&e.to_node_id))
        .collect();

    // Summary
    let mut by_type: HashMap<String, usize> = HashMap::new();
    let mut by_status: HashMap<String, usize> = HashMap::new();
    let mut confidence = ConfidenceDistribution {
        high: 0,
        medium: 0,
        low: 0,
        unset: 0,
    };

    for node in &nodes {
        *by_type.entry(node.node_type.clone()).or_insert(0) += 1;
        *by_status.entry(node.status.clone()).or_insert(0) += 1;
        let ref_node = node_to_ref(node);
        match ref_node.confidence {
            Some(c) if c >= 80 => confidence.high += 1,
            Some(c) if c >= 50 => confidence.medium += 1,
            Some(_) => confidence.low += 1,
            None => confidence.unset += 1,
        }
    }

    let summary = PulseSummary {
        total_nodes: nodes.len(),
        total_edges: edges.len(),
        by_type,
        by_status,
        confidence,
    };

    // Open goal trees
    let active_goals: Vec<&DecisionNode> = nodes
        .iter()
        .filter(|n| n.node_type == "goal" && is_open(&n.status))
        .copied()
        .collect();

    // Build adjacency for BFS
    let mut outgoing: HashMap<i32, Vec<i32>> = HashMap::new();
    for edge in &edges {
        outgoing
            .entry(edge.from_node_id)
            .or_default()
            .push(edge.to_node_id);
    }

    let node_map: HashMap<i32, &DecisionNode> = nodes.iter().map(|n| (n.id, *n)).collect();

    let mut goal_trees: Vec<GoalTree> = active_goals
        .iter()
        .map(|goal| {
            let mut children = Vec::new();
            let mut closed_children = 0;
            let mut last_activity = goal.created_at.clone();
            let mut visited = HashSet::new();
            let mut queue = std::collections::VecDeque::new();
            visited.insert(goal.id);
            if let Some(outs) = outgoing.get(&goal.id) {
                for &child_id in outs {
                    queue.push_back(child_id);
                }
            }
            while let Some(nid) = queue.pop_front() {
                if visited.insert(nid) {
                    if let Some(node) = node_map.get(&nid) {
                        if node.created_at > last_activity {
                            last_activity = node.created_at.clone();
                        }
                        if is_open(&node.status) {
                            children.push(node_to_ref(node));
                        } else {
                            closed_children += 1;
                        }
                    }
                    if let Some(outs) = outgoing.get(&nid) {
                        for &child_id in outs {
                            queue.push_back(child_id);
                        }
                    }
                }
            }
            GoalTree {
                goal: node_to_ref(goal),
                children,
                closed_children,
                last_activity,
            }
        })
        .collect();
    // Most recently touched first: what is being worked on leads.
    goal_trees.sort_by(|a, b| {
        b.last_activity
            .cmp(&a.last_activity)
            .then(b.goal.id.cmp(&a.goal.id))
    });

    // Orphan nodes (no edges at all, excluding root goals)
    let mut nodes_with_edges: HashSet<i32> = HashSet::new();
    for edge in &edges {
        nodes_with_edges.insert(edge.from_node_id);
        nodes_with_edges.insert(edge.to_node_id);
    }
    let orphans: Vec<NodeRef> = nodes
        .iter()
        .filter(|n| !nodes_with_edges.contains(&n.id) && n.node_type != "goal")
        .map(|n| node_to_ref(n))
        .collect();

    // Recent activity
    let mut recent_sorted: Vec<&DecisionNode> = nodes.to_vec();
    recent_sorted.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    let recent: Vec<NodeRef> = recent_sorted
        .iter()
        .take(recent_count)
        .map(|n| node_to_ref(n))
        .collect();

    // Coverage gaps
    let mut gaps = Vec::new();
    for node in &nodes {
        let has_outgoing = outgoing.contains_key(&node.id);
        match node.node_type.as_str() {
            "goal" if !has_outgoing && is_open(&node.status) => {
                gaps.push(CoverageGap {
                    node_id: node.id,
                    node_type: node.node_type.clone(),
                    title: node.title.clone(),
                    gap_type: "goal_without_options".to_string(),
                });
            }
            "decision" if !has_outgoing && is_open(&node.status) => {
                gaps.push(CoverageGap {
                    node_id: node.id,
                    node_type: node.node_type.clone(),
                    title: node.title.clone(),
                    gap_type: "decision_without_actions".to_string(),
                });
            }
            "action" if !has_outgoing && is_open(&node.status) => {
                gaps.push(CoverageGap {
                    node_id: node.id,
                    node_type: node.node_type.clone(),
                    title: node.title.clone(),
                    gap_type: "action_without_outcomes".to_string(),
                });
            }
            _ => {}
        }
    }

    Ok(PulseReport {
        summary,
        active_goals: goal_trees,
        orphan_nodes: orphans,
        recent_nodes: recent,
        coverage_gaps: gaps,
    })
}

/// Print the pulse report in colored terminal format
pub fn print_pulse(report: &PulseReport, summary_only: bool, limits: PulseLimits) {
    println!("{}", "=== PULSE ===".bold());
    println!();

    // Summary
    println!("{}:", "Summary".bold());
    println!(
        "  Nodes: {} | Edges: {}",
        report.summary.total_nodes.to_string().cyan(),
        report.summary.total_edges.to_string().cyan()
    );

    // Types
    let type_order = [
        "goal",
        "option",
        "decision",
        "action",
        "outcome",
        "observation",
        "revisit",
    ];
    let type_parts: Vec<String> = type_order
        .iter()
        .filter_map(|t| {
            report
                .summary
                .by_type
                .get(*t)
                .map(|c| format!("{}({})", t, c))
        })
        .collect();
    if !type_parts.is_empty() {
        println!("  Types:  {}", type_parts.join(" "));
    }

    // Status
    let mut statuses: Vec<(&String, &usize)> = report.summary.by_status.iter().collect();
    statuses.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    let status_parts: Vec<String> = statuses
        .iter()
        .map(|(s, c)| format!("{}({})", s, c))
        .collect();
    if !status_parts.is_empty() {
        println!("  Status: {}", status_parts.join(" "));
    }

    // Confidence
    let conf = &report.summary.confidence;
    println!(
        "  Confidence: high({}) medium({}) low({}) unset({})",
        conf.high, conf.medium, conf.low, conf.unset
    );

    if summary_only {
        return;
    }

    // Open goals
    println!();
    if report.active_goals.is_empty() {
        println!("{}: none", "Open Goals".bold());
    } else {
        println!("{} ({}):", "Open Goals".bold(), report.active_goals.len());
        for tree in report.active_goals.iter().take(limits.goals) {
            let conf_str = tree
                .goal
                .confidence
                .map(|c| format!(" {}%", c))
                .unwrap_or_default();
            println!(
                "  #{} {} {} ({}){}",
                tree.goal.id,
                format!("[{}]", tree.goal.node_type).yellow(),
                tree.goal.title,
                tree.goal.status.green(),
                conf_str.dimmed()
            );
            for child in tree.children.iter().take(limits.children) {
                let child_conf = child
                    .confidence
                    .map(|c| format!(" {}%", c))
                    .unwrap_or_default();
                println!(
                    "    ├── #{} {} {} ({}){}",
                    child.id,
                    format!("[{}]", child.node_type).blue(),
                    child.title,
                    child.status.green(),
                    child_conf.dimmed()
                );
            }
            let hidden = tree.children.len().saturating_sub(limits.children);
            let mut tail = Vec::new();
            if hidden > 0 {
                tail.push(format!("{} more open", hidden));
            }
            if tree.closed_children > 0 {
                tail.push(format!("{} closed", tree.closed_children));
            }
            if !tail.is_empty() {
                println!("    └── {}", tail.join(", ").dimmed());
            }
        }
        let hidden_goals = report.active_goals.len().saturating_sub(limits.goals);
        if hidden_goals > 0 {
            println!(
                "  {}",
                format!(
                    "{} more open goals, least recently touched last: --all, or `deciduous nodes -t goal --open`",
                    hidden_goals
                )
                .dimmed()
            );
        }
    }

    // Orphans
    if !report.orphan_nodes.is_empty() {
        println!();
        println!("{} ({}):", "Orphan Nodes".bold(), report.orphan_nodes.len());
        for node in report.orphan_nodes.iter().take(limits.list) {
            println!(
                "  #{} {} \"{}\" - {}",
                node.id,
                format!("[{}]", node.node_type).yellow(),
                node.title,
                "no connections".red()
            );
        }
        more_line(report.orphan_nodes.len(), limits.list);
    }

    // Recent activity
    if !report.recent_nodes.is_empty() {
        println!();
        println!("{}:", "Recent Activity".bold());
        for node in &report.recent_nodes {
            let date = node.created_at.get(..10).unwrap_or(&node.created_at);
            let conf_str = node
                .confidence
                .map(|c| format!(" {}%", c))
                .unwrap_or_default();
            println!(
                "  {}  #{:<3} {} {}{}",
                date.dimmed(),
                node.id,
                format!("[{:<11}]", node.node_type).blue(),
                node.title,
                conf_str.dimmed()
            );
        }
    }

    // Coverage gaps
    if !report.coverage_gaps.is_empty() {
        println!();
        println!(
            "{} ({}):",
            "Coverage Gaps".bold(),
            report.coverage_gaps.len()
        );
        for gap in report.coverage_gaps.iter().take(limits.list) {
            let gap_desc = match gap.gap_type.as_str() {
                "goal_without_options" => "no options/decisions",
                "decision_without_actions" => "no actions",
                "action_without_outcomes" => "no outcomes",
                _ => &gap.gap_type,
            };
            println!(
                "  #{:<3} {} \"{}\" - {}",
                gap.node_id,
                format!("[{}]", gap.node_type).yellow(),
                gap.title,
                gap_desc.red()
            );
        }
        more_line(report.coverage_gaps.len(), limits.list);
    }
}

fn more_line(total: usize, shown: usize) {
    if total > shown {
        println!("  {}", format!("{} more: --all", total - shown).dimmed());
    }
}
