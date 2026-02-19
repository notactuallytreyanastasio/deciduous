CREATE TABLE themes (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    change_id TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL UNIQUE,
    color TEXT NOT NULL DEFAULT '#6b7280',
    description TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE node_themes (
    node_id INTEGER NOT NULL,
    theme_id INTEGER NOT NULL,
    source TEXT NOT NULL DEFAULT 'manual',
    created_at TEXT NOT NULL,
    PRIMARY KEY (node_id, theme_id),
    FOREIGN KEY (node_id) REFERENCES decision_nodes(id) ON DELETE CASCADE,
    FOREIGN KEY (theme_id) REFERENCES themes(id) ON DELETE CASCADE
);

CREATE INDEX idx_themes_name ON themes(name);
CREATE INDEX idx_themes_change_id ON themes(change_id);
CREATE INDEX idx_node_themes_node ON node_themes(node_id);
CREATE INDEX idx_node_themes_theme ON node_themes(theme_id);
