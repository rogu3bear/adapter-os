# Optional RAG Golden Scenario

Goal: mirror the `doc-chat` scenario with a deterministic RAG flow that verifies
ingestion, retrieval, and replay stability.

- Tenant: `tenant-rag-golden`
- Collection: `rag-golden`
- Example doc: `examples/docs/rag-golden/note.md`
- Model/backend: reuse `qwen2.5-7b-mlx` with deterministic routing (strict mode)

## Flow
1. Ingest the example doc into the tenant collection using the dataset ingestion
   path from `USER_GUIDE_DATASETS.md` (keep the document ID stable, e.g.,
   `rag-golden-note`).
2. Run a RAG query via `/v1/infer/stream` with `collection_id = "rag-golden"`
   and prompt: `Where does Aurora Station run inference?`. The response should
   cite the indexed card and return evidence with `rag_doc_ids =
   ["rag-golden-note"]` (see `RAG_DETERMINISM.md` for ordering rules).
3. Capture `inference_id` and run `replay_inference` with
   `use_original_rag_docs = true`; assert that `rag_doc_ids`, `rag_scores`, and
   `context_hash` match the original inference and that `degraded = false`.

## Notes
- Keep the collection size to a single document to avoid variability.
- Prefer deterministic seeds and backend profiles identical to `doc-chat`.
- Record failures as golden deltas rather than updating expectations silently.

MLNavigator Inc 2025-12-08.

