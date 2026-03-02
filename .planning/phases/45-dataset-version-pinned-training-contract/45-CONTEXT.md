# Phase 45: Dataset Version-Pinned Training Contract - Context

**Gathered:** 2026-02-28
**Status:** Ready for planning

<domain>
## Phase Boundary

Upgrade training wizard submission from legacy dataset endpoint payload to typed `CreateTrainingJobRequest` and carry explicit `dataset_version_id` whenever available.

This phase focuses on request correctness, provenance continuity, and UI review visibility; it does not add new backend endpoints.

</domain>

<decisions>
## Implementation Decisions

### Contract hardening
- Use `create_training_job` with `CreateTrainingJobRequest` and `TrainingConfigRequest`.
- Populate `dataset_version_id` from dataset selection/upload context.
- Preserve existing backend preference fields via typed enum mapping.

### Provenance continuity
- Continue passing source repository/version context when available.
- Surface dataset version in wizard knowledge and review steps.

### Claude's Discretion
- Exact fallback behavior when dataset version cannot be resolved.
- UI phrasing for dataset version display in wizard cards.

</decisions>

<deferred>
## Deferred Ideas

- Multi-version dataset blending (`dataset_version_ids`) remains out of scope.
- Dataset-version trust override UX remains out of scope.

</deferred>

---

*Phase: 45-dataset-version-pinned-training-contract*
*Context gathered: 2026-02-28*
