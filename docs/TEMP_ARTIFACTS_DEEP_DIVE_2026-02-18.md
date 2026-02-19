# Temp Artifacts Deep Dive (2026-02-18)

**Purpose:** Catalog temp/generated artifacts, what was removed, what was kept, and optional cleanup.

## Removed from Git (2026-02-18)

| Path | Reason |
|------|--------|
| `output/playwright/*` | Playwright screenshots, audit results |
| `reports/*` | Audit reports, PRD summaries, unfinished_feature_audit |
| `training/synthesis_model/output/*` | Generated training output |
| `docs/reconciliation/partial_branch_*.json` | Merge reconciliation artifacts |
| `docs/audits/unfinished_feature_isolation_2026-02-17.json` | Feature isolation scan output |
| `docs/engineering/ROUTE_MAP.md` | Redundant; `api/ROUTE_MAP.md` is canonical |
| `.harmony/*` | Harmony restoration tool state |
| `.integrator/WORKLOG.md` | Integrator tool worklog |
| `.staging-manifests/*.json` | Staging manifests (not referenced in code) |
| `.worker_logs/*` | Worker log artifacts |
| `training/bundle_000000.ndjson` | Telemetry bundle output |

## Added to .gitignore

- `output/`
- `reports/`
- `training/synthesis_model/output/`
- `training/bundle_*`
- `training/*.ndjson`
- `.integrator/`
- `.staging-manifests/`
- `.worker_logs/`

(Note: `.harmony/` was already gitignored.)

## Kept (Intentional)

| Path | Purpose |
|------|---------|
| `var/` | Canonical runtime data (see VAR_STRUCTURE.md). User confirmed: keep alive. |
| `adapters/` (root) | May be dev symlink or shortcut to var/adapters; contains registry.db |
| `.agent/workflows/` | Workflow definitions; referenced in commands/README.md |
| `agents/` | Codex agent definitions (tracked) |
| `baselines/` | Baseline manifests (tracked) |
| `codegen/` | Codegen config (tracked) |
| `training/datasets/` | Dataset structure, READMEs, manifests (tracked) |
| `training/synthesis_model/` | Config, data, README (tracked); only `output/` ignored |

## Optional Cleanup (Not Automated)

Per VAR_STRUCTURE.md and AGENTS.md:

```bash
# Clean crate-level var directories (test artifacts)
find ./crates -type d -name "var" -not -path "*/target/*" -exec rm -rf {} +

# Clean test databases
rm -f ./var/*-test.sqlite3* ./var/*_test.sqlite3*

# Clean var/tmp
rm -rf ./var/tmp
```

**Crate-level var dirs found:** `crates/adapteros-db/var`, `crates/adapteros-server-api/var`

## Not Temp (Build / Tooling)

- `**/benches/` — Cargo benchmark dirs
- `**/.cache/clang/` — Clang module cache (gitignored)
- `.serena/` — Serena tool cache/logs (not tracked)
- `.codex/` — Codex logs (gitignored)
- `.cursor/` — Cursor rules (partial; `.cursor/rules/` kept)
- `crates/adapteros-codegraph-viewer/frontend/dist/` — Tauri build output (gitignored)

## Policy

1. **var/** — Keep. Canonical runtime data per VAR_STRUCTURE.md.
2. **output/, reports/** — Generated; gitignored and removed from index.
3. **Tool state** (.harmony, .integrator, .staging-manifests, .worker_logs) — Local only; gitignored.
4. **Crate-level var/** — Test artifacts; run cleanup command if desired.
