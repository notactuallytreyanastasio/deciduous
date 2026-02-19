//! Event log system for multi-user sync
//!
//! Instead of exporting/importing patch snapshots, this module provides:
//! - Append-only event logs per user
//! - Checkpoint files for compaction
//! - Rebuild from events + checkpoint
//!
//! Events are stored in `.deciduous/sync/events/{user}.jsonl` (git-tracked)
//! Checkpoints are stored in `.deciduous/sync/checkpoint.json` (git-tracked)
//! The local database `.deciduous/deciduous.db` remains gitignored.

use chrono::{DateTime, Utc};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

/// All possible event types in the event log
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Event {
    /// Add a new node
    AddNode {
        change_id: String,
        node_type: String,
        title: String,
        description: Option<String>,
        status: String,
        metadata_json: Option<String>,
        #[serde(with = "chrono::serde::ts_milliseconds")]
        timestamp: DateTime<Utc>,
        author: String,
    },
    /// Update an existing node's fields
    UpdateNode {
        change_id: String,
        /// Fields to update (only present fields are updated)
        title: Option<String>,
        description: Option<String>,
        status: Option<String>,
        metadata_json: Option<String>,
        #[serde(with = "chrono::serde::ts_milliseconds")]
        timestamp: DateTime<Utc>,
        author: String,
    },
    /// Delete a node
    DeleteNode {
        change_id: String,
        #[serde(with = "chrono::serde::ts_milliseconds")]
        timestamp: DateTime<Utc>,
        author: String,
    },
    /// Add an edge between nodes (uses change_ids, not local ids)
    AddEdge {
        /// Unique ID for this edge (for deduplication)
        edge_id: String,
        from_change_id: String,
        to_change_id: String,
        edge_type: String,
        rationale: Option<String>,
        #[serde(with = "chrono::serde::ts_milliseconds")]
        timestamp: DateTime<Utc>,
        author: String,
    },
    /// Delete an edge
    DeleteEdge {
        edge_id: String,
        #[serde(with = "chrono::serde::ts_milliseconds")]
        timestamp: DateTime<Utc>,
        author: String,
    },
    /// Create a theme
    AddTheme {
        change_id: String,
        name: String,
        color: String,
        description: Option<String>,
        #[serde(with = "chrono::serde::ts_milliseconds")]
        timestamp: DateTime<Utc>,
        author: String,
    },
    /// Delete a theme
    DeleteTheme {
        change_id: String,
        #[serde(with = "chrono::serde::ts_milliseconds")]
        timestamp: DateTime<Utc>,
        author: String,
    },
    /// Tag a node with a theme
    TagNode {
        node_change_id: String,
        theme_change_id: String,
        source: String,
        #[serde(with = "chrono::serde::ts_milliseconds")]
        timestamp: DateTime<Utc>,
        author: String,
    },
    /// Untag a node
    UntagNode {
        node_change_id: String,
        theme_change_id: String,
        #[serde(with = "chrono::serde::ts_milliseconds")]
        timestamp: DateTime<Utc>,
        author: String,
    },
    /// Attach a document to a node
    AttachDocument {
        doc_change_id: String,
        node_change_id: String,
        content_hash: String,
        original_filename: String,
        storage_filename: String,
        mime_type: String,
        file_size: i32,
        description: Option<String>,
        description_source: String,
        #[serde(with = "chrono::serde::ts_milliseconds")]
        timestamp: DateTime<Utc>,
        author: String,
    },
    /// Detach a document
    DetachDocument {
        doc_change_id: String,
        #[serde(with = "chrono::serde::ts_milliseconds")]
        timestamp: DateTime<Utc>,
        author: String,
    },
}

impl Event {
    /// Get the timestamp of this event
    pub fn timestamp(&self) -> DateTime<Utc> {
        match self {
            Event::AddNode { timestamp, .. }
            | Event::UpdateNode { timestamp, .. }
            | Event::DeleteNode { timestamp, .. }
            | Event::AddEdge { timestamp, .. }
            | Event::DeleteEdge { timestamp, .. }
            | Event::AddTheme { timestamp, .. }
            | Event::DeleteTheme { timestamp, .. }
            | Event::TagNode { timestamp, .. }
            | Event::UntagNode { timestamp, .. }
            | Event::AttachDocument { timestamp, .. }
            | Event::DetachDocument { timestamp, .. } => *timestamp,
        }
    }

    /// Get the author of this event
    pub fn author(&self) -> &str {
        match self {
            Event::AddNode { author, .. }
            | Event::UpdateNode { author, .. }
            | Event::DeleteNode { author, .. }
            | Event::AddEdge { author, .. }
            | Event::DeleteEdge { author, .. }
            | Event::AddTheme { author, .. }
            | Event::DeleteTheme { author, .. }
            | Event::TagNode { author, .. }
            | Event::UntagNode { author, .. }
            | Event::AttachDocument { author, .. }
            | Event::DetachDocument { author, .. } => author,
        }
    }
}

/// A checkpoint containing full graph state at a point in time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    /// When this checkpoint was created
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub created_at: DateTime<Utc>,
    /// All nodes at checkpoint time
    pub nodes: Vec<CheckpointNode>,
    /// All edges at checkpoint time
    pub edges: Vec<CheckpointEdge>,
    /// Schema version
    pub version: String,
    /// Theme definitions at checkpoint time
    #[serde(default)]
    pub themes: Vec<CheckpointTheme>,
    /// Node-theme associations at checkpoint time
    #[serde(default)]
    pub node_themes: Vec<CheckpointNodeTheme>,
    /// Document attachments at checkpoint time
    #[serde(default)]
    pub documents: Vec<CheckpointDocument>,
}

/// Node state in a checkpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointNode {
    pub change_id: String,
    pub node_type: String,
    pub title: String,
    pub description: Option<String>,
    pub status: String,
    pub metadata_json: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Edge state in a checkpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointEdge {
    pub edge_id: String,
    pub from_change_id: String,
    pub to_change_id: String,
    pub edge_type: String,
    pub rationale: Option<String>,
    pub created_at: String,
}

/// Theme state in a checkpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointTheme {
    pub change_id: String,
    pub name: String,
    pub color: String,
    pub description: Option<String>,
}

/// Node-theme association in a checkpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointNodeTheme {
    pub node_change_id: String,
    pub theme_change_id: String,
    pub source: String,
}

/// Document attachment in a checkpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointDocument {
    pub change_id: String,
    pub node_change_id: String,
    pub content_hash: String,
    pub original_filename: String,
    pub storage_filename: String,
    pub mime_type: String,
    pub file_size: i32,
    pub description: Option<String>,
    pub description_source: String,
}

/// Manages the event log and checkpoints
pub struct EventLog {
    /// Path to .deciduous/sync/ directory
    sync_dir: PathBuf,
    /// Current user/author name
    author: String,
}

impl EventLog {
    /// Create a new EventLog for the given .deciduous directory
    pub fn new(deciduous_dir: &Path, author: String) -> Result<Self, EventLogError> {
        let sync_dir = deciduous_dir.join("sync");

        // Create directories if they don't exist
        fs::create_dir_all(sync_dir.join("events"))
            .map_err(|e| EventLogError::Io(format!("Failed to create events dir: {}", e)))?;

        Ok(Self { sync_dir, author })
    }

    /// Get the path to this user's event log file
    fn event_file_path(&self) -> PathBuf {
        self.sync_dir.join("events").join(format!("{}.jsonl", self.author))
    }

    /// Get the path to the checkpoint file
    fn checkpoint_path(&self) -> PathBuf {
        self.sync_dir.join("checkpoint.json")
    }

    /// Append an event to the log
    pub fn append(&self, event: Event) -> Result<(), EventLogError> {
        let path = self.event_file_path();
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| EventLogError::Io(format!("Failed to open event log: {}", e)))?;

        let json = serde_json::to_string(&event)
            .map_err(|e| EventLogError::Serialization(e.to_string()))?;

        writeln!(file, "{}", json)
            .map_err(|e| EventLogError::Io(format!("Failed to write event: {}", e)))?;

        Ok(())
    }

    /// Read all events from all users' event logs
    pub fn read_all_events(&self) -> Result<Vec<Event>, EventLogError> {
        let events_dir = self.sync_dir.join("events");
        if !events_dir.exists() {
            return Ok(Vec::new());
        }

        let mut all_events = Vec::new();

        let entries = fs::read_dir(&events_dir)
            .map_err(|e| EventLogError::Io(format!("Failed to read events dir: {}", e)))?;

        for entry in entries {
            let entry = entry.map_err(|e| EventLogError::Io(e.to_string()))?;
            let path = entry.path();

            if path.extension().map(|e| e == "jsonl").unwrap_or(false) {
                let file = File::open(&path)
                    .map_err(|e| EventLogError::Io(format!("Failed to open {:?}: {}", path, e)))?;

                let reader = BufReader::new(file);
                for line in reader.lines() {
                    let line = line.map_err(|e| EventLogError::Io(e.to_string()))?;
                    if line.trim().is_empty() {
                        continue;
                    }
                    let event: Event = serde_json::from_str(&line)
                        .map_err(|e| EventLogError::Serialization(format!("Failed to parse event: {} - line: {}", e, line)))?;
                    all_events.push(event);
                }
            }
        }

        // Sort by timestamp
        all_events.sort_by_key(|e| e.timestamp());

        Ok(all_events)
    }

    /// Load checkpoint if it exists
    pub fn load_checkpoint(&self) -> Result<Option<Checkpoint>, EventLogError> {
        let path = self.checkpoint_path();
        if !path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&path)
            .map_err(|e| EventLogError::Io(format!("Failed to read checkpoint: {}", e)))?;

        let checkpoint: Checkpoint = serde_json::from_str(&content)
            .map_err(|e| EventLogError::Serialization(format!("Failed to parse checkpoint: {}", e)))?;

        Ok(Some(checkpoint))
    }

    /// Save a checkpoint and optionally clear old events
    pub fn save_checkpoint(&self, checkpoint: &Checkpoint, clear_events: bool) -> Result<(), EventLogError> {
        let path = self.checkpoint_path();

        let json = serde_json::to_string_pretty(checkpoint)
            .map_err(|e| EventLogError::Serialization(e.to_string()))?;

        fs::write(&path, json)
            .map_err(|e| EventLogError::Io(format!("Failed to write checkpoint: {}", e)))?;

        if clear_events {
            let events_dir = self.sync_dir.join("events");
            if events_dir.exists() {
                for entry in fs::read_dir(&events_dir).map_err(|e| EventLogError::Io(e.to_string()))? {
                    let entry = entry.map_err(|e| EventLogError::Io(e.to_string()))?;
                    fs::remove_file(entry.path())
                        .map_err(|e| EventLogError::Io(format!("Failed to remove event file: {}", e)))?;
                }
            }
        }

        Ok(())
    }

    /// Get events that occurred after the checkpoint timestamp
    pub fn get_events_after_checkpoint(&self) -> Result<Vec<Event>, EventLogError> {
        let checkpoint = self.load_checkpoint()?;
        let all_events = self.read_all_events()?;

        match checkpoint {
            Some(cp) => {
                Ok(all_events.into_iter()
                    .filter(|e| e.timestamp() > cp.created_at)
                    .collect())
            }
            None => Ok(all_events),
        }
    }

    /// Check if there are any events newer than the checkpoint
    pub fn has_pending_events(&self) -> Result<bool, EventLogError> {
        let events = self.get_events_after_checkpoint()?;
        Ok(!events.is_empty())
    }

    /// Get the sync directory path
    pub fn sync_dir(&self) -> &Path {
        &self.sync_dir
    }
}

/// Result of rebuilding the database from events
#[derive(Debug, Default)]
pub struct RebuildResult {
    pub nodes_created: usize,
    pub nodes_updated: usize,
    pub nodes_deleted: usize,
    pub edges_created: usize,
    pub edges_deleted: usize,
    pub events_processed: usize,
    pub from_checkpoint: bool,
}

/// Materialized state from replaying events
/// This is an in-memory representation before writing to the database
#[derive(Debug, Default)]
pub struct MaterializedState {
    /// Nodes indexed by change_id
    pub nodes: HashMap<String, MaterializedNode>,
    /// Edges indexed by edge_id
    pub edges: HashMap<String, MaterializedEdge>,
    /// Deleted node change_ids (tombstones)
    pub deleted_nodes: std::collections::HashSet<String>,
    /// Deleted edge_ids (tombstones)
    pub deleted_edges: std::collections::HashSet<String>,
}

/// A node in the materialized state
#[derive(Debug, Clone)]
pub struct MaterializedNode {
    pub change_id: String,
    pub node_type: String,
    pub title: String,
    pub description: Option<String>,
    pub status: String,
    pub metadata_json: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// An edge in the materialized state
#[derive(Debug, Clone)]
pub struct MaterializedEdge {
    pub edge_id: String,
    pub from_change_id: String,
    pub to_change_id: String,
    pub edge_type: String,
    pub rationale: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl MaterializedState {
    /// Create from a checkpoint
    pub fn from_checkpoint(checkpoint: &Checkpoint) -> Self {
        let mut state = Self::default();

        for node in &checkpoint.nodes {
            state.nodes.insert(node.change_id.clone(), MaterializedNode {
                change_id: node.change_id.clone(),
                node_type: node.node_type.clone(),
                title: node.title.clone(),
                description: node.description.clone(),
                status: node.status.clone(),
                metadata_json: node.metadata_json.clone(),
                created_at: DateTime::parse_from_rfc3339(&node.created_at)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                updated_at: DateTime::parse_from_rfc3339(&node.updated_at)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
            });
        }

        for edge in &checkpoint.edges {
            state.edges.insert(edge.edge_id.clone(), MaterializedEdge {
                edge_id: edge.edge_id.clone(),
                from_change_id: edge.from_change_id.clone(),
                to_change_id: edge.to_change_id.clone(),
                edge_type: edge.edge_type.clone(),
                rationale: edge.rationale.clone(),
                created_at: DateTime::parse_from_rfc3339(&edge.created_at)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
            });
        }

        state
    }

    /// Apply an event to the state
    pub fn apply(&mut self, event: &Event) {
        match event {
            Event::AddNode {
                change_id,
                node_type,
                title,
                description,
                status,
                metadata_json,
                timestamp,
                ..
            } => {
                // Remove from tombstones if re-adding
                self.deleted_nodes.remove(change_id);

                self.nodes.insert(change_id.clone(), MaterializedNode {
                    change_id: change_id.clone(),
                    node_type: node_type.clone(),
                    title: title.clone(),
                    description: description.clone(),
                    status: status.clone(),
                    metadata_json: metadata_json.clone(),
                    created_at: *timestamp,
                    updated_at: *timestamp,
                });
            }
            Event::UpdateNode {
                change_id,
                title,
                description,
                status,
                metadata_json,
                timestamp,
                ..
            } => {
                if let Some(node) = self.nodes.get_mut(change_id) {
                    if let Some(t) = title {
                        node.title = t.clone();
                    }
                    if let Some(d) = description {
                        node.description = Some(d.clone());
                    }
                    if let Some(s) = status {
                        node.status = s.clone();
                    }
                    if let Some(m) = metadata_json {
                        node.metadata_json = Some(m.clone());
                    }
                    node.updated_at = *timestamp;
                }
            }
            Event::DeleteNode { change_id, .. } => {
                self.nodes.remove(change_id);
                self.deleted_nodes.insert(change_id.clone());
            }
            Event::AddEdge {
                edge_id,
                from_change_id,
                to_change_id,
                edge_type,
                rationale,
                timestamp,
                ..
            } => {
                self.deleted_edges.remove(edge_id);

                self.edges.insert(edge_id.clone(), MaterializedEdge {
                    edge_id: edge_id.clone(),
                    from_change_id: from_change_id.clone(),
                    to_change_id: to_change_id.clone(),
                    edge_type: edge_type.clone(),
                    rationale: rationale.clone(),
                    created_at: *timestamp,
                });
            }
            Event::DeleteEdge { edge_id, .. } => {
                self.edges.remove(edge_id);
                self.deleted_edges.insert(edge_id.clone());
            }
            // Theme and document events are tracked in the checkpoint
            // but don't need in-memory materialization for rebuild
            Event::AddTheme { .. }
            | Event::DeleteTheme { .. }
            | Event::TagNode { .. }
            | Event::UntagNode { .. }
            | Event::AttachDocument { .. }
            | Event::DetachDocument { .. } => {}
        }
    }

    /// Replay a series of events onto the state
    pub fn replay(&mut self, events: &[Event]) {
        for event in events {
            self.apply(event);
        }
    }
}

/// Errors that can occur in event log operations
#[derive(Debug)]
pub enum EventLogError {
    Io(String),
    Serialization(String),
    InvalidEvent(String),
}

impl std::fmt::Display for EventLogError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EventLogError::Io(msg) => write!(f, "IO error: {}", msg),
            EventLogError::Serialization(msg) => write!(f, "Serialization error: {}", msg),
            EventLogError::InvalidEvent(msg) => write!(f, "Invalid event: {}", msg),
        }
    }
}

impl std::error::Error for EventLogError {}

/// Emit a sync event for a newly created node, if sync is initialized.
/// Silently skips if sync is not set up. Prints warning on error.
pub fn maybe_emit_add_node(node: &crate::db::DecisionNode) {
    let sync_dir = std::path::PathBuf::from(".deciduous/sync");
    if sync_dir.exists() {
        let author = get_current_author();
        if let Ok(event_log) = EventLog::new(&std::path::PathBuf::from(".deciduous"), author.clone()) {
            let event = Event::AddNode {
                change_id: node.change_id.clone(),
                node_type: node.node_type.clone(),
                title: node.title.clone(),
                description: node.description.clone(),
                status: node.status.clone(),
                metadata_json: node.metadata_json.clone(),
                timestamp: chrono::Utc::now(),
                author,
            };
            if let Err(e) = event_log.append(event) {
                eprintln!("{} Sync event: {}", "Warning:".yellow(), e);
            }
        }
    }
}

/// Emit a sync event for a newly created edge, if sync is initialized.
pub fn maybe_emit_add_edge(
    from_change_id: &str,
    to_change_id: &str,
    edge_type: &str,
    rationale: Option<&str>,
) {
    let sync_dir = std::path::PathBuf::from(".deciduous/sync");
    if sync_dir.exists() {
        let author = get_current_author();
        if let Ok(event_log) = EventLog::new(&std::path::PathBuf::from(".deciduous"), author.clone()) {
            let event = Event::AddEdge {
                edge_id: generate_edge_id(from_change_id, to_change_id, edge_type),
                from_change_id: from_change_id.to_string(),
                to_change_id: to_change_id.to_string(),
                edge_type: edge_type.to_string(),
                rationale: rationale.map(|s| s.to_string()),
                timestamp: chrono::Utc::now(),
                author,
            };
            if let Err(e) = event_log.append(event) {
                eprintln!("{} Sync event: {}", "Warning:".yellow(), e);
            }
        }
    }
}

/// Emit a sync event for a node status update, if sync is initialized.
pub fn maybe_emit_status_update(change_id: &str, new_status: &str) {
    let sync_dir = std::path::PathBuf::from(".deciduous/sync");
    if sync_dir.exists() {
        let author = get_current_author();
        if let Ok(event_log) = EventLog::new(&std::path::PathBuf::from(".deciduous"), author.clone()) {
            let event = Event::UpdateNode {
                change_id: change_id.to_string(),
                title: None,
                description: None,
                status: Some(new_status.to_string()),
                metadata_json: None,
                timestamp: chrono::Utc::now(),
                author,
            };
            if let Err(e) = event_log.append(event) {
                eprintln!("{} Sync event: {}", "Warning:".yellow(), e);
            }
        }
    }
}

/// Get the current user name for event authorship
pub fn get_current_author() -> String {
    // Try git config first
    if let Ok(output) = std::process::Command::new("git")
        .args(["config", "user.name"])
        .output()
    {
        if output.status.success() {
            if let Ok(name) = String::from_utf8(output.stdout) {
                let name = name.trim();
                if !name.is_empty() {
                    return name.to_string();
                }
            }
        }
    }

    // Fall back to system username
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "unknown".to_string())
}

/// Generate a unique edge ID from the edge components
pub fn generate_edge_id(from_change_id: &str, to_change_id: &str, edge_type: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    from_change_id.hash(&mut hasher);
    to_change_id.hash(&mut hasher);
    edge_type.hash(&mut hasher);
    format!("edge-{:016x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_add_node_event(change_id: &str, title: &str) -> Event {
        Event::AddNode {
            change_id: change_id.to_string(),
            node_type: "goal".to_string(),
            title: title.to_string(),
            description: None,
            status: "pending".to_string(),
            metadata_json: None,
            timestamp: Utc::now(),
            author: "test".to_string(),
        }
    }

    fn make_add_edge_event(from: &str, to: &str) -> Event {
        Event::AddEdge {
            edge_id: generate_edge_id(from, to, "leads_to"),
            from_change_id: from.to_string(),
            to_change_id: to.to_string(),
            edge_type: "leads_to".to_string(),
            rationale: None,
            timestamp: Utc::now(),
            author: "test".to_string(),
        }
    }

    #[test]
    fn test_event_serialization_roundtrip() {
        let event = make_add_node_event("abc-123", "Test Goal");
        let json = serde_json::to_string(&event).unwrap();
        let parsed: Event = serde_json::from_str(&json).unwrap();

        if let Event::AddNode { change_id, title, .. } = parsed {
            assert_eq!(change_id, "abc-123");
            assert_eq!(title, "Test Goal");
        } else {
            panic!("Wrong event type");
        }
    }

    #[test]
    fn test_materialized_state_apply_add_node() {
        let mut state = MaterializedState::default();
        let event = make_add_node_event("abc-123", "Test Goal");

        state.apply(&event);

        assert!(state.nodes.contains_key("abc-123"));
        assert_eq!(state.nodes.get("abc-123").unwrap().title, "Test Goal");
    }

    #[test]
    fn test_materialized_state_apply_update_node() {
        let mut state = MaterializedState::default();

        // First add a node
        state.apply(&make_add_node_event("abc-123", "Original Title"));

        // Then update it
        let update = Event::UpdateNode {
            change_id: "abc-123".to_string(),
            title: Some("Updated Title".to_string()),
            description: None,
            status: None,
            metadata_json: None,
            timestamp: Utc::now(),
            author: "test".to_string(),
        };
        state.apply(&update);

        assert_eq!(state.nodes.get("abc-123").unwrap().title, "Updated Title");
    }

    #[test]
    fn test_materialized_state_apply_delete_node() {
        let mut state = MaterializedState::default();

        state.apply(&make_add_node_event("abc-123", "Test Goal"));
        assert!(state.nodes.contains_key("abc-123"));

        let delete = Event::DeleteNode {
            change_id: "abc-123".to_string(),
            timestamp: Utc::now(),
            author: "test".to_string(),
        };
        state.apply(&delete);

        assert!(!state.nodes.contains_key("abc-123"));
        assert!(state.deleted_nodes.contains("abc-123"));
    }

    #[test]
    fn test_materialized_state_edges() {
        let mut state = MaterializedState::default();

        state.apply(&make_add_node_event("node-1", "Node 1"));
        state.apply(&make_add_node_event("node-2", "Node 2"));
        state.apply(&make_add_edge_event("node-1", "node-2"));

        assert_eq!(state.edges.len(), 1);
        let edge = state.edges.values().next().unwrap();
        assert_eq!(edge.from_change_id, "node-1");
        assert_eq!(edge.to_change_id, "node-2");
    }

    #[test]
    fn test_event_log_append_and_read() {
        let temp_dir = TempDir::new().unwrap();
        let log = EventLog::new(temp_dir.path(), "alice".to_string()).unwrap();

        let event = make_add_node_event("test-123", "Test Node");
        log.append(event).unwrap();

        let events = log.read_all_events().unwrap();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn test_event_log_multiple_users() {
        let temp_dir = TempDir::new().unwrap();

        // Alice writes an event
        let alice_log = EventLog::new(temp_dir.path(), "alice".to_string()).unwrap();
        alice_log.append(make_add_node_event("alice-node", "Alice's Node")).unwrap();

        // Bob writes an event
        let bob_log = EventLog::new(temp_dir.path(), "bob".to_string()).unwrap();
        bob_log.append(make_add_node_event("bob-node", "Bob's Node")).unwrap();

        // Reading should get both
        let events = alice_log.read_all_events().unwrap();
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn test_checkpoint_save_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let log = EventLog::new(temp_dir.path(), "test".to_string()).unwrap();

        let checkpoint = Checkpoint {
            created_at: Utc::now(),
            nodes: vec![CheckpointNode {
                change_id: "cp-node".to_string(),
                node_type: "goal".to_string(),
                title: "Checkpoint Node".to_string(),
                description: None,
                status: "pending".to_string(),
                metadata_json: None,
                created_at: Utc::now().to_rfc3339(),
                updated_at: Utc::now().to_rfc3339(),
            }],
            edges: vec![],
            version: "1.0".to_string(),
            themes: vec![],
            node_themes: vec![],
            documents: vec![],
        };

        log.save_checkpoint(&checkpoint, false).unwrap();

        let loaded = log.load_checkpoint().unwrap().unwrap();
        assert_eq!(loaded.nodes.len(), 1);
        assert_eq!(loaded.nodes[0].change_id, "cp-node");
    }

    #[test]
    fn test_last_writer_wins() {
        let mut state = MaterializedState::default();

        // Alice adds a node
        let alice_add = Event::AddNode {
            change_id: "shared-node".to_string(),
            node_type: "goal".to_string(),
            title: "Alice's Title".to_string(),
            description: None,
            status: "pending".to_string(),
            metadata_json: None,
            timestamp: Utc::now() - chrono::Duration::seconds(10),
            author: "alice".to_string(),
        };

        // Bob updates the same node later
        let bob_update = Event::UpdateNode {
            change_id: "shared-node".to_string(),
            title: Some("Bob's Title".to_string()),
            description: None,
            status: None,
            metadata_json: None,
            timestamp: Utc::now(),
            author: "bob".to_string(),
        };

        state.apply(&alice_add);
        state.apply(&bob_update);

        // Bob's update wins (later timestamp)
        assert_eq!(state.nodes.get("shared-node").unwrap().title, "Bob's Title");
    }

    #[test]
    fn test_edge_id_generation() {
        let id1 = generate_edge_id("node-a", "node-b", "leads_to");
        let id2 = generate_edge_id("node-a", "node-b", "leads_to");
        let id3 = generate_edge_id("node-a", "node-c", "leads_to");

        // Same inputs = same ID (deterministic)
        assert_eq!(id1, id2);
        // Different inputs = different ID
        assert_ne!(id1, id3);
    }
}
