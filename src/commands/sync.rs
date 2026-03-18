use chrono::Utc;
use clap::Subcommand;
use colored::Colorize;
use deciduous::{
    generate_edge_id, get_current_author, Checkpoint, CheckpointEdge, CheckpointNode, Database,
    Event, EventLog, MaterializedState,
};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

#[derive(Subcommand, Debug)]
pub enum EventsAction {
    /// Rebuild local database from event logs and checkpoint
    ///
    /// Loads checkpoint (if exists), then replays all events after the checkpoint.
    /// This reconstructs the database from the shared event history.
    Rebuild {
        /// Only show what would be done without modifying the database
        #[arg(long)]
        dry_run: bool,
    },

    /// Create a checkpoint and optionally clear old events
    ///
    /// Checkpoints capture full graph state. Events older than the checkpoint
    /// can be safely deleted to keep the repo size bounded.
    Checkpoint {
        /// Clear event logs after creating checkpoint
        #[arg(long)]
        clear_events: bool,
    },

    /// Show sync status (pending events, last checkpoint, etc.)
    Status,

    /// Initialize event-based sync in this repository
    ///
    /// Creates .deciduous/sync/ directory structure and adds to .gitignore
    /// the local database while tracking the sync directory.
    Init,

    /// Emit an event for a node (for testing/manual sync)
    Emit {
        /// Node ID to emit event for
        node_id: i32,
    },
}

pub fn handle_events(db: &Database, action: EventsAction) {
    // Get the .deciduous directory
    let deciduous_dir = PathBuf::from(".deciduous");
    if !deciduous_dir.exists() {
        eprintln!(
            "{} No .deciduous directory found. Run 'deciduous init' first.",
            "Error:".red()
        );
        std::process::exit(1);
    }

    let author = get_current_author();

    match action {
        EventsAction::Init => {
            let sync_dir = deciduous_dir.join("sync");
            let events_dir = sync_dir.join("events");

            if sync_dir.exists() {
                println!(
                    "{} Sync directory already exists at {}",
                    "Info:".cyan(),
                    sync_dir.display()
                );
            } else {
                match std::fs::create_dir_all(&events_dir) {
                    Ok(()) => {
                        println!(
                            "{} Created sync directory at {}",
                            "Success:".green(),
                            sync_dir.display()
                        );
                        println!("  Events will be stored in {}", events_dir.display());
                        println!();
                        println!("{}", "Next steps:".cyan());
                        println!("  1. Add .deciduous/sync/ to git tracking");
                        println!(
                            "  2. Team members pull and run 'deciduous events rebuild'"
                        );
                    }
                    Err(e) => {
                        eprintln!("{} Creating sync directory: {}", "Error:".red(), e);
                        std::process::exit(1);
                    }
                }
            }
        }

        EventsAction::Status => {
            match EventLog::new(&deciduous_dir, author.clone()) {
                Ok(event_log) => {
                    println!("{} Event-based sync status", "Sync:".cyan());
                    println!("  Author: {}", author);
                    println!("  Sync dir: {}", event_log.sync_dir().display());

                    // Check for checkpoint
                    match event_log.load_checkpoint() {
                        Ok(Some(cp)) => {
                            println!(
                                "  Checkpoint: {} ({} nodes, {} edges)",
                                cp.created_at.format("%Y-%m-%d %H:%M:%S"),
                                cp.nodes.len(),
                                cp.edges.len()
                            );
                        }
                        Ok(None) => {
                            println!("  Checkpoint: none");
                        }
                        Err(e) => {
                            println!("  Checkpoint: error loading ({})", e);
                        }
                    }

                    // Count events
                    match event_log.get_events_after_checkpoint() {
                        Ok(events) => {
                            println!("  Pending events: {}", events.len());

                            // Count by author
                            let mut by_author: HashMap<String, usize> =
                                HashMap::new();
                            for event in &events {
                                *by_author
                                    .entry(event.author().to_string())
                                    .or_default() += 1;
                            }
                            if !by_author.is_empty() {
                                println!("  Events by author:");
                                for (a, count) in by_author {
                                    println!("    {}: {}", a, count);
                                }
                            }
                        }
                        Err(e) => {
                            println!("  Pending events: error ({})", e);
                        }
                    }

                    // List event files
                    let events_dir = event_log.sync_dir().join("events");
                    if events_dir.exists() {
                        println!("  Event files:");
                        if let Ok(entries) = std::fs::read_dir(&events_dir) {
                            for entry in entries.flatten() {
                                let path = entry.path();
                                if path.extension().map(|e| e == "jsonl").unwrap_or(false) {
                                    let size = std::fs::metadata(&path)
                                        .map(|m| m.len())
                                        .unwrap_or(0);
                                    println!(
                                        "    {} ({} bytes)",
                                        path.file_name()
                                            .unwrap_or_default()
                                            .to_string_lossy(),
                                        size
                                    );
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("{} {}", "Error:".red(), e);
                    std::process::exit(1);
                }
            }
        }

        EventsAction::Rebuild { dry_run } => {
            match EventLog::new(&deciduous_dir, author) {
                Ok(event_log) => {
                    println!("{} Rebuilding database from events...", "Sync:".cyan());

                    // Load checkpoint
                    let checkpoint = match event_log.load_checkpoint() {
                        Ok(cp) => cp,
                        Err(e) => {
                            eprintln!("{} Loading checkpoint: {}", "Error:".red(), e);
                            std::process::exit(1);
                        }
                    };

                    // Build materialized state
                    let mut state = match &checkpoint {
                        Some(cp) => {
                            println!(
                                "  Loaded checkpoint: {} nodes, {} edges",
                                cp.nodes.len(),
                                cp.edges.len()
                            );
                            MaterializedState::from_checkpoint(cp)
                        }
                        None => {
                            println!("  No checkpoint found, starting fresh");
                            MaterializedState::default()
                        }
                    };

                    // Get events after checkpoint
                    let events = match event_log.get_events_after_checkpoint() {
                        Ok(e) => e,
                        Err(e) => {
                            eprintln!("{} Reading events: {}", "Error:".red(), e);
                            std::process::exit(1);
                        }
                    };

                    println!("  Replaying {} events...", events.len());
                    state.replay(&events);

                    println!(
                        "  Result: {} nodes, {} edges",
                        state.nodes.len(),
                        state.edges.len()
                    );

                    if dry_run {
                        println!();
                        println!(
                            "{} Dry run - no changes made to database",
                            "Info:".cyan()
                        );
                    } else {
                        // Apply to database
                        // For now, we'll use the existing patch apply mechanism
                        // by creating nodes with specific change_ids
                        let mut nodes_created = 0;
                        let mut nodes_skipped = 0;
                        let mut edges_created = 0;
                        let mut edges_failed = 0;

                        // Get existing nodes
                        let existing_nodes = db.get_all_nodes().unwrap_or_default();
                        let existing_change_ids: HashSet<String> =
                            existing_nodes.iter().map(|n| n.change_id.clone()).collect();

                        // Create nodes
                        for node in state.nodes.values() {
                            if existing_change_ids.contains(&node.change_id) {
                                nodes_skipped += 1;
                                continue;
                            }

                            // Parse metadata from the stored JSON
                            let meta = node.metadata_json.as_ref().and_then(|m| {
                                serde_json::from_str::<serde_json::Value>(m).ok()
                            });

                            let confidence = meta
                                .as_ref()
                                .and_then(|m| m.get("confidence"))
                                .and_then(|c| c.as_u64())
                                .map(|c| c as u8);
                            let commit = meta
                                .as_ref()
                                .and_then(|m| m.get("commit"))
                                .and_then(|c| c.as_str());
                            let prompt = meta
                                .as_ref()
                                .and_then(|m| m.get("prompt"))
                                .and_then(|p| p.as_str());
                            let files =
                                meta.as_ref().and_then(|m| m.get("files")).and_then(|f| {
                                    f.as_array().map(|arr| {
                                        arr.iter()
                                            .filter_map(|v| v.as_str())
                                            .collect::<Vec<_>>()
                                            .join(",")
                                    })
                                });
                            let branch = meta
                                .as_ref()
                                .and_then(|m| m.get("branch"))
                                .and_then(|b| b.as_str());

                            match db.create_node_with_change_id(
                                &node.change_id,
                                &node.node_type,
                                &node.title,
                                node.description.as_deref(),
                                confidence,
                                commit,
                                prompt,
                                files.as_deref(),
                                branch,
                            ) {
                                Ok(_) => nodes_created += 1,
                                Err(e) => {
                                    eprintln!(
                                        "  Warning: Failed to create node {}: {}",
                                        node.change_id, e
                                    );
                                }
                            }
                        }

                        // Refresh node list to get local IDs
                        let all_nodes = db.get_all_nodes().unwrap_or_default();
                        let change_id_to_local_id: HashMap<String, i32> =
                            all_nodes
                                .iter()
                                .map(|n| (n.change_id.clone(), n.id))
                                .collect();

                        // Get existing edges
                        let existing_edges = db.get_all_edges().unwrap_or_default();
                        let existing_edge_keys: HashSet<(
                            String,
                            String,
                            String,
                        )> = existing_edges
                            .iter()
                            .filter_map(|e| match (&e.from_change_id, &e.to_change_id) {
                                (Some(from), Some(to)) => {
                                    Some((from.clone(), to.clone(), e.edge_type.clone()))
                                }
                                _ => None,
                            })
                            .collect();

                        // Create edges
                        for edge in state.edges.values() {
                            let edge_key = (
                                edge.from_change_id.clone(),
                                edge.to_change_id.clone(),
                                edge.edge_type.clone(),
                            );

                            if existing_edge_keys.contains(&edge_key) {
                                continue;
                            }

                            let from_id = change_id_to_local_id.get(&edge.from_change_id);
                            let to_id = change_id_to_local_id.get(&edge.to_change_id);

                            match (from_id, to_id) {
                                (Some(&from), Some(&to)) => {
                                    match db.create_edge(
                                        from,
                                        to,
                                        &edge.edge_type,
                                        edge.rationale.as_deref(),
                                    ) {
                                        Ok(_) => edges_created += 1,
                                        Err(e) => {
                                            eprintln!(
                                                "  Warning: Failed to create edge: {}",
                                                e
                                            );
                                            edges_failed += 1;
                                        }
                                    }
                                }
                                _ => {
                                    edges_failed += 1;
                                }
                            }
                        }

                        println!();
                        println!(
                            "{} Created {} nodes ({} skipped), {} edges ({} failed)",
                            "Done:".green(),
                            nodes_created,
                            nodes_skipped,
                            edges_created,
                            edges_failed
                        );
                    }
                }
                Err(e) => {
                    eprintln!("{} {}", "Error:".red(), e);
                    std::process::exit(1);
                }
            }
        }

        EventsAction::Checkpoint { clear_events } => {
            match EventLog::new(&deciduous_dir, author) {
                Ok(event_log) => {
                    // Get current database state
                    let nodes = match db.get_all_nodes() {
                        Ok(n) => n,
                        Err(e) => {
                            eprintln!("{} Getting nodes: {}", "Error:".red(), e);
                            std::process::exit(1);
                        }
                    };

                    let edges = match db.get_all_edges() {
                        Ok(e) => e,
                        Err(e) => {
                            eprintln!("{} Getting edges: {}", "Error:".red(), e);
                            std::process::exit(1);
                        }
                    };

                    // Convert to checkpoint format
                    let checkpoint = Checkpoint {
                        created_at: Utc::now(),
                        nodes: nodes
                            .iter()
                            .map(|n| CheckpointNode {
                                change_id: n.change_id.clone(),
                                node_type: n.node_type.clone(),
                                title: n.title.clone(),
                                description: n.description.clone(),
                                status: n.status.clone(),
                                metadata_json: n.metadata_json.clone(),
                                created_at: n.created_at.clone(),
                                updated_at: n.updated_at.clone(),
                            })
                            .collect(),
                        edges: edges
                            .iter()
                            .filter_map(|e| match (&e.from_change_id, &e.to_change_id) {
                                (Some(from), Some(to)) => Some(CheckpointEdge {
                                    edge_id: generate_edge_id(from, to, &e.edge_type),
                                    from_change_id: from.clone(),
                                    to_change_id: to.clone(),
                                    edge_type: e.edge_type.clone(),
                                    rationale: e.rationale.clone(),
                                    created_at: e.created_at.clone(),
                                }),
                                _ => None,
                            })
                            .collect(),
                        version: "1.0".to_string(),
                        themes: vec![],
                        node_themes: vec![],
                        documents: vec![],
                    };

                    match event_log.save_checkpoint(&checkpoint, clear_events) {
                        Ok(()) => {
                            println!(
                                "{} Checkpoint created: {} nodes, {} edges",
                                "Success:".green(),
                                checkpoint.nodes.len(),
                                checkpoint.edges.len()
                            );
                            if clear_events {
                                println!("  Event logs cleared");
                            }
                        }
                        Err(e) => {
                            eprintln!("{} Saving checkpoint: {}", "Error:".red(), e);
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("{} {}", "Error:".red(), e);
                    std::process::exit(1);
                }
            }
        }

        EventsAction::Emit { node_id } => {
            // Get the node
            let node = match db.get_node(node_id) {
                Ok(Some(n)) => n,
                Ok(None) => {
                    eprintln!("{} Node {} not found", "Error:".red(), node_id);
                    std::process::exit(1);
                }
                Err(e) => {
                    eprintln!("{} {}", "Error:".red(), e);
                    std::process::exit(1);
                }
            };

            match EventLog::new(&deciduous_dir, author.clone()) {
                Ok(event_log) => {
                    let event = Event::AddNode {
                        change_id: node.change_id.clone(),
                        node_type: node.node_type.clone(),
                        title: node.title.clone(),
                        description: node.description.clone(),
                        status: node.status.clone(),
                        metadata_json: node.metadata_json.clone(),
                        timestamp: Utc::now(),
                        author,
                    };

                    match event_log.append(event) {
                        Ok(()) => {
                            println!(
                                "{} Emitted event for node {} ({})",
                                "Success:".green(),
                                node_id,
                                node.change_id
                            );
                        }
                        Err(e) => {
                            eprintln!("{} {}", "Error:".red(), e);
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("{} {}", "Error:".red(), e);
                    std::process::exit(1);
                }
            }
        }
    }
}
