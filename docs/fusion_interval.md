# Fusion Interval Specification

## Why
- Weight fusion and per-token gating can drift without an explicit cadence.
- Fusion intervals define when fused tensors are recomputed so gating and fused weights stay consistent.
- Each interval records evidence to make replay and audit deterministic.

## Interval Modes
- `per_request`: fuse once for the full request; lowest overhead.
- `per_segment`: fuse every N tokens (segment length from `FusionInterval::PerSegment`); balance between reuse and alignment.
- `per_token`: fuse for every token; maximally aligned to router gating.

## Recording
- Context manifest now includes `fusion_interval` (default `per_request`).
- Trace events support `interval_id` and `fused_weight_hash`; `fusion.interval` events carry boundaries and hash evidence.
- Response traces expose `fusion_intervals` with `interval_id`, `start_token`, `end_token`, and the `fused_weight_hash` for each interval.

## Fused Weight Hash
- Hash is deterministic over base model hash plus interval routing decisions (adapter indices, gates, scores, entropy, stack hash).
- Same inputs within an interval produce the same `fused_weight_hash`; different inputs produce a different hash.

## Backend Expectation
- In fused mode, GEMM should consume the fused tensor corresponding to the active interval.
- Interval boundaries must be honored when re-fusing or swapping fused tensors during generation.

MLNavigator Inc December 11, 2025.
