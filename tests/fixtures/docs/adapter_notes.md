# Adapter Notes

The Safe Mode CLI in this repo toggles state and reports metrics.

- `cargo run -- toggle-safe-mode` flips the guard and logs the event in telemetry.
- `cargo run -- refresh-docs` re-ingests the knowledge graph before evaluation.

Safe mode toggling lives in `src/safe_mode.rs` and emits metrics through `metrics::report_safe_mode`.
