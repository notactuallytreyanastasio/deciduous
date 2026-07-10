//! MCP tool registry — definitions and dispatch.
//!
//! **Functional core**: tool definitions are pure data, dispatch is a pure
//! function mapping tool names to handler functions. No IO in this module.

use crate::mcp::protocol::ToolDefinition;
use serde_json::{json, Value};

/// Build the complete list of tool definitions for `tools/list`.
///
/// Each tool has a name, description, and JSON Schema for its parameters.
/// This is a pure function — returns fresh data each call.
pub fn all_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        // -----------------------------------------------------------------
        // Graph CRUD
        // -----------------------------------------------------------------
        ToolDefinition {
            name: "add_node".to_string(),
            description: "Add a new node to the decision graph. Node types: goal, decision, option, action, outcome, observation, revisit.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "node_type": {
                        "type": "string",
                        "description": "Node type: goal, decision, option, action, outcome, observation, or revisit",
                        "enum": ["goal", "decision", "option", "action", "outcome", "observation", "revisit"]
                    },
                    "title": {
                        "type": "string",
                        "description": "Title of the node"
                    },
                    "description": {
                        "type": "string",
                        "description": "Optional description with more detail"
                    },
                    "confidence": {
                        "type": "integer",
                        "description": "Confidence level 0-100",
                        "minimum": 0,
                        "maximum": 100
                    },
                    "prompt": {
                        "type": "string",
                        "description": "The user prompt that triggered this node (capture verbatim for context recovery)"
                    },
                    "files": {
                        "type": "string",
                        "description": "Comma-separated list of associated files"
                    },
                    "branch": {
                        "type": "string",
                        "description": "Git branch (auto-detected if omitted)"
                    },
                    "commit": {
                        "type": "string",
                        "description": "Git commit hash to link. Use 'HEAD' to auto-detect current commit."
                    }
                },
                "required": ["node_type", "title"]
            }),
        },

        ToolDefinition {
            name: "link_nodes".to_string(),
            description: "Create an edge between two nodes in the decision graph.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "from_id": {
                        "type": "integer",
                        "description": "Source node ID"
                    },
                    "to_id": {
                        "type": "integer",
                        "description": "Target node ID"
                    },
                    "rationale": {
                        "type": "string",
                        "description": "Reason for this connection"
                    },
                    "edge_type": {
                        "type": "string",
                        "description": "Edge type (default: leads_to)",
                        "enum": ["leads_to", "requires", "chosen", "rejected", "blocks", "enables"],
                        "default": "leads_to"
                    }
                },
                "required": ["from_id", "to_id"]
            }),
        },

        ToolDefinition {
            name: "unlink_nodes".to_string(),
            description: "Remove an edge between two nodes.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "from_id": {
                        "type": "integer",
                        "description": "Source node ID"
                    },
                    "to_id": {
                        "type": "integer",
                        "description": "Target node ID"
                    }
                },
                "required": ["from_id", "to_id"]
            }),
        },

        ToolDefinition {
            name: "delete_node".to_string(),
            description: "Delete a node and all its connected edges. Use dry_run to preview.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "node_id": {
                        "type": "integer",
                        "description": "ID of the node to delete"
                    },
                    "dry_run": {
                        "type": "boolean",
                        "description": "If true, show what would be deleted without actually deleting",
                        "default": false
                    }
                },
                "required": ["node_id"]
            }),
        },

        ToolDefinition {
            name: "update_status".to_string(),
            description: "Update the status of a node (active, superseded, abandoned, pending, completed, rejected).".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "node_id": {
                        "type": "integer",
                        "description": "Node ID to update"
                    },
                    "status": {
                        "type": "string",
                        "description": "New status",
                        "enum": ["pending", "active", "completed", "rejected", "superseded", "abandoned"]
                    }
                },
                "required": ["node_id", "status"]
            }),
        },

        ToolDefinition {
            name: "update_prompt".to_string(),
            description: "Add or update the verbatim user prompt stored on a node.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "node_id": {
                        "type": "integer",
                        "description": "Node ID to update"
                    },
                    "prompt": {
                        "type": "string",
                        "description": "The verbatim user prompt text"
                    }
                },
                "required": ["node_id", "prompt"]
            }),
        },

        // -----------------------------------------------------------------
        // Graph Querying
        // -----------------------------------------------------------------
        ToolDefinition {
            name: "list_nodes".to_string(),
            description: "List all nodes in the decision graph with optional filters for branch, type, theme, and status.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "branch": {
                        "type": "string",
                        "description": "Filter by git branch"
                    },
                    "node_type": {
                        "type": "string",
                        "description": "Filter by type: goal, decision, option, action, outcome, observation, revisit"
                    },
                    "status": {
                        "type": "string",
                        "description": "Filter by status: active, pending, completed, superseded, abandoned"
                    },
                    "theme": {
                        "type": "string",
                        "description": "Filter by theme name"
                    }
                }
            }),
        },

        ToolDefinition {
            name: "list_edges".to_string(),
            description: "List all edges in the decision graph.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },

        ToolDefinition {
            name: "show_node".to_string(),
            description: "Show detailed information about a single node including its connections, themes, and documents.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "node_id": {
                        "type": "integer",
                        "description": "Node ID to display"
                    }
                },
                "required": ["node_id"]
            }),
        },

        ToolDefinition {
            name: "get_graph".to_string(),
            description: "Export the full decision graph as JSON (all nodes and edges).".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },

        ToolDefinition {
            name: "search_nodes".to_string(),
            description: "Search nodes by text across titles, descriptions, and prompts. Returns matching nodes with context.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query (matched against title, description, and prompt)"
                    },
                    "node_type": {
                        "type": "string",
                        "description": "Optional: filter results by node type"
                    },
                    "branch": {
                        "type": "string",
                        "description": "Optional: filter results by branch"
                    }
                },
                "required": ["query"]
            }),
        },

        // -----------------------------------------------------------------
        // Analysis / Reporting
        // -----------------------------------------------------------------
        ToolDefinition {
            name: "trace_chain".to_string(),
            description: "Trace the full chain of connected nodes from a starting node using BFS. Returns the complete connected subgraph — follow all edges in both directions to find the entire decision tree.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "node_id": {
                        "type": "integer",
                        "description": "Starting node ID for the trace"
                    },
                    "max_depth": {
                        "type": "integer",
                        "description": "Maximum traversal depth (0 = unlimited)",
                        "default": 0
                    },
                    "direction": {
                        "type": "string",
                        "description": "Traversal direction: both, outgoing, incoming",
                        "enum": ["both", "outgoing", "incoming"],
                        "default": "both"
                    }
                },
                "required": ["node_id"]
            }),
        },

        ToolDefinition {
            name: "get_node_context".to_string(),
            description: "Get the full context around a node: the node itself, its parents (incoming edges), children (outgoing edges), sibling nodes, associated themes, and documents. Ideal for understanding a single decision point.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "node_id": {
                        "type": "integer",
                        "description": "Node ID to get context for"
                    }
                },
                "required": ["node_id"]
            }),
        },

        ToolDefinition {
            name: "get_timeline".to_string(),
            description: "Get a chronological timeline of nodes, newest first. Useful for understanding what happened and when.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of nodes to return (0 = all)",
                        "default": 50
                    },
                    "node_type": {
                        "type": "string",
                        "description": "Filter by node type"
                    },
                    "branch": {
                        "type": "string",
                        "description": "Filter by git branch"
                    },
                    "since": {
                        "type": "string",
                        "description": "Only show nodes created after this date (YYYY-MM-DD)"
                    }
                }
            }),
        },

        ToolDefinition {
            name: "get_pulse".to_string(),
            description: "Get the health and status of the decision graph: summary statistics, active goals, recent activity, orphaned nodes, and connection gaps.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "branch": {
                        "type": "string",
                        "description": "Filter by git branch"
                    }
                }
            }),
        },

        ToolDefinition {
            name: "find_orphans".to_string(),
            description: "Find nodes that violate connection rules: outcomes without parent actions, actions without parent decisions, decisions without parent options, etc. Root goals are excluded (they are valid orphans).".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },

        ToolDefinition {
            name: "get_branch_summary".to_string(),
            description: "Get a summary of all decisions and activity on a specific git branch. Shows goals, decisions made, actions taken, and outcomes.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "branch": {
                        "type": "string",
                        "description": "Git branch name to summarize"
                    }
                },
                "required": ["branch"]
            }),
        },

        // -----------------------------------------------------------------
        // Documents
        // -----------------------------------------------------------------
        ToolDefinition {
            name: "attach_document".to_string(),
            description: "Attach a file to a decision graph node.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "node_id": {
                        "type": "integer",
                        "description": "Node ID to attach the file to"
                    },
                    "file_path": {
                        "type": "string",
                        "description": "Path to the file to attach"
                    },
                    "description": {
                        "type": "string",
                        "description": "Description of the document"
                    }
                },
                "required": ["node_id", "file_path"]
            }),
        },

        ToolDefinition {
            name: "list_documents".to_string(),
            description: "List documents attached to a specific node, or all documents if no node ID given.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "node_id": {
                        "type": "integer",
                        "description": "Node ID to list documents for (omit for all)"
                    }
                }
            }),
        },

        // -----------------------------------------------------------------
        // Themes
        // -----------------------------------------------------------------
        ToolDefinition {
            name: "list_themes".to_string(),
            description: "List all defined themes.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },

        ToolDefinition {
            name: "create_theme".to_string(),
            description: "Create a new theme for tagging nodes.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Theme name (lowercase, dash-separated)"
                    },
                    "color": {
                        "type": "string",
                        "description": "Hex color code (e.g., '#ef4444')",
                        "default": "#6b7280"
                    },
                    "description": {
                        "type": "string",
                        "description": "Theme description"
                    }
                },
                "required": ["name"]
            }),
        },

        ToolDefinition {
            name: "tag_node".to_string(),
            description: "Add a theme tag to a node.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "node_id": {
                        "type": "integer",
                        "description": "Node ID to tag"
                    },
                    "theme": {
                        "type": "string",
                        "description": "Theme name to apply"
                    }
                },
                "required": ["node_id", "theme"]
            }),
        },

        ToolDefinition {
            name: "untag_node".to_string(),
            description: "Remove a theme tag from a node.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "node_id": {
                        "type": "integer",
                        "description": "Node ID to untag"
                    },
                    "theme": {
                        "type": "string",
                        "description": "Theme name to remove"
                    }
                },
                "required": ["node_id", "theme"]
            }),
        },

        // -----------------------------------------------------------------
        // Export
        // -----------------------------------------------------------------
        ToolDefinition {
            name: "export_dot".to_string(),
            description: "Export the decision graph (or a subset) as DOT format for visualization.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "roots": {
                        "type": "string",
                        "description": "Root node IDs (comma-separated) — traverses children"
                    },
                    "nodes": {
                        "type": "string",
                        "description": "Specific node IDs or ranges (e.g., '1-11' or '1,3,5-10')"
                    },
                    "title": {
                        "type": "string",
                        "description": "Graph title"
                    },
                    "rankdir": {
                        "type": "string",
                        "description": "Graph direction: TB (top-bottom) or LR (left-right)",
                        "enum": ["TB", "LR"],
                        "default": "TB"
                    }
                }
            }),
        },

        ToolDefinition {
            name: "generate_writeup".to_string(),
            description: "Generate a PR writeup from the decision graph. Returns markdown text summarizing goals, decisions, and outcomes.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "title": {
                        "type": "string",
                        "description": "PR title"
                    },
                    "roots": {
                        "type": "string",
                        "description": "Root node IDs (comma-separated)"
                    },
                    "nodes": {
                        "type": "string",
                        "description": "Specific node IDs or ranges"
                    },
                    "no_dot": {
                        "type": "boolean",
                        "description": "Skip DOT graph section",
                        "default": false
                    },
                    "no_test_plan": {
                        "type": "boolean",
                        "description": "Skip test plan section",
                        "default": false
                    }
                }
            }),
        },

        // -----------------------------------------------------------------
        // Sessions (conversation-scoped trees)
        // -----------------------------------------------------------------
        ToolDefinition {
            name: "start_session".to_string(),
            description: "Start a new conversation session. Creates a root goal node for this conversation and returns a session ID. All nodes created during this session are automatically associated with it, giving each conversation its own decision tree.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Session name (e.g., 'cowork: auth refactor' or 'chat: debugging memory leak')"
                    },
                    "goal_title": {
                        "type": "string",
                        "description": "Title for the root goal node of this session"
                    },
                    "goal_prompt": {
                        "type": "string",
                        "description": "The initial user prompt/request that started this conversation (verbatim)"
                    }
                },
                "required": ["name", "goal_title"]
            }),
        },

        ToolDefinition {
            name: "end_session".to_string(),
            description: "End the current conversation session. Marks it as complete with an optional summary.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "summary": {
                        "type": "string",
                        "description": "Brief summary of what was accomplished in this session"
                    }
                }
            }),
        },

        ToolDefinition {
            name: "get_session".to_string(),
            description: "Get details about the current session or a specific session, including all its nodes.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "session_id": {
                        "type": "integer",
                        "description": "Session ID to query (omit for current session)"
                    }
                }
            }),
        },

        ToolDefinition {
            name: "resume_session".to_string(),
            description: "Resume a previously started session by ID. Use this to reconnect to a long-running conversation's decision tree after a break or server restart.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "session_id": {
                        "type": "integer",
                        "description": "Session ID to resume"
                    }
                },
                "required": ["session_id"]
            }),
        },

        ToolDefinition {
            name: "list_sessions".to_string(),
            description: "List all conversation sessions (decision trees), optionally filtering to only active ones.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "active_only": {
                        "type": "boolean",
                        "description": "If true, only show sessions that haven't ended",
                        "default": false
                    }
                }
            }),
        },

        ToolDefinition {
            name: "events_status".to_string(),
            description: "Show the status of event-based multi-user sync: pending events, last checkpoint, teammate activity.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
    ]
}

/// Look up a tool definition by name. Returns None if not found.
pub fn find_tool(name: &str) -> Option<ToolDefinition> {
    all_tool_definitions().into_iter().find(|t| t.name == name)
}

/// Get all tool names as a sorted list.
pub fn tool_names() -> Vec<String> {
    let mut names: Vec<String> = all_tool_definitions().into_iter().map(|t| t.name).collect();
    names.sort();
    names
}

/// Validate that a tool name exists in the registry.
pub fn is_valid_tool(name: &str) -> bool {
    all_tool_definitions().iter().any(|t| t.name == name)
}

/// Validate tool arguments against the tool's input schema.
/// Returns Ok(()) if valid, Err(message) if invalid.
pub fn validate_tool_args(tool_name: &str, args: &Value) -> Result<(), String> {
    let tool = find_tool(tool_name).ok_or_else(|| format!("Unknown tool: {tool_name}"))?;

    let schema = &tool.input_schema;

    // Check required fields
    if let Some(required) = schema.get("required").and_then(Value::as_array) {
        for field in required {
            if let Some(field_name) = field.as_str() {
                let empty = serde_json::Map::new();
                let args_obj = args.as_object().unwrap_or(&empty);
                if !args_obj.contains_key(field_name) {
                    return Err(format!("Missing required parameter: {field_name}"));
                }
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_tools_have_valid_schemas() {
        let tools = all_tool_definitions();
        for tool in &tools {
            assert!(!tool.name.is_empty(), "Tool name must not be empty");
            assert!(
                !tool.description.is_empty(),
                "Tool {} must have a description",
                tool.name
            );
            assert!(
                tool.input_schema.is_object(),
                "Tool {} input_schema must be an object",
                tool.name
            );
            assert_eq!(
                tool.input_schema["type"], "object",
                "Tool {} input_schema type must be 'object'",
                tool.name
            );
        }
    }

    #[test]
    fn test_no_duplicate_tool_names() {
        let tools = all_tool_definitions();
        let mut seen = std::collections::HashSet::new();
        for tool in &tools {
            assert!(
                seen.insert(&tool.name),
                "Duplicate tool name: {}",
                tool.name
            );
        }
    }

    #[test]
    fn test_tool_count() {
        let tools = all_tool_definitions();
        // 6 CRUD + 5 Query + 6 Analysis + 2 Docs + 4 Themes + 5 Sessions + 3 Export = 31
        assert_eq!(tools.len(), 31, "Expected 31 tools, got {}", tools.len());
    }

    #[test]
    fn test_find_tool_exists() {
        assert!(find_tool("add_node").is_some());
        assert!(find_tool("link_nodes").is_some());
        assert!(find_tool("trace_chain").is_some());
        assert!(find_tool("get_pulse").is_some());
    }

    #[test]
    fn test_find_tool_not_exists() {
        assert!(find_tool("nonexistent_tool").is_none());
        assert!(find_tool("").is_none());
    }

    #[test]
    fn test_is_valid_tool() {
        assert!(is_valid_tool("add_node"));
        assert!(is_valid_tool("list_nodes"));
        assert!(!is_valid_tool("fake_tool"));
    }

    #[test]
    fn test_tool_names_sorted() {
        let names = tool_names();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted, "tool_names() must return sorted names");
    }

    #[test]
    fn test_validate_tool_args_valid() {
        let args = serde_json::json!({"node_type": "goal", "title": "Test"});
        assert!(validate_tool_args("add_node", &args).is_ok());
    }

    #[test]
    fn test_validate_tool_args_missing_required() {
        // add_node requires node_type and title
        let args = serde_json::json!({"node_type": "goal"});
        let result = validate_tool_args("add_node", &args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("title"));
    }

    #[test]
    fn test_validate_tool_args_no_required_fields() {
        // list_nodes has no required fields
        let args = serde_json::json!({});
        assert!(validate_tool_args("list_nodes", &args).is_ok());
    }

    #[test]
    fn test_validate_tool_args_unknown_tool() {
        let args = serde_json::json!({});
        let result = validate_tool_args("fake_tool", &args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown tool"));
    }

    #[test]
    fn test_crud_tools_present() {
        let names = tool_names();
        for name in &[
            "add_node",
            "link_nodes",
            "unlink_nodes",
            "delete_node",
            "update_status",
            "update_prompt",
        ] {
            assert!(
                names.contains(&name.to_string()),
                "Missing CRUD tool: {name}"
            );
        }
    }

    #[test]
    fn test_query_tools_present() {
        let names = tool_names();
        for name in &[
            "list_nodes",
            "list_edges",
            "show_node",
            "get_graph",
            "search_nodes",
        ] {
            assert!(
                names.contains(&name.to_string()),
                "Missing query tool: {name}"
            );
        }
    }

    #[test]
    fn test_analysis_tools_present() {
        let names = tool_names();
        for name in &[
            "trace_chain",
            "get_node_context",
            "get_timeline",
            "get_pulse",
            "find_orphans",
            "get_branch_summary",
        ] {
            assert!(
                names.contains(&name.to_string()),
                "Missing analysis tool: {name}"
            );
        }
    }

    #[test]
    fn test_add_node_schema_has_enum() {
        let tool = find_tool("add_node").unwrap();
        let node_type_enum = &tool.input_schema["properties"]["node_type"]["enum"];
        assert!(node_type_enum.is_array());
        let types: Vec<&str> = node_type_enum
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert!(types.contains(&"goal"));
        assert!(types.contains(&"decision"));
        assert!(types.contains(&"observation"));
        assert!(types.contains(&"revisit"));
    }

    #[test]
    fn test_link_nodes_schema_has_edge_types() {
        let tool = find_tool("link_nodes").unwrap();
        let edge_enum = &tool.input_schema["properties"]["edge_type"]["enum"];
        assert!(edge_enum.is_array());
        let types: Vec<&str> = edge_enum
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert!(types.contains(&"leads_to"));
        assert!(types.contains(&"rejected"));
    }

    #[test]
    fn test_all_tools_serialize_cleanly() {
        let tools = all_tool_definitions();
        for tool in &tools {
            let serialized = serde_json::to_string(tool);
            assert!(
                serialized.is_ok(),
                "Tool {} failed to serialize: {:?}",
                tool.name,
                serialized.err()
            );
        }
    }
}
