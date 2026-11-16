//! `deciduous audit`: what in the graph contradicts itself.
//!
//! A graph is written one node at a time and nothing reads it back, so it
//! drifts: outcomes left `pending` for good, actions whose commit landed
//! months ago still listed as open, goals nobody closed. Each check here
//! names the nodes; the two whose fix needs no judgement (`close_records`,
//! `close_committed`) can be applied.

use crate::db::{
    Database, DecisionEdge, DecisionNode, NODE_STATUSES, OPEN_STATUSES, RECORD_NODE_TYPES,
};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};

/// A node named by a finding.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Found {
    pub id: i32,
    pub node_type: String,
    pub status: String,
    pub title: String,
}

impl From<&DecisionNode> for Found {
    fn from(n: &DecisionNode) -> Self {
        Self {
            id: n.id,
            node_type: n.node_type.clone(),
            status: n.status.clone(),
            title: n.title.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct AuditReport {
    pub total_nodes: usize,
    pub total_edges: usize,
    /// A status no command accepts.
    pub unknown_status: Vec<Found>,
    /// Outcomes and observations still open. `--close-records` closes them.
    pub open_records: Vec<Found>,
    /// Open actions that carry a commit. `--close-committed` closes them.
    pub open_with_commit: Vec<Found>,
    /// Open goals with at least one outcome and nothing open beneath them.
    pub goals_with_nothing_open: Vec<Found>,
    /// Nodes other than goals with no edge at all.
    pub orphans: Vec<Found>,
    /// Edges from an option to a decision typed `leads_to`, which does not
    /// say whether the option was taken.
    pub untyped_choices: usize,
    pub edge_types: BTreeMap<String, usize>,
}

impl AuditReport {
    pub fn findings(&self) -> usize {
        self.unknown_status.len()
            + self.open_records.len()
            + self.open_with_commit.len()
            + self.goals_with_nothing_open.len()
            + self.orphans.len()
            + self.untyped_choices
    }
}

fn is_open(n: &DecisionNode) -> bool {
    OPEN_STATUSES.contains(&n.status.as_str())
}

fn has_commit(n: &DecisionNode) -> bool {
    n.metadata_json
        .as_deref()
        .and_then(|m| serde_json::from_str::<serde_json::Value>(m).ok())
        .and_then(|v| v.get("commit").and_then(|c| c.as_str()).map(str::to_owned))
        .is_some_and(|c| !c.trim().is_empty())
}

pub fn audit(nodes: &[DecisionNode], edges: &[DecisionEdge]) -> AuditReport {
    let by_id: HashMap<i32, &DecisionNode> = nodes.iter().map(|n| (n.id, n)).collect();
    let mut outgoing: HashMap<i32, Vec<i32>> = HashMap::new();
    let mut connected: HashSet<i32> = HashSet::new();
    let mut edge_types: BTreeMap<String, usize> = BTreeMap::new();
    let mut untyped_choices = 0;
    for e in edges {
        outgoing
            .entry(e.from_node_id)
            .or_default()
            .push(e.to_node_id);
        connected.insert(e.from_node_id);
        connected.insert(e.to_node_id);
        *edge_types.entry(e.edge_type.clone()).or_insert(0) += 1;
        if e.edge_type == "leads_to"
            && by_id
                .get(&e.from_node_id)
                .is_some_and(|n| n.node_type == "option")
            && by_id
                .get(&e.to_node_id)
                .is_some_and(|n| n.node_type == "decision")
        {
            untyped_choices += 1;
        }
    }

    let found = |keep: &dyn Fn(&DecisionNode) -> bool| -> Vec<Found> {
        nodes.iter().filter(|n| keep(n)).map(Found::from).collect()
    };

    let goals_with_nothing_open = found(&|goal| {
        if goal.node_type != "goal" || !is_open(goal) {
            return false;
        }
        let mut seen = HashSet::from([goal.id]);
        let mut stack: Vec<i32> = outgoing.get(&goal.id).cloned().unwrap_or_default();
        let mut outcome = false;
        while let Some(id) = stack.pop() {
            if !seen.insert(id) {
                continue;
            }
            let Some(n) = by_id.get(&id) else { continue };
            // A record left open is `open_records`' finding, not evidence
            // that the goal still has work under it.
            let record = RECORD_NODE_TYPES.contains(&n.node_type.as_str());
            if is_open(n) && !record {
                return false;
            }
            outcome |= n.node_type == "outcome";
            stack.extend(outgoing.get(&id).into_iter().flatten());
        }
        outcome
    });

    AuditReport {
        total_nodes: nodes.len(),
        total_edges: edges.len(),
        unknown_status: found(&|n| !NODE_STATUSES.contains(&n.status.as_str())),
        open_records: found(&|n| is_open(n) && RECORD_NODE_TYPES.contains(&n.node_type.as_str())),
        open_with_commit: found(&|n| n.node_type == "action" && is_open(n) && has_commit(n)),
        goals_with_nothing_open,
        orphans: found(&|n| n.node_type != "goal" && !connected.contains(&n.id)),
        untyped_choices,
        edge_types,
    }
}

pub fn audit_db(db: &Database) -> Result<AuditReport, String> {
    let nodes = db.get_all_nodes().map_err(|e| e.to_string())?;
    let edges = db.get_all_edges().map_err(|e| e.to_string())?;
    Ok(audit(&nodes, &edges))
}

/// Set every node in `found` to `completed`. Returns the ids that changed
/// and the ones that refused, each with its error.
pub fn close(db: &Database, found: &[Found]) -> (Vec<i32>, Vec<(i32, String)>) {
    let mut closed = Vec::new();
    let mut failed = Vec::new();
    for f in found {
        match db.update_node_status(f.id, "completed") {
            Ok(()) => closed.push(f.id),
            Err(e) => failed.push((f.id, e.to_string())),
        }
    }
    (closed, failed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: i32, node_type: &str, status: &str, meta: Option<&str>) -> DecisionNode {
        DecisionNode {
            id,
            change_id: format!("c{id}"),
            node_type: node_type.into(),
            title: format!("n{id}"),
            description: None,
            status: status.into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            metadata_json: meta.map(str::to_owned),
        }
    }

    fn edge(id: i32, from: i32, to: i32, edge_type: &str) -> DecisionEdge {
        DecisionEdge {
            id,
            from_node_id: from,
            to_node_id: to,
            from_change_id: None,
            to_change_id: None,
            edge_type: edge_type.into(),
            weight: None,
            rationale: None,
            created_at: "2026-01-01T00:00:00Z".into(),
        }
    }

    fn ids(found: &[Found]) -> Vec<i32> {
        found.iter().map(|f| f.id).collect()
    }

    #[test]
    fn each_check_names_its_nodes_and_no_others() {
        let nodes = vec![
            node(1, "goal", "pending", None),
            node(2, "option", "completed", None),
            node(3, "decision", "completed", None),
            node(4, "action", "pending", Some(r#"{"commit":"abc1234"}"#)),
            node(5, "outcome", "pending", None),
            node(6, "observation", "resolved", None),
            node(7, "action", "pending", Some(r#"{"commit":""}"#)),
            node(8, "observation", "completed", None),
        ];
        let edges = vec![
            edge(1, 1, 2, "leads_to"),
            edge(2, 2, 3, "leads_to"),
            edge(3, 3, 4, "leads_to"),
            edge(4, 4, 5, "leads_to"),
            edge(5, 1, 8, "leads_to"),
        ];
        let r = audit(&nodes, &edges);
        assert_eq!(ids(&r.unknown_status), [6]);
        assert_eq!(ids(&r.open_records), [5]);
        assert_eq!(ids(&r.open_with_commit), [4]);
        assert_eq!(ids(&r.orphans), [6, 7]);
        assert_eq!(r.untyped_choices, 1);
        // Action 4 is still open, so goal 1 has work under it.
        assert!(r.goals_with_nothing_open.is_empty());
        assert_eq!(r.edge_types["leads_to"], 5);
    }

    #[test]
    fn a_goal_whose_work_is_all_closed_is_named_even_while_its_outcome_is_pending() {
        let nodes = vec![
            node(1, "goal", "active", None),
            node(2, "action", "completed", None),
            node(3, "outcome", "pending", None),
            node(4, "goal", "pending", None),
            node(5, "option", "rejected", None),
            node(6, "goal", "completed", None),
            node(7, "outcome", "completed", None),
        ];
        let edges = vec![
            edge(1, 1, 2, "leads_to"),
            edge(2, 2, 3, "leads_to"),
            // Goal 4 has nothing open but no outcome either: not done, just empty.
            edge(3, 4, 5, "rejected"),
            edge(4, 6, 7, "leads_to"),
        ];
        let r = audit(&nodes, &edges);
        assert_eq!(ids(&r.goals_with_nothing_open), [1]);
    }

    #[test]
    fn a_cycle_under_a_goal_terminates() {
        let nodes = vec![
            node(1, "goal", "pending", None),
            node(2, "outcome", "completed", None),
        ];
        let edges = vec![edge(1, 1, 2, "leads_to"), edge(2, 2, 1, "leads_to")];
        assert_eq!(ids(&audit(&nodes, &edges).goals_with_nothing_open), [1]);
    }
}
