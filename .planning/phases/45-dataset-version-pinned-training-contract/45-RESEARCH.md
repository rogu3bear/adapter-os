---
phase: 45-dataset-version-pinned-training-contract
created: 2026-02-28
status: ready_for_planning
---

# Phase 45: Dataset Version-Pinned Training Contract - Research

**Researched:** 2026-02-28
**Domain:** typed training request + dataset version continuity
**Confidence:** HIGH

## Evidence Highlights

- UI client already exposes typed `create_training_job`.
- API type contract supports optional `dataset_version_id` in create request.
- Wizard already tracks source repo/branch/source-version context but submit path used legacy endpoint.

## Planning Implications

- Switch submit path to typed request builder.
- Resolve and persist dataset version ID during dataset selection/upload flow.
- Display selected dataset version in wizard status/review for operator clarity.

## Citations

- `crates/adapteros-ui/src/api/client.rs`
- `crates/adapteros-api-types/src/training.rs`
- `crates/adapteros-ui/src/pages/training/wizard.rs`

## Best-Practice Citations

- API robustness via explicit request contracts (OpenAPI): https://swagger.io/resources/articles/best-practices-in-api-design/
- Plain language guidance for critical workflow review steps: https://digital.gov/guides/plain-language/writing
