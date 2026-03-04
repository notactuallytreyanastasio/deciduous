-- Deciduous Decision Graph Schema
-- Combined schema for SQLite database initialization

-- Decision Graph Nodes
CREATE TABLE IF NOT EXISTS decision_nodes (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    change_id TEXT UNIQUE,
    node_type TEXT NOT NULL,
    title TEXT NOT NULL,
    description TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    metadata_json TEXT
);

-- Decision Graph Edges (connections between nodes)
CREATE TABLE IF NOT EXISTS decision_edges (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    change_id TEXT UNIQUE,
    from_node_id INTEGER NOT NULL,
    to_node_id INTEGER NOT NULL,
    from_change_id TEXT,
    to_change_id TEXT,
    edge_type TEXT NOT NULL,
    weight REAL DEFAULT 1.0,
    rationale TEXT,
    created_at TEXT NOT NULL,
    FOREIGN KEY (from_node_id) REFERENCES decision_nodes(id),
    FOREIGN KEY (to_node_id) REFERENCES decision_nodes(id),
    UNIQUE(from_node_id, to_node_id, edge_type)
);

-- Command Log (audit trail)
CREATE TABLE IF NOT EXISTS command_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    command TEXT NOT NULL,
    description TEXT,
    working_dir TEXT,
    exit_code INTEGER,
    stdout TEXT,
    stderr TEXT,
    started_at TEXT NOT NULL,
    completed_at TEXT,
    duration_ms INTEGER,
    decision_node_id INTEGER,
    FOREIGN KEY (decision_node_id) REFERENCES decision_nodes(id)
);

-- Document Attachments
CREATE TABLE IF NOT EXISTS node_documents (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    change_id TEXT NOT NULL UNIQUE,
    node_id INTEGER NOT NULL,
    node_change_id TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    original_filename TEXT NOT NULL,
    storage_filename TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    file_size INTEGER NOT NULL,
    description TEXT,
    description_source TEXT NOT NULL DEFAULT 'none',
    attached_at TEXT NOT NULL,
    attached_by TEXT,
    detached_at TEXT,
    FOREIGN KEY (node_id) REFERENCES decision_nodes(id)
);

-- Decision Sessions
CREATE TABLE IF NOT EXISTS decision_sessions (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    name TEXT,
    started_at TEXT NOT NULL,
    ended_at TEXT,
    root_node_id INTEGER,
    summary TEXT,
    FOREIGN KEY (root_node_id) REFERENCES decision_nodes(id)
);

-- Session-Node Links
CREATE TABLE IF NOT EXISTS session_nodes (
    session_id INTEGER NOT NULL,
    node_id INTEGER NOT NULL,
    added_at TEXT NOT NULL,
    PRIMARY KEY (session_id, node_id),
    FOREIGN KEY (session_id) REFERENCES decision_sessions(id),
    FOREIGN KEY (node_id) REFERENCES decision_nodes(id)
);

-- Indexes for performance
CREATE INDEX IF NOT EXISTS idx_nodes_type ON decision_nodes(node_type);
CREATE INDEX IF NOT EXISTS idx_nodes_status ON decision_nodes(status);
CREATE INDEX IF NOT EXISTS idx_nodes_change_id ON decision_nodes(change_id);
CREATE INDEX IF NOT EXISTS idx_edges_from ON decision_edges(from_node_id);
CREATE INDEX IF NOT EXISTS idx_edges_to ON decision_edges(to_node_id);
CREATE INDEX IF NOT EXISTS idx_edges_change_id ON decision_edges(change_id);
CREATE INDEX IF NOT EXISTS idx_command_started_at ON command_log(started_at);
CREATE INDEX IF NOT EXISTS idx_command_decision_node ON command_log(decision_node_id);
CREATE INDEX IF NOT EXISTS idx_docs_node_id ON node_documents(node_id);
CREATE INDEX IF NOT EXISTS idx_docs_content_hash ON node_documents(content_hash);
CREATE INDEX IF NOT EXISTS idx_docs_change_id ON node_documents(change_id);
