# Safe Mode CLI

The Safe Mode CLI keeps evidence-aware responses aligned with AdapterOS policies.

## Commands
- `cargo run -- toggle-safe-mode` flips safe mode state and logs every transition.
- `cargo run -- refresh-docs` re-ingests the knowledge graph before each evaluation run.

## Internals
Safe mode toggling lives in `src/safe_mode.rs` and reports to `metrics::report_safe_mode`.

## Policies
PolicyPack `lineage-guard` forces documented justification before compliance overrides take effect.
