# Phase 52: Full Portability - Context

**Gathered:** 2026-03-04
**Status:** Ready for planning

<domain>
## Phase Boundary

Make AdapterOS fully portable: cross-platform builds, relocatable runtime paths, and environment-independent configuration so the system runs on any Apple Silicon Mac from a clean clone with `./start`. No new features — this is infrastructure that removes friction.

</domain>

<decisions>
## Implementation Decisions

### Build prerequisites
- Single idempotent `./bootstrap.sh` script that checks/installs all dependencies
- Homebrew and rustup are **required prerequisites** — bootstrap fails with clear message if either is missing
- Bootstrap handles everything else: MLX headers, WASM target, other brew deps
- `./bootstrap.sh --verify` flag optionally runs `cargo check --workspace` + `./scripts/ui-check.sh` after installing deps
- Without `--verify`, bootstrap only checks/installs deps and exits fast

### Path relocation strategy
- All runtime paths resolve **relative to project root** (current behavior, made robust)
- Project root detection: walk up from CWD looking for marker (Cargo.toml or `.adapteros-root`); `AOS_ROOT` env var overrides if set
- **Exception: shared model cache** at `~/.cache/adapteros/models/` to avoid duplicating large weight files across clones
- Model path lookup is **layered**: `var/models/` (project-local) → `~/.cache/adapteros/models/` (shared cache) → `AOS_MODEL_PATH` env var (explicit). First hit wins.
- All other runtime data (DB, sockets, logs, PIDs) stays project-local under `var/`

### Fresh clone experience
- `./start` **auto-creates** `var/` directory structure if missing, with a warning: "First run detected. Run ./bootstrap.sh --verify to ensure all deps are present."
- Missing models: **fail fast** with actionable message: "No model found. Run: aosctl model pull <name>" — user chooses which model
- Missing DB: **auto-migrate with log** — "Initializing database (N migrations)..." so user knows what's happening on first boot
- `./start` runs **pre-flight dependency checks** before attempting build — fails early with clear messages instead of cryptic linker errors

### Config zero-touch defaults
- Keep current `adapteros-config` system — just ensure all defaults are built-in so no config file is required to exist
- **Zero env vars required** for default operation — every var (AOS_SERVER_PORT, AOS_MODEL_BACKEND, etc.) has a sensible built-in default. Only AOS_DEV_NO_AUTH needed for dev mode.
- Model backend: **auto-detect** available backends (MLX installed? CoreML available?) and pick best one. AOS_MODEL_BACKEND overrides if set.
- Add `aosctl config show` command that prints all resolved config values with their source (built-in / TOML / env) for debugging

### Claude's Discretion
- Exact marker file choice for project root detection
- Pre-flight check implementation (shell script vs Rust binary)
- Auto-detect backend priority ordering
- `aosctl config show` output format

</decisions>

<specifics>
## Specific Ideas

- Bootstrap should be idempotent — safe to run repeatedly, skips already-installed deps
- Model cache lookup should be transparent — user shouldn't need to know about the layered resolution
- Pre-flight checks in `./start` should catch the most common failure modes (missing MLX, missing WASM target) before wasting time on a doomed build

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 52-full-portability*
*Context gathered: 2026-03-04*
