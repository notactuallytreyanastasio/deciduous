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
    build_metadata_json, get_current_git_branch, get_current_git_commit, CheckboxState, CommandLog,
    Database, DbRecord, DbSummary, DecisionContext, DecisionEdge, DecisionGraph, DecisionNode,
    DecisionSession, GitHubIssueCache, RoadmapConflict, RoadmapItem, RoadmapSyncState,
    TraceContent, TraceSession, TraceSpan, CURRENT_SCHEMA,
};
pub use diff::{ApplyResult, GraphPatch, PatchEdge, PatchNode};
pub use export::{
    filter_graph_by_ids, filter_graph_from_roots, generate_pr_writeup, graph_to_dot,
    parse_node_range, DotConfig, WriteupConfig,
};

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
