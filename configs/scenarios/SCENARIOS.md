Scenarios are versioned configs in `configs/scenarios/*.toml` (default root overridable via `AOS_SCENARIOS_DIR`). They drive scenario-ready checks without touching the DB.

- Load with `aosctl scenario list` or `aosctl scenario check --name <id>`; `aosctl scenario up` runs `dev up` then the same check.
- Checks: polls `/system/ready`, verifies base model exists with `status/import_status = available`, verifies adapter for the scenario tenant with `lifecycle_state = active`, and optionally enforces `load_state != cold` when `--require-loaded` or `[adapter].require_loaded` is set.
- Optional 1-token probe: use `--chat-probe` or `[chat].probe_enabled = true`; uses `/v1/infer` with the scenario model/adapters.
- Doc configs stay git-tracked for reproducible flows; override root via `AOS_SCENARIOS_DIR` when testing local variants.

Doc-chat quickstart:
- Scenario: `configs/scenarios/doc-chat.toml` (`tenant-doc-chat`, model `qwen2.5-7b-mlx`, adapter `doc-chat-adapter`, warm load required).
- Readiness: `aosctl scenario check --name doc-chat --require-loaded --chat-probe` (probe flag optional; config already enforces active/warm).
- Bring-up: `aosctl scenario up --name doc-chat --chat-probe` (add `--ui` to start UI; combine with `--db-reset`/`--skip-migrations` as needed).

MLNavigator Inc 2025-12-07.

