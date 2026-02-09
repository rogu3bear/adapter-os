# Verified Repo Facts

**Date**: 2026-02-04
**Auditor**: Principal Engineer (Precision Mode)
**Commit**: See `git log -1` at time of audit

---

## Summary

This document records verified facts about the AdapterOS codebase. Each claim is backed by specific file paths, line numbers, and identifiers. Items marked "NOT FOUND" must be added or explicitly deferred.

---

## Verified Repo Facts Table

| Claim | Verified? | Evidence | Notes |
|-------|-----------|----------|-------|
| **A) Document Ingest Pipeline** | | | |
| PDF parsing uses `lopdf` | ✅ YES | `crates/adapteros-ingest-docs/src/pdf.rs` | Calls `document.extract_text()` |
| `PageExtractionResult` struct exists | ✅ YES | `crates/adapteros-ingest-docs/src/types.rs:84-100` | Has `has_unextracted_images`, `visual_content_extracted`, `visual_description` |
| `IngestedDocumentWithErrors` tracks partial success | ✅ YES | `crates/adapteros-ingest-docs/src/types.rs:117-130` | Has `pages_with_images`, `pages_with_visual_extraction` |
| OCR is implemented | ❌ NO | `crates/adapteros-ingest-docs/src/pdf.rs:132` | `let (visual_content_extracted, visual_description) = (false, None);` hardcoded |
| `ExtractionConfidence` type exists | ✅ YES | `crates/adapteros-ingest-docs/src/types.rs:169-306` | Has `text_score`, `method`, `degraded_pages`, `degradation_reason`, `total_pages`, `pages_with_text` |
| `ExtractionMethod` enum exists | ✅ YES | `crates/adapteros-ingest-docs/src/types.rs:142-159` | `TextNative`, `OcrTesseract`, `OcrAppleVision`, `Mixed` |
| `ExtractionResult` wrapper exists | ✅ YES | `crates/adapteros-ingest-docs/src/types.rs:312-318` | Bundles `IngestedDocumentWithErrors` + `ExtractionConfidence` |
| `ExtractionConfidence::compute()` exists | ✅ YES | `crates/adapteros-ingest-docs/src/types.rs:213-280` | Canonical v1 scoring with degraded page penalty |
| `IngestedDocumentWithErrors.extraction_confidence` field | ✅ YES | `crates/adapteros-ingest-docs/src/types.rs:131` | Threaded through to callers |
| **B) Trust Gating** | | | |
| `derive_trust_state()` function exists | ✅ YES | `crates/adapteros-db/src/training_datasets/trust.rs:54-101` | Returns: `allowed`, `allowed_with_warning`, `needs_approval`, `blocked`, `unknown` |
| `derive_overall_safety_status()` exists | ✅ YES | `crates/adapteros-db/src/training_datasets/trust.rs:17-36` | Returns: `block`, `warn`, `unknown`, `clean` |
| **C) Runs + Events Tables** | | | |
| `diag_runs` table exists | ✅ YES | `migrations/0272_diagnostics_tables.sql:5-18` | PK: `id TEXT`, FK: `tenant_id → tenants` |
| `diag_events` table exists | ✅ YES | `migrations/0272_diagnostics_tables.sql:40-53` | PK: `id INTEGER AUTOINCREMENT`, FK: `run_id → diag_runs` |
| **D) Inference Traces** | | | |
| `inference_traces` table exists | ✅ YES | `migrations/0192_inference_trace_v2.sql:5-13` | PK: `trace_id TEXT`, FK: `tenant_id → tenants` |
| `inference_trace_tokens` table exists | ✅ YES | `migrations/0192_inference_trace_v2.sql:48-64` | PK: `(trace_id, token_index)`, FK: `trace_id → inference_traces` |
| `inference_trace_receipts` table exists | ✅ YES | `migrations/0192_inference_trace_v2.sql:98-112` | PK: `trace_id`, FK: `trace_id → inference_traces` |
| Receipt schema version | ✅ YES | `crates/adapteros-core/src/receipt_digest.rs:30-42` | V1-V7 defined, current = V7 |
| **E) Replay Endpoints** | | | |
| Session-based: `GET /v1/replay/sessions` | ✅ YES | `crates/adapteros-server-api/src/routes/mod.rs:1437` + `handlers/replay.rs:146` | `list_replay_sessions` handler |
| Session-based: `GET /v1/replay/sessions/{id}` | ✅ YES | `crates/adapteros-server-api/src/routes/mod.rs:1441-1442` | `get_replay_session` handler |
| Session-based: `POST /v1/replay/sessions/{id}/verify` | ✅ YES | `crates/adapteros-server-api/src/routes/mod.rs:1445-1446` | `verify_replay_session` handler |
| Session-based: `POST /v1/replay/sessions/{id}/execute` | ✅ YES | `crates/adapteros-server-api/src/routes/mod.rs:1449-1450` | `execute_replay_session` handler |
| Inference-based: `GET /v1/replay/check/{inference_id}` | ✅ YES | `crates/adapteros-server-api/src/routes/mod.rs:1454-1455` | `check_availability` handler |
| Inference-based: `POST /v1/replay` | ✅ YES | `crates/adapteros-server-api/src/routes/mod.rs:1458-1459` | `execute_replay` handler |
| Inference-based: `GET /v1/replay/history/{inference_id}` | ✅ YES | `crates/adapteros-server-api/src/routes/mod.rs:1462-1463` | `get_replay_history` handler |
| AdapterOS: `POST /v1/adapteros/replay` | ✅ YES | `crates/adapteros-server-api/src/routes/mod.rs:1470-1471` | `adapteros_replay` handler |
| `replay_sessions` table exists | ✅ YES | `migrations/0016_replay_sessions.sql:4-21` | PK: `id TEXT`, FK: `tenant_id → tenants`, `plan_id → plans` |
| `replay_executions` table exists | ✅ YES | `migrations/0127_replay_executions.sql:5-39` | PK: `id TEXT`, FK: `original_inference_id → inference_replay_metadata` |
| `ReplayMatchStatus` enum | ✅ YES | `crates/adapteros-server-api/src/types/replay.rs:74-85` | `Exact`, `Semantic`, `Divergent`, `Error` |
| `ReplayStatus` enum | ✅ YES | `crates/adapteros-server-api/src/types/replay.rs:43-58` | `Available`, `Approximate`, `Degraded`, `FailedInference`, `FailedCapture`, `Unavailable` |
| `ReplayKey` struct | ✅ YES | `crates/adapteros-server-api/src/types/replay.rs:12-40` | Contains `manifest_hash`, `router_seed`, `sampler_params`, `backend`, etc. |
| **F) Router Swap Constraints** | | | |
| `MAX_REASONING_SWAPS` constant | ✅ YES | `crates/adapteros-lora-worker/src/lib.rs:351` | Value: `50` |
| `ReasoningRouterConfig` defaults | ✅ YES | `crates/adapteros-lora-worker/src/reasoning_router.rs:65-78` | `confidence_threshold: 0.82`, `debounce_tokens: 50`, `min_boundary_kind: Sentence` |
| `BoundaryKind` enum | ✅ YES | `crates/adapteros-lora-worker/src/reasoning_router.rs:51-63` | `None`, `Word`, `Sentence`, `Paragraph`, `Explicit` |
| `ReasoningSwapGuard` exists | ✅ YES | `crates/adapteros-lora-worker/src/lib.rs:3889` | Used with `MAX_REASONING_SWAPS` limit |
| **G) Topology Endpoint** | | | |
| `GET /v1/topology` endpoint | ✅ YES | `crates/adapteros-server-api/src/handlers/topology.rs:21-32` | Query param: `preview_text` |
| Returns `TopologyGraph` | ✅ YES | `crates/adapteros-server-api/src/handlers/topology.rs:4` | Imported from `adapteros_api_types` |
| `predicted_path` in response | ✅ YES | `crates/adapteros-api-types/src/topology.rs` | `PredictedPathNode` type used |
| **H) .aos Format** | | | |
| `AOS_FORMAT_VERSION` constant | ✅ YES | `crates/adapteros-aos/src/single_file/format.rs:14` | Value: `2` |
| `MANIFEST_SCHEMA_VERSION` constant | ✅ YES | `crates/adapteros-aos/src/single_file/format.rs:20` | Value: `"1.0.0"` |
| `SingleFileAdapter` struct | ✅ YES | `crates/adapteros-aos/src/single_file/format.rs:26-34` | `manifest`, `weights`, `training_data`, `config`, `lineage`, `signature` |
| Content hash uses BLAKE3 | ✅ YES | `crates/adapteros-aos/src/single_file/format.rs:97-99` | `content_hash: Option<String>` comment says BLAKE3 |
| **I) GAPS (Status)** | | | |
| OCR implementation | ❌ NO | N/A | Hardcoded `(false, None)` - deferred to future iteration |
| `inference_verdicts` table | ✅ ADDED | `migrations/20260204140100_inference_verdicts.sql` | Verdict enum: high/medium/low/paused, evaluator_type: rule/human/model |
| `discrepancy_cases` table | ✅ ADDED | `migrations/20260204140000_discrepancy_cases.sql` | Privacy-conscious storage with store_content flag |

---

## Replay Endpoint Consolidation Required

**Problem**: Three separate replay endpoint families exist:

1. **Session-based** (`handlers/replay.rs`):
   - `GET/POST /v1/replay/sessions`
   - `GET /v1/replay/sessions/{id}`
   - `POST /v1/replay/sessions/{id}/verify`
   - `POST /v1/replay/sessions/{id}/execute`

2. **Inference-based** (`handlers/replay_inference.rs`):
   - `GET /v1/replay/check/{inference_id}`
   - `POST /v1/replay`
   - `GET /v1/replay/history/{inference_id}`

3. **AdapterOS** (`handlers/adapteros_receipts.rs`):
   - `POST /v1/adapteros/replay`

**Decision**: Canonical = Session-based family. Inference-based routes will be deprecated and internally redirect.

---

## Database Tables Summary

| Table | Migration | PK | FKs |
|-------|-----------|----|----|
| `inference_traces` | 0192 | `trace_id TEXT` | `tenant_id → tenants` |
| `inference_trace_tokens` | 0192 | `(trace_id, token_index)` | `trace_id → inference_traces` |
| `inference_trace_receipts` | 0192 | `trace_id` | `trace_id → inference_traces` |
| `replay_sessions` | 0016 | `id TEXT` | `tenant_id → tenants`, `plan_id → plans` |
| `replay_executions` | 0127 | `id TEXT` | `original_inference_id → inference_replay_metadata`, `tenant_id → tenants` |
| `diag_runs` | 0272 | `id TEXT` | `tenant_id → tenants` |
| `diag_events` | 0272 | `id INTEGER` | `run_id → diag_runs` |

---

## Corrections to Previous Summary

| Previous Claim | Correction |
|----------------|------------|
| "migration 0253 has inference_traces" | Replaced by 0192 (v2 rebuild) |
| "migration 0272_diagnostic_telemetry.sql" | Correct name: `0272_diagnostics_tables.sql` |
| "ExtractionConfidence type NOT FOUND" | **FOUND** at `types.rs:169-306` (added 2026-02-04) |

---

## Work Plan Status (2026-02-04)

| Step | Description | Status |
|------|-------------|--------|
| 0 | Baseline Audit | ✅ COMPLETE |
| 1 | Collapse Replay Semantics | ✅ COMPLETE (`docs/replay.md` written) |
| 2 | Add Extraction Confidence | ✅ ALREADY EXISTS |
| 3 | Add Discrepancy Cases | ✅ COMPLETE (migration + DB module + API handlers) |
| 4 | Verdict Loop (Minimum Viable) | ✅ COMPLETE (migration + DB module + API handlers + derive_rule_verdict) |
| 5 | UI Integration | ✅ COMPLETE (API client methods added) |
| 6 | Training Loop Hooks | ✅ COMPLETE (`aosctl train-from-discrepancies` exists) |

## API Endpoints Added

### Discrepancies (`/v1/discrepancies`)
- `POST /v1/discrepancies` - Create discrepancy case
- `GET /v1/discrepancies` - List with status filter
- `GET /v1/discrepancies/export` - Export confirmed errors (JSONL)
- `GET /v1/discrepancies/{id}` - Get single case
- `PATCH /v1/discrepancies/{id}/resolve` - Update resolution

### Verdicts (`/v1/verdicts`)
- `POST /v1/verdicts` - Create/upsert verdict
- `POST /v1/verdicts/derive` - Derive rule-based verdict
- `GET /v1/verdicts/{inference_id}` - Get verdict for inference

### CLI Commands
- `aosctl train-from-discrepancies` - Export confirmed errors to JSONL for training
  - `--status <status>` - Filter by resolution status (default: confirmed_error)
  - `--output <path>` - Write to file instead of stdout
  - `--dataset <id>` - Append to existing dataset
  - `--dry-run` - Preview what would be exported
  - `--include-incomplete` - Include cases without ground_truth

### UI API Client Methods
- `create_discrepancy()` - Create a new discrepancy case
- `list_discrepancy_cases()` - List with optional status filter
- `get_discrepancy()` - Get single case by ID
- `resolve_discrepancy()` - Update resolution status
- `get_inference_verdict()` - Get verdict for inference
- `derive_rule_verdict()` - Derive rule-based verdict
- `create_replay_session()` - Create replay session
- `get_replay_session()` - Get session details
- `verify_replay_session()` - Verify session integrity
