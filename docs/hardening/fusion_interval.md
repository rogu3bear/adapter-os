## Fusion Interval Hardening

- **Goal:** keep weight fusion cadence aligned with per-token router gates and record boundaries for replay.
- **Modes:** `per_request` (single `request-0` interval), `per_segment` (`segment-{n}` buckets sized by `tokens_per_segment` with a floor of 1), `per_token` (`token-{step}` for every router step).
- **Interval IDs:** derived deterministically via `FusionInterval::interval_id_for_step`; every router decision now carries its interval ID so traces and replay stay linked.
- **Trace evidence:** `fusion_intervals` include `interval_id`, `start_token`, `end_token`, and `fused_weight_hash`. The hash is computed from canonical (JCS) bytes over the base model hash plus the router decisions in that interval.
- **Manifest binding:** context manifest schema v2 already carries `fusion_interval`; replay digests now reflect the active fusion cadence.
- **Replay stability:** canonical hashing plus interval IDs in decisions ensures stable hashes under replay; acceptance guards cover per-request single interval, per-token N intervals, and hash equality for identical inputs.
- **Operational notes:** per-segment uses floor(1) for segment size; custom interval IDs in incoming decisions are respected to avoid drift between routing and fusion spans.

MLNavigator Inc Dec 11, 2025.
