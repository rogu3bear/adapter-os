# Canonical User Workflow (Ingest to Verifiable Replay)

> **Agent note:** Code is authoritative. Routes and file paths may have changed. Re-verify in `crates/adapteros-ui/src/lib.rs` and `crates/adapteros-server-api/src/routes/` before trusting. See [CANONICAL_SOURCES.md](CANONICAL_SOURCES.md) and [DOCS_AUDIT_2026-02-18.md](DOCS_AUDIT_2026-02-18.md).

**Canonical source:** `crates/adapteros-ui/src/lib.rs`, `crates/adapteros-server-api/src/routes/`  
**Last Updated:** 2026-02-18

This is the canonical AdapterOS user journey using existing surfaces only:

1. Ingest source documents.
2. Build a dataset from those documents.
3. Train (or select) an adapter.
4. Chat with adapter routing visible.
5. Escalate reasoning path when needed.
6. Inspect run receipts, replay, and token accounting.

No separate "demo mode" feature is required. This document maps directly to current routes, handlers, and tests.

## Preconditions

- Start stack: `AOS_DEV_NO_AUTH=1 ./start` for full UI access during guided walkthroughs.
- Route ownership: `/Users/star/Dev/adapter-os/crates/adapteros-ui/src/lib.rs`.
- API route ownership: `/Users/star/Dev/adapter-os/crates/adapteros-server-api/src/routes/mod.rs` and `/Users/star/Dev/adapter-os/crates/adapteros-server-api/src/routes/training_routes.rs`.

## 10-Minute Flow

| Minute | User action | Existing surface | Completion signal | Connects to next |
|---|---|---|---|---|
| 0-1 | Open Chat and choose run config inputs | `/chat` and `/chat/:session_id` in `/Users/star/Dev/adapter-os/crates/adapteros-ui/src/pages/chat.rs` | Session is active and prompt can be sent | Establishes deterministic baseline inputs (seed/config/policy/budget) |
| 1-3 | Upload 1-2 documents | `/documents` upload dialog in `/Users/star/Dev/adapter-os/crates/adapteros-ui/src/pages/documents.rs` + `POST /v1/documents/upload` | Document rows visible; status/progress shown; retry path available | Provides source artifacts for dataset creation |
| 3-4 | Create dataset from documents | `/datasets` and `/datasets/:id` in `/Users/star/Dev/adapter-os/crates/adapteros-ui/src/pages/datasets.rs` + `POST /v1/datasets/from-documents` | Dataset appears with rows/statistics/preview | Produces training-ready input with provenance |
| 4-6 | Start training job or select existing adapter | `/training` in `/Users/star/Dev/adapter-os/crates/adapteros-ui/src/pages/training/mod.rs` + `/v1/training/jobs` `/v1/training/start` | Job progress/logs update; adapter is present in `/adapters` | Adapter identity becomes routable in chat |
| 6-7 | Chat against domain prompt and observe routing magnets | Chat adapter bar and routing state in `/Users/star/Dev/adapter-os/crates/adapteros-ui/src/pages/chat.rs` (`AdapterMagnet`) | Active adapters and routing indicators update during response | Produces trace/run artifacts for verification |
| 7-8 | Trigger higher-cost reasoning path | Same chat flow with reasoning mode/threshold path (existing control-plane behavior) | Response labeled by path/policy in UI context | Demonstrates intentional compute escalation |
| 8-10 | Open run detail, receipt, replay, token tabs | `/runs` and `/runs/:id` in `/Users/star/Dev/adapter-os/crates/adapteros-ui/src/pages/flight_recorder.rs` | Receipt digest visible, replay verify executes, token/cached attribution visible | Closes loop with verifiable execution + reproducibility |

## Route and API Anchors

### UI routes

- `/documents`, `/documents/:id`
- `/datasets`, `/datasets/:id`
- `/training`
- `/adapters`, `/adapters/:id`
- `/chat`, `/chat/:session_id`
- `/runs`, `/runs/:id`
- `/routing`

Source of truth: `/Users/star/Dev/adapter-os/crates/adapteros-ui/src/lib.rs`.

### API routes used by this flow

- Documents: `/v1/documents/upload`, `/v1/documents`, `/v1/documents/{id}`, `/v1/documents/{id}/retry`
- Datasets: `/v1/datasets`, `/v1/datasets/from-documents`, `/v1/datasets/{dataset_id}/statistics`, `/v1/datasets/{dataset_id}/preview`
- Training: `/v1/training/jobs`, `/v1/training/start`, `/v1/training/jobs/{job_id}/progress`, `/v1/training/jobs/{job_id}/logs`
- Inference/streaming: `/v1/infer`, `/v1/infer/stream`, `/v1/infer/stream/progress`
- Routing visibility: `/v1/routing/decisions`, `/v1/routing/history`, `/v1/routing/chain`
- Run detail and diagnostics: `/v1/diag/runs`, `/v1/diag/runs/{trace_id}`, `/v1/diag/runs/{trace_id}/events`, `/v1/traces/inference/{trace_id}`
- Replay and receipts: `/v1/replay`, `/v1/replay/check/{inference_id}`, `/v1/replay/history/{inference_id}`, `/v1/adapteros/receipts/{digest}`, `/v1/runs/{run_id}/evidence`

Source of truth: `/Users/star/Dev/adapter-os/crates/adapteros-server-api/src/routes/mod.rs` and `/Users/star/Dev/adapter-os/crates/adapteros-server-api/src/routes/training_routes.rs`.

## Acceptance Criteria (Canonical Flow)

- Every page in the flow shows explicit loading/empty/error/retry behavior.
- Chat displays adapter routing in a human-readable format.
- Run detail links trace, receipt, routing, and token accounting in one view.
- Replay verification executes from existing run/receipt surfaces.
- Identical config + seed path can be verified with determinism/replay tests.

## Verification Commands (Smallest Relevant)

- `cargo check -p adapteros-ui`
- `cargo test --test determinism_core_suite -- --test-threads=8`
- `cargo test --test determinism_replay_harness -- --test-threads=1 --nocapture`
- `cargo test --test prefix_kv_cache_integration`
- Optional end-to-end script already in repo: `./scripts/golden_path_adapter_chat.sh`

## Determinism and Audit Notes

- Determinism substrate and tie-break/Q15 expectations: `/Users/star/Dev/adapter-os/docs/DETERMINISM.md`.
- Receipt structure and verification model: `/Users/star/Dev/adapter-os/docs/CRYPTO_RECEIPTS.md`.
- Token cache attribution economics and receipt binding: `/Users/star/Dev/adapter-os/docs/TOKEN_CACHING_ECONOMICS.md`.
