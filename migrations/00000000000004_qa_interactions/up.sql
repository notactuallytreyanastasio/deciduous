-- Q&A interactions storage for user reference
-- Stores user questions, full prompts, and Claude's responses

CREATE TABLE qa_interactions (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    user_prompt TEXT NOT NULL,
    total_prompt TEXT NOT NULL,
    response TEXT NOT NULL,
    context_json TEXT,
    inserted_at TEXT NOT NULL,
    deleted_at TEXT
);

CREATE INDEX idx_qa_inserted_at ON qa_interactions(inserted_at);
CREATE INDEX idx_qa_deleted_at ON qa_interactions(deleted_at);
