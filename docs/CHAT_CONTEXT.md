# Chat Context and RAG Collection Scoping

## Decision

Chat uses one effective `collection_id` per inference request to preserve existing deterministic retrieval behavior.

## Scopes

- Per-session collection:
  - Created when a user attaches a document in chat.
  - Stored as `active_collection_id` on `ChatState`.
  - Sent as `collection_id` in `StreamingInferRequest` when present.

- Persistent knowledge collection:
  - Stored as `knowledge_collection_id` on `ChatState`.
  - Represents user-level long-lived knowledge.

## Composition Rule

When both session and persistent knowledge should apply, composition happens at collection membership level:

- Create/fork a session-effective collection.
- Add document references from knowledge + session documents via `collection_documents` join table.
- Do not merge retrieval results at query time.

This keeps server retrieval scoped to one collection while preserving deterministic ranking semantics.

## Security and Trust Notes

- `entity_id` and page-context inputs from client are untrusted.
- Server-side enrichment must use tenant-scoped lookups and ignore out-of-tenant entities.
- Keep enrichment compact and size-bounded before prompt injection.
