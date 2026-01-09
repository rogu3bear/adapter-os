# Training Pipeline Orchestration

This document describes the strict, resumable training pipeline used by the orchestrator.

## Phases and State Machine

The pipeline executes phases in order and persists each transition:

1. Dataset build
2. Optional preprocessing
3. Deterministic train/validation split
4. Training loop
5. Validation + early stopping
6. Packaging
7. Complete

Each phase has explicit inputs, outputs, a deterministic phase ID, and a receipt on disk.
No phase is executed implicitly; transitions are guarded by the persisted state machine.

## Receipts and Artifacts

Pipeline state and receipts live under `var/training_pipeline/<job_id>/`:

- `pipeline_state.json`: current phase + status
- `pipeline_receipt.json`: `PipelineReceiptV1` aggregate
- `receipts/<phase>.json`: per-phase receipts
- `training_result.json`: persisted training result for resume

`PipelineReceiptV1` (contract version 1) includes:

- `pipeline_id`
- `contract_version`
- `dataset_id`
- `dataset_content_hash`
- `preprocess_id`
- `preprocess_hash`
- `split_hash`
- `training_config_hash`
- `base_model_hash`
- `started_at_unix_ms`
- `finished_at_unix_ms`
- `phase_statuses`

The `pipeline_id` is deterministic: hash of
`dataset_content_hash + training_config_hash + base_model_hash`.

## Resume Semantics

Resume is guarded and explicit:

- `contract_version` must match
- `dataset_content_hash` must match
- `split_hash` must match
- `base_model_hash` must match
- `training_config_hash` must match

Use `--force-resume` to override mismatches; the override is logged and emits
`training_pipeline_resume_forced`.

Training loop resume uses checkpoints. After the training loop completes, the
orchestrator persists `training_result.json` and records `training_result_hash`
in the training loop receipt. Resumes at validation/packaging verify the hash
when present to ensure the loaded training result matches the receipt.

## Observability Events

Structured events are emitted per phase:

- `training_pipeline_phase_start` (`phase_start`)
- `training_pipeline_phase_progress` (`phase_progress`)
- `training_pipeline_phase_end` (`phase_end`)
- `training_pipeline_phase_error` (`phase_error`)

Each event carries `job_id`, `pipeline_id`, `phase`, and phase-specific metadata.
