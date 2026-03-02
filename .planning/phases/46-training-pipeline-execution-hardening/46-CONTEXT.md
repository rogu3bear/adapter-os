# Phase 46 Context: Training Pipeline Execution Hardening

## Problem Framing

Training jobs are accepted by API but can fail at step 0 when the training worker is unavailable or preflight compatibility is incomplete. The failure reason is not consistently surfaced to operators, slowing remediation.

## Operator Intent

1. Start adapter training once and get either deterministic execution or immediate actionable rejection.
2. Keep `Qwen3.5-27B` as primary model identity for training runs.
3. Preserve deterministic dataset/version constraints before enqueue.

## Constraints

1. Reuse existing training endpoints and orchestration flow; no parallel training path.
2. Preserve tenant isolation and existing policy gates.
3. Keep changes minimal and codebase-aligned across `server-api`, `orchestrator`, and worker supervision.

## Phase Citations

1. `/Users/star/Dev/adapter-os/crates/adapteros-server-api/src/handlers/training.rs`
2. `/Users/star/Dev/adapter-os/crates/adapteros-server/src/boot/background_tasks.rs`
3. `/Users/star/Dev/adapter-os/crates/adapteros-server-api/src/handlers/models.rs`
4. `/Users/star/Dev/adapter-os/crates/adapteros-core/src/version.rs`
