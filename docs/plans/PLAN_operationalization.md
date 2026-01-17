# PLAN_3: Operationalization

One command. One smoke test. One truth UI. One runbook. No excuses.

## Goals
- Provide a single dev entrypoint that runs the full loop end-to-end and exits only when a real reply + receipt is returned.
- Surface honest readiness in the UI, including a single, clear reason chat is blocked.
- Add a hard CI gate that exercises the golden path and fails on regressions.
- Normalize dev config + runtime artifacts to stable `var/` locations and avoid absolute user paths.
- Ship a short, actionable runbook for first chat and common blockers.

## Non-goals
- Large-scale UI redesigns or backend refactors beyond what is needed for the golden path.
- Production hardening beyond existing operations docs.

## Alignment Anchors
- `./start up` is the orchestrator; the golden path builds on it.
- CLI workflows already exist (`aosctl models seed`, `aosctl dataset ingest`, `aosctl train start`, `aosctl adapter load`).
- `/v1/system/status` (`SystemStatusResponse`) is the truth contract for readiness.
- Existing smoke/E2E scaffolding to reuse: `scripts/test/smoke_e2e.sh`,
  `scripts/test/smoke_happy_path.sh`, and `crates/adapteros-server-api/tests/golden_path_api_e2e.rs`.

## 1) Golden Path Command
Intent: "stop arguing, run this" for dev.

Design (single entrypoint):
- Add `scripts/golden_path.sh` (preferred) or `cargo xtask golden-path`.
- Script runs `./start up` (no `--skip-worker`); for dev it can set `AOS_DEV_NO_AUTH=1`.
- Validate model + tokenizer inputs:
  - Canonical: `AOS_MODEL_CACHE_DIR` + `AOS_BASE_MODEL_ID`.
  - Dev fallback: `AOS_MODEL_PATH`, `AOS_TOKENIZER_PATH`.
- Seed DB via `aosctl models seed` (idempotent; relies on `var/models` by default).
- Create or reuse a repo named `golden-path` (API `POST /v1/repos`).
- Upload a minimal dataset fixture (new `test_data/golden_path.jsonl`) via `POST /v1/datasets`
  using multipart; capture dataset + version IDs.
- Validate/trust dataset and wait for `ready` + `allowed` trust state
  (`POST /v1/datasets/{id}/validate` or trust override when dev bypass is on).
- Start training (`aosctl train start` or `POST /v1/training/jobs`) and poll until `completed`;
  capture `adapter_id` from the job.
- Hydrate the worker:
  - `POST /v1/adapters/{id}/load` or `aosctl adapter load`.
  - Confirm `SystemStatusResponse` has no `ModelNotHydrated`/`AdapterNotHydrated` blockers.
- Send one chat prompt (`POST /v1/infer` or `aosctl chat prompt`).
- Print a receipt summary (trace_id, determinism_mode_applied, backend_used, adapters_used).
- Exit 0 only when response text is non-empty and receipt fields exist.

Notes:
- `aosctl` API commands require stored auth; in dev, prefer `AOS_DEV_NO_AUTH=1` and direct
  HTTP calls unless a dev-login helper is added.
- Keep runtime artifacts under `var/` unless a custom `AOS_VAR_DIR` is paired with matching config.

Acceptance:
- `./scripts/golden_path.sh` is the single entrypoint.
- It returns 0 only when it gets a real reply plus receipts; otherwise it fails loudly.

## 2) Truth Surface UI
Intent: minimum UI that tells the truth, always.

Design:
- Add a compact "Truth Surface" panel (dashboard + chat) backed by `/v1/system/status`.
- Required fields:
  - Model status: loaded / not loaded.
  - Adapter status: present / selected / hydrated.
  - Worker status: connected / hydrated.
  - Determinism tier: requested vs allowed.
  - One clear "why chat can't run yet" message.
- Use `SystemStatusResponse.inference_blockers` to derive the blocker text; reuse mappings already
  present in `crates/adapteros-ui/src/components/status_center/` and `crates/adapteros-ui/src/pages/system/`.
- Requested determinism comes from current stack settings; allowed determinism comes from tenant
  settings/execution policy (`/v1/tenants/{tenant_id}/settings`).
- When `inference_ready` is false: disable send and show the blocker. Blank chat is allowed only
  before PLAN_2 is satisfied.
- Use shared Rust types (`adapteros-api-types`), no ad-hoc JSON structs.

Acceptance:
- Chat shows truthful blockers and never implies readiness when blocked.
- Determinism mismatch is visible and actionable.

## 3) CI Regression Gate
Intent: prevent "it worked yesterday" regressions.

Design:
- Add compile gate: `cargo test --workspace --no-run`.
- Add one golden-path smoke test:
  - Option A: wrap the existing harness (`crates/adapteros-server-api/tests/golden_path_api_e2e.rs`)
    with a CI script (new `scripts/ci/golden_path_smoke.sh`).
  - Option B: extend `scripts/test/smoke_e2e.sh` to exercise real training + adapter load.
- Test flow:
  - Spin sqlite DB.
  - Boot server.
  - Spawn worker or simulate (acceptable for CI).
  - Hit `/readyz`.
  - Run one inference/chat against a known adapter fixture.
  - Assert receipt fields exist (`run_receipt.trace_id`, determinism metadata, adapters_used).
- Use small fixtures (`tests/fixtures/models/tiny-test` or a new tiny adapter fixture).

Acceptance:
- CI fails if golden path cannot produce a reply + receipt.
- The smoke test is deterministic and fast enough to be run on every PR.

## 4) Artifact and Config Hygiene
Intent: eliminate foot-guns and path drift.

Design:
- Add `configs/dev.toml` as the canonical dev config.
- Standardize runtime roots:
  - `var/models`
  - `var/adapters`
  - `var/datasets`
  - `var/run`
- Document env overrides:
  - `AOS_MODEL_CACHE_DIR` + `AOS_BASE_MODEL_ID` (canonical).
  - `AOS_MODEL_PATH` (dev legacy).
  - `AOS_TOKENIZER_PATH`.
  - `AOS_MODEL_CACHE_MAX_MB`.
- Add a CI guard to reject absolute user paths (e.g., `/Users/`) in `configs/`.

Acceptance:
- Dev config is single source of truth.
- No repo configs contain user-absolute paths.

## 5) Short Runbook
Intent: zero to first reply in minutes, not hours.

Design:
- Update `docs/OPERATIONS_RUNBOOK.md` with an "Operationalization" section (or add
  `docs/OPERATIONALIZATION.md`).
- Include:
  - "From zero to first chat reply" (use `./scripts/golden_path.sh`).
  - "Common blockers and fixes": unsigned migrations, no worker, not hydrated,
    determinism tier mismatch, missing model/tokenizer.

Acceptance:
- Runbook is one screen, actionable, and matches the golden path.

## Sequencing
1. Add `configs/dev.toml` + minimal dataset fixture + path conventions.
2. Implement `scripts/golden_path.sh`.
3. Add truth surface UI.
4. Add CI golden-path gate.
5. Update runbook.

## Risks and Notes
- `aosctl` API commands require stored auth; dev bypass or a dev-login helper may be needed.
- CI hardware constraints may require a simulated worker or CPU backend.
