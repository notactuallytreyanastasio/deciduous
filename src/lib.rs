//! Deciduous - Decision graph tooling for AI-assisted development
//!
//! Track every decision, query your reasoning, preserve context across sessions.
//!
//! # Overview
//!
//! Deciduous provides a persistent decision graph that survives context loss.
//! Every goal, decision, option, action, and outcome is captured and linked,
//! creating a queryable history of your development process.
//!
//! # Node Types
//!
//! | Type | Purpose |
//! |------|---------|
//! | `goal` | High-level objectives |
//! | `decision` | Choice points with options |
//! | `option` | Approaches considered |
//! | `action` | What was implemented |
//! | `outcome` | What happened |
//! | `observation` | Technical insights |
//!
//! # Quick Start
//!
//! ```no_run
//! use deciduous::Database;
//!
//! let db = Database::new("deciduous.db").unwrap();
//!
//! // Add a goal
//! let goal_id = db.add_node("goal", "Implement feature X", None, Some(90), None).unwrap();
//!
//! // Add an action linked to it
//! let action_id = db.add_node("action", "Writing the code", None, Some(85), None).unwrap();
//! db.add_edge(goal_id, action_id, "leads_to", None).unwrap();
//!
//! // Query the graph
//! let graph = db.get_graph().unwrap();
//! println!("Nodes: {}, Edges: {}", graph.nodes.len(), graph.edges.len());
//! ```

pub mod config;
pub mod db;
pub mod diff;
pub mod export;
pub mod github;
pub mod init;
pub mod roadmap;
pub mod schema;
pub mod serve;
pub mod tui;

pub use config::Config;
pub use db::{
    CommandLog, Database, DbRecord, DbSummary, DecisionEdge, DecisionGraph, DecisionNode,
    DecisionContext, DecisionSession, CheckboxState, RoadmapItem, RoadmapSyncState, RoadmapConflict,
    GitHubIssueCache, CURRENT_SCHEMA, get_current_git_branch, get_current_git_commit, build_metadata_json,
};
pub use diff::{GraphPatch, PatchNode, PatchEdge, ApplyResult};
pub use export::{graph_to_dot, generate_pr_writeup, filter_graph_from_roots, filter_graph_by_ids, parse_node_range, DotConfig, WriteupConfig};

// Re-export TS trait for downstream use
#[cfg(feature = "ts-rs")]
pub use ts_rs::TS;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_public_exports() {
        // Verify core types are re-exported from crate root
        let _ = CURRENT_SCHEMA;
    }
}
