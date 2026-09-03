//! Legacy event-log reader (pre-0.17 multi-user sync).
//!
//! Older versions synced through per-author JSONL files in
//! `.deciduous/sync/events/` plus a monolithic `checkpoint.json`. That
//! design is gone: the record store in [`crate::records`] replaced it. This
//! module only exists so `deciduous sync` can read those files once and
//! convert them into records. Nothing writes this format any more.
//!
//! The reader is deliberately forgiving. The old appender wrote the JSON
//! and its newline in separate syscalls, so two processes appending at once
//! could glue two objects onto one line. Every object on such a line is
//! recovered.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// All event types the old log could contain.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Event {
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
    UpdateNode {
        change_id: String,
        title: Option<String>,
        description: Option<String>,
        status: Option<String>,
        metadata_json: Option<String>,
        #[serde(with = "chrono::serde::ts_milliseconds")]
        timestamp: DateTime<Utc>,
        author: String,
    },
    DeleteNode {
        change_id: String,
        #[serde(with = "chrono::serde::ts_milliseconds")]
        timestamp: DateTime<Utc>,
        author: String,
    },
    AddEdge {
        edge_id: String,
        from_change_id: String,
        to_change_id: String,
        edge_type: String,
        rationale: Option<String>,
        #[serde(with = "chrono::serde::ts_milliseconds")]
        timestamp: DateTime<Utc>,
        author: String,
    },
    DeleteEdge {
        edge_id: String,
        #[serde(with = "chrono::serde::ts_milliseconds")]
        timestamp: DateTime<Utc>,
        author: String,
    },
    AddTheme {
        change_id: String,
        name: String,
        color: String,
        description: Option<String>,
        #[serde(with = "chrono::serde::ts_milliseconds")]
        timestamp: DateTime<Utc>,
        author: String,
    },
    DeleteTheme {
        change_id: String,
        #[serde(with = "chrono::serde::ts_milliseconds")]
        timestamp: DateTime<Utc>,
        author: String,
    },
    TagNode {
        node_change_id: String,
        theme_change_id: String,
        source: String,
        #[serde(with = "chrono::serde::ts_milliseconds")]
        timestamp: DateTime<Utc>,
        author: String,
    },
    UntagNode {
        node_change_id: String,
        theme_change_id: String,
        #[serde(with = "chrono::serde::ts_milliseconds")]
        timestamp: DateTime<Utc>,
        author: String,
    },
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
    DetachDocument {
        doc_change_id: String,
        #[serde(with = "chrono::serde::ts_milliseconds")]
        timestamp: DateTime<Utc>,
        author: String,
    },
}

impl Event {
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

/// The old full-state checkpoint file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub created_at: DateTime<Utc>,
    pub nodes: Vec<CheckpointNode>,
    pub edges: Vec<CheckpointEdge>,
    pub version: String,
    #[serde(default)]
    pub themes: Vec<CheckpointTheme>,
    #[serde(default)]
    pub node_themes: Vec<CheckpointNodeTheme>,
    #[serde(default)]
    pub documents: Vec<CheckpointDocument>,
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointEdge {
    pub edge_id: String,
    pub from_change_id: String,
    pub to_change_id: String,
    pub edge_type: String,
    pub rationale: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointTheme {
    pub change_id: String,
    pub name: String,
    pub color: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointNodeTheme {
    pub node_change_id: String,
    pub theme_change_id: String,
    pub source: String,
}

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

/// Load a checkpoint file if it exists.
pub fn read_checkpoint(path: &Path) -> Result<Option<Checkpoint>, String> {
    if !path.is_file() {
        return Ok(None);
    }
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&content)
        .map(Some)
        .map_err(|e| e.to_string())
}

/// Read every `*.jsonl` file in `dir`, recovering all objects on each line
/// even when several were glued together. Returns the events sorted by
/// timestamp, plus one message per line that could not be read at all.
pub fn read_events_tolerant(dir: &Path) -> (Vec<Event>, Vec<String>) {
    let mut events = Vec::new();
    let mut errors = Vec::new();

    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return (events, errors),
    };
    let mut paths: Vec<_> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|e| e == "jsonl").unwrap_or(false))
        .collect();
    paths.sort();

    for path in paths {
        let text = match fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                errors.push(format!("{}: {}", path.display(), e));
                continue;
            }
        };
        for (lineno, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let mut got_any = false;
            let mut stream = serde_json::Deserializer::from_str(line).into_iter::<Event>();
            loop {
                match stream.next() {
                    Some(Ok(ev)) => {
                        got_any = true;
                        events.push(ev);
                    }
                    Some(Err(e)) => {
                        errors.push(format!(
                            "{}:{}: {}{}",
                            path.display(),
                            lineno + 1,
                            e,
                            if got_any {
                                " (earlier objects on the line were recovered)"
                            } else {
                                ""
                            }
                        ));
                        break;
                    }
                    None => break,
                }
            }
        }
    }

    events.sort_by_key(|e| e.timestamp());
    (events, errors)
}

/// In-memory graph state produced by replaying events onto a checkpoint.
#[derive(Debug, Default)]
pub struct MaterializedState {
    pub nodes: HashMap<String, MaterializedNode>,
    pub edges: HashMap<String, MaterializedEdge>,
    /// Deleted nodes with their last known fields and deletion time.
    pub tombstoned_nodes: HashMap<String, (MaterializedNode, DateTime<Utc>)>,
    /// Deleted edges with their last known fields and deletion time.
    pub tombstoned_edges: HashMap<String, (MaterializedEdge, DateTime<Utc>)>,
}

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
    pub author: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MaterializedEdge {
    pub edge_id: String,
    pub from_change_id: String,
    pub to_change_id: String,
    pub edge_type: String,
    pub rationale: Option<String>,
    pub created_at: DateTime<Utc>,
    pub author: Option<String>,
}

impl MaterializedState {
    pub fn from_checkpoint(checkpoint: &Checkpoint) -> Self {
        let mut state = Self::default();
        let parse = |s: &str| crate::records::parse_ts(s);

        for node in &checkpoint.nodes {
            state.nodes.insert(
                node.change_id.clone(),
                MaterializedNode {
                    change_id: node.change_id.clone(),
                    node_type: node.node_type.clone(),
                    title: node.title.clone(),
                    description: node.description.clone(),
                    status: node.status.clone(),
                    metadata_json: node.metadata_json.clone(),
                    created_at: parse(&node.created_at),
                    updated_at: parse(&node.updated_at),
                    author: None,
                },
            );
        }

        for edge in &checkpoint.edges {
            state.edges.insert(
                edge.edge_id.clone(),
                MaterializedEdge {
                    edge_id: edge.edge_id.clone(),
                    from_change_id: edge.from_change_id.clone(),
                    to_change_id: edge.to_change_id.clone(),
                    edge_type: edge.edge_type.clone(),
                    rationale: edge.rationale.clone(),
                    created_at: parse(&edge.created_at),
                    author: None,
                },
            );
        }

        state
    }

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
                author,
            } => {
                self.tombstoned_nodes.remove(change_id);
                self.nodes.insert(
                    change_id.clone(),
                    MaterializedNode {
                        change_id: change_id.clone(),
                        node_type: node_type.clone(),
                        title: title.clone(),
                        description: description.clone(),
                        status: status.clone(),
                        metadata_json: metadata_json.clone(),
                        created_at: *timestamp,
                        updated_at: *timestamp,
                        author: Some(author.clone()),
                    },
                );
            }
            Event::UpdateNode {
                change_id,
                title,
                description,
                status,
                metadata_json,
                timestamp,
                author,
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
                    node.author = Some(author.clone());
                }
            }
            Event::DeleteNode {
                change_id,
                timestamp,
                author,
            } => {
                if let Some(mut node) = self.nodes.remove(change_id) {
                    node.author = Some(author.clone());
                    self.tombstoned_nodes
                        .insert(change_id.clone(), (node, *timestamp));
                }
            }
            Event::AddEdge {
                edge_id,
                from_change_id,
                to_change_id,
                edge_type,
                rationale,
                timestamp,
                author,
            } => {
                self.tombstoned_edges.remove(edge_id);
                self.edges.insert(
                    edge_id.clone(),
                    MaterializedEdge {
                        edge_id: edge_id.clone(),
                        from_change_id: from_change_id.clone(),
                        to_change_id: to_change_id.clone(),
                        edge_type: edge_type.clone(),
                        rationale: rationale.clone(),
                        created_at: *timestamp,
                        author: Some(author.clone()),
                    },
                );
            }
            Event::DeleteEdge {
                edge_id,
                timestamp,
                author,
            } => {
                if let Some(mut edge) = self.edges.remove(edge_id) {
                    edge.author = Some(author.clone());
                    self.tombstoned_edges
                        .insert(edge_id.clone(), (edge, *timestamp));
                }
            }
            // Themes and documents were declared but never emitted by any
            // released version, so there is nothing to carry over.
            Event::AddTheme { .. }
            | Event::DeleteTheme { .. }
            | Event::TagNode { .. }
            | Event::UntagNode { .. }
            | Event::AttachDocument { .. }
            | Event::DetachDocument { .. } => {}
        }
    }

    pub fn replay(&mut self, events: &[Event]) {
        for event in events {
            self.apply(event);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn add(change_id: &str, title: &str, ts: i64) -> String {
        format!(
            r#"{{"op":"add_node","change_id":"{change_id}","node_type":"goal","title":"{title}","description":null,"status":"pending","metadata_json":null,"timestamp":{ts},"author":"t"}}"#
        )
    }

    #[test]
    fn tolerant_reader_splits_glued_objects_and_sorts() {
        let dir = TempDir::new().unwrap();
        let glued = format!(
            "{}{}\n{}\n",
            add("b", "B", 20),
            add("c", "C", 30),
            add("a", "A", 10)
        );
        fs::write(dir.path().join("x.jsonl"), glued).unwrap();
        let (events, errors) = read_events_tolerant(dir.path());
        assert!(errors.is_empty());
        let ids: Vec<_> = events
            .iter()
            .map(|e| match e {
                Event::AddNode { change_id, .. } => change_id.as_str(),
                _ => "?",
            })
            .collect();
        assert_eq!(ids, ["a", "b", "c"]);
    }

    #[test]
    fn tolerant_reader_reports_broken_lines_and_keeps_going() {
        let dir = TempDir::new().unwrap();
        let text = format!("{}\n{{broken\n{}\n", add("a", "A", 1), add("b", "B", 2));
        fs::write(dir.path().join("x.jsonl"), text).unwrap();
        let (events, errors) = read_events_tolerant(dir.path());
        assert_eq!(events.len(), 2);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains(":2:"));
    }

    #[test]
    fn delete_keeps_last_known_fields_as_tombstone() {
        let mut state = MaterializedState::default();
        let (events, _) = {
            let dir = TempDir::new().unwrap();
            fs::write(
                dir.path().join("x.jsonl"),
                format!(
                    "{}\n{{\"op\":\"delete_node\",\"change_id\":\"a\",\"timestamp\":5,\"author\":\"z\"}}\n",
                    add("a", "A", 1)
                ),
            )
            .unwrap();
            read_events_tolerant(dir.path())
        };
        state.replay(&events);
        assert!(state.nodes.is_empty());
        let (node, when) = &state.tombstoned_nodes["a"];
        assert_eq!(node.title, "A");
        assert_eq!(node.author.as_deref(), Some("z"));
        assert_eq!(when.timestamp_millis(), 5);
    }

    #[test]
    fn last_writer_wins_on_update() {
        let mut state = MaterializedState::default();
        state.apply(&Event::AddNode {
            change_id: "n".into(),
            node_type: "goal".into(),
            title: "old".into(),
            description: None,
            status: "pending".into(),
            metadata_json: None,
            timestamp: Utc::now() - chrono::Duration::seconds(10),
            author: "alice".into(),
        });
        state.apply(&Event::UpdateNode {
            change_id: "n".into(),
            title: Some("new".into()),
            description: None,
            status: None,
            metadata_json: None,
            timestamp: Utc::now(),
            author: "bob".into(),
        });
        let n = &state.nodes["n"];
        assert_eq!(n.title, "new");
        assert_eq!(n.author.as_deref(), Some("bob"));
    }
}
