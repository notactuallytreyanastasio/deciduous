CREATE TABLE node_documents (
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

CREATE INDEX idx_docs_node_id ON node_documents(node_id);
CREATE INDEX idx_docs_content_hash ON node_documents(content_hash);
CREATE INDEX idx_docs_change_id ON node_documents(change_id);
