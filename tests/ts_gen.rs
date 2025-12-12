use ts_rs::TS;
use deciduous::{DecisionNode, DecisionEdge, DecisionContext, DecisionSession, CommandLog};

#[test]
fn test_decision_node_generation() {
    assert_eq!(
        DecisionNode::decl(),
        "type DecisionNode = { id: number, change_id: string, node_type: string, title: string, description: string | null, status: string, created_at: string, updated_at: string, metadata_json: string | null, };"
    );
}

#[test]
fn test_decision_edge_generation() {
    assert_eq!(
        DecisionEdge::decl(),
        "type DecisionEdge = { id: number, from_node_id: number, to_node_id: number, from_change_id: string | null, to_change_id: string | null, edge_type: string, weight: number | null, rationale: string | null, created_at: string, };"
    );
}

#[test]
fn test_decision_context_generation() {
    assert_eq!(
        DecisionContext::decl(),
        "type DecisionContext = { id: number, node_id: number, context_type: string, content_json: string, captured_at: string, };"
    );
}

#[test]
fn test_decision_session_generation() {
    assert_eq!(
        DecisionSession::decl(),
        "type DecisionSession = { id: number, name: string | null, started_at: string, ended_at: string | null, root_node_id: number | null, summary: string | null, };"
    );
}

#[test]
fn test_command_log_generation() {
    assert_eq!(
        CommandLog::decl(),
        "type CommandLog = { id: number, command: string, description: string | null, working_dir: string | null, exit_code: number | null, stdout: string | null, stderr: string | null, started_at: string, completed_at: string | null, duration_ms: number | null, decision_node_id: number | null, };"
    );
}
