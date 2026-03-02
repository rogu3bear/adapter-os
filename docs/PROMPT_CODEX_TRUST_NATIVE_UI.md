# Prompt: Codex Trust-Native UI

This document is the source-of-truth plan for the AdapterOS trust-native chat UX.

## Product Direction

AdapterOS is deterministic and auditable by design. The UI must make that visible by default in chat:

- Show where answers come from.
- Show what adapters contributed.
- Show receipt/replay proof paths.
- Keep deterministic/replay semantics intact.

Trust is not an optional mode; it is the default rendering language.

## Hard Constraints

- Minimal diffs, prefer existing files/components.
- No new CSS framework; use existing utilities and focused CSS additions.
- Backward-compatible serde changes (`#[serde(default)]` on additive fields).
- Keep heavy payloads (full citations) out of localStorage when possible.
- Preserve deterministic ordering and retrieval contracts.
- Do not weaken tenant boundaries or policy enforcement.

## Required Surfaces

1. Chat-first landing and empty-state behavior.
2. Per-response provenance rendering (citations/adapters/receipt links).
3. Attach-to-RAG flow from chat (session-scoped collection).
4. Context wiring from route/entity selection into chat inference context.
5. Lifecycle feedback via system messages (training started/ready, adapter set changes).
6. Active configuration line above input (static facts only).
7. Stream loading phase visibility.

## Citation Quality Policy (UI Editorial Rule)

Citations are shown based on `relevance_score` quality tiers:

- `strong`: `score >= 0.7`
- `weak`: `0.3 <= score < 0.7`
- `none`: `< 0.3` or missing citations

Rendering rules:

- Provenance border is tied to quality tier, not citation existence.
- Trust strip chips show only strong citations.
- Weak citations appear as collapsed summary text (e.g. "and 2 lower-confidence sources").

Thresholds are constants in chat rendering code (not user-configurable).

## Replay Divergence UX

When replay metadata indicates environment drift, show:

- Banner: "Environment changed since original response"
- Compact reason list (adapter retrained/model updated/policy modified)
- Side-by-side compare view

When no divergence and outputs match, show a single success confirmation.

## Collection Scoping

- Session RAG collection: `active_collection_id` on `ChatState`.
- Persistent knowledge collection: `knowledge_collection_id` on `ChatState`.
- Server receives one `collection_id` per request.
- If both sources are needed, merge at collection membership level (join-table references), not query-time retrieval.

## Test Expectations

All trust surfaces must be covered by:

1. Visual regression screenshots.
2. Journey tests (SSE-driven flows).
3. Demo operation walkthrough (narrated flow with mocks).

Keep `helpers/sse.ts` and demo mocks in sync with streaming event contracts.
