% aosctl — adapterOS CLI Manual

This manual provides an overview of the `aosctl` command‑line interface, including command groups, flag conventions, and usage examples. It is intended as a stable high‑level reference; for exhaustive per‑flag help, run `aosctl <command> --help`.

**New to adapterOS?** Start with [docs/CONCEPTS.md](../../../docs/CONCEPTS.md) to learn about Tenants, Adapters, Stacks, Router, Telemetry, and Replay.

---

## 0. Core Concepts (Quick Reference)

Before using `aosctl`, understand these core entities:

- **Tenant**: Top-level isolation unit (user, org, environment). Create with `aosctl init-tenant`.
- **Adapter**: LoRA module that specializes a base model. Register with `aosctl register-adapter`.
- **Stack**: Tenant-scoped set of adapters + workflow rules. Create with `aosctl create-stack`.
- **Router**: K-sparse gating mechanism that selects top-K adapters per request.
- **Telemetry**: Structured event logging for audit trail. Verify with `aosctl telemetry-verify`.
- **Golden Run**: Verified, deterministic execution for replay verification.
- **Replay**: Re-execute golden run to verify determinism with `aosctl replay`.

For full details, see [docs/CONCEPTS.md](../../../docs/CONCEPTS.md).

---

## 1. Quickstart Overview

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

**What is a Tenant?** A tenant is the top-level isolation unit in adapterOS, representing a user, organization, or environment. Tenants own adapters and stacks, and enforce resource limits.

Tenant commands create and manage isolated tenants on a node.

- `aosctl init-tenant` / `aosctl init`
  - Initialize a new tenant with specific UID/GID.
  - Key flags: `--id`, `--uid`, `--gid`.
  - See [docs/CONCEPTS.md#tenant](../../../docs/CONCEPTS.md#1-tenant) for details.

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

**What is an Adapter?** An adapter is a LoRA (Low-Rank Adaptation) module that specializes a base model for a specific task. Adapters have business lifecycle states (draft -> training -> ready -> active -> deprecated -> retired/failed) and runtime memory states (Unloaded -> Loading -> Loaded -> Active -> Unloading, with Error on failure); they can be pinned to prevent eviction.

**Naming Convention**: `{tenant}/{domain}/{purpose}/{revision}` (e.g., `tenant-a/engineering/code-review/r001`)

Adapter commands manage adapters in the registry (listing, registration, pinning, and air‑gap transfers).

- List adapters
  - `aosctl list-adapters`
  - Key flags: `--tier` (filter by tier), `--json` for machine‑readable output.
  - See [docs/CONCEPTS.md#adapter](../../../docs/CONCEPTS.md#2-adapter) for details.
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

**What is Status?** Status commands query the system state to show **Adapters** (memory, tier, pinned), **Tenants** (cluster nodes), tick ledger (for determinism tracking), and memory pressure.

The `status` tree makes `aosctl` the "system brain" for high‑level state.

- `aosctl status adapters`
  - Lists adapters from the control‑plane DB with: `name`, `tenant_id`, `active`, `pinned`, `expires_at`, and `memory_bytes`.
  - Shows **Lifecycle** tier and **Pinning** status.
  - Respects `--json` for structured output.
  - See [docs/CONCEPTS.md#adapter](../../../docs/CONCEPTS.md#2-adapter) for lifecycle details.

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

**What is Deploying?** Deployment copies **Adapter** files to the system directory, registers them with semantic names (tenant/domain/purpose/revision), and makes them available for use in **Stacks**.

The `deploy` tree replaces the legacy `scripts/deploy_adapters.sh` script.

- `aosctl deploy adapters`
  - Deploys adapter directories, `.aos` files, or `.safetensors` weights.
  - Registers adapters in the system so they can be used in **Stacks**.
  - Key flags:
    - `--path <dir-or-file>` (repeatable): directories, `.aos`, or `.safetensors`.
    - `--adapters-dir`: target adapter directory (default `/opt/adapteros/adapters`).
    - `--backup-existing`: back up any existing adapter with the same name.
    - `--dry-run`: show what would be done without touching disk or registry.
  - See [docs/CONCEPTS.md#adapter](../../../docs/CONCEPTS.md#2-adapter) for naming conventions.

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

**What is Inference?** Inference sends a prompt to the system. The **Router** selects top-K adapters from a stack, the **Kernel** executes them, and **Telemetry** records all events.

**What is Replay?** Replay re-executes a **Golden Run** (verified execution) to verify determinism by comparing outputs byte-for-byte.

These commands interact with running workers and telemetry bundles.

- `aosctl infer`
  - Run an inference against a worker UDS.
  - Key flags: `--adapter`, `--prompt`, `--socket`, `--max-tokens`, `--timeout`, `--require-evidence`.
  - See [docs/CONCEPTS.md#workflow-1](../../../docs/CONCEPTS.md#workflow-1-training--adapter--stack--inference) for full flow.
- `aosctl replay`
  - Replay a telemetry bundle and optionally check determinism.
  - See [docs/CONCEPTS.md#golden-run](../../../docs/CONCEPTS.md#7-golden-run--replay) for details.

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

## 6a. Interactive Chat

**What is Interactive Chat?** The `chat` command provides an interactive REPL-style interface for chatting with the adapterOS inference runtime, with support for streaming responses, model/stack selection, and session management.

**Commands:**

- `aosctl chat interactive` - Start interactive chat REPL
  - Key flags:
    - `--stack <id>` - Use specific adapter stack
    - `--model <id>` - Use specific model
    - `--server-url <url>` - Custom server URL (default: http://127.0.0.1:8080/api)
    - `--timeout <secs>` - Request timeout (default: 30)
  
- `aosctl chat prompt` - Send a single prompt and exit
  - Key flags:
    - `--text <prompt>` - Prompt text (required)
    - `--stack <id>` - Use specific adapter stack
    - `--model <id>` - Use specific model
    - `--max-tokens <n>` - Maximum tokens in response
    - `--temperature <f>` - Sampling temperature (0.0-2.0)
  
- `aosctl chat list` - List chat sessions
  - Key flags:
    - `--json` - JSON output format
  
- `aosctl chat history <session-id>` - View chat session history
  - Key flags:
    - `--json` - JSON output format

**REPL Commands (in interactive mode):**
- `exit`, `quit`, `Ctrl+D` - Exit chat
- `/help` - Show available commands
- `/clear` - Clear screen
- `/stack <id>` - Switch adapter stack
- `/model <id>` - Switch model
- `/status` - Show current configuration

**Examples:**

```bash
# Start interactive chat
aosctl chat interactive --stack my-stack

# Send single prompt
aosctl chat prompt --text "Explain async in Rust" --stack my-stack

# List sessions
aosctl chat list --json
```

## 6b. Development Commands

**What are Development Commands?** The `dev` command provides utilities for managing the local development environment, including starting/stopping services, viewing logs, and checking status.

**Commands:**

- `aosctl dev up` - Start development services
  - Key flags:
    - `--ui` - Start UI dev server
    - `--db-reset` - Reset database before starting
    - `--skip-migrations` - Skip database migrations
  
- `aosctl dev down` - Stop development services
  
- `aosctl dev status` - Show development service status
  - Key flags:
    - `--json` - JSON output format
  
- `aosctl dev logs` - Tail development service logs
  - Key flags:
    - `--service <name>` - Service name (api or ui)
    - `--lines <n>` - Number of lines to show (default: 50)

**Examples:**

```bash
# Start all services
aosctl dev up

# Start with UI
aosctl dev up --ui

# Check status
aosctl dev status --json

# View API logs
aosctl dev logs --service api --lines 100
```

## 6c. Scenario Utilities

**What are Scenario Utilities?** The `scenario` command provides utilities for managing and testing scenarios in adapterOS.

**Commands:**

- `aosctl scenario <subcommand>` - Various scenario management commands
  - See `aosctl scenario --help` for available subcommands

**Examples:**

```bash
# List available scenario commands
aosctl scenario --help
```

## 6d. Documentation Training

**What is Documentation Training?** The `train-docs` command trains adapters using documentation as the training dataset.

**Commands:**

- `aosctl train-docs` - Train adapter from documentation
  - Key flags:
    - `--docs-dir <path>` - Documentation directory
    - `--revision <version>` - Training revision/version
    - `--dry-run` - Preview training without executing

**Examples:**

```bash
# Train from documentation
aosctl train-docs --docs-dir ./my-docs --revision v2

# Preview training plan
aosctl train-docs --docs-dir ./my-docs --dry-run
```

## 6e. Bootstrap Workflow

**What is Bootstrap?** The `bootstrap` command initializes a new adapterOS installation with required configuration and setup.

**Commands:**

- `aosctl bootstrap` - Bootstrap adapterOS installation
  - Key flags:
    - `--mode <mode>` - Bootstrap mode: `full` or `minimal`
    - `--air-gapped` - Air-gapped installation mode
    - `--checkpoint-file <path>` - Checkpoint file for resumable bootstrap
    - `--json` - JSON output format

**Examples:**

```bash
# Full bootstrap
aosctl bootstrap --mode full

# Minimal bootstrap
aosctl bootstrap --mode minimal

# Air-gapped bootstrap
aosctl bootstrap --mode full --air-gapped

# Bootstrap with checkpoint
aosctl bootstrap --mode full --checkpoint-file ./checkpoint.json
```

## 6f. Backend Status

**What is Backend Status?** The `backend-status` command checks the status and health of configured backends (MLX, CoreML, Metal).

**Commands:**

- `aosctl backend-status` - Check backend status
  - Key flags:
    - `--detailed` - Show detailed backend information
    - `--json` - JSON output format

**Examples:**

```bash
# Check backend status
aosctl backend-status

# Detailed status
aosctl backend-status --detailed --json
```

## 6g. Terminal UI Dashboard

**What is the TUI?** The `tui` command launches an interactive terminal UI dashboard for monitoring adapterOS services, logs, and configuration (requires `--features tui` build).

**Commands:**

- `aosctl tui` - Launch interactive TUI dashboard
  - Key flags:
    - `--server-url <url>` - Server URL (default: http://localhost:8080)

**Quick Keys:**
- `b` - Boot services
- `s` - Services view
- `l` - Logs view
- `m` - Metrics view
- `c` - Config view
- `q` - Quit

**Examples:**

```bash
# Launch TUI
aosctl tui

# TUI with custom server
aosctl tui --server-url http://localhost:9000
```

**Note:** TUI requires building with `--features tui`:
```bash
cargo build --release -p adapteros-cli --features tui
```

## 6h. Offline Manual

**What is the Manual Command?** The `manual` command displays offline documentation directly in the terminal.

**Commands:**

- `aosctl manual` - Display offline manual
  - Key flags:
    - `--format <format>` - Output format: `man` or `md` (default: md)
    - `--search <term>` - Search for specific terms

**Examples:**

```bash
# Display manual
aosctl manual

# Manual in man format
aosctl manual --format man

# Search manual
aosctl manual --format md --search "error codes"
```

## 6i. Chat Sessions

**What is a Chat Session?** A chat session is a persistent conversation context that maintains message history across multiple prompts. Sessions enable multi-turn conversations with adapters and stacks.

Chat commands provide interactive and programmatic access to chat sessions.

- `aosctl chat interactive`
  - Start an interactive chat session with an adapter or stack.
  - Key flags:
    - `--stack`: Specify stack ID for multi-adapter routing.
    - `--owner-system`: Mark session as system-owned (for automated workflows).
    - `--base-url`: Control plane URL (default: http://127.0.0.1:8080).
    - `-v, --verbose`: Show detailed request/response info.
  - Type `/quit` or `/exit` to end the session.
  - Use `Ctrl+C` to cancel the current request.

- `aosctl chat prompt`
  - Send a single prompt and get a response (non-interactive).
  - Key flags:
    - `--text`: The prompt text to send.
    - `--stack`: Stack ID for routing.
    - `--owner-system`: Mark as system-owned.
    - `--json`: Output response as JSON.
  - Useful for scripting and automation.

- `aosctl chat list`
  - List all saved chat sessions for the current tenant.
  - Key flags:
    - `--json`: Output as JSON array.
    - `--base-url`: Control plane URL.

- `aosctl chat history`
  - View the full message history for a specific session.
  - Key flags:
    - `<session-id>`: The session ID to view.
    - `--json`: Output messages as JSON.
    - `--base-url`: Control plane URL.

**Session Lifecycle**:
1. Sessions are created automatically when starting interactive mode or sending a prompt.
2. Each session has a unique ID that can be used to resume or view history.
3. Sessions track all user and assistant messages with timestamps.
4. System-owned sessions (`--owner-system`) are tagged for automated workflow tracking.

**Examples**

```bash
# Start interactive chat with a stack
aosctl chat interactive --stack my-stack

# Start system-owned session (for CI/automation)
aosctl chat interactive --owner-system

# Send a single prompt and get JSON response
aosctl chat prompt --text "What is adapterOS?" --json

# List all chat sessions
aosctl chat list --json

# View history for a specific session
aosctl chat history sess_abc123 --json
```

---

## 7. Determinism and Verification

**What is Determinism?** Determinism means identical inputs produce identical outputs. Verification checks that **Kernels** are precompiled, **Router** uses fixed seeds, and **Replay** matches **Golden Runs** byte-for-byte.

Determinism and adapter deliverable checks are fronted through `aosctl`.

- `aosctl verify determinism-loop`
  - Runs the Determinism Loop verification pipeline:
    - Validates presence of key federation, policy, tick ledger, telemetry, CAB, orchestrator, and doc files.
    - Runs `cargo check` for determinism‑critical crates.
    - Optionally runs `cargo xtask determinism-report`.
  - Verifies **Kernel** precompilation, HKDF seeding, and canonical JSON serialization.
  - Exit code: `0` if all checks pass, `1` otherwise.
  - With `--json`, emits a `DeterminismLoopResult { ok, checks[] }`.
  - See [docs/CONCEPTS.md#golden-run](../../../docs/CONCEPTS.md#7-golden-run--replay) for replay details.

- `aosctl verify-adapters`  
  - Wraps `cargo xtask verify-agents` (adapter deliverables A–F).  
  - Ideal for CI and pre‑release gates.  
  - With `--json`, emits `VerifyAdaptersResult { ok, exit_code, stdout_head, stderr_head }`.

- Telemetry verification
  - `aosctl telemetry-verify --bundle-dir var/telemetry`
  - Validates the **Merkle chain** and Ed25519 signatures of **Telemetry** bundles.
  - See [docs/CONCEPTS.md#telemetry](../../../docs/CONCEPTS.md#6-telemetry) for bundle format.

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

**What is GC?** Garbage collection removes old **Telemetry** bundles to manage disk space while preserving bundles needed for audit, incident response, and **Golden Run** replay.

Maintenance commands manage long‑term storage and housekeeping.

- `aosctl db unlock`
  - Clears dirty `_sqlx_migrations` rows and truncates SQLite WAL/SHM files after failed or interrupted migrations.
  - Key flags:
    - `--db-path` (default `var/aos-cp.sqlite3` or `DATABASE_URL` if set)
  - Safe to run after disk guard fallback (`AOS_VAR_DIR` redirect) or when migrations time out; successful migrations remain intact.

- `aosctl maintenance gc-bundles`
  - Garbage‑collects telemetry bundles according to Ruleset #10.
  - Preserves bundles needed for audit and **Replay**.
  - Key flags:
    - `--bundles-path` (default `/srv/aos/bundles`)
    - `--db-path` (default `var/aos-cp.sqlite3`)
    - `--keep-count N` (default `12`)
    - `--dry-run`
  - See [docs/CONCEPTS.md#telemetry](../../../docs/CONCEPTS.md#6-telemetry) for bundle lifecycle.

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

**What is Registry Migration?** Migration upgrades the database schema that stores **Adapters** and **Tenants** while preserving all data, semantic names, and ACLs.

The `registry` tree now owns safe migration of the adapter registry.

- `aosctl registry migrate`
  - Migrates a legacy `registry.db` into the current `adapteros-registry` schema.
  - Preserves **Adapter** names, hashes, tiers, and **Tenant** ACLs.
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

**What are Metrics?** Metrics track system performance: **Router** latency, **Adapter** activation %, memory usage, and **Telemetry** event rates. Use for monitoring and alerting.

These commands inspect system health, metrics, and diagnostics.

- Metrics and health (may be gated by feature flags)
  - `aosctl metrics ...` subcommands: current metrics, history, policy thresholds.
  - Queries **Telemetry** aggregations for real-time system stats.
- Diagnostics
  - `aosctl diag` – system and tenant diagnostics, with optional bundle creation.
  - `aosctl manual` – display this manual.
- SECD status
  - `aosctl secd-status`
  - Key flags: `--database`, `--json`.
  - See [docs/CONCEPTS.md](../../../docs/CONCEPTS.md) for system architecture.

**Examples**

```bash
# Show SECD status in JSON
aosctl secd-status --json > secd-status.json

# Full system diagnostics bundle
aosctl diag --full --bundle var/diag-bundle.zip
```

---

## 11. Learning Resources

These commands provide documentation and tutorials directly in the CLI.

- `aosctl tutorial`
  - Guided walkthrough of common workflows (tenant setup, adapter registration, inference).
  - **Recommended**: Start with the concepts overview to understand the mental model.
- `aosctl manual`
  - Prints this manual in the terminal.
  - Use `--help` on specific commands for additional sections and examples.
- **External Docs**:
  - [docs/CONCEPTS.md](../../../docs/CONCEPTS.md) - **START HERE** - Unified mental model
  - [docs/ARCHITECTURE.md](../../../docs/ARCHITECTURE.md) - Architecture documentation
  - [AGENTS.md](../../../AGENTS.md) - Developer guide

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
