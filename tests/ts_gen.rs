use ts_rs::TS;

#[derive(TS)]
#[ts(export)]
#[allow(dead_code)]
struct User {
    user_id: i32,
    first_name: String,
    last_name: String,
}

#[derive(TS)]
#[ts(export)]
#[allow(dead_code)]
struct DecisionNode {
    id: i32,
    change_id: String,
    node_type: String,
    title: String,
    description: Option<String>,
    status: String,
    created_at: String,
    updated_at: String,
    metadata_json: Option<String>,
}

#[test]
fn test_ts_generation() {
    assert_eq!(
        User::decl(),
        "type User = { user_id: number, first_name: string, last_name: string, };"
    );
    assert_eq!(
        DecisionNode::decl(),
        "type DecisionNode = { id: number, change_id: string, node_type: string, title: string, description: string | null, status: string, created_at: string, updated_at: string, metadata_json: string | null, };"
    );
}
