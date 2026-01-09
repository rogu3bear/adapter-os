# Training Reporting (PRD-06)

This document describes the dataset-aware training report artifact produced after a training job.
Reports are written deterministically and include dataset identity, split identity, and metric
definitions so there are no mystery numbers.

## Artifact location

Reports are written under the artifacts root:

- `var/artifacts/training-reports/<pipeline_id>/report.json`

The artifacts root can be overridden via `paths.artifacts_root` in config.

## Schema: TrainingReportV1

Field overview (all required):

- `report_version`: Schema version (u32). Currently `1`.
- `pipeline_id`: Training pipeline identifier (uses the training job ID).
- `dataset_id`: Dataset identifier. For multi-dataset merges, this is a deterministic
  `multi:<hash>` identifier derived from the sorted dataset IDs.
- `dataset_content_hash`: BLAKE3 hash of the dataset content used for training.
- `split_hash`: BLAKE3 hash of the deterministic train/validation split.
- `base_model_id`: Base model identifier used for training.
- `base_model_hash`: BLAKE3 hash of the base model content (from the model registry when available).
- `optimizer`: Optimizer configuration summary.
- `training_config_hash`: BLAKE3 hash of the training config parameters.
- `curves`: Per-epoch curves for loss and perplexity.
- `summary`: Aggregate run summary metrics.
- `metric_definitions`: Embedded metric definitions for the report fields.
- `generated_at_unix_ms`: Report generation timestamp in Unix milliseconds.

### Optimizer summary

`optimizer` contains:

- `optimizer_type`: `adam`, `adamw`, or `sgd`.
- `beta1`, `beta2`: Adam/AdamW moment decay factors.
- `epsilon`: Numerical stability constant.
- `weight_decay`: Weight decay factor.
- `momentum`: Momentum factor for SGD.

### Curves

`curves` contains:

- `train_loss`: Mean cross-entropy loss per epoch on training split.
- `train_ppl`: Perplexity per epoch (exp(train_loss)).
- `val_loss`: Mean cross-entropy loss per epoch on validation split.
- `val_ppl`: Perplexity per epoch (exp(val_loss)).

### Summary

`summary` contains:

- `best_epoch`: Epoch (1-based) with the lowest validation loss. Defaults to `final_epoch`
  when validation is disabled.
- `final_epoch`: Last completed epoch (1-based).
- `early_stopped`: True if training stopped before target epochs without cancellation.
- `total_steps`: Total training steps, defined as examples processed across all epochs.
- `total_tokens`: Total tokens processed across the training split.

### Metric definitions

`metric_definitions` embeds human-readable definitions for every curve and summary field
so the report is self-explanatory.

## Reproducibility expectations

Reports are deterministic given identical inputs:

- Dataset identity is captured by `dataset_id`, `dataset_content_hash`, and `split_hash`.
- Training configuration is captured via `training_config_hash` and the optimizer summary.
- Curves and summary values are emitted from the recorded training result.

To regenerate reports deterministically, use the stored dataset artifacts, pipeline receipts,
training configuration, and metrics. If the base model registry entry changes, `base_model_hash`
may differ; treat the model registry as a stable source of truth for reproducibility audits.

## API and CLI access

API endpoint:

- `GET /v1/training/jobs/{pipeline_id}/report`

CLI:

- `aosctl train report --id <pipeline_id>`
- `aosctl train report --id <pipeline_id> --json`
