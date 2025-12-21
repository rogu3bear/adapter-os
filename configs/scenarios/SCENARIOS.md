Scenarios are versioned configs in `configs/scenarios/*.toml` (default root overridable via `AOS_SCENARIOS_DIR`). They drive scenario-ready checks without touching the DB.

- Load with `aosctl scenario list` or `aosctl scenario check --name <id>`; `aosctl scenario up` runs `dev up` then the same check.
- Checks: polls `/system/ready`, verifies base model exists with `status/import_status = available`, enforces `adapter.base_model_id == model.id`, verifies adapter for the scenario tenant with `lifecycle_state = active`, and enforces warm-load when `--require-loaded`, `[adapter].require_loaded`, `[adapter].load_state = warm`, or `[model].warmup = true` (fails if the adapter is cold).
- Optional 1-token probe: use `--chat-probe` or `[chat].probe_enabled = true`; uses `/v1/infer` with the scenario model/adapters (keep probes simple; full replay belongs in `scenario verify`).
- Chat calls use `[chat].backend_profile` and `[chat].determinism_mode`; replay verification defaults to `[replay].runs` when not overridden.
- Training: `aosctl scenario up --name <id> --train` will run `train-docs` with `[training].docs_path`, `tenant.id`, `model.id`, and `register_after_train`.
- Base model location resolves to `${AOS_MODEL_CACHE_DIR}/${AOS_BASE_MODEL_ID}` by default; `[model].id` overrides the canonical ID but still resolves under the cache root unless a custom path is configured.
- Doc configs stay git-tracked for reproducible flows; override root via `AOS_SCENARIOS_DIR` when testing local variants.

Doc-chat quickstart:
- Scenario: `configs/scenarios/doc-chat.toml` (`tenant-doc-chat`, model `qwen2.5-7b-mlx`, adapter `doc-chat-adapter`, warm load required).
- Readiness: `aosctl scenario check --name doc-chat --require-loaded --chat-probe` (probe flag optional; config already enforces active/warm and base-model match).
- Bring-up: `aosctl scenario up --name doc-chat --chat-probe --train` (add `--ui` to start UI; combine with `--db-reset`/`--skip-migrations` as needed).

Optional RAG golden scenario: see `docs/scenarios/RAG_GOLDEN.md` for ingesting
`examples/docs/rag-golden/note.md`, running a RAG query, and replay verification.

MLNavigator Inc 2025-12-08.

