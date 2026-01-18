-- FTS5 full-text search for Q&A interactions
-- Uses external content mode: data stored in qa_interactions, FTS stores index only

CREATE VIRTUAL TABLE qa_interactions_fts USING fts5(
    user_prompt,
    response,
    content='qa_interactions',
    content_rowid='id'
);

-- Populate FTS with existing non-deleted data
INSERT INTO qa_interactions_fts(rowid, user_prompt, response)
SELECT id, user_prompt, response FROM qa_interactions WHERE deleted_at IS NULL;

-- Trigger: keep FTS in sync on INSERT
CREATE TRIGGER qa_fts_insert AFTER INSERT ON qa_interactions BEGIN
    INSERT INTO qa_interactions_fts(rowid, user_prompt, response)
    VALUES (new.id, new.user_prompt, new.response);
END;

-- Trigger: keep FTS in sync on DELETE
CREATE TRIGGER qa_fts_delete AFTER DELETE ON qa_interactions BEGIN
    INSERT INTO qa_interactions_fts(qa_interactions_fts, rowid, user_prompt, response)
    VALUES ('delete', old.id, old.user_prompt, old.response);
END;

-- Trigger: keep FTS in sync on UPDATE
CREATE TRIGGER qa_fts_update AFTER UPDATE ON qa_interactions BEGIN
    INSERT INTO qa_interactions_fts(qa_interactions_fts, rowid, user_prompt, response)
    VALUES ('delete', old.id, old.user_prompt, old.response);
    INSERT INTO qa_interactions_fts(rowid, user_prompt, response)
    VALUES (new.id, new.user_prompt, new.response);
END;

-- Trigger: remove from FTS when soft-deleted
CREATE TRIGGER qa_fts_soft_delete AFTER UPDATE OF deleted_at ON qa_interactions
WHEN new.deleted_at IS NOT NULL AND old.deleted_at IS NULL BEGIN
    INSERT INTO qa_interactions_fts(qa_interactions_fts, rowid, user_prompt, response)
    VALUES ('delete', old.id, old.user_prompt, old.response);
END;
