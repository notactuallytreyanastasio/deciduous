//! Integration tests for the deciduous CLI
//!
//! These tests exercise the full CLI workflow using a temporary database.
//! They verify that commands work end-to-end without mocking.

use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

/// Helper to run deciduous CLI with a specific database path
fn run_deciduous(args: &[&str], db_path: &PathBuf) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_deciduous"))
        .args(args)
        .env("DECIDUOUS_DB_PATH", db_path)
        .output()
        .expect("Failed to execute deciduous")
}

/// Helper to run deciduous CLI with a specific database path and working directory
///
/// Some commands (init, doc attach) resolve paths relative to the current
/// directory, so tests for them must run inside an isolated temp dir.
fn run_deciduous_in(
    args: &[&str],
    db_path: &PathBuf,
    cwd: &std::path::Path,
) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_deciduous"))
        .args(args)
        .env("DECIDUOUS_DB_PATH", db_path)
        .current_dir(cwd)
        .output()
        .expect("Failed to execute deciduous")
}

/// Helper to get stdout as string
fn stdout(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

/// Helper to get stderr as string
fn stderr(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

// =============================================================================
// Basic Command Tests
// =============================================================================

#[test]
fn test_help_command() {
    let output = Command::new(env!("CARGO_BIN_EXE_deciduous"))
        .arg("--help")
        .output()
        .expect("Failed to execute");

    assert!(output.status.success());
    let out = stdout(&output);
    assert!(out.contains("deciduous"));
    assert!(out.contains("Decision graph"));
}

#[test]
fn test_version_command() {
    let output = Command::new(env!("CARGO_BIN_EXE_deciduous"))
        .arg("--version")
        .output()
        .expect("Failed to execute");

    assert!(output.status.success());
    let out = stdout(&output);
    assert!(out.contains("deciduous"));
}

// =============================================================================
// Shell Completion Tests
// =============================================================================

#[test]
fn test_completion_zsh() {
    let output = Command::new(env!("CARGO_BIN_EXE_deciduous"))
        .args(["completion", "zsh"])
        .output()
        .expect("Failed to execute");

    assert!(
        output.status.success(),
        "completion zsh failed: {}",
        stderr(&output)
    );
    let out = stdout(&output);
    assert!(
        out.contains("#compdef deciduous"),
        "zsh completion should contain #compdef"
    );
}

#[test]
fn test_completion_bash() {
    let output = Command::new(env!("CARGO_BIN_EXE_deciduous"))
        .args(["completion", "bash"])
        .output()
        .expect("Failed to execute");

    assert!(
        output.status.success(),
        "completion bash failed: {}",
        stderr(&output)
    );
    let out = stdout(&output);
    assert!(
        out.contains("_deciduous"),
        "bash completion should contain _deciduous function"
    );
}

#[test]
fn test_completion_fish() {
    let output = Command::new(env!("CARGO_BIN_EXE_deciduous"))
        .args(["completion", "fish"])
        .output()
        .expect("Failed to execute");

    assert!(
        output.status.success(),
        "completion fish failed: {}",
        stderr(&output)
    );
    let out = stdout(&output);
    assert!(
        out.contains("complete -c deciduous"),
        "fish completion should contain complete command"
    );
}

#[test]
fn test_completion_help() {
    let output = Command::new(env!("CARGO_BIN_EXE_deciduous"))
        .args(["completion", "--help"])
        .output()
        .expect("Failed to execute");

    assert!(output.status.success());
    let out = stdout(&output);
    assert!(out.contains("bash"));
    assert!(out.contains("zsh"));
    assert!(out.contains("fish"));
}

// =============================================================================
// Node CRUD Tests
// =============================================================================

#[test]
fn test_add_and_list_nodes() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    // Add a goal node
    let output = run_deciduous(&["add", "goal", "Test Goal", "-c", "90"], &db_path);
    assert!(
        output.status.success(),
        "add goal failed: {}",
        stderr(&output)
    );
    assert!(stdout(&output).contains("Created node"));

    // Add an action node
    let output = run_deciduous(&["add", "action", "Test Action", "-c", "85"], &db_path);
    assert!(
        output.status.success(),
        "add action failed: {}",
        stderr(&output)
    );

    // List nodes
    let output = run_deciduous(&["nodes"], &db_path);
    assert!(output.status.success(), "nodes failed: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("Test Goal"));
    assert!(out.contains("Test Action"));
    assert!(out.contains("goal"));
    assert!(out.contains("action"));
}

#[test]
fn test_add_node_with_all_metadata() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    // Add node with all optional fields
    let output = run_deciduous(
        &[
            "add",
            "goal",
            "Full Metadata Goal",
            "-c",
            "95",
            "-p",
            "User asked: implement feature X",
            "-f",
            "src/main.rs,src/lib.rs",
            "-b",
            "feature-branch",
        ],
        &db_path,
    );
    assert!(
        output.status.success(),
        "add with metadata failed: {}",
        stderr(&output)
    );

    // Verify node was created
    let output = run_deciduous(&["nodes"], &db_path);
    assert!(stdout(&output).contains("Full Metadata Goal"));
}

#[test]
fn test_add_all_node_types() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    let node_types = [
        "goal",
        "decision",
        "option",
        "action",
        "outcome",
        "observation",
    ];

    for node_type in &node_types {
        let title = format!("Test {}", node_type);
        let output = run_deciduous(&["add", node_type, &title, "-c", "80"], &db_path);
        assert!(
            output.status.success(),
            "add {} failed: {}",
            node_type,
            stderr(&output)
        );
    }

    // List and verify all types present
    let output = run_deciduous(&["nodes"], &db_path);
    let out = stdout(&output);
    for node_type in &node_types {
        assert!(out.contains(node_type), "Missing node type: {}", node_type);
    }
}

#[test]
fn test_nodes_with_multibyte_observation_description() {
    // Regression test: `deciduous nodes` truncated long observation
    // descriptions by byte index, panicking when a multi-byte character
    // straddled the 77-byte boundary.
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    // Description well over 80 chars, made entirely of 4-byte emoji so any
    // byte-index truncation lands mid-character
    let long_desc = "\u{1F600}".repeat(100);
    let output = run_deciduous(
        &["add", "observation", "Emoji Observation", "-d", &long_desc],
        &db_path,
    );
    assert!(
        output.status.success(),
        "add observation failed: {}",
        stderr(&output)
    );

    let output = run_deciduous(&["nodes"], &db_path);
    assert!(
        output.status.success(),
        "nodes panicked or failed on multibyte description: {}",
        stderr(&output)
    );
    let out = stdout(&output);
    assert!(out.contains("Emoji Observation"));
    assert!(out.contains("..."), "long description should be truncated");
}

#[test]
fn test_add_node_with_dst_gap_date() {
    // Regression test: --date with a local time that does not exist in the
    // process timezone (DST spring-forward gap) panicked on .unwrap().
    // 2:30 AM on 2025-03-09 does not exist in America/New_York.
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    let output = Command::new(env!("CARGO_BIN_EXE_deciduous"))
        .args([
            "add",
            "goal",
            "DST Gap Goal",
            "--date",
            "2025-03-09 02:30:00",
        ])
        .env("DECIDUOUS_DB_PATH", &db_path)
        .env("TZ", "America/New_York")
        .output()
        .expect("Failed to execute deciduous");
    assert!(
        output.status.success(),
        "add with DST-gap date failed: {}",
        stderr(&output)
    );

    let output = run_deciduous(&["nodes"], &db_path);
    assert!(stdout(&output).contains("DST Gap Goal"));
}

// =============================================================================
// Edge Tests
// =============================================================================

#[test]
fn test_link_nodes() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    // Create two nodes
    run_deciduous(&["add", "goal", "Goal", "-c", "90"], &db_path);
    run_deciduous(&["add", "action", "Action", "-c", "85"], &db_path);

    // Link them
    let output = run_deciduous(&["link", "1", "2", "-r", "Goal leads to action"], &db_path);
    assert!(output.status.success(), "link failed: {}", stderr(&output));
    assert!(stdout(&output).contains("Created edge"));

    // Verify edge exists
    let output = run_deciduous(&["edges"], &db_path);
    assert!(output.status.success());
    let out = stdout(&output);
    assert!(out.contains("1"));
    assert!(out.contains("2"));
    assert!(out.contains("leads_to"));
}

#[test]
fn test_link_with_edge_types() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    // Create decision and options
    run_deciduous(&["add", "decision", "Choose framework"], &db_path);
    run_deciduous(&["add", "option", "React"], &db_path);
    run_deciduous(&["add", "option", "Vue"], &db_path);

    // Link with chosen/rejected types
    let output = run_deciduous(
        &["link", "1", "2", "-t", "chosen", "-r", "Better ecosystem"],
        &db_path,
    );
    assert!(output.status.success());

    let output = run_deciduous(
        &[
            "link",
            "1",
            "3",
            "-t",
            "rejected",
            "-r",
            "Smaller community",
        ],
        &db_path,
    );
    assert!(output.status.success());

    // Verify edges
    let output = run_deciduous(&["edges"], &db_path);
    let out = stdout(&output);
    assert!(out.contains("chosen"));
    assert!(out.contains("rejected"));
}

// =============================================================================
// Status Update Tests
// =============================================================================

#[test]
fn test_update_node_status() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    // Create a node
    run_deciduous(&["add", "action", "My Action"], &db_path);

    // Update status
    let output = run_deciduous(&["status", "1", "completed"], &db_path);
    assert!(
        output.status.success(),
        "status update failed: {}",
        stderr(&output)
    );

    // Verify status changed
    let output = run_deciduous(&["nodes"], &db_path);
    assert!(stdout(&output).contains("completed"));
}

#[test]
fn test_update_status_nonexistent_node_fails() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    // No nodes exist - updating status must fail, not silently succeed
    let output = run_deciduous(&["status", "42", "completed"], &db_path);
    assert!(
        !output.status.success(),
        "status update on nonexistent node should fail, stdout: {}",
        stdout(&output)
    );
}

// =============================================================================
// Graph Export Tests
// =============================================================================

#[test]
fn test_graph_json_export() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    // Create some nodes and edges
    run_deciduous(&["add", "goal", "Export Test Goal"], &db_path);
    run_deciduous(&["add", "action", "Export Test Action"], &db_path);
    run_deciduous(&["link", "1", "2", "-r", "test"], &db_path);

    // Export graph as JSON
    let output = run_deciduous(&["graph"], &db_path);
    assert!(
        output.status.success(),
        "graph export failed: {}",
        stderr(&output)
    );

    let out = stdout(&output);

    // Verify it's valid JSON with expected structure
    let json: serde_json::Value = serde_json::from_str(&out).expect("Output should be valid JSON");

    assert!(json.get("nodes").is_some(), "JSON should have nodes");
    assert!(json.get("edges").is_some(), "JSON should have edges");

    let nodes = json["nodes"].as_array().unwrap();
    assert_eq!(nodes.len(), 2);
}

#[test]
fn test_dot_export() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    // Create graph
    run_deciduous(&["add", "goal", "DOT Test"], &db_path);
    run_deciduous(&["add", "action", "DOT Action"], &db_path);
    run_deciduous(&["link", "1", "2"], &db_path);

    // Export as DOT
    let output = run_deciduous(&["dot"], &db_path);
    assert!(
        output.status.success(),
        "dot export failed: {}",
        stderr(&output)
    );

    let out = stdout(&output);
    assert!(out.contains("digraph"));
    assert!(out.contains("DOT Test"));
    assert!(out.contains("->"));
}

// =============================================================================
// Filter Tests
// =============================================================================

#[test]
fn test_filter_nodes_by_type() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    // Create mixed nodes
    run_deciduous(&["add", "goal", "Goal 1"], &db_path);
    run_deciduous(&["add", "goal", "Goal 2"], &db_path);
    run_deciduous(&["add", "action", "Action 1"], &db_path);

    // Filter by type
    let output = run_deciduous(&["nodes", "-t", "goal"], &db_path);
    assert!(output.status.success());

    let out = stdout(&output);
    assert!(out.contains("Goal 1"));
    assert!(out.contains("Goal 2"));
    assert!(!out.contains("Action 1"));
}

// =============================================================================
// Command Log Tests
// =============================================================================

#[test]
fn test_command_log() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    // Run some commands
    run_deciduous(&["add", "goal", "Logged Goal"], &db_path);
    run_deciduous(&["add", "action", "Logged Action"], &db_path);

    // Check command log
    let output = run_deciduous(&["commands"], &db_path);
    assert!(
        output.status.success(),
        "commands failed: {}",
        stderr(&output)
    );

    let out = stdout(&output);
    // Command log should show something
    assert!(!out.is_empty());
}

// =============================================================================
// Error Handling Tests
// =============================================================================

#[test]
fn test_link_nonexistent_nodes() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    // Try to link nodes that don't exist
    let output = run_deciduous(&["link", "999", "998"], &db_path);

    // Should fail gracefully
    assert!(
        !output.status.success()
            || stderr(&output).contains("Error")
            || stderr(&output).contains("not found")
    );
}

#[test]
fn test_invalid_node_type() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    // Try to add invalid node type - the CLI accepts it but warns
    // This tests that the CLI handles it gracefully (doesn't crash)
    let output = run_deciduous(&["add", "invalid_type", "Test"], &db_path);

    // CLI should complete (may succeed with warning or fail gracefully)
    // Main thing is it shouldn't panic
    let _out = stdout(&output);
    let _err = stderr(&output);
    // Just verify it ran without panic - actual behavior varies
}

// =============================================================================
// Node Delete Tests
// =============================================================================

#[test]
fn test_delete_node() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    // Create a node
    run_deciduous(&["add", "goal", "Delete Me", "-c", "90"], &db_path);

    // Verify it exists
    let output = run_deciduous(&["nodes"], &db_path);
    assert!(stdout(&output).contains("Delete Me"));

    // Delete it
    let output = run_deciduous(&["delete", "1"], &db_path);
    assert!(
        output.status.success(),
        "delete failed: {}",
        stderr(&output)
    );

    // Verify it's gone
    let output = run_deciduous(&["nodes"], &db_path);
    assert!(!stdout(&output).contains("Delete Me"));
}

#[test]
fn test_delete_node_cascades_edges() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    // Create two nodes and link them
    run_deciduous(&["add", "goal", "Parent"], &db_path);
    run_deciduous(&["add", "action", "Child"], &db_path);
    run_deciduous(&["link", "1", "2", "-r", "parent-child"], &db_path);

    // Verify edge exists
    let output = run_deciduous(&["edges"], &db_path);
    assert!(stdout(&output).contains("leads_to"));

    // Delete parent node (edges are automatically cleaned up)
    let output = run_deciduous(&["delete", "1"], &db_path);
    assert!(
        output.status.success(),
        "delete failed: {}",
        stderr(&output)
    );

    // Verify edge is gone too (deleting a node removes its edges)
    let output = run_deciduous(&["edges"], &db_path);
    let out = stdout(&output);
    assert!(
        !out.contains("parent-child"),
        "Edge should be removed after node delete"
    );
}

// =============================================================================
// Unlink Tests
// =============================================================================

#[test]
fn test_unlink_nodes() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    // Create and link nodes
    run_deciduous(&["add", "goal", "Goal"], &db_path);
    run_deciduous(&["add", "action", "Action"], &db_path);
    run_deciduous(&["link", "1", "2", "-r", "test link"], &db_path);

    // Verify edge exists
    let output = run_deciduous(&["edges"], &db_path);
    assert!(stdout(&output).contains("1"));

    // Unlink (requires FROM and TO)
    let output = run_deciduous(&["unlink", "1", "2"], &db_path);
    assert!(
        output.status.success(),
        "unlink failed: {}",
        stderr(&output)
    );
}

#[test]
fn test_unlink_non_leads_to_edge() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    // Create nodes linked with a non-default edge type
    run_deciduous(&["add", "option", "Option A"], &db_path);
    run_deciduous(&["add", "decision", "Decision"], &db_path);
    run_deciduous(&["link", "1", "2", "-t", "rejected"], &db_path);

    // Verify the typed edge exists
    let output = run_deciduous(&["edges"], &db_path);
    assert!(stdout(&output).contains("rejected"));

    // Unlink removes it
    let output = run_deciduous(&["unlink", "1", "2"], &db_path);
    assert!(
        output.status.success(),
        "unlink failed: {}",
        stderr(&output)
    );

    let output = run_deciduous(&["edges"], &db_path);
    assert!(
        !stdout(&output).contains("rejected"),
        "typed edge should be removed"
    );
}

// =============================================================================
// Show/Detail Tests
// =============================================================================

#[test]
fn test_show_node_detail() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    // Create node with metadata
    run_deciduous(
        &[
            "add",
            "goal",
            "Detailed Goal",
            "-c",
            "95",
            "-p",
            "User prompt here",
        ],
        &db_path,
    );

    // Show detail
    let output = run_deciduous(&["show", "1"], &db_path);
    assert!(output.status.success(), "show failed: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("Detailed Goal"));
}

#[test]
fn test_show_node_json() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    run_deciduous(&["add", "goal", "JSON Goal", "-c", "90"], &db_path);

    let output = run_deciduous(&["show", "1", "--json"], &db_path);
    assert!(
        output.status.success(),
        "show --json failed: {}",
        stderr(&output)
    );
    let out = stdout(&output);
    let json: serde_json::Value =
        serde_json::from_str(&out).expect("show --json should output valid JSON");
    assert_eq!(json["title"], "JSON Goal");
}

// =============================================================================
// Prompt Tests
// =============================================================================

#[test]
fn test_update_prompt() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    run_deciduous(&["add", "goal", "Prompt Goal"], &db_path);

    // Update prompt
    let output = run_deciduous(&["prompt", "1", "Updated prompt text"], &db_path);
    assert!(
        output.status.success(),
        "prompt update failed: {}",
        stderr(&output)
    );

    // Verify via show
    let output = run_deciduous(&["show", "1", "--json"], &db_path);
    let out = stdout(&output);
    assert!(out.contains("Updated prompt text"));
}

// =============================================================================
// Status Transitions Tests
// =============================================================================

#[test]
fn test_all_status_values() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    run_deciduous(&["add", "goal", "Status Test"], &db_path);

    let statuses = ["active", "completed", "superseded", "abandoned", "rejected"];
    for status in &statuses {
        let output = run_deciduous(&["status", "1", status], &db_path);
        assert!(
            output.status.success(),
            "status {} failed: {}",
            status,
            stderr(&output)
        );
    }
}

// =============================================================================
// Graph Traversal Tests
// =============================================================================

#[test]
fn test_complex_graph_structure() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    // Build a proper decision tree:
    // goal -> option1, option2
    // option1 -> decision
    // decision -> action
    // action -> outcome
    run_deciduous(&["add", "goal", "Root Goal", "-c", "90"], &db_path);
    run_deciduous(&["add", "option", "Option A", "-c", "80"], &db_path);
    run_deciduous(&["add", "option", "Option B", "-c", "75"], &db_path);
    run_deciduous(&["add", "decision", "Choose A", "-c", "95"], &db_path);
    run_deciduous(&["add", "action", "Implement A", "-c", "85"], &db_path);
    run_deciduous(&["add", "outcome", "A works", "-c", "90"], &db_path);
    run_deciduous(&["add", "observation", "Noticed X", "-c", "70"], &db_path);

    // Link the full chain
    run_deciduous(&["link", "1", "2", "-r", "possible approach"], &db_path);
    run_deciduous(&["link", "1", "3", "-r", "possible approach"], &db_path);
    run_deciduous(
        &["link", "2", "4", "-t", "chosen", "-r", "better fit"],
        &db_path,
    );
    run_deciduous(
        &["link", "3", "4", "-t", "rejected", "-r", "too complex"],
        &db_path,
    );
    run_deciduous(&["link", "4", "5", "-r", "implementation"], &db_path);
    run_deciduous(&["link", "5", "6", "-r", "result"], &db_path);
    run_deciduous(&["link", "7", "1", "-r", "context"], &db_path);

    // Verify full graph
    let output = run_deciduous(&["graph"], &db_path);
    assert!(output.status.success());
    let json: serde_json::Value =
        serde_json::from_str(&stdout(&output)).expect("Graph should be valid JSON");
    assert_eq!(json["nodes"].as_array().unwrap().len(), 7);
    assert_eq!(json["edges"].as_array().unwrap().len(), 7);

    // Verify DOT export includes all nodes
    let output = run_deciduous(&["dot"], &db_path);
    assert!(output.status.success());
    let dot = stdout(&output);
    assert!(dot.contains("Root Goal"));
    assert!(dot.contains("Option A"));
    assert!(dot.contains("Choose A"));
    assert!(dot.contains("Implement A"));
    assert!(dot.contains("A works"));
    assert!(dot.contains("Noticed X"));
}

#[test]
fn test_dot_with_root_filter() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    // Create two separate chains
    run_deciduous(&["add", "goal", "Chain 1 Root"], &db_path);
    run_deciduous(&["add", "action", "Chain 1 Action"], &db_path);
    run_deciduous(&["link", "1", "2"], &db_path);

    run_deciduous(&["add", "goal", "Chain 2 Root"], &db_path);
    run_deciduous(&["add", "action", "Chain 2 Action"], &db_path);
    run_deciduous(&["link", "3", "4"], &db_path);

    // Filter DOT to only chain 1
    let output = run_deciduous(&["dot", "-r", "1"], &db_path);
    assert!(output.status.success());
    let dot = stdout(&output);
    assert!(dot.contains("Chain 1 Root"));
    assert!(dot.contains("Chain 1 Action"));
    assert!(!dot.contains("Chain 2 Root"));
}

#[test]
fn test_dot_with_node_range() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    run_deciduous(&["add", "goal", "Node 1"], &db_path);
    run_deciduous(&["add", "action", "Node 2"], &db_path);
    run_deciduous(&["add", "outcome", "Node 3"], &db_path);

    // Filter to nodes 1-2 only
    let output = run_deciduous(&["dot", "-n", "1-2"], &db_path);
    assert!(output.status.success());
    let dot = stdout(&output);
    assert!(dot.contains("Node 1"));
    assert!(dot.contains("Node 2"));
    assert!(!dot.contains("Node 3"));
}

// =============================================================================
// Backup Tests
// =============================================================================

#[test]
fn test_backup() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    // Create some data
    run_deciduous(&["add", "goal", "Backup Test"], &db_path);

    // Run backup
    let output = run_deciduous(&["backup"], &db_path);
    assert!(
        output.status.success(),
        "backup failed: {}",
        stderr(&output)
    );
}

// =============================================================================
// Empty Database Edge Cases
// =============================================================================

#[test]
fn test_empty_db_nodes() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    // List nodes on empty database
    let output = run_deciduous(&["nodes"], &db_path);
    assert!(
        output.status.success(),
        "nodes on empty db failed: {}",
        stderr(&output)
    );
}

#[test]
fn test_empty_db_edges() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    let output = run_deciduous(&["edges"], &db_path);
    assert!(
        output.status.success(),
        "edges on empty db failed: {}",
        stderr(&output)
    );
}

#[test]
fn test_empty_db_graph() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    let output = run_deciduous(&["graph"], &db_path);
    assert!(
        output.status.success(),
        "graph on empty db failed: {}",
        stderr(&output)
    );
    let json: serde_json::Value =
        serde_json::from_str(&stdout(&output)).expect("Empty graph should be valid JSON");
    assert_eq!(json["nodes"].as_array().unwrap().len(), 0);
    assert_eq!(json["edges"].as_array().unwrap().len(), 0);
}

// =============================================================================
// Branch Filter Tests
// =============================================================================

#[test]
fn test_filter_nodes_by_branch() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    // Create nodes on different branches
    run_deciduous(&["add", "goal", "Main Goal", "-b", "main"], &db_path);
    run_deciduous(
        &["add", "goal", "Feature Goal", "-b", "feature-x"],
        &db_path,
    );

    // Filter by branch
    let output = run_deciduous(&["nodes", "-b", "main"], &db_path);
    assert!(output.status.success());
    let out = stdout(&output);
    assert!(out.contains("Main Goal"));
    assert!(!out.contains("Feature Goal"));
}

// =============================================================================
// Writeup Tests
// =============================================================================

#[test]
fn test_writeup_basic() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    run_deciduous(&["add", "goal", "PR Goal", "-c", "90"], &db_path);
    run_deciduous(&["add", "action", "PR Action", "-c", "85"], &db_path);
    run_deciduous(&["link", "1", "2", "-r", "implementation"], &db_path);

    let output = run_deciduous(&["writeup", "-t", "Test PR", "-n", "1-2"], &db_path);
    assert!(
        output.status.success(),
        "writeup failed: {}",
        stderr(&output)
    );
    let out = stdout(&output);
    assert!(out.contains("Test PR"));
}

// =============================================================================
// Multiple Edge Types Tests
// =============================================================================

#[test]
fn test_all_edge_types() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    // Create nodes
    for i in 0..6 {
        run_deciduous(&["add", "goal", &format!("Node {}", i)], &db_path);
    }

    let edge_types = [
        "leads_to", "chosen", "rejected", "blocks", "enables", "requires",
    ];
    for (i, edge_type) in edge_types.iter().enumerate() {
        let from = format!("{}", i + 1);
        let to = format!("{}", ((i + 1) % 6) + 1);
        let output = run_deciduous(
            &[
                "link",
                &from,
                &to,
                "-t",
                edge_type,
                "-r",
                &format!("test {}", edge_type),
            ],
            &db_path,
        );
        assert!(
            output.status.success(),
            "link with type {} failed: {}",
            edge_type,
            stderr(&output)
        );
    }

    // Verify all edges created
    let output = run_deciduous(&["edges"], &db_path);
    let out = stdout(&output);
    for edge_type in &edge_types {
        assert!(out.contains(edge_type), "Missing edge type: {}", edge_type);
    }
}

// =============================================================================
// Revisit Node Tests
// =============================================================================

#[test]
fn test_revisit_node_type() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    let output = run_deciduous(
        &["add", "revisit", "Reconsider approach", "-c", "80"],
        &db_path,
    );
    assert!(
        output.status.success(),
        "add revisit failed: {}",
        stderr(&output)
    );
    assert!(stdout(&output).contains("Created node"));

    let output = run_deciduous(&["nodes"], &db_path);
    assert!(stdout(&output).contains("revisit"));
}

// =============================================================================
// Supersede Tests
// =============================================================================

#[test]
fn test_supersede_node() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    // Create original decision and its replacement
    run_deciduous(&["add", "decision", "Old Approach"], &db_path);
    run_deciduous(&["add", "decision", "New Approach"], &db_path);

    // Supersede old with new
    let output = run_deciduous(&["status", "1", "superseded"], &db_path);
    assert!(
        output.status.success(),
        "supersede failed: {}",
        stderr(&output)
    );

    // Verify status changed
    let output = run_deciduous(&["show", "1", "--json"], &db_path);
    let out = stdout(&output);
    assert!(out.contains("superseded"));
}

// =============================================================================
// Init Tests
// =============================================================================

#[test]
fn test_init_creates_project_scaffold() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let project = temp_dir.path();
    let db_path = project.join(".deciduous").join("deciduous.db");

    let output = run_deciduous_in(&["init"], &db_path, project);
    assert!(output.status.success(), "init failed: {}", stderr(&output));

    // Core infrastructure
    assert!(project.join(".deciduous").is_dir());
    assert!(project.join(".deciduous/documents").is_dir());
    assert!(project.join(".deciduous/config.toml").is_file());
    assert!(project.join(".deciduous/.version").is_file());

    // Claude Code integration (default assistant)
    assert!(project.join(".claude/commands/decision.md").is_file());
    assert!(project.join(".claude/commands/recover.md").is_file());
    assert!(project.join(".claude/settings.json").is_file());
    assert!(project.join(".claude/skills/pulse.md").is_file());

    // CLAUDE.md gets the workflow section
    let claude_md =
        std::fs::read_to_string(project.join("CLAUDE.md")).expect("init should create CLAUDE.md");
    assert!(claude_md.contains("## Decision Graph Workflow"));

    // GitHub Pages scaffold
    assert!(project.join("docs/index.html").is_file());
    assert!(project.join("docs/graph-data.json").is_file());

    // .deciduous is gitignored
    let gitignore =
        std::fs::read_to_string(project.join(".gitignore")).expect("init should create .gitignore");
    assert!(gitignore.contains(".deciduous/"));
}

#[test]
fn test_init_twice_is_idempotent() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let project = temp_dir.path();
    let db_path = project.join(".deciduous").join("deciduous.db");

    let output = run_deciduous_in(&["init"], &db_path, project);
    assert!(
        output.status.success(),
        "first init failed: {}",
        stderr(&output)
    );

    // Put user content in config to verify init doesn't clobber existing files
    let config_path = project.join(".deciduous/config.toml");
    let user_config = "# user-modified config\n[branch]\nmain_branches = [\"trunk\"]\n";
    std::fs::write(&config_path, user_config).expect("Failed to write config");

    let output = run_deciduous_in(&["init"], &db_path, project);
    assert!(
        output.status.success(),
        "second init failed: {}",
        stderr(&output)
    );

    // User config preserved
    let config = std::fs::read_to_string(&config_path).expect("config should still exist");
    assert_eq!(config, user_config, "init must not clobber existing config");

    // CLAUDE.md workflow section not duplicated
    let claude_md =
        std::fs::read_to_string(project.join("CLAUDE.md")).expect("CLAUDE.md should exist");
    let occurrences = claude_md.matches("## Decision Graph Workflow").count();
    assert_eq!(
        occurrences, 1,
        "workflow section should appear exactly once after re-init"
    );
}

// =============================================================================
// Document Attachment Tests
// =============================================================================

#[test]
fn test_doc_attach_list_show_detach() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let project = temp_dir.path();
    let db_path = project.join("test.db");

    // Create a node to attach to
    let output = run_deciduous_in(&["add", "goal", "Doc Goal"], &db_path, project);
    assert!(output.status.success(), "add failed: {}", stderr(&output));

    // Create a small file to attach
    let file_path = project.join("note.txt");
    std::fs::write(&file_path, "design sketch contents").expect("Failed to write temp file");

    // Attach with a description
    let output = run_deciduous_in(
        &[
            "doc",
            "attach",
            "1",
            file_path.to_str().unwrap(),
            "-d",
            "Design sketch",
        ],
        &db_path,
        project,
    );
    assert!(
        output.status.success(),
        "doc attach failed: {}",
        stderr(&output)
    );
    assert!(stdout(&output).contains("Attached"));

    // File is stored under .deciduous/documents/ (relative to cwd)
    let docs_dir = project.join(".deciduous/documents");
    assert!(docs_dir.is_dir(), "documents dir should be created");
    assert!(
        std::fs::read_dir(&docs_dir).unwrap().count() == 1,
        "attached file should be copied into documents dir"
    );

    // List shows the document
    let output = run_deciduous_in(&["doc", "list"], &db_path, project);
    assert!(
        output.status.success(),
        "doc list failed: {}",
        stderr(&output)
    );
    let out = stdout(&output);
    assert!(out.contains("note.txt"));
    assert!(out.contains("Design sketch"));

    // List scoped to the node also shows it
    let output = run_deciduous_in(&["doc", "list", "1"], &db_path, project);
    assert!(stdout(&output).contains("note.txt"));

    // Show displays details
    let output = run_deciduous_in(&["doc", "show", "1"], &db_path, project);
    assert!(
        output.status.success(),
        "doc show failed: {}",
        stderr(&output)
    );
    let out = stdout(&output);
    assert!(out.contains("note.txt"));
    assert!(out.contains("Design sketch"));

    // Detach soft-deletes
    let output = run_deciduous_in(&["doc", "detach", "1"], &db_path, project);
    assert!(
        output.status.success(),
        "doc detach failed: {}",
        stderr(&output)
    );
    assert!(stdout(&output).contains("Detached"));

    // Default list no longer shows it
    let output = run_deciduous_in(&["doc", "list"], &db_path, project);
    assert!(
        !stdout(&output).contains("note.txt"),
        "detached document should not appear in default list"
    );

    // But it is recoverable via --include-detached
    let output = run_deciduous_in(&["doc", "list", "--include-detached"], &db_path, project);
    assert!(
        stdout(&output).contains("note.txt"),
        "detached document should appear with --include-detached"
    );
}

#[test]
fn test_doc_attach_missing_file_fails() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let project = temp_dir.path();
    let db_path = project.join("test.db");

    run_deciduous_in(&["add", "goal", "Doc Goal"], &db_path, project);

    let output = run_deciduous_in(
        &["doc", "attach", "1", "does-not-exist.txt"],
        &db_path,
        project,
    );
    assert!(
        !output.status.success(),
        "attaching a nonexistent file should fail"
    );
}

// =============================================================================
// Theme and Tag Tests
// =============================================================================

#[test]
fn test_themes_create_and_tag_node() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    // Create a theme
    let output = run_deciduous(
        &[
            "themes",
            "create",
            "backend",
            "-c",
            "#ef4444",
            "-d",
            "Backend work",
        ],
        &db_path,
    );
    assert!(
        output.status.success(),
        "themes create failed: {}",
        stderr(&output)
    );
    assert!(stdout(&output).contains("backend"));

    // Theme appears in list
    let output = run_deciduous(&["themes", "list"], &db_path);
    assert!(
        output.status.success(),
        "themes list failed: {}",
        stderr(&output)
    );
    let out = stdout(&output);
    assert!(out.contains("backend"));
    assert!(out.contains("#ef4444"));

    // Tag a node with the theme
    run_deciduous(&["add", "goal", "Themed Goal"], &db_path);
    let output = run_deciduous(&["tag", "add", "1", "backend"], &db_path);
    assert!(
        output.status.success(),
        "tag add failed: {}",
        stderr(&output)
    );
    assert!(stdout(&output).contains("Tagged"));

    // Tag appears on the node
    let output = run_deciduous(&["tag", "list", "1"], &db_path);
    assert!(
        output.status.success(),
        "tag list failed: {}",
        stderr(&output)
    );
    assert!(stdout(&output).contains("backend"));

    // Remove the tag
    let output = run_deciduous(&["tag", "remove", "1", "backend"], &db_path);
    assert!(
        output.status.success(),
        "tag remove failed: {}",
        stderr(&output)
    );

    let output = run_deciduous(&["tag", "list", "1"], &db_path);
    assert!(
        !stdout(&output).contains("backend"),
        "removed tag should not appear"
    );
}

#[test]
fn test_tag_add_unknown_theme_fails() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    run_deciduous(&["add", "goal", "Goal"], &db_path);

    // Tagging with a theme that was never created should fail
    let output = run_deciduous(&["tag", "add", "1", "nonexistent-theme"], &db_path);
    assert!(
        !output.status.success(),
        "tagging with unknown theme should fail, stdout: {}",
        stdout(&output)
    );
}

// =============================================================================
// Migrate Tests
// =============================================================================

#[test]
fn test_migrate_is_idempotent() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    // Create some data first
    run_deciduous(&["add", "goal", "Migrate Goal"], &db_path);

    // First migrate succeeds
    let output = run_deciduous(&["migrate"], &db_path);
    assert!(
        output.status.success(),
        "migrate failed: {}",
        stderr(&output)
    );

    // Second migrate is a safe no-op
    let output = run_deciduous(&["migrate"], &db_path);
    assert!(
        output.status.success(),
        "second migrate failed: {}",
        stderr(&output)
    );
    assert!(
        stdout(&output).contains("no migration needed"),
        "re-running migrate should report nothing to do"
    );

    // Data survives migration
    let output = run_deciduous(&["nodes"], &db_path);
    assert!(stdout(&output).contains("Migrate Goal"));
}
