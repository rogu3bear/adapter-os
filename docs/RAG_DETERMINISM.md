# RAG Determinism Contract

## Overview

AdapterOS guarantees deterministic RAG (Retrieval-Augmented Generation) results
through a strict ordering contract and comprehensive evidence tracking.

## Ordering Contract (Ruleset #2)

Documents retrieved from the RAG index are ordered by:

1. **Score DESC** - Highest relevance score first
2. **doc_id ASC** - Alphabetical document ID for tie-breaking

This ensures that two identical queries against identical database state
return documents in the same order every time.

### Implementation

The deterministic sorting is implemented in `crates/adapteros-lora-rag/src/pgvector.rs`:

```rust
// Deterministic sorting: score DESC, doc_id ASC
scored_docs.sort_by(|(row_a, score_a), (row_b, score_b)| {
    score_b
        .partial_cmp(score_a)
        .unwrap_or(std::cmp::Ordering::Equal)
        .then_with(|| row_a.doc_id.cmp(&row_b.doc_id))
});
```

## Evidence Tracking

Every RAG-enabled inference creates `inference_evidence` records with:

| Field | Description |
|-------|-------------|
| `rag_doc_ids` | JSON array of document IDs in retrieval order |
| `rag_scores` | JSON array of relevance scores (parallel to doc_ids) |
| `rag_collection_id` | Collection used for scoped retrieval |
| `document_id` | Individual document contributing to context |
| `chunk_id` | Specific chunk within the document |
| `relevance_score` | Cosine similarity score for this chunk |
| `rank` | Position in result set (0 = most relevant) |
| `context_hash` | BLAKE3 hash of concatenated context |

## Replay Support

Replay sessions can use original RAG documents via `use_original_rag_docs: true`.

### Degraded Mode

If some original documents have been deleted since the original inference:
- Replay continues with available documents (preserving order)
- Response includes `degraded: true`
- `missing_doc_ids` lists documents that could not be found

This allows for "best effort" replay while being transparent about data availability.

## Verification

### Query Evidence for an Inference

```sql
SELECT inference_id, rag_doc_ids, rag_scores, rag_collection_id
FROM inference_evidence
WHERE inference_id = 'your-inference-id'
LIMIT 1;
```

### Verify Determinism

Run the same query twice against the same database state:

```bash
# Query 1
curl -X POST http://localhost:8080/v1/infer/stream \
  -H "Content-Type: application/json" \
  -d '{"prompt": "test query", "collection_id": "col-123"}'

# Query 2 (should return same doc order)
curl -X POST http://localhost:8080/v1/infer/stream \
  -H "Content-Type: application/json" \
  -d '{"prompt": "test query", "collection_id": "col-123"}'
```

Compare the `rag_doc_ids` in the evidence records - they should be identical.

## Related Documentation

- [CLAUDE.md Section 8.4](../CLAUDE.md#84-rag-vs-adapter-positioning) - RAG vs Adapter positioning and when to use each
- [docs/LIFECYCLE.md](LIFECYCLE.md) - Adapter lifecycle states
- [docs/ARCHITECTURE_PATTERNS.md](ARCHITECTURE_PATTERNS.md) - System patterns
- [CLAUDE.md](../CLAUDE.md) - Developer guide
