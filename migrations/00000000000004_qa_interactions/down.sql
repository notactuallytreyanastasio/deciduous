-- Revert Q&A interactions
DROP INDEX IF EXISTS idx_qa_deleted_at;
DROP INDEX IF EXISTS idx_qa_inserted_at;
DROP TABLE IF EXISTS qa_interactions;
