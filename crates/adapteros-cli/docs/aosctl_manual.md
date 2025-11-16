% aosctl — AdapterOS CLI Manual

This manual provides an overview of the `aosctl` command‑line interface, including command groups, flag conventions, and usage examples. It is intended as a stable high‑level reference; for exhaustive per‑flag help, run `aosctl <command> --help`.

---

## 0. Quickstart Overview

If you just want to bring a node up, migrate the DB, deploy adapters, and verify determinism, the “happy path” looks like:

```bash
# 1. Create production tenant
aosctl init-tenant --id production --uid 5000 --gid 5000

# 2. Migrate control‑plane and registry databases
aosctl db migrate
aosctl registry migrate

# 3. Deploy adapters from local artifacts
aosctl deploy adapters \
  --path ./adapters \
  --backup-existing

# 4. Start serving for a given CPID
aosctl serve --tenant production --plan cp_abc123

# 5. Inspect status
aosctl status adapters        # adapter residency / memory
aosctl status cluster         # nodes and heartbeats
aosctl status tick            # latest tick and divergences
aosctl status memory          # host memory and headroom

# 6. Verify Determinism Loop and adapter deliverables
aosctl verify determinism-loop --json
aosctl verify adapters --json
```

For more detail, the sections below organize commands by responsibility: tenants, adapters, status, deploy, determinism, maintenance, and registry.

---

## 1. Global Flags and Conventions

- `--json` / `-q` / `--quiet`  
  - `--json`: machine‑readable JSON output when supported.  
  - `-q, --quiet`: suppress non‑essential human output (errors still printed).  
- `-v, --verbose`  
  - Enables verbose progress output for long‑running operations.  
- Exit codes  
  - `0` on success.  
  - Non‑zero on failure; error codes (e.g. `E2002`) can be inspected via `aosctl explain` and `aosctl error-codes`.
- Help and completion  
  - `aosctl --help` and `aosctl <command> --help` for detailed usage.  
  - `aosctl completions <shell>` to generate shell completion scripts.

### Output Modes

- Human‑friendly text (default): structured sections, progress lines, and tables.  
- JSON: structured JSON, suitable for automation (`--json`).  
- Quiet: minimal output for scripts/CI (`--quiet` or CI auto‑detection).

---

## 2. Tenant Management

Tenant commands create and manage isolated tenants on a node.

- `aosctl init-tenant` / `aosctl init`  
  - Initialize a new tenant with specific UID/GID.  
  - Key flags: `--id`, `--uid`, `--gid`.

**Examples**

- Create a development tenant:

```bash
aosctl init-tenant --id tenant_dev --uid 1000 --gid 1000
```

- Create a production tenant:

```bash
aosctl init-tenant --id tenant_prod --uid 5000 --gid 5000
```

---

## 3. Adapter Management

Adapter commands manage adapters in the registry (listing, registration, pinning, and air‑gap transfers).

- List adapters  
  - `aosctl list-adapters`  
  - Key flags: `--tier` (filter by tier), `--json` for machine‑readable output.
- Register adapters  
  - `aosctl register-adapter <id> <hash>`  
  - Key flags: `--tier`, `--rank`.
- Pin and unpin adapters  
  - `aosctl pin-adapter` / `aosctl unpin-adapter`  
  - Key flags: `--tenant`, `--adapter`, `--reason`, `--ttl-hours`.
- Adapter lifecycle / air‑gap flows  
  - `aosctl adapters register --path <dir-or-file>` – register a packaged adapter by path.  
  - `aosctl aos create/load/verify` – create or load `.aos` single‑file adapters.

**Examples**

- List adapters for scripting:

```bash
aosctl list-adapters --tier persistent --json > adapters.json
```

- Register a persistent adapter:

```bash
aosctl register-adapter my_adapter b3:abc123... \
  --tier persistent \
  --rank 16
```

---

## 4. Status and Health

The `status` tree makes `aosctl` the “system brain” for high‑level state.

- `aosctl status adapters`  
  - Lists adapters from the control‑plane DB with: `name`, `tenant_id`, `active`, `pinned`, `expires_at`, and `memory_bytes`.  
  - Respects `--json` for structured output.

- `aosctl status cluster`  
  - Lists registered nodes and last heartbeats from the `nodes` table.  
  - Useful for quickly spotting offline or unhealthy nodes.

- `aosctl status tick`  
  - Shows latest tick entry (tick, tenant, host, event type) from `tick_ledger_entries`.  
  - Also shows the last divergence report from `tick_ledger_consistency_reports`, if any.

- `aosctl status memory`  
  - Reads host memory via `sysinfo` and reports total/used bytes and headroom percentage.  
  - Use this before high‑memory operations (large model loads, replay).

**Examples**

```bash
# Check adapter residency and memory footprint
aosctl status adapters

# Get last tick and last divergence in JSON
aosctl --json status tick

# See node heartbeats
aosctl status cluster
```

---

## 5. Deploying Adapters

The `deploy` tree replaces the legacy `scripts/deploy_adapters.sh` script.

- `aosctl deploy adapters`  
  - Deploys adapter directories, `.aos` files, or `.safetensors` weights.  
  - Key flags:  
    - `--path <dir-or-file>` (repeatable): directories, `.aos`, or `.safetensors`.  
    - `--adapters-dir`: target adapter directory (default `/opt/adapteros/adapters`).  
    - `--backup-existing`: back up any existing adapter with the same name.  
    - `--dry-run`: show what would be done without touching disk or registry.

Behavior:

- Directories: copied into the adapters dir; existing directories can be backed up; then registered via `aosctl adapters register` over HTTP.  
- `.aos` files: verified via `aosctl aos verify`, backed up if requested, copied, then loaded into the registry via `aosctl aos load`.  
- `.safetensors` weights: the parent directory is treated as the adapter package.

**Examples**

```bash
# Dry run: see what would be deployed
aosctl deploy adapters \
  --path ./adapters \
  --backup-existing \
  --dry-run

# Deploy and register everything in ./adapters
aosctl deploy adapters \
  --path ./adapters \
  --backup-existing

# Deploy a single .aos file
aosctl deploy adapters \
  --path ./artifacts/my_adapter.aos
```

---

## 6. Inference and Replay

These commands interact with running workers and telemetry bundles.

- `aosctl infer`  
  - Run an inference against a worker UDS.  
  - Key flags: `--adapter`, `--prompt`, `--socket`, `--max-tokens`, `--timeout`, `--require-evidence`.
- `aosctl replay`  
  - Replay a telemetry bundle and optionally check determinism.

**Examples**

```bash
# Basic inference
aosctl infer --prompt "Hello world" \
  --socket /var/run/adapteros.sock

# Inference using a specific adapter
aosctl infer --adapter my_adapter \
  --prompt "Use adapter" \
  --socket /var/run/adapteros.sock
```

---

## 7. Determinism and Verification

Determinism and adapter deliverable checks are fronted through `aosctl`.

- `aosctl verify determinism-loop`  
  - Runs the Determinism Loop verification pipeline:  
    - Validates presence of key federation, policy, tick ledger, telemetry, CAB, orchestrator, and doc files.  
    - Runs `cargo check` for determinism‑critical crates.  
    - Optionally runs `cargo xtask determinism-report`.  
  - Exit code: `0` if all checks pass, `1` otherwise.  
  - With `--json`, emits a `DeterminismLoopResult { ok, checks[] }`.

- `aosctl verify-adapters`  
  - Wraps `cargo xtask verify-agents` (adapter deliverables A–F).  
  - Ideal for CI and pre‑release gates.  
  - With `--json`, emits `VerifyAdaptersResult { ok, exit_code, stdout_head, stderr_head }`.

- Telemetry verification  
  - `aosctl telemetry-verify --bundle-dir ./var/telemetry`  
  - Validates the Merkle chain and signatures of telemetry bundles.

**Examples**

```bash
# Full determinism loop verification (human output)
aosctl verify determinism-loop

# Determinism loop check in CI
aosctl --json verify determinism-loop > determinism_report.json

# Verify adapter deliverables A–F
aosctl verify adapters --json > verify_adapters.json
```

---

## 8. Maintenance and Garbage Collection

Maintenance commands manage long‑term storage and housekeeping.

- `aosctl maintenance gc-bundles`  
  - Garbage‑collects telemetry bundles according to Ruleset #10.  
  - Key flags:  
    - `--bundles-path` (default `/srv/aos/bundles`)  
    - `--db-path` (default `var/aos-cp.sqlite3`)  
    - `--keep-count N` (default `12`)  
    - `--dry-run`

Semantics:

- Keep last K bundles per CPID (ordered by `created_at`).  
- Always keep bundles referenced by open incidents.  
- Always keep promotion bundles referenced by `cp_pointers`.

**Examples**

```bash
# Preview GC actions
aosctl maintenance gc-bundles \
  --bundles-path /srv/aos/bundles \
  --db-path var/aos-cp.sqlite3 \
  --keep-count 12 \
  --dry-run

# Apply GC
aosctl maintenance gc-bundles \
  --bundles-path /srv/aos/bundles \
  --db-path var/aos-cp.sqlite3 \
  --keep-count 12
```

---

## 9. Registry Migration

The `registry` tree now owns safe migration of the adapter registry.

- `aosctl registry migrate`  
  - Migrates a legacy `registry.db` into the current `adapteros-registry` schema.  
  - Key flags:  
    - `--from-db` (default `deprecated/registry.db`)  
    - `--to-db` (default `var/registry.db`)  
    - `--dry-run`

Behavior:

- Reads legacy `tenants` and `adapters` tables via `rusqlite`.  
- Writes tenants and adapters into the new registry via `adapteros_registry::Registry`.  
- Emits a `MigrationStats` summary (`adapters_processed`, `migrated`, `failed`, etc.).

**Examples**

```bash
# Standard migration (prod)
aosctl registry migrate

# Explicit paths with dry run
aosctl registry migrate \
  --from-db deprecated/registry.db \
  --to-db var/registry.db \
  --dry-run
```

---

## 10. Metrics, Diagnostics, and SECD

These commands inspect system health, metrics, and diagnostics.

- Metrics and health (may be gated by feature flags)  
  - `aosctl metrics ...` subcommands: current metrics, history, policy thresholds.
- Diagnostics  
  - `aosctl diag` – system and tenant diagnostics, with optional bundle creation.  
  - `aosctl manual` – display this manual.
- SECD status  
  - `aosctl secd-status`  
  - Key flags: `--database`, `--json`.

**Examples**

```bash
# Show SECD status in JSON
aosctl secd-status --json > secd-status.json

# Full system diagnostics bundle
aosctl diag --full --bundle ./var/diag-bundle.zip
```

---

## 11. Learning Resources

These commands provide documentation and tutorials directly in the CLI.

- `aosctl tutorial`  
  - Guided walkthrough of common workflows (tenant setup, adapter registration, inference).
- `aosctl manual`  
  - Prints this manual in the terminal.  
  - Use `--help` on specific commands for additional sections and examples.

---

## 12. Scripting and CI Recommendations

- Prefer `--json` when integrating with CI and ops tooling.  
- Use `--quiet` (or rely on CI auto‑detection) for minimal noise; errors and structured telemetry events are still emitted.  
- When a command fails with an error code like `E2002`, capture the event ID from the message:

```text
✗ E2002 – see: aosctl explain E2002 (event: <EVENT_ID>)
```

You can then use `<EVENT_ID>` with future tooling to look up the corresponding telemetry event produced by the CLI.

[source: crates/adapteros-cli/src/main.rs L1-L220]
