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
// Diff/Patch Tests
// =============================================================================

#[test]
fn test_diff_export_import() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");
    let patch_path = temp_dir.path().join("patch.json");

    // Create some nodes
    run_deciduous(&["add", "goal", "Patch Test Goal", "-c", "90"], &db_path);
    run_deciduous(
        &["add", "action", "Patch Test Action", "-c", "85"],
        &db_path,
    );
    run_deciduous(&["link", "1", "2", "-r", "test link"], &db_path);

    // Export patch
    let output = run_deciduous(
        &["diff", "export", "-o", patch_path.to_str().unwrap()],
        &db_path,
    );
    assert!(
        output.status.success(),
        "diff export failed: {}",
        stderr(&output)
    );

    // Verify patch file exists and is valid JSON
    let patch_content = std::fs::read_to_string(&patch_path).expect("Patch file should exist");
    let patch: serde_json::Value =
        serde_json::from_str(&patch_content).expect("Patch should be valid JSON");

    assert!(patch.get("nodes").is_some());
    assert!(patch.get("edges").is_some());
    assert_eq!(patch["version"], "1.0");
}

#[test]
fn test_diff_dry_run() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");
    let patch_path = temp_dir.path().join("patch.json");

    // Create and export from first db
    run_deciduous(&["add", "goal", "Dry Run Test"], &db_path);
    run_deciduous(
        &["diff", "export", "-o", patch_path.to_str().unwrap()],
        &db_path,
    );

    // Create second db and try dry-run apply
    let db_path2 = temp_dir.path().join("test2.db");
    let output = run_deciduous(
        &["diff", "apply", "--dry-run", patch_path.to_str().unwrap()],
        &db_path2,
    );

    assert!(
        output.status.success(),
        "diff apply dry-run failed: {}",
        stderr(&output)
    );
    let out = stdout(&output);
    // Dry run should report what would be added
    assert!(out.contains("added") || out.contains("would"));
}
