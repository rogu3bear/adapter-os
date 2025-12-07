Scenarios are versioned configs in `configs/scenarios/*.toml` (default root overridable via `AOS_SCENARIOS_DIR`). They drive scenario-ready checks without touching the DB.

- Load with `aosctl scenario list` or `aosctl scenario check --name <id>`; `aosctl scenario up` runs `dev up` then the same check.
- Checks: polls `/system/ready`, verifies base model exists with `status/import_status = available`, verifies adapter for the scenario tenant with `lifecycle_state = active`, and optionally enforces `load_state != cold` when `--require-loaded` or `[adapter].require_loaded` is set.
- Optional 1-token probe: use `--chat-probe` or `[chat].probe_enabled = true`; uses `/v1/infer` with the scenario model/adapters.
- Doc configs stay git-tracked for reproducible flows; override root via `AOS_SCENARIOS_DIR` when testing local variants.

MLNavigator Inc 2025-12-07.

