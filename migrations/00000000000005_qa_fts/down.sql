-- Revert FTS5 for Q&A interactions
DROP TRIGGER IF EXISTS qa_fts_soft_delete;
DROP TRIGGER IF EXISTS qa_fts_update;
DROP TRIGGER IF EXISTS qa_fts_delete;
DROP TRIGGER IF EXISTS qa_fts_insert;
DROP TABLE IF EXISTS qa_interactions_fts;
