//! Archaeology - retroactive graph building with atomic operations
//!
//! Provides commands for common archaeology patterns:
//! - `pivot`: Create a full pivot chain atomically (7 steps → 1 command)
//! - `timeline`: Show nodes chronologically
//! - `supersede`: Mark nodes as superseded with optional cascade

use crate::db::{build_metadata_json, Database, DecisionNode};
use crate::events::{maybe_emit_add_edge, maybe_emit_add_node, maybe_emit_status_update};
use colored::Colorize;
use serde::Serialize;
use std::collections::{HashSet, VecDeque};

/// Result of an atomic pivot creation
#[derive(Debug, Serialize)]
pub struct PivotResult {
    pub observation_id: i32,
    pub observation_title: String,
    pub revisit_id: i32,
    pub revisit_title: String,
    pub new_decision_id: i32,
    pub new_decision_title: String,
    pub superseded_id: i32,
    pub superseded_title: String,
}

/// Result of a supersede operation
#[derive(Debug, Serialize)]
pub struct SupersedeResult {
    pub superseded: Vec<SupersededNode>,
}

#[derive(Debug, Serialize)]
pub struct SupersededNode {
    pub id: i32,
    pub title: String,
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

fn get_confidence(node: &DecisionNode) -> Option<u8> {
    node.metadata_json
        .as_ref()
        .and_then(|m| serde_json::from_str::<serde_json::Value>(m).ok())
        .and_then(|v| v.get("confidence").and_then(|c| c.as_u64()))
        .map(|c| c.min(100) as u8)
}

/// Create a full pivot chain atomically.
///
/// Performs these steps:
/// 1. Validate from_id exists
/// 2. Create observation node
/// 3. Link from_id → observation
/// 4. Create revisit node
/// 5. Link observation → revisit
/// 6. Create new decision node
/// 7. Link revisit → new decision
/// 8. Mark from_id as superseded
pub fn create_pivot(
    db: &Database,
    from_id: i32,
    observation_text: &str,
    new_approach_text: &str,
    confidence: Option<u8>,
    reason: Option<&str>,
    dry_run: bool,
) -> Result<PivotResult, String> {
    // Step 1: Validate from_id exists
    let from_node = db
        .get_node(from_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Node {} not found", from_id))?;

    let revisit_title = format!("Reconsidering: {}", from_node.title);

    if dry_run {
        return Ok(PivotResult {
            observation_id: -1,
            observation_title: observation_text.to_string(),
            revisit_id: -2,
            revisit_title,
            new_decision_id: -3,
            new_decision_title: new_approach_text.to_string(),
            superseded_id: from_id,
            superseded_title: from_node.title.clone(),
        });
    }

    // Inherit branch from the source node
    let branch = get_branch(&from_node);

    // Step 2: Create observation node
    let obs_id = db
        .create_node_full(
            "observation",
            observation_text,
            None,
            None,
            None,
            None,
            None,
            branch.as_deref(),
            None,
        )
        .map_err(|e| e.to_string())?;

    // Emit sync event for observation
    if let Ok(Some(obs_node)) = db.get_node(obs_id) {
        maybe_emit_add_node(&obs_node);
    }

    // Step 3: Link from_id → observation
    let reason_str = reason.unwrap_or("Discovery");
    let _edge1 = db
        .create_edge(from_id, obs_id, "leads_to", Some(reason_str))
        .map_err(|e| e.to_string())?;

    // Emit sync event for edge
    if let Ok(Some(obs_node)) = db.get_node(obs_id) {
        maybe_emit_add_edge(
            &from_node.change_id,
            &obs_node.change_id,
            "leads_to",
            Some(reason_str),
        );
    }

    // Step 4: Create revisit node
    let revisit_id = db
        .create_node_full(
            "revisit",
            &revisit_title,
            None,
            None,
            None,
            None,
            None,
            branch.as_deref(),
            None,
        )
        .map_err(|e| e.to_string())?;

    if let Ok(Some(revisit_node)) = db.get_node(revisit_id) {
        maybe_emit_add_node(&revisit_node);
    }

    // Step 5: Link observation → revisit
    let _edge2 = db
        .create_edge(obs_id, revisit_id, "leads_to", Some("Forced rethinking"))
        .map_err(|e| e.to_string())?;

    if let (Ok(Some(obs_node)), Ok(Some(revisit_node))) =
        (db.get_node(obs_id), db.get_node(revisit_id))
    {
        maybe_emit_add_edge(
            &obs_node.change_id,
            &revisit_node.change_id,
            "leads_to",
            Some("Forced rethinking"),
        );
    }

    // Step 6: Create new decision node
    let metadata = build_metadata_json(confidence, None, None, None, branch.as_deref());
    let decision_id = db
        .create_node_full(
            "decision",
            new_approach_text,
            None,
            confidence,
            None,
            None,
            None,
            branch.as_deref(),
            None,
        )
        .map_err(|e| e.to_string())?;

    if let Ok(Some(decision_node)) = db.get_node(decision_id) {
        maybe_emit_add_node(&decision_node);
    }

    // Step 7: Link revisit → new decision
    let _edge3 = db
        .create_edge(revisit_id, decision_id, "leads_to", Some("New direction"))
        .map_err(|e| e.to_string())?;

    if let (Ok(Some(revisit_node)), Ok(Some(decision_node))) =
        (db.get_node(revisit_id), db.get_node(decision_id))
    {
        maybe_emit_add_edge(
            &revisit_node.change_id,
            &decision_node.change_id,
            "leads_to",
            Some("New direction"),
        );
    }

    // Step 8: Mark from_id as superseded
    db.update_node_status(from_id, "superseded")
        .map_err(|e| e.to_string())?;
    maybe_emit_status_update(&from_node.change_id, "superseded");

    // Suppress unused variable warning
    let _ = metadata;

    Ok(PivotResult {
        observation_id: obs_id,
        observation_title: observation_text.to_string(),
        revisit_id,
        revisit_title,
        new_decision_id: decision_id,
        new_decision_title: new_approach_text.to_string(),
        superseded_id: from_id,
        superseded_title: from_node.title.clone(),
    })
}

/// Print the pivot result
pub fn print_pivot_result(result: &PivotResult, dry_run: bool) {
    if dry_run {
        println!("{}", "=== PIVOT (dry run) ===".bold().yellow());
    } else {
        println!("{}", "=== PIVOT CREATED ===".bold().green());
    }
    println!();

    let id_prefix = if dry_run { "?" } else { "#" };
    let obs_id = if dry_run {
        "?".to_string()
    } else {
        result.observation_id.to_string()
    };
    let rev_id = if dry_run {
        "?".to_string()
    } else {
        result.revisit_id.to_string()
    };
    let dec_id = if dry_run {
        "?".to_string()
    } else {
        result.new_decision_id.to_string()
    };

    println!(
        "  {}{} {} \"{}\"",
        id_prefix,
        obs_id,
        "[observation]".yellow(),
        result.observation_title
    );
    println!(
        "    └── linked from #{} \"{}\"",
        result.superseded_id, result.superseded_title
    );

    println!(
        "  {}{} {} \"{}\"",
        id_prefix,
        rev_id,
        "[revisit]".yellow(),
        result.revisit_title
    );
    println!("    └── linked from {}{} [observation]", id_prefix, obs_id);

    println!(
        "  {}{} {} \"{}\"",
        id_prefix,
        dec_id,
        "[decision]".green(),
        result.new_decision_title
    );
    println!("    └── linked from {}{} [revisit]", id_prefix, rev_id);

    println!();
    println!(
        "  Superseded: #{} \"{}\" -> status: {}",
        result.superseded_id,
        result.superseded_title,
        "superseded".dimmed()
    );

    let verb = if dry_run { "would be" } else { "" };
    if dry_run {
        println!();
        println!(
            "  Summary: 3 nodes {} created, 3 edges {} created, 1 node {} superseded",
            verb, verb, verb
        );
    } else {
        println!();
        println!("  Summary: 3 nodes created, 3 edges created, 1 node superseded");
    }
}

/// Show all nodes sorted chronologically
pub fn timeline(
    db: &Database,
    limit: usize,
    node_type: Option<&str>,
    branch: Option<&str>,
) -> Result<Vec<DecisionNode>, String> {
    let mut nodes = db.get_all_nodes().map_err(|e| e.to_string())?;

    // Filter by type
    if let Some(nt) = node_type {
        nodes.retain(|n| n.node_type == nt);
    }

    // Filter by branch
    if let Some(br) = branch {
        nodes.retain(|n| get_branch(n).as_deref() == Some(br));
    }

    // Sort chronologically (already sorted by created_at asc from DB, but re-sort to be safe)
    nodes.sort_by(|a, b| a.created_at.cmp(&b.created_at));

    // Apply limit
    if limit > 0 && nodes.len() > limit {
        let skip = nodes.len() - limit;
        nodes = nodes.into_iter().skip(skip).collect();
    }

    Ok(nodes)
}

/// Print timeline in colored terminal format
pub fn print_timeline(nodes: &[DecisionNode]) {
    if nodes.is_empty() {
        println!("No nodes found.");
        return;
    }

    println!("{} ({} nodes)", "=== TIMELINE ===".bold(), nodes.len());
    println!();

    for node in nodes {
        let date = node.created_at.get(..10).unwrap_or(&node.created_at);
        let conf = get_confidence(node)
            .map(|c| format!(" {}%", c))
            .unwrap_or_default();
        let status_color = match node.status.as_str() {
            "superseded" => node.status.dimmed().to_string(),
            "abandoned" => node.status.red().to_string(),
            "active" => node.status.green().to_string(),
            _ => node.status.clone(),
        };
        println!(
            "  {}  #{:<3} {:<15} {:<40} {:<12} {}",
            date.dimmed(),
            node.id,
            format!("[{}]", node.node_type).blue(),
            node.title,
            status_color,
            conf.dimmed()
        );
    }
}

/// Mark a node as superseded, optionally cascading to descendants
pub fn supersede(
    db: &Database,
    node_id: i32,
    cascade: bool,
    dry_run: bool,
) -> Result<SupersedeResult, String> {
    let node = db
        .get_node(node_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Node {} not found", node_id))?;

    let mut to_supersede = vec![SupersededNode {
        id: node.id,
        title: node.title.clone(),
    }];

    if cascade {
        let all_edges = db.get_all_edges().map_err(|e| e.to_string())?;
        let all_nodes = db.get_all_nodes().map_err(|e| e.to_string())?;
        let node_map: std::collections::HashMap<i32, &DecisionNode> =
            all_nodes.iter().map(|n| (n.id, n)).collect();

        // BFS to find all descendants
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        visited.insert(node_id);
        queue.push_back(node_id);

        while let Some(current) = queue.pop_front() {
            for edge in &all_edges {
                if edge.from_node_id == current && visited.insert(edge.to_node_id) {
                    if let Some(desc) = node_map.get(&edge.to_node_id) {
                        if desc.status != "superseded" {
                            to_supersede.push(SupersededNode {
                                id: desc.id,
                                title: desc.title.clone(),
                            });
                        }
                    }
                    queue.push_back(edge.to_node_id);
                }
            }
        }
    }

    if !dry_run {
        for s in &to_supersede {
            db.update_node_status(s.id, "superseded")
                .map_err(|e| e.to_string())?;
            // Emit sync event
            if let Ok(Some(n)) = db.get_node(s.id) {
                maybe_emit_status_update(&n.change_id, "superseded");
            }
        }
    }

    Ok(SupersedeResult {
        superseded: to_supersede,
    })
}

/// Print supersede result
pub fn print_supersede_result(result: &SupersedeResult, dry_run: bool) {
    let verb = if dry_run {
        "Would supersede"
    } else {
        "Superseded"
    };

    if let Some(first) = result.superseded.first() {
        println!("{} node #{} \"{}\"", verb, first.id, first.title);
    }

    if result.superseded.len() > 1 {
        let cascade_verb = if dry_run {
            "Would also supersede (cascade)"
        } else {
            "Also superseded (cascade)"
        };
        println!("  {}:", cascade_verb);
        for s in result.superseded.iter().skip(1) {
            println!("    #{} \"{}\"", s.id, s.title);
        }
    }

    println!(
        "  Total: {} node(s) {}marked superseded",
        result.superseded.len(),
        if dry_run { "would be " } else { "" }
    );
}
