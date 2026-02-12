# AGENTS.md

Minimal guidance for deterministic builds and tests in AdapterOS.
See `CLAUDE.md` for the extended developer guide (CLI workflows, feature flags, UI build/serve).

## Default: Existing Code First

Agents should assume the code already exists; new code is only appropriate when you have proof it does not.

## Operating Contract (Pre-Prompt)

- Start by restating: goal, non-goals, constraints, acceptance criteria, and the smallest relevant verification command(s). If any are missing, ask.
- Prefer minimal diffs and existing patterns; avoid refactors unless explicitly requested.
- Before running expensive/slow actions (full-workspace tests, `cargo clean`, starting services, downloading models), ask or clearly justify why it's necessary.

## Local Overrides

- Put machine- or branch-specific agent guidance in `AGENTS.override.md` (gitignored) rather than editing committed instructions.
- For component-specific guidance, prefer adding a scoped `AGENTS.md` inside that directory over growing the root file.

## Build And Test Commands

```bash
# Development
./scripts/dev-up.sh
./start
AOS_DEV_NO_AUTH=1 ./start
./start backend
./start worker
./start secd
./start node
scripts/service-manager.sh start <backend|worker|secd|node|ui>  # ui is a no-op (backend serves static/)

# Build
cargo build --release --workspace
ln -sf target/release/aosctl ./aosctl
cargo check -p <crate>
./scripts/build-ui.sh  # Build Leptos UI to crates/adapteros-server/static/

# Testing
# Prefer fast-mode variants for tight inner loops to keep compile/test cycles lean.
# Before merging substantial shutdown/lifecycle changes, run full-mode verification.
cargo test -p <crate>
cargo test --workspace
cargo test -- --test-threads=1
cargo test -- --nocapture
cargo test --workspace --tests
cd crates/adapteros-ui && trunk test
cargo test --workspace -- --ignored
cargo test --test determinism_core_suite
cargo test -p adapteros-lora-router --test determinism
cargo test -p adapteros-server-api --test replay_determinism_tests

# Quality
cargo fmt --all
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings
```

## Repo Info Preference

- Prefer using the `gh` CLI to fetch repository info and fill out repo metadata.

## Service Supervisor Notes

- Service control APIs (`/v1/services/*`) require a configured supervisor (`SUPERVISOR_API_URL` or `AOS_PANEL_PORT`), see `deploy/supervisor.yaml`.
- Admin safe-restart triggers an in-process shutdown after drain; an external supervisor should restart the process if configured.

## UI Visuals Priority (Current Focus)

- Goal: get the Leptos UI visible and iterate on visuals; avoid backend/CI edits unless explicitly requested.
- Dev mode for full access: use `AOS_DEV_NO_AUTH=1 ./start` to view all pages without RBAC gates.
- If the backend warns `ui:assets missing` or the UI is blank, run `./scripts/build-ui.sh` (or `./scripts/dev-up.sh` from a fresh clone).
- Do not change role/RBAC logic to unblock UI; treat auth as non-blocking for visual work.
- Prefer one-off commands or throwaway local scripts instead of committing new tooling.

## Frontend Surfaces And Routes (Explicit)

### Surface Inventory

- **Primary web UI (Leptos CSR):** `crates/adapteros-ui/index.html` mounts `App` from `crates/adapteros-ui/src/lib.rs`.
- **CodeGraph Viewer (Tauri + React):** `crates/adapteros-codegraph-viewer/frontend/index.html` mounts `App` from `crates/adapteros-codegraph-viewer/frontend/src/main.tsx`.
- **Static minimal UI:** `crates/adapteros-server/static-minimal/index-minimal.html` (prebuilt bundle) and `crates/adapteros-server/static-minimal/api-test.html` (standalone API test harness).

### UI Responsibility Boundary (per `crates/adapteros-ui/UI_CONTRACT.md`)

- **Frontend:** rendering/layout, user input, API calls and SSE streaming, client-side filtering/pagination for display.
- **Backend (never in UI):** cryptography, policy evaluation, receipt generation, determinism enforcement, core business logic.

### Leptos Route Map (single source: `crates/adapteros-ui/src/lib.rs`)

To refresh this list:
`rg -o 'path!\\(\"[^\"]+\"\\)' crates/adapteros-ui/src/lib.rs | sed -E 's/^path!\\(\"(.*)\"\\)$/\\1/' | awk '!seen[$0]++'`

```text
/login
/
/dashboard
/flight-recorder
/flight-recorder/:id
/adapters
/adapters/:id
/chat
/chat/:session_id
/system
/settings
/user
/models
/policies
/training
/stacks
/stacks/:id
/collections
/collections/:id
/documents
/documents/:id
/datasets
/datasets/:id
/admin
/audit
/runs
/runs/:id
/diff
/workers
/workers/:id
/monitoring
/errors
/routing
/repositories
/repositories/:id
/reviews
/reviews/:pause_id
/agents
/welcome
/safe
/style-audit
```

- **Public routes:** `/login`, `/safe`, `/style-audit`.
- **Protected + Shell-wrapped routes:** all others above.
- **Fallback:** NotFound (router fallback view).

### CodeGraph Viewer Routing

- Single-page UI with no client router. `App` shows: loading → error → empty (no graph data) → graph view. `DiffControls` is a toggleable panel (not its own route) that can appear alongside other states. `SidePanel` opens when `selectedDetails` is set. `SearchBar` only renders when graph data exists.
- `DiffControls` can still render when no graph/diff data is loaded; this is intentional (diagnostic affordance).

## Determinism Rules

- Seed derivation: HKDF-SHA256 with BLAKE3 global seed (`crates/adapteros-core/src/seed.rs`).
- Router determinism: score DESC, index ASC tie-break; Q15 denominator is 32767.0 (`crates/adapteros-lora-router/src/quantization.rs`).
- No `-ffast-math` compiler flags (CI scans build artifacts and `Cargo.toml` via `scripts/check_fast_math_flags.sh`; keep the flag absent).
- Set `AOS_DEBUG_DETERMINISM=1` to log seed inputs and router tie-break details.
- CI determinism gate runs determinism tests and scans build artifacts for `-ffast-math`.
- OpenAPI spec must stay in sync; CI checks `docs/api/openapi.json` (see `scripts/ci/check_openapi_drift.sh`).

## Troubleshooting

### Determinism

1. Check seed derivation inputs.
2. Verify router sorting (score DESC, index ASC tie-break).
3. Confirm Q15 denominator = 32767.0.
4. Run `cargo test --test determinism_core_suite` and `cargo test -p adapteros-lora-router --test determinism`.

### Build

1. `cargo clean && cargo build`
2. Check feature flags.
3. `cargo sqlx prepare` for offline mode.
4. Verify migration signatures: `migrations/signatures.json`.

## Health Endpoints

- Liveness: `/healthz`
- Component health: `/healthz/all`, `/healthz/{component}` (router, loader, kernel, db, telemetry, system-metrics, kv, background-tasks).
- Readiness: `/readyz` (canonical; no `/api/readyz` alias). `/system/ready` exposes system gate status.

## Backend Understanding (Verified Snapshot)

This section is a **verified snapshot** of backend behavior. **Update it only after an improvement has been proven** (tests, benchmarks, or merged code evidence). Do not update based on intent or plans.

### Topology

- Control plane: `adapteros-server` bootstraps and serves; `adapteros-server-api` owns routes, handlers, middleware, and `AppState`.
- Workers: `aos-worker` processes handle inference/training; control plane ↔ workers communicate over HTTP/UDS.
- Determinism substrate: `adapteros-core` + deterministic exec tick ledger; determinism modes and seed isolation are enforced.

### Control Plane Boot Phases

- Config -> security -> deterministic executor -> preflight -> invariants -> DB connect -> migrations -> post-DB invariants -> startup recovery -> router build -> federation -> metrics -> app state -> background tasks -> finalize -> bind.

### API Route Tiers

- Health (no middleware) -> public -> optional-auth -> internal (worker->CP) -> protected (auth+policy+audit).
- Training routes move to optional-auth when dev bypass is enabled; otherwise protected.

### Middleware Guarantees (Protected)

- Ordered: auth -> tenant guard -> CSRF -> context -> policy -> audit.
- Global layers: error-code enforcement, idempotency, rate limiting, request size limits, security headers, caching, versioning, trace context, request ID, seed isolation, lifecycle/drain gates, observability, compression.

### Determinism & Replay

- Seed isolation middleware + determinism context; strict mode rejects missing seeds.
- Global tick ledger in control plane boot; determinism checks gate promotions/replay.
- Replay endpoints and diagnostics are first-class.

### Token Caching Economics

- Attribution formula: `A = L − C` (attributed = logical − cached)
- Receipts commit cached token counts cryptographically
- Speedup is non-linear due to memory pressure reduction
- See `docs/TOKEN_CACHING_ECONOMICS.md` for details

### AppState (Central Services)

- DB + config + clock + metrics + policy + crypto + lifecycle manager + registry + telemetry buffers + SSE manager + idempotency store + load coordinator + optional federation daemon + tick ledger + boot attestation.

## File System Hygiene (CRITICAL FOR AGENTS)

### Forbidden Actions

1. **NEVER create `var/` or `tmp/` directories inside crates** - These pollute the repo
2. **NEVER write to `/tmp`, `/private/tmp`, `/var/tmp`** - System rejects these paths
3. **NEVER leave test databases behind** - Clean up `*-test.sqlite3`, UUID dirs
4. **NEVER create arbitrary files in repo root** - Runtime data goes in `./var/` (gitignored). Exceptions: the `./aosctl` symlink and `.env` (gitignored).

### Why This Matters

Agents have historically created orphaned directories that consume gigabytes:
- `crates/*/var/` - Test isolation artifacts (cleaned: 6+ GB)
- `var/tmp/` - Temporary test databases (cleaned: 5.9 GB)
- `var/test-dbs/` - Integration test leftovers (cleaned: 5.2 GB)

### Canonical var/ Structure

All runtime data goes in `./var/` (gitignored). See `docs/VAR_STRUCTURE.md`.

```
var/
├── aos-cp.sqlite3      # Main database
├── adapters/           # Trained LoRA adapters
├── models/             # Base models (~16 GB)
├── model-cache/        # Downloaded models
├── keys/               # Signing keys
├── logs/               # Rotated logs
├── run/                # Sockets, PIDs
└── [other canonical dirs]
```

### After Running Tests

```bash
# Clean crate-level var directories
find ./crates -type d -name "var" -not -path "*/target/*" -exec rm -rf {} +

# Clean test databases
rm -f ./var/*-test.sqlite3* ./var/*_test.sqlite3*

# Clean var/tmp
rm -rf ./var/tmp
```

### Path Security Enforcement

`crates/adapteros-core/src/path_security.rs` enforces:
- Rejects `/tmp`, `/private/tmp`, `/var/tmp` for persistent paths
- Validates symlinks don't resolve to forbidden paths
- Guard test in `crates/adapteros-config/tests/tmp_usage_guard.rs` scans runtime code

## Canonical Anchors (Compatibility)

Some code/docs link directly to `AGENTS.md#...` anchors. Keep these headings stable; if content moves, leave a stub with a pointer.

### Core Standards

- Determinism: no unseeded randomness; use HKDF-derived seeds (see `crates/adapteros-core/src/seed.rs` and `docs/DETERMINISM.md`).
- Errors: use `Result<T, AosError>` and follow error message standards (see `crates/adapteros-core/src/error.rs` and `docs/ERRORS.md`).
- Logging: prefer `tracing` macros over `println!` (see `docs/ERRORS.md`).
- Runtime paths: persistent/runtime state belongs under `./var/` (see `docs/VAR_STRUCTURE.md` and `crates/adapteros-core/src/path_security.rs`).

### Policy Engine

Canonical references: `docs/POLICIES.md`, `crates/adapteros-policy/src/registry.rs`, and `crates/adapteros-server-api/src/middleware/policy_enforcement.rs`.

### Policy Hooks

Canonical references: `docs/POLICIES.md` and `crates/adapteros-policy/src/hooks.rs` (hooks: `OnRequestBeforeRouting`, `OnBeforeInference`, `OnAfterInference`).

### Error Handling

Canonical reference: `docs/ERRORS.md` and `crates/adapteros-core/src/error.rs`.

### K-Sparse Routing

Canonical reference: `docs/DETERMINISM.md` and `crates/adapteros-lora-router/` (tie-break: score DESC, index ASC; Q15 denom 32767.0).

<a id="uma-backpressure--eviction"></a>
### UMA Backpressure And Eviction

Canonical reference: `crates/adapteros-memory/src/pressure_manager.rs` and `docs/runbooks/MEMORY_PRESSURE.md` (headroom policy and eviction coordination).

### Telemetry Event Catalog

Canonical reference: `crates/adapteros-telemetry/src/unified_events.rs` and `crates/adapteros-telemetry/src/lib.rs` (see `TelemetryWriter`).

### Deterministic Executor Seeding

Canonical reference: `crates/adapteros-server/src/boot/executor.rs` and `crates/adapteros-core/src/seed.rs`.

```rust
use adapteros_core::{seed::derive_seed, B3Hash};

let base_seed = B3Hash::hash(b"default-seed-non-production");
let global_seed = derive_seed(&base_seed, "executor");
```

### Global Tick Ledger (Issue C-6 Fix)

Canonical reference: `crates/adapteros-deterministic-exec/src/global_ledger.rs` and `crates/adapteros-deterministic-exec/tests/tick_ledger_concurrency.rs`.

```rust
let entry_hash = ledger.record_tick(task_id, &event).await?;
```

### Multi-Agent Coordination: Dead Agent Handling (Issue C-8)

Canonical reference: `crates/adapteros-deterministic-exec/src/multi_agent.rs`.

```rust
barrier.wait("agent_a", tick).await?;
barrier.mark_agent_dead("agent_b")?;
```
