-- Migration: RAG Evidence Extension
-- Purpose: Add aggregate RAG trace fields to inference_evidence for citation tracking
--          and replay support with original RAG documents.
-- Dependencies: 0094_documents_collections.sql (inference_evidence, document_collections tables)
-- Policy: Determinism Ruleset (#2) - score DESC, doc_id ASC tie-breaking

-- Add aggregate RAG fields to inference_evidence
-- These store the complete RAG retrieval context for an inference

-- JSON array of document IDs in retrieval order, e.g., ["doc-1", "doc-2"]
ALTER TABLE inference_evidence ADD COLUMN rag_doc_ids TEXT;

-- JSON array of relevance scores parallel to rag_doc_ids, e.g., [0.95, 0.87]
ALTER TABLE inference_evidence ADD COLUMN rag_scores TEXT;

-- Collection ID used for scoped RAG retrieval
ALTER TABLE inference_evidence ADD COLUMN rag_collection_id TEXT
    REFERENCES document_collections(id) ON DELETE SET NULL;

-- Index for efficient evidence lookup by collection
CREATE INDEX IF NOT EXISTS idx_inference_evidence_collection
    ON inference_evidence(rag_collection_id)
    WHERE rag_collection_id IS NOT NULL;

-- Extend replay_sessions to store RAG document state for deterministic replay
-- JSON object: {"doc_ids": [...], "scores": [...], "collection_id": "...", "embedding_model_hash": "..."}
ALTER TABLE replay_sessions ADD COLUMN rag_state_json TEXT;
