# AGENTS.md

Minimal guidance for deterministic builds and tests in AdapterOS.

## Tasteful Default: Existing Code First

Agents should assume the code already exists; new code is only tasteful when you have proof it does not.

## Build And Test Commands

```bash
# Development
./start
AOS_DEV_NO_AUTH=1 ./start
./start backend
./start worker
./start secd
./start node
scripts/service-manager.sh start <backend|worker|secd|node|ui>

# Build
cargo build --release --workspace
ln -sf target/release/aosctl ./aosctl
cargo check -p <crate>

# Testing
cargo test -p <crate>
cargo test --workspace
cargo test -- --test-threads=1
cargo test -- --nocapture
cargo test --workspace
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
- Do not change role/RBAC logic to unblock UI; treat auth as non-blocking for visual work.
- Prefer one-off commands or throwaway local scripts instead of committing new tooling.

## Frontend Surfaces And Routes (Explicit)

### Surface Inventory

- **Primary web UI (Leptos CSR):** `crates/adapteros-ui/index.html` mounts `App` from `crates/adapteros-ui/src/lib.rs`.
- **CodeGraph Viewer (Tauri + React):** `crates/adapteros-codegraph-viewer/frontend/index.html` mounts `App` from `crates/adapteros-codegraph-viewer/frontend/src/main.tsx`.
- **Static minimal UI:** `crates/adapteros-server/static-minimal/index-minimal.html` (prebuilt bundle) and `crates/adapteros-server/static-minimal/api-test.html` (standalone API test harness).

### UI Responsibility Boundary (per UI_CONTRACT.md)

- **Frontend:** rendering/layout, user input, API calls and SSE streaming, client-side filtering/pagination for display.
- **Backend (never in UI):** cryptography, policy evaluation, receipt generation, determinism enforcement, core business logic.

### Leptos Route Map (single source: `crates/adapteros-ui/src/lib.rs`)

```text
/login
/
/dashboard
/adapters
/adapters/:id
/chat
/chat/:session_id
/system
/settings
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
/agents
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
- OpenAPI/TypeScript clients must stay in sync; CI regenerates and diffs `docs/api/openapi.json` and `ui/src/api/generated.ts`.

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
4. **NEVER create files in repo root** - Use `./var/` for all runtime data

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
