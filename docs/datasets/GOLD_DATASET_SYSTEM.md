# GOLD DATASET SYSTEM (AdapterOS)

## Purpose
Define the production dataset pipeline for high-signal AdapterOS training data, from raw source intake to promotion-ready training runs.

This spec is operational and bound to existing AdapterOS filesystem and API surfaces.

## System boundaries
- In scope: dataset intake, extraction to canonical rows, quality scoring, rejection handling, staged scale gates (`10k -> 100k -> 1M`), and promotion eligibility.
- Out of scope: model architecture changes, new API design, and speculative endpoints.

## Canonical AdapterOS surfaces
- Local dataset artifacts:
  - `var/datasets/generated/spark/train/`
  - `var/datasets/generated/spark/pipeline/`
  - `var/datasets/generated/spark/pipeline/reports/`
- Canonical runbook references:
  - `/Users/star/Dev/adapter-os/docs/runbooks/FULL_TUNING_QWEN35_27B.md`
- Dataset APIs:
  - `POST /v1/datasets`
  - `POST /v1/datasets/from-documents`
  - `POST /v1/datasets/from-text`
  - `POST /v1/datasets/from-chat`
  - `POST /v1/datasets/{dataset_id}/preprocess`
  - `GET /v1/datasets/{dataset_id}/preprocess/status`
  - `POST /v1/datasets/{dataset_id}/validate`
  - `GET /v1/datasets/{dataset_id}/statistics`
  - `GET /v1/datasets/{dataset_id}/preview`
  - `POST /v1/datasets/{dataset_id}/versions`
  - `GET /v1/datasets/{dataset_id}/versions/{version_id}/safety`
  - `POST /v1/datasets/{dataset_id}/versions/{version_id}/trust-override`
- Training APIs:
  - `POST /v1/training/start`
  - `GET /v1/training/jobs/{job_id}`
  - `GET /v1/training/jobs/{job_id}/progress`
  - `GET /v1/training/jobs/{job_id}/metrics`
  - `GET /v1/training/jobs/{job_id}/report`
  - `GET /v1/training/dataset_versions/{dataset_version_id}/manifest`
  - `GET /v1/training/dataset_versions/{dataset_version_id}/rows`
- Promotion and gates:
  - `GET /v1/cp/promotion-gates/{cpid}`
  - `GET /v1/golden/{run_id}/gates`
  - `POST /v1/golden/{run_id}/promote`
  - `POST /v1/golden/{stage}/rollback`

## Pipeline phases

## 1) Source intake
Goal: accept only attributable, versionable sources with reproducible lineage.

### Accepted sources
- Curated docs and codebase artifacts from trusted repos/workspaces.
- User-provided text/documents via dataset APIs.
- Conversation-derived exemplars via `POST /v1/datasets/from-chat` only when provenance metadata is present.

### Intake contract
Each intake unit MUST record:
- `source_id`: stable source handle.
- `source_type`: `document | code | chat | text`.
- `origin_uri`: file path, repo ref, or chat identifier.
- `license_class`: `internal | approved_external | restricted`.
- `collected_at`: UTC timestamp.
- `collector`: service/user principal.
- `sha256`: content fingerprint.

### Intake reject conditions
Hard reject source before extraction if:
- missing provenance fields,
- restricted license,
- unreadable/binary-corrupt content,
- duplicate `(origin_uri, sha256)` already accepted in active dataset stream.

## 2) Extraction to canonical training rows
Goal: transform heterogeneous sources into deterministic supervised rows.

### Canonical row schema (`adapteros_sft_row_v1`)
Each row MUST include:
- `row_id`: deterministic hash of normalized content + source pointer.
- `instruction`: user intent / task.
- `context`: bounded supporting content.
- `response`: expected high-quality completion.
- `citations`: array of source anchors (`source_id`, span/path locator).
- `policy_tags`: safety/policy classes (for routing and redaction audit).
- `quality_signals`:
  - `grounding_score` (0-1)
  - `specificity_score` (0-1)
  - `adapteros_relevance_score` (0-1)
  - `determinism_risk_score` (0-1, lower is better)
- `split`: `train | eval | holdout`.
- `spec_version`: fixed at `1.0`.

### Extraction invariants
- Deterministic normalization (whitespace, unicode normalization, line ending normalization).
- Citation completeness: every non-trivial factual claim in `response` must map to `citations`.
- No unresolved placeholders (`TODO`, `TBD`, `???`, template markers).
- Max row token budget defined by current training profile; over-budget rows are trimmed by deterministic strategy (head+salient-window), never ad hoc.

### Extraction outputs (filesystem)
- `var/datasets/generated/spark/train/*.jsonl` (canonical rows)
- `var/datasets/generated/spark/manifest_collections.json`
- `var/datasets/generated/spark/pipeline/citation_index_collections.json`
- `var/datasets/generated/spark/pipeline/reports/latest_audit_real_artifacts.json`

## 3) Quality rubric
Goal: enforce high signal, grounded, adapter-relevant examples.

Each row receives 0-5 per axis:
- Grounding fidelity:
  - 5: directly supported by citations.
  - 3: mostly supported, minor implied step.
  - 0-1: unsupported or fabricated.
- AdapterOS specificity:
  - 5: uses real AdapterOS concepts/paths/endpoints/contracts.
  - 3: partially specific.
  - 0-1: generic LLM advice.
- Instruction-response fit:
  - 5: complete, constraint-following, minimal ambiguity.
  - 3: partially complete.
  - 0-1: misses core request.
- Safety/compliance:
  - 5: no policy concern.
  - 0: disallowed content or unsafe guidance.
- Determinism friendliness:
  - 5: stable outputs and bounded variance.
  - 0-1: encourages stochastic/irreproducible behavior.

### Row acceptance threshold
- Accept if all are true:
  - `grounding_score >= 0.85`
  - `adapteros_relevance_score >= 0.80`
  - `determinism_risk_score <= 0.20`
  - rubric composite >= `4.2/5.0`
- Otherwise route to reject taxonomy.

## 4) Reject taxonomy
Goal: classify rejects for remediation and trend tracking.

Use exactly one primary reject code per row:
- `R1_UNGROUNDED`: claims not backed by citations.
- `R2_LOW_SIGNAL`: generic/boilerplate content with low AdapterOS value.
- `R3_SCHEMA_INVALID`: missing required fields or invalid `spec_version`.
- `R4_DUPLICATE`: semantic duplicate of accepted row cluster.
- `R5_POLICY`: policy/safety violation.
- `R6_LICENSE`: unapproved or unclear rights.
- `R7_DETERMINISM`: content likely to induce non-reproducible behavior.
- `R8_FORMAT_CORRUPT`: malformed JSONL or encoding issues.

### Reject handling
- `R1/R2/R7`: send to rewriting queue with targeted prompts.
- `R3/R8`: send to extraction bugfix queue.
- `R4`: keep one canonical exemplar, archive others.
- `R5/R6`: quarantine and exclude from future auto-intake until manual override.

## 5) Staged dataset gates (`10k -> 100k -> 1M`)
Goal: scale only when quality and training behavior are stable.

## Stage A: 10k rows (pilot)
Entry:
- >= 10,000 accepted train rows.
- >= 1,000 eval rows and >= 1,000 holdout rows.
- Reject rate <= 25% over latest extraction cycle.

Required checks:
- `POST /v1/datasets/{dataset_id}/validate` passes.
- `GET /v1/datasets/{dataset_id}/statistics` confirms split counts and no schema drift.
- Dataset version created via `POST /v1/datasets/{dataset_id}/versions`.
- Training smoke run via `POST /v1/training/start` (single repo/base model profile).
- `GET /v1/training/jobs/{job_id}/report` shows no contract violations.

Exit to Stage B when:
- Pilot run completes with no hard gate failures,
- manifest hash and `data_spec_hash` are recorded and stable,
- trust/safety checks pass for produced dataset version.

## Stage B: 100k rows (pre-production)
Entry:
- >= 100,000 accepted train rows.
- Domain/source balance constraints met (no single source family > 35%).
- Reject rate <= 18% over previous two cycles.

Required checks:
- Re-run preprocess and validate APIs; no schema evolution without version bump.
- Run at least 2 training jobs across policy variants/backends used in production profile (excluding CPU fallback for full-tune).
- `GET /v1/training/jobs/{job_id}/metrics` demonstrates non-regressive eval trends vs Stage A baseline.
- `GET /v1/cp/promotion-gates/{cpid}` and `GET /v1/golden/{run_id}/gates` all green in dry-run golden cycle.

Exit to Stage C when:
- consecutive 2 cycles pass all quality and training gates,
- no unresolved `R5`/`R6` quarantines linked to accepted rows,
- rollback drill succeeds on golden stage.

## Stage C: 1M rows (production scale)
Entry:
- >= 1,000,000 accepted train rows.
- Long-tail source coverage demonstrated.
- Reject rate <= 12% with stable taxonomy distribution.

Required checks:
- Full runbook-aligned training (`/Users/star/Dev/adapter-os/docs/runbooks/FULL_TUNING_QWEN35_27B.md`).
- Determinism audit required before promotion.
- Gate snapshots archived: control-plane + golden.
- Dataset manifest, citation index, and hardening report frozen and content-addressed.

Promotion eligibility from Stage C:
- no hard failures in dataset validation, training report, determinism audit, or golden gates,
- trust-allowed dataset version(s),
- explicit promotion request via `POST /v1/golden/{run_id}/promote`,
- rollback path ready via `POST /v1/golden/{stage}/rollback`.

## 6) Promotion criteria (global)
A dataset version is promotion-ready only if all conditions hold:
- Contract integrity:
  - `spec_version == 1.0`
  - manifest hash == `data_spec_hash` expectation for run
- Quality:
  - acceptance thresholds met
  - no unresolved critical reject clusters
- Training behavior:
  - job report clean
  - metrics non-regressive against current production baseline
- Governance:
  - trust/safety pass
  - evidence bundle complete (fingerprints, manifests, reports, gates, decision timestamp)

If any criterion fails, status remains `BLOCKED` and promotion is denied.

## 7) Operational run sequence
1. Intake and fingerprint sources.
2. Extract canonical rows and emit pipeline artifacts.
3. Validate dataset (`/v1/datasets/.../validate`, stats, preview).
4. Version dataset.
5. Start training and monitor job/report.
6. Evaluate control-plane and golden gates.
7. Promote only on full pass; otherwise rollback/quarantine.

## 8) Evidence and audit retention
For each promoted run, persist:
- dataset file hash and manifest hash,
- dataset version id(s), training job id(s), golden run id,
- training report + metrics snapshots,
- determinism audit output,
- promotion gate snapshots and final decision record.

Retention location should remain under `var/datasets/generated/spark/pipeline/reports/` (or linked immutable artifact store) with timestamped indices.

## 9) Non-negotiables
- No training on unversioned datasets.
- No promotion when any hard gate is red.
- No silent schema drift; schema changes require explicit version evolution.
- No trust override in production without recorded approver + rationale.
