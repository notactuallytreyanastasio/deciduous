// Deciduous schema - Decision graph tables for Diesel ORM

diesel::table! {
    schema_versions (id) {
        id -> Integer,
        version -> Text,
        name -> Text,
        features -> Text,
        introduced_at -> Text,
    }
}

diesel::table! {
    decision_nodes (id) {
        id -> Integer,
        change_id -> Text,
        node_type -> Text,
        title -> Text,
        description -> Nullable<Text>,
        status -> Text,
        created_at -> Text,
        updated_at -> Text,
        metadata_json -> Nullable<Text>,
    }
}

diesel::table! {
    decision_edges (id) {
        id -> Integer,
        from_node_id -> Integer,
        to_node_id -> Integer,
        from_change_id -> Nullable<Text>,
        to_change_id -> Nullable<Text>,
        edge_type -> Text,
        weight -> Nullable<Double>,
        rationale -> Nullable<Text>,
        created_at -> Text,
    }
}

diesel::table! {
    decision_context (id) {
        id -> Integer,
        node_id -> Integer,
        context_type -> Text,
        content_json -> Text,
        captured_at -> Text,
    }
}

diesel::table! {
    decision_sessions (id) {
        id -> Integer,
        name -> Nullable<Text>,
        started_at -> Text,
        ended_at -> Nullable<Text>,
        root_node_id -> Nullable<Integer>,
        summary -> Nullable<Text>,
    }
}

diesel::table! {
    session_nodes (session_id, node_id) {
        session_id -> Integer,
        node_id -> Integer,
        added_at -> Text,
    }
}

diesel::table! {
    command_log (id) {
        id -> Integer,
        command -> Text,
        description -> Nullable<Text>,
        working_dir -> Nullable<Text>,
        exit_code -> Nullable<Integer>,
        stdout -> Nullable<Text>,
        stderr -> Nullable<Text>,
        started_at -> Text,
        completed_at -> Nullable<Text>,
        duration_ms -> Nullable<Integer>,
        decision_node_id -> Nullable<Integer>,
    }
}

// ============================================================================
// Roadmap Board Tables
// ============================================================================

diesel::table! {
    roadmap_items (id) {
        id -> Integer,
        change_id -> Text,
        title -> Text,
        description -> Nullable<Text>,
        section -> Nullable<Text>,
        parent_id -> Nullable<Integer>,
        checkbox_state -> Text,
        github_issue_number -> Nullable<Integer>,
        github_issue_state -> Nullable<Text>,
        outcome_node_id -> Nullable<Integer>,
        outcome_change_id -> Nullable<Text>,
        markdown_line_start -> Nullable<Integer>,
        markdown_line_end -> Nullable<Integer>,
        content_hash -> Nullable<Text>,
        created_at -> Text,
        updated_at -> Text,
        last_synced_at -> Nullable<Text>,
    }
}

diesel::table! {
    roadmap_sync_state (id) {
        id -> Integer,
        roadmap_path -> Text,
        roadmap_content_hash -> Nullable<Text>,
        github_repo -> Nullable<Text>,
        last_github_sync -> Nullable<Text>,
        last_markdown_parse -> Nullable<Text>,
        conflict_count -> Integer,
    }
}

diesel::table! {
    roadmap_conflicts (id) {
        id -> Integer,
        item_change_id -> Text,
        conflict_type -> Text,
        local_value -> Nullable<Text>,
        remote_value -> Nullable<Text>,
        resolution -> Nullable<Text>,
        detected_at -> Text,
        resolved_at -> Nullable<Text>,
    }
}

// ============================================================================
// GitHub Issue Cache - Local cache for TUI/Web display
// ============================================================================

diesel::table! {
    github_issue_cache (id) {
        id -> Integer,
        issue_number -> Integer,
        repo -> Text,
        title -> Text,
        body -> Nullable<Text>,
        state -> Text,
        html_url -> Text,
        created_at -> Text,
        updated_at -> Text,
        cached_at -> Text,
    }
}

// ============================================================================
// Claude Trace Tables - API traffic capture for decision graph correlation
// ============================================================================

diesel::table! {
    trace_sessions (id) {
        id -> Integer,
        session_id -> Text,              // UUID - unique across all sessions
        started_at -> Text,
        ended_at -> Nullable<Text>,
        working_dir -> Nullable<Text>,
        git_branch -> Nullable<Text>,
        command -> Nullable<Text>,       // What was run (e.g., "claude")
        summary -> Nullable<Text>,
        total_input_tokens -> Integer,
        total_output_tokens -> Integer,
        total_cache_read -> Integer,
        total_cache_write -> Integer,
        linked_node_id -> Nullable<Integer>,   // FK to decision_nodes
        linked_change_id -> Nullable<Text>,    // For sync compatibility
    }
}

diesel::table! {
    trace_spans (id) {
        id -> Integer,
        change_id -> Text,               // UUID for sync
        session_id -> Text,              // FK to trace_sessions.session_id
        sequence_num -> Integer,         // Order within session
        started_at -> Text,
        completed_at -> Nullable<Text>,
        duration_ms -> Nullable<Integer>,
        model -> Nullable<Text>,
        request_id -> Nullable<Text>,    // Anthropic request ID
        stop_reason -> Nullable<Text>,
        // Token counts
        input_tokens -> Nullable<Integer>,
        output_tokens -> Nullable<Integer>,
        cache_read -> Nullable<Integer>,
        cache_write -> Nullable<Integer>,
        // Previews for list views
        user_preview -> Nullable<Text>,
        thinking_preview -> Nullable<Text>,
        response_preview -> Nullable<Text>,
        tool_names -> Nullable<Text>,    // Comma-separated
        // Linking
        linked_node_id -> Nullable<Integer>,
        linked_change_id -> Nullable<Text>,
    }
}

diesel::table! {
    trace_content (id) {
        id -> Integer,
        span_id -> Integer,              // FK to trace_spans.id
        content_type -> Text,            // 'thinking', 'response', 'tool_input', 'tool_output', 'system'
        tool_name -> Nullable<Text>,
        tool_use_id -> Nullable<Text>,   // Anthropic tool_use_id
        content -> Text,
        sequence_num -> Integer,         // For ordering multiple tool calls
    }
}
