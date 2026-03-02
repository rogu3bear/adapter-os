# adapterOS Scripts

Overview of the `scripts/` directory and critical paths.

---

## Critical Path

The main boot flow:

```
./start  →  scripts/service-manager.sh  →  scripts/worker-up.sh (for worker)
```

- **`./start`** – Entry point; sources `scripts/lib/freeze-guard.sh`, `env-loader.sh`, `manifest-reader.sh`; delegates to `service-manager.sh`
- **`scripts/service-manager.sh`** – Starts/stops backend, worker, secd, node, UI; manages PIDs, ports, graceful shutdown
- **`scripts/worker-up.sh`** – Worker startup; used when worker is started separately

---

## Canonical Replacements

Prefer these over legacy scripts:

| Legacy | Use Instead |
|--------|-------------|
| `scripts/diagnose.sh` (removed) | `aosctl diag run --full` |
| `scripts/migrate.sh` (removed) | `aosctl db migrate` |
| `scripts/gc_bundles.sh` (removed) | `aosctl maintenance gc-bundles` |
| `scripts/deploy_adapters.sh` (removed) | `aosctl deploy adapters` |
| `scripts/verify-determinism-loop.sh` (removed) | `aosctl verify determinism-loop` |

---

## Canonical Script Entry Points

Use these canonical paths for automation and docs:

| Capability | Canonical Path | Compatibility Shim |
|------------|----------------|--------------------|
| Model download | `scripts/download-model.sh` | `scripts/download_model.sh` |
| Git hooks install | `scripts/install_git_hooks.sh` | `scripts/setup-git-hooks.sh` |
| UI build | `scripts/build-ui.sh` | `scripts/build-leptos-ui.sh` |
| JSCPD scan | `scripts/run_jscpd.sh` | `scripts/run_jscpd_batched.sh` (`--batched`) |
| Happy path smoke | `scripts/test/smoke_happy_path.sh` | `scripts/ci/golden_path_smoke.sh` |

Compatibility shims print a deprecation notice and currently target a removal window after `2026-06-30`.

---

## Script Inventory

| Purpose | Scripts |
|---------|---------|
| **Boot** | `service-manager.sh`, `worker-up.sh`, `dev-up.sh`, `bootstrap_with_checkpoints.sh` |
| **CI** | `ci/*.sh` (check_*, validate_*, execute_*), `check-config.sh`, `check-db.sh` |
| **Smoke** | `smoke-inference.sh`, `foundation-smoke.sh`, `functional-path-smoke.sh`, `golden_path_adapter_chat.sh`, `mvp_smoke.sh` |
| **Dev** | `dev-up.sh`, `dev/aos_doctor.sh`, `dev/generate_route_map.sh`, `build-ui.sh`, `fresh-build.sh` |
| **Contracts** | `contracts/*.sh`, `contracts/*.py` |
| **Environment** | `setup_env.sh`, `validate_env.sh`, `switch_env_profile.sh` – see [ENVIRONMENT_SCRIPTS_README.md](ENVIRONMENT_SCRIPTS_README.md) |

---

## Migration Check Ownership

Migration checks remain intentionally split. Do not collapse these without a dedicated behavior review.

| Script | Responsibility |
|--------|----------------|
| `scripts/check_migrations.sh` | Migration numbering hygiene (gaps, collisions, duplicate IDs) |
| `scripts/check-migrations.sh` | Signature freshness and duplicate-number guard |
| `scripts/db/check_migrations.sh` | CI/test wrapper that orchestrates migration checks and signature verification fallback |

---

## Smoke Script Ownership

Smoke scripts are purpose-specific and currently tied to different gates:

| Script | Primary Purpose | Typical Owner/Gate |
|--------|------------------|--------------------|
| `scripts/test/smoke_happy_path.sh` | Fast boot/health happy-path regression | CI lightweight gate (`scripts/ci/golden_path_smoke.sh`) |
| `scripts/mvp_smoke.sh` | MVP path validation | Release and checklist workflows |
| `scripts/foundation-smoke.sh` | Foundation stack confidence checks | Foundation run workflow |
| `scripts/demo-smoke.sh` | Demo readiness checks | Demo operators |
| `scripts/functional-path-smoke.sh` | Functional path integrity checks | Regression/manual validation |
| `scripts/smoke-inference.sh` | Inference path validation | Runtime/inference checks |
| `scripts/ui_smoke.sh` | UI runtime smoke behavior | Config and UI health checks |

---

## Path Hygiene

All runtime data lives under `./var/` (per [CONTRIBUTING.md](../CONTRIBUTING.md)):

- `var/` – PID files, logs, sockets, run state
- Never use `/tmp`, `/private/tmp`, `/var/tmp`
- Never create `var/` or `tmp/` inside crates

---

## Library Scripts

Sourced by other scripts (do not run directly):

- `lib/env-loader.sh` – Load `.env` with `--no-override`
- `lib/freeze-guard.sh` – Port conflict detection, adapterOS process prompts
- `lib/manifest-reader.sh` – TOML manifest parsing
- `lib/manifest-wizard.sh` – Interactive manifest generation
- `lib/build-targets.sh` – Binary resolution (flow-partitioned + legacy targets)
- `lib/http.sh` – HTTP helpers for smoke tests

---

## Deferred Consolidation Backlog

- Migration deep-merge design: evaluate a single orchestrator vs current split.
- Smoke harness design: evaluate profile-driven runner (`mvp`, `foundation`, `demo`, `functional`).
- Shim retirement: remove compatibility paths after deprecation window and call-site cleanup.
