# KV RAG fallback and pgvector guardrails

- KV is the primary path for embeddings in `kv_primary`/`kv_only`; dual-write keeps SQL/pgvector hydrated for validation.
- Deterministic retrieval order is fixed: score DESC, doc_id ASC; both KV and SQL implementations must produce identical ordering for the same state.
- Dual-write drift checks warn if KV results diverge from SQL; use `StorageMode::DualWrite` while validating and `StorageMode::KvPrimary` only after KV parity is proven.
- Backfill: use the Db backfill helper to hydrate KV from `rag_documents` + `rag_document_embeddings` before enabling KV reads.
- Pgvector/SQLite remains an optional secondary; when enabled, keep it strictly read-only once `kv_only` is active to avoid split-brain.
- Tenant isolation is enforced in both stores; never query without `tenant_id` + `model_hash` scoping.
- Failure policy: in `kv_primary`, retrieval falls back to SQL if KV errors; in `kv_only`, errors fail fast to avoid silent divergence.

MLNavigator Inc 2025-12-05.

