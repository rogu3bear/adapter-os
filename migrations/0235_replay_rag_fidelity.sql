-- Migration: Add RAG fidelity tracking to replay metadata
-- Purpose: Track actual RAG document availability during replay execution
-- to prevent silent degradation and false evidence comparability claims.

ALTER TABLE inference_replay_metadata
    ADD COLUMN rag_available_count INTEGER;

ALTER TABLE inference_replay_metadata
    ADD COLUMN rag_total_count INTEGER;

ALTER TABLE inference_replay_metadata
    ADD COLUMN rag_fidelity TEXT;

ALTER TABLE inference_replay_metadata
    ADD COLUMN rag_staleness_checked_at TEXT;

-- Index for staleness checker background job to efficiently find
-- replay metadata with RAG docs that need staleness checking
CREATE INDEX idx_replay_rag_staleness
    ON inference_replay_metadata(rag_staleness_checked_at)
    WHERE rag_doc_ids_json IS NOT NULL;

-- Index for querying by RAG fidelity status
CREATE INDEX idx_replay_rag_fidelity
    ON inference_replay_metadata(rag_fidelity)
    WHERE rag_fidelity IS NOT NULL;
