# Timeline (UTC)

- 2026-03-02T07:40:03Z: Drill started.
- 2026-03-02T07:40:03Z: Baseline memory pressure telemetry captured (`vm_stat`, `top -l 1 | head -n 20`).
- 2026-03-02T07:40:03Z: Synthetic constrained-memory allocation attempted (`ulimit -v 65536` + allocation probe).
- 2026-03-02T07:40:03Z: Eviction endpoint probe attempted.
- 2026-03-02T07:40:03Z: Readiness post-check captured (`/readyz`).
