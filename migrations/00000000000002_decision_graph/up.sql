-- Decision Graph Schema (Directed Acyclic Graph)
-- Models decisions, alternatives, and the paths taken during development

-- Node types in the decision graph
-- Types: 'goal', 'decision', 'option', 'action', 'outcome', 'observation', 'revisit'
CREATE TABLE decision_nodes (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    node_type TEXT NOT NULL,
    title TEXT NOT NULL,
    description TEXT,
    status TEXT NOT NULL DEFAULT 'pending',  -- pending, active, completed, rejected
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    metadata_json TEXT  -- flexible JSON for node-type-specific data
);

-- Edges connecting nodes (directed)
-- Edge types: 'leads_to', 'requires', 'chosen', 'rejected', 'blocks', 'enables'
CREATE TABLE decision_edges (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    from_node_id INTEGER NOT NULL,
    to_node_id INTEGER NOT NULL,
    edge_type TEXT NOT NULL,
    weight REAL DEFAULT 1.0,  -- for prioritization
    rationale TEXT,  -- why this edge exists
    created_at TEXT NOT NULL,
    FOREIGN KEY (from_node_id) REFERENCES decision_nodes(id),
    FOREIGN KEY (to_node_id) REFERENCES decision_nodes(id),
    UNIQUE(from_node_id, to_node_id, edge_type)
);

-- Context snapshots at decision points
CREATE TABLE decision_context (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    node_id INTEGER NOT NULL,
    context_type TEXT NOT NULL,  -- 'file_state', 'test_result', 'analysis_result', 'user_input'
    content_json TEXT NOT NULL,
    captured_at TEXT NOT NULL,
    FOREIGN KEY (node_id) REFERENCES decision_nodes(id)
);

-- Sessions group related decisions together
CREATE TABLE decision_sessions (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    name TEXT,
    started_at TEXT NOT NULL,
    ended_at TEXT,
    root_node_id INTEGER,
    summary TEXT,
    FOREIGN KEY (root_node_id) REFERENCES decision_nodes(id)
);

-- Link nodes to sessions
CREATE TABLE session_nodes (
    session_id INTEGER NOT NULL,
    node_id INTEGER NOT NULL,
    added_at TEXT NOT NULL,
    PRIMARY KEY (session_id, node_id),
    FOREIGN KEY (session_id) REFERENCES decision_sessions(id),
    FOREIGN KEY (node_id) REFERENCES decision_nodes(id)
);

CREATE INDEX idx_nodes_type ON decision_nodes(node_type);
CREATE INDEX idx_nodes_status ON decision_nodes(status);
CREATE INDEX idx_edges_from ON decision_edges(from_node_id);
CREATE INDEX idx_edges_to ON decision_edges(to_node_id);
CREATE INDEX idx_context_node ON decision_context(node_id);
