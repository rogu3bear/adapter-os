# AdapterOS CLI Guide

**Purpose**: Comprehensive guide to understanding and using the AdapterOS CLI (`aosctl`)  
**Last Updated**: 2025-01-15  
**Version**: alpha-v0.11-unstable-pre-release

---

## Table of Contents

- [Overview](#overview)
- [Architectural Layers](#architectural-layers)
- [Command Mapping](#command-mapping)
- [When to Use Which Layer](#when-to-use-which-layer)
- [Common Workflows](#common-workflows)
- [Quick Reference](#quick-reference)

---

## Overview

The AdapterOS CLI (`aosctl`) provides access to different layers of the AdapterOS system. Understanding which layer each command operates on is crucial for choosing the right tool for your task.

### Key Concepts

- **Layers**: Different parts of the system with different purposes and access patterns
- **State**: Persistent (database) vs Runtime (memory)
- **Access**: Direct (database) vs API (HTTP) vs Runtime (UDS socket)

---

## Architectural Layers

AdapterOS CLI commands operate at five distinct architectural layers:

### 1. Registry Layer (`registry.db`)

**What it is**: Legacy SQLite registry file for simple adapter management operations.

**Characteristics**:
- File-based storage (`registry.db`)
- Simple, lightweight operations
- No tenant isolation
- Suitable for local development and simple workflows

**Use cases**:
- Quick adapter listing without worker running
- Simple adapter registration for development
- Local adapter management

**Commands**:
- `list-adapters` - List adapters from registry file
- `sync-registry` - Sync adapters from directory to registry
- `adapter-info` - Show adapter information from registry

**Limitations**:
- Does not reflect runtime state
- No tenant-scoped operations
- Limited to basic adapter metadata

---

### 2. Control Plane Database

**What it is**: SQLite database with full tenant management and lifecycle tracking.

**Characteristics**:
- Full multi-tenant support
- Persistent state management
- Administrative operations
- Direct database access (bypasses API)

**Use cases**:
- Administrative adapter registration
- Persistent adapter pinning policies
- Tenant-scoped operations
- Operations that need to persist across worker restarts

**Commands**:
- `register-adapter` - Direct database registration
- `pin-adapter` - Persistent pinning policy (survives restarts)
- `unpin-adapter` - Remove persistent pinning policy
- `list-pinned` - List persistently pinned adapters

**When to use**:
- Setting up adapters before workers start
- Creating persistent policies
- Administrative operations
- Operations that must survive worker restarts

---

### 3. Control Plane HTTP API

**What it is**: User-facing REST API with authentication, validation, and business logic.

**Characteristics**:
- HTTP-based (typically `http://127.0.0.1:8080/api`)
- Authentication and authorization
- Input validation and error handling
- Production-ready workflows

**Use cases**:
- Production adapter registration
- Authenticated operations
- Operations through web UI or external tools
- Path-based adapter discovery and registration

**Commands**:
- `adapters register` - Register adapter via HTTP API (discovers manifest, computes hash)
- `adapter directory-upsert` - Upsert directory adapter via HTTP API

**When to use**:
- Production workflows
- When you have adapter files/directories (not just hash)
- When you need API-level validation
- Integration with external tools

---

### 4. Worker Runtime (UDS)

**What it is**: Runtime state in worker memory, accessed via Unix Domain Socket (UDS).

**Characteristics**:
- Runtime state (in-memory)
- Immediate effect
- Requires worker to be running
- Tenant-scoped operations

**Use cases**:
- Querying runtime adapter state
- Immediate runtime operations
- Hot-swapping adapters
- Runtime adapter management

**Commands**:
- `adapter list` - List adapters currently loaded in worker
- `adapter pin` - Pin adapter in runtime (immediate, may not persist)
- `adapter unpin` - Unpin adapter in runtime
- `adapter profile` - Show runtime adapter metrics
- `adapter-swap` - Hot-swap adapters in running worker
- `infer` - Run inference against worker

**When to use**:
- Worker is running and you need runtime state
- Immediate operations on loaded adapters
- Runtime debugging and monitoring
- Hot-reloading adapters without restart

**Limitations**:
- Requires worker to be running
- State may not persist across restarts
- No direct access to persistent storage

---

### 5. File System (Verification)

**What it is**: File system operations for adapter verification without requiring registry or worker.

**Characteristics**:
- File-based operations
- No database or worker required
- Verification and validation only
- Standalone operations

**Use cases**:
- Verify adapter files before registration
- Check adapter integrity
- Validate signatures and hashes
- Pre-flight checks

**Commands**:
- `verify-adapter` - Verify adapter directory files (hash, signature, manifest)

**When to use**:
- Before registering adapters
- Validating adapter packages
- Pre-flight verification
- Standalone integrity checks

**Limitations**:
- Does not interact with registry or worker
- File system only

---

| Command | Layer | Use Case | Alternative |
|---------|-------|----------|-------------|
| `list-adapters` | Registry | List adapters from registry file | `adapter list` (runtime) |
| `adapter list` | Worker Runtime | List adapters loaded in worker | `list-adapters` (registry) |
| `register-adapter` | Control Plane DB | Direct DB registration (admin) | `adapters register` (HTTP API) |
| `adapters register` | HTTP API | Register via API (discovers files) | `register-adapter` (direct DB) |
| `sync-registry` | Registry | Import adapters from directory to registry | `adapters register` (HTTP API) |
| `pin-adapter` | Control Plane DB | Persistent pinning policy | `adapter pin` (runtime) |
| `adapter pin` | Worker Runtime | Immediate runtime pinning | `pin-adapter` (persistent) |
| `unpin-adapter` | Control Plane DB | Remove persistent pinning policy | `adapter unpin` (runtime) |
| `adapter unpin` | Worker Runtime | Immediate runtime unpinning | `unpin-adapter` (persistent) |
| `list-pinned` | Control Plane DB | List persistent pinning policies | `adapter list` (runtime state) |
| `adapter-info` | Registry | Show adapter info from registry | `adapter profile` (runtime) |
| `adapter profile` | Worker Runtime | Runtime adapter metrics | `adapter-info` (registry) |
| `adapter promote` | Worker Runtime | Promote adapter priority (runtime) | N/A (runtime-only) |
| `adapter demote` | Worker Runtime | Demote adapter priority (runtime) | N/A (runtime-only) |
| `adapter-swap` | Worker Runtime | Hot-swap adapters | N/A (runtime-only) |
| `adapter directory-upsert` | HTTP API | Upsert directory adapter | N/A (API-only) |
| `verify-adapter` | File System | Verify adapter files (no registry/worker) | N/A (verification-only) |
| `infer` | Worker Runtime | Run inference (can activate adapters) | N/A (runtime-only) |

---

## When to Use Which Layer

### Decision Tree

```
Do you need runtime state?
├─ Yes → Use Worker Runtime (UDS) commands
│   └─ Is worker running?
│       ├─ Yes → Use `adapter list`, `adapter pin`, etc.
│       └─ No → Start worker first, or use Registry/DB commands
│
└─ No → Do you need persistent state?
    ├─ Yes → Use Control Plane DB commands
    │   └─ Do you have adapter files to discover?
    │       ├─ Yes → Use HTTP API (`adapters register`)
    │       └─ No → Use direct DB (`register-adapter`)
    │
    └─ No → Use Registry commands (simple, local)
```

### Use Case Examples

#### Scenario 1: Quick Adapter Check (No Worker Running)

**Goal**: See what adapters are registered

**Solution**: Use Registry layer
```bash
aosctl list-adapters
```

**Why**: Registry file is always available, doesn't require worker.

---

#### Scenario 2: Register Adapter from Files

**Goal**: Register adapter from directory with manifest

**Solution**: Use HTTP API layer
```bash
aosctl adapters register --path ./my-adapter/
```

**Why**: Discovers manifest, computes hash automatically, validates through API.

---

#### Scenario 3: Register Adapter with Known Hash

**Goal**: Register adapter when you already have the hash

**Solution**: Use Control Plane DB layer
```bash
aosctl register-adapter my_adapter b3:abc123... --tier persistent --rank 16
```

**Why**: Direct database operation, faster for administrative tasks.

---

#### Scenario 4: Pin Adapter Permanently

**Goal**: Pin adapter so it survives worker restarts

**Solution**: Use Control Plane DB layer
```bash
aosctl pin-adapter --tenant dev --adapter specialist --reason "Production critical"
```

**Why**: Persistent pinning policy stored in database.

---

#### Scenario 5: Pin Adapter Immediately (Worker Running)

**Goal**: Pin adapter right now in running worker

**Solution**: Use Worker Runtime layer
```bash
aosctl adapter pin specialist --tenant dev
```

**Why**: Immediate effect on runtime state.

---

#### Scenario 6: Check What's Actually Loaded

**Goal**: See adapters currently in worker memory

**Solution**: Use Worker Runtime layer
```bash
aosctl adapter list --tenant dev
```

**Why**: Shows actual runtime state, not just registry.

---

#### Scenario 7: Hot-Swap Adapters

**Goal**: Change adapters without restarting worker

**Solution**: Use Worker Runtime layer
```bash
aosctl adapter-swap --tenant dev --add new_adapter --remove old_adapter --commit
```

**Why**: Runtime-only operation, requires worker to be running.

---

## Common Workflows

### Workflow 1: Register and Use Adapter

**Step 1**: Register adapter (choose based on what you have)

If you have adapter files:
```bash
aosctl adapters register --path ./my-adapter/
```

If you only have hash:
```bash
aosctl register-adapter my_adapter b3:abc123... --tier persistent --rank 16
```

**Step 2**: Verify registration
```bash
aosctl list-adapters
```

**Step 3**: Start worker (if not running)
```bash
aosctl serve --tenant dev --plan my_plan
```

**Step 4**: Verify adapter is available in runtime
```bash
aosctl adapter list --tenant dev
```

**Step 5**: Use adapter
```bash
aosctl infer --prompt "..." --adapter my_adapter
```

---

### Workflow 2: Pin Adapter for Production

**Step 1**: Pin adapter persistently (survives restarts)
```bash
aosctl pin-adapter --tenant prod --adapter critical_adapter --reason "Production required"
```

**Step 2**: Verify persistent pin
```bash
aosctl list-pinned --tenant prod
```

**Step 3**: If worker is running, also pin in runtime
```bash
aosctl adapter pin critical_adapter --tenant prod
```

**Step 4**: Verify runtime pin
```bash
aosctl adapter list --tenant prod
```

---

### Workflow 3: Debug Adapter Issues

**Step 1**: Check registry state
```bash
aosctl adapter-info my_adapter
```

**Step 2**: Check if worker is running
```bash
aosctl adapter list --tenant dev
```

**Step 3**: If adapter not loaded, check why
```bash
aosctl adapter profile my_adapter --tenant dev
```

**Step 4**: Hot-reload if needed
```bash
aosctl adapter-swap --tenant dev --add my_adapter --commit
```

---

## Quick Reference

### By Layer

**Registry Layer**:
- `list-adapters` - List from registry file
- `adapter-info` - Show adapter info
- `sync-registry` - Sync directory to registry

**Control Plane Database**:
- `register-adapter` - Direct DB registration
- `pin-adapter` - Persistent pinning
- `unpin-adapter` - Remove persistent pin
- `list-pinned` - List persistent pins

**HTTP API**:
- `adapters register` - Register via API
- `adapter directory-upsert` - Upsert directory adapter

**Worker Runtime (UDS)**:
- `adapter list` - List runtime adapters
- `adapter pin` - Runtime pinning
- `adapter unpin` - Runtime unpinning
- `adapter profile` - Runtime metrics
- `adapter promote` - Promote adapter priority
- `adapter demote` - Demote adapter priority
- `adapter-swap` - Hot-swap adapters
- `infer` - Run inference

**File System**:
- `verify-adapter` - Verify adapter files (no registry/worker required)

### By Task

**List adapters**:
- Registry: `aosctl list-adapters`
- Runtime: `aosctl adapter list --tenant <tenant>`

**Register adapter**:
- With files: `aosctl adapters register --path <path>`
- With hash: `aosctl register-adapter <id> <hash> --tier <tier> --rank <rank>`

**Pin adapter**:
- Persistent: `aosctl pin-adapter --tenant <tenant> --adapter <adapter> --reason <reason>`
- Runtime: `aosctl adapter pin <adapter> --tenant <tenant>`

**Unpin adapter**:
- Persistent: `aosctl unpin-adapter --tenant <tenant> --adapter <adapter>`
- Runtime: `aosctl adapter unpin <adapter> --tenant <tenant>`

**List pinned adapters**:
- Persistent policies: `aosctl list-pinned --tenant <tenant>`
- Runtime state: `aosctl adapter list --tenant <tenant>`

**Check adapter state**:
- Registry: `aosctl adapter-info <adapter>`
- Runtime: `aosctl adapter profile <adapter> --tenant <tenant>`

---

---

## Complete Command Reference

This section provides a comprehensive reference for all CLI commands. For architectural context and usage patterns, see the sections above.

### Global Options

All commands support these global options:

- `--json`: Output in JSON format
- `--quiet`, `-q`: Suppress non-essential output
- `--verbose`, `-v`: Enable verbose output

---

## Tenant Management

### `init-tenant`

Initialize a new tenant

**Usage**:
```bash
aosctl init-tenant --id <TENANT_ID> --uid <UID> --gid <GID>
```

**Parameters**:
- `--id` (required): Tenant ID
- `--uid` (required): Unix UID
- `--gid` (required): Unix GID

**Examples**:
```bash
# Create a development tenant
aosctl init-tenant --id tenant_dev --uid 1000 --gid 1000

# Create a production tenant with custom IDs
aosctl init-tenant --id tenant_prod --uid 5000 --gid 5000

# Quick alias (hidden)
aosctl init --id tenant_test --uid 1000 --gid 1000
```

---

## Adapter Management

### `list-adapters`

List adapters in the registry

**Usage**:
```bash
aosctl list-adapters [OPTIONS]
```

**Parameters**:
- `--tier` (optional): Filter by tier (persistent or ephemeral)

**Examples**:
```bash
aosctl list-adapters
aosctl list-adapters --tier persistent
aosctl list-adapters --json > adapters.json
```

### `register-adapter`

Register a new adapter

**Usage**:
```bash
aosctl register-adapter <ADAPTER_ID> <HASH> --tier <TIER> --rank <RANK>
```

**Parameters**:
- `ADAPTER_ID` (required): Adapter ID
- `HASH` (required): Artifact hash
- `--tier` (optional): Tier (persistent or ephemeral, default: ephemeral)
- `--rank` (required): Rank

**Examples**:
```bash
# Register a persistent adapter
aosctl register-adapter my_adapter b3:abc123... --tier persistent --rank 16

# Register an ephemeral adapter (default)
aosctl register-adapter temp_fix b3:def456... --rank 8

# High-rank adapter for complex tasks
aosctl register-adapter specialist b3:789ghi... --tier persistent --rank 32
```

### `pin-adapter`

Pin adapter to prevent eviction

**Usage**:
```bash
aosctl pin-adapter --tenant <TENANT> --adapter <ADAPTER> --reason <REASON> [OPTIONS]
```

**Parameters**:
- `--tenant` (required): Tenant ID
- `--adapter` (required): Adapter ID
- `--ttl-hours` (optional): TTL in hours (omit for permanent pin)
- `--reason` (required): Reason for pinning

**Examples**:
```bash
# Pin adapter permanently
aosctl pin-adapter --tenant dev --adapter specialist --reason "Production critical"

# Pin adapter with TTL
aosctl pin-adapter --tenant dev --adapter temp_fix --ttl-hours 24 --reason "Testing"

# List pinned adapters
aosctl list-pinned --tenant dev
```

### `unpin-adapter`

Unpin adapter to allow eviction

**Usage**:
```bash
aosctl unpin-adapter --tenant <TENANT> --adapter <ADAPTER>
```

**Parameters**:
- `--tenant` (required): Tenant ID
- `--adapter` (required): Adapter ID

**Examples**:
```bash
# Unpin adapter
aosctl unpin-adapter --tenant dev --adapter temp_fix

# Verify unpinning
aosctl list-pinned --tenant dev
```

### `list-pinned`

List pinned adapters

**Usage**:
```bash
aosctl list-pinned --tenant <TENANT>
```

**Parameters**:
- `--tenant` (required): Tenant ID

**Examples**:
```bash
# List all pinned adapters for tenant
aosctl list-pinned --tenant dev

# Check specific adapter status
aosctl adapter-info specialist
```

### `adapter-swap`

Hot-swap adapters in running worker

**Usage**:
```bash
aosctl adapter-swap --tenant <TENANT> --add <ADAPTERS> --remove <ADAPTERS> [OPTIONS]
```

**Parameters**:
- `--tenant` (required): Tenant ID
- `--add` (optional): Adapter IDs to add (comma-separated)
- `--remove` (optional): Adapter IDs to remove (comma-separated)
- `--timeout` (optional): Timeout in milliseconds (default: 5000)
- `--commit` (optional): Commit the swap (otherwise dry-run)
- `--uds-socket` (optional): UDS socket path (default: /var/run/aos/aos.sock)

**Examples**:
```bash
# Dry-run adapter swap
aosctl adapter-swap --tenant dev --add specialist --remove temp_fix

# Commit adapter swap
aosctl adapter-swap --tenant dev --add specialist --remove temp_fix --commit

# Add multiple adapters
aosctl adapter-swap --tenant dev --add adapter1,adapter2 --commit
```

### `adapter-info`

Show adapter information and provenance

**Usage**:
```bash
aosctl adapter-info <ADAPTER_ID>
```

**Parameters**:
- `ADAPTER_ID` (required): Adapter ID

---

## Adapter Lifecycle Management

### `adapters register`

Register a packaged adapter by path (dir or weights file)

**Usage**:
```bash
aosctl adapters register --path <PATH> [OPTIONS]
```

**Parameters**:
- `--path` (required): Path to packaged adapter dir or weights.safetensors
- `--adapter-id` (optional): Adapter ID (defaults to directory name)
- `--name` (optional): Name to display (defaults to adapter_id)
- `--rank` (optional): Rank (defaults from manifest if present; else 8)
- `--tier` (optional): Tier (ephemeral=0, persistent=1) default ephemeral
- `--base-url` (optional): Control plane base URL (default: http://127.0.0.1:8080/api)

### `adapter list`

List adapters currently loaded in worker

**Usage**:
```bash
aosctl adapter list --tenant <TENANT>
```

**Parameters**:
- `--tenant` (required): Tenant ID

### `adapter pin`

Pin adapter in runtime (immediate, may not persist)

**Usage**:
```bash
aosctl adapter pin <ADAPTER> --tenant <TENANT>
```

**Parameters**:
- `ADAPTER` (required): Adapter ID
- `--tenant` (required): Tenant ID

### `adapter unpin`

Unpin adapter in runtime

**Usage**:
```bash
aosctl adapter unpin <ADAPTER> --tenant <TENANT>
```

**Parameters**:
- `ADAPTER` (required): Adapter ID
- `--tenant` (required): Tenant ID

### `adapter profile`

Show runtime adapter metrics

**Usage**:
```bash
aosctl adapter profile <ADAPTER> --tenant <TENANT>
```

**Parameters**:
- `ADAPTER` (required): Adapter ID
- `--tenant` (required): Tenant ID

### `adapter promote`

Promote adapter priority (runtime)

**Usage**:
```bash
aosctl adapter promote <ADAPTER> --tenant <TENANT>
```

**Parameters**:
- `ADAPTER` (required): Adapter ID
- `--tenant` (required): Tenant ID

### `adapter demote`

Demote adapter priority (runtime)

**Usage**:
```bash
aosctl adapter demote <ADAPTER> --tenant <TENANT>
```

**Parameters**:
- `ADAPTER` (required): Adapter ID
- `--tenant` (required): Tenant ID

### `adapter load`

Load an adapter into runtime memory

**Usage**:
```bash
aosctl adapter load <ADAPTER_ID> [--tenant <TENANT_ID>]
```

**Examples**:
```bash
aosctl adapter load my-adapter
aosctl adapter load my-adapter --tenant tenant_dev
```

### `adapter unload`

Unload an adapter from runtime memory

**Usage**:
```bash
aosctl adapter unload <ADAPTER_ID> [--tenant <TENANT_ID>]
```

---

## Cluster Management

### `node-list`

List cluster nodes

**Usage**:
```bash
aosctl node-list [OPTIONS]
```

**Parameters**:
- `--offline` (optional): Offline mode (use cached database state)

### `node-verify`

Verify cross-node determinism

**Usage**:
```bash
aosctl node-verify [OPTIONS]
```

**Parameters**:
- `--all` (optional): Verify all nodes
- `--nodes` (optional): Specific node IDs to verify (comma-separated)

### `node-sync`

Sync adapters across nodes

**Usage**:
```bash
aosctl node-sync --mode <MODE>
```

### `verify-sync`

Verify sync between two nodes

**Usage**:
```bash
aosctl verify-sync --source <SOURCE> --target <TARGET>
```

**Parameters**:
- `--source` (required): Source node ID
- `--target` (required): Target node ID

### `push`

Push adapters to target node

**Usage**:
```bash
aosctl push --target <TARGET> --adapters <ADAPTERS>
```

**Parameters**:
- `--target` (required): Target node ID
- `--adapters` (required): Adapter IDs to push (comma-separated)

### `pull`

Pull adapters from source node

**Usage**:
```bash
aosctl pull --source <SOURCE> --adapters <ADAPTERS>
```

**Parameters**:
- `--source` (required): Source node ID
- `--adapters` (required): Adapter IDs to pull (comma-separated)

### `export`

Export adapters for air-gap transfer

**Usage**:
```bash
aosctl export --output <OUTPUT>
```

**Parameters**:
- `--output` (required): Output file path

### `import`

Import adapters from air-gap bundle

**Usage**:
```bash
aosctl import --input <INPUT>
```

**Parameters**:
- `--input` (required): Input file path

---

## Model Operations

### `import-model`

Import a model

**Usage**:
```bash
aosctl import-model --name <NAME> --weights <WEIGHTS> --config <CONFIG> --tokenizer <TOKENIZER> [OPTIONS]
```

**Parameters**:
- `--name` (required): Model name
- `--weights` (required): Weights file path
- `--config` (required): Config file path
- `--tokenizer` (required): Tokenizer file path
- `--tokenizer-config` (optional): Tokenizer config file path
- `--license` (required): License file path

### `registry-sync`

Sync adapters from local directory to registry

**Usage**:
```bash
aosctl registry-sync --directory <DIRECTORY> [OPTIONS]
```

**Parameters**:
- `--directory` (required): Directory containing adapters with SBOM and signatures
- `--cas-root` (optional): CAS root directory (default: ./var/cas)
- `--registry-db` (optional): Registry database path (default: ./var/registry.db)

### `build-plan`

Build a plan from manifest

**Usage**:
```bash
aosctl build-plan <MANIFEST> [OPTIONS]
```

**Parameters**:
- `MANIFEST` (required): Manifest path
- `--output`, `-o` (optional): Output path
- `--tenant` (optional): Tenant ID (defaults to "default")

---

## Telemetry & Auditing

### `verify-telemetry`

Verify telemetry bundle chain

**Usage**:
```bash
aosctl verify-telemetry --bundle-dir <DIRECTORY>
```

**Parameters**:
- `--bundle-dir` (required): Telemetry bundle directory

### `trace-validate`

Validate a trace file for integrity and limits

**Usage**:
```bash
aosctl trace-validate <PATH> [OPTIONS]
```

**Parameters**:
- `PATH` (required): Path to trace file (.ndjson or .ndjson.zst)
- `--strict` (optional): Strict mode (default)
- `--tolerant` (optional): Tolerant mode (skip invalid lines/events)
- `--verify-hashes` (optional): Verify per-event hashes
- `--max-events` (optional): Maximum number of events to read
- `--max-bytes` (optional): Maximum total bytes to read
- `--max-line-length` (optional): Maximum line length in bytes

### `federation-verify`

Verify cross-host federation signatures

**Usage**:
```bash
aosctl federation-verify --bundle-dir <DIRECTORY> [OPTIONS]
```

**Parameters**:
- `--bundle-dir` (required): Telemetry bundle directory
- `--database` (optional): Database path (default: ./var/cp.db)

### `drift-check`

Check for environment drift

**Usage**:
```bash
aosctl drift-check [OPTIONS]
```

**Parameters**:
- `--database` (optional): Database path
- `--baseline` (optional): Baseline fingerprint path
- `--save-fingerprint` (optional): Save current fingerprint
- `--save-baseline` (optional): Save as new baseline

### `callgraph-export`

Export call graph to various formats

**Usage**:
```bash
aosctl callgraph-export --codegraph-db <DB> --output <OUTPUT> [OPTIONS]
```

**Parameters**:
- `--codegraph-db` (required): CodeGraph database path
- `--output` (required): Output file path
- `--format` (optional): Export format (dot, json, csv) (default: dot)

### `codegraph-stats`

Generate CodeGraph statistics

**Usage**:
```bash
aosctl codegraph-stats --codegraph-db <DB>
```

**Parameters**:
- `--codegraph-db` (required): CodeGraph database path

---

## Policy Management

### `policy`

Manage policy packs

**Usage**:
```bash
aosctl policy <SUBCOMMAND>
```

**Subcommands**: Policy pack management commands

---

## Serving & Inference

### `serve`

Start serving

**Usage**:
```bash
aosctl serve --tenant <TENANT> --plan <PLAN> [OPTIONS]
```

**Parameters**:
- `--tenant` (required): Tenant ID
- `--plan` (required): Plan ID
- `--uds-socket` (optional): UDS socket path (default: /var/run/aos/aos.sock)
- `--backend` (optional): Backend selection: metal, mlx, or coreml (default: metal)
- `--dry-run` (optional): Dry-run: validate preflight checks without starting server
- `--skip-pf-deny` (optional): INSECURE: Skip PF egress preflight (development only)
- `--telemetry-dir` (optional): Capture telemetry events to this directory

### `infer`

Run a local inference against the worker UDS

**Usage**:
```bash
aosctl infer --prompt <PROMPT> [OPTIONS]
```

**Parameters**:
- `--adapter` (optional): Optional adapter to activate before inference
- `--prompt` (required): Prompt text to infer on
- `--uds-socket` (optional): UDS socket path (default: /var/run/aos/aos.sock)
- `--max-tokens` (optional): Max tokens to generate
- `--require-evidence` (optional): Require evidence (RAG/open-book) if enabled in worker
- `--timeout` (optional): Timeout in milliseconds
- `--show-citations` (optional): Show citations (trace.evidence) in output
- `--show-trace` (optional): Show full trace (router summary, token counts)

---

## Development & Testing

### `audit`

Run audit checks

**Usage**:
```bash
aosctl audit <CPID> [OPTIONS]
```

**Parameters**:
- `CPID` (required): CPID to audit
- `--suite` (optional): Test suite path

### `audit-determinism`

Audit backend determinism attestation

**Usage**:
```bash
aosctl audit-determinism <ARGS>
```

### `replay`

Replay a bundle

**Usage**:
```bash
aosctl replay <BUNDLE> [OPTIONS]
```

**Parameters**:
- `BUNDLE` (required): Bundle path
- `--verbose` (optional): Show divergence details

### `rollback`

Rollback to previous checkpoint

**Usage**:
```bash
aosctl rollback --tenant <TENANT> --cpid <CPID>
```

**Parameters**:
- `--tenant` (required): Tenant ID
- `--cpid` (required): Target CPID

### `baseline`

Baseline management (record/verify/delta with BLAKE3+Ed25519)

**Usage**:
```bash
aosctl baseline <SUBCOMMAND>
```

### `golden`

Golden run archive management (audit reproducibility)

**Usage**:
```bash
aosctl golden <SUBCOMMAND>
```

### `router`

Router weight calibration and management

**Usage**:
```bash
aosctl router <SUBCOMMAND>
```

### `report`

Generate HTML report from bundle

**Usage**:
```bash
aosctl report <BUNDLE> [OPTIONS]
```

**Parameters**:
- `BUNDLE` (required): Bundle path
- `--output` (required): Output HTML file

---

## System Administration

### `secd-status`

Show aos-secd daemon status

**Usage**:
```bash
aosctl secd-status [OPTIONS]
```

**Parameters**:
- `--pid-file` (optional): PID file path (default: /var/run/aos-secd.pid)
- `--heartbeat-file` (optional): Heartbeat file path (default: /var/run/aos-secd.heartbeat)
- `--socket` (optional): Socket path (default: /var/run/aos-secd.sock)
- `--database` (optional): Database path (default: ./var/aos-cp.sqlite3)

### `secd-audit`

Show aos-secd operation audit trail

**Usage**:
```bash
aosctl secd-audit [OPTIONS]
```

**Parameters**:
- `--database` (optional): Database path (default: ./var/aos-cp.sqlite3)
- `--limit` (optional): Number of operations to show (default: 50)
- `--filter` (optional): Filter by operation type (sign, seal, unseal, get_public_key)

### `bootstrap-admin`

Create the initial control plane admin user with a generated password

**Usage**:
```bash
aosctl bootstrap-admin --email <EMAIL> [OPTIONS]
```

**Parameters**:
- `--email` (required): Email for the admin user
- `--display-name` (optional): Optional display name (defaults to email prefix)

### `bootstrap`

Bootstrap AdapterOS installation

**Usage**:
```bash
aosctl bootstrap [OPTIONS]
```

**Parameters**:
- `--mode` (optional): Installation mode (full or minimal)
- `--air-gapped` (optional): Enable air-gapped mode (skip network operations)
- `--json-progress` (optional): Output JSON progress updates
- `--checkpoint` (optional): Checkpoint file path

### `diag`

Run system diagnostics

**Usage**:
```bash
aosctl diag [OPTIONS]
```

**Parameters**:
- `--profile` (optional): Diagnostic profile: system, tenant, or full (default: full)
- `--tenant` (optional): Tenant ID for tenant-specific checks
- `--json` (optional): Output JSON format
- `--bundle` (optional): Create diagnostic bundle
- `--system-only` (optional): System checks only
- `--tenant-only` (optional): Tenant checks only

---

## Code Intelligence

### `code-init`

Initialize a code repository

**Usage**:
```bash
aosctl code-init --path <PATH> --tenant <TENANT> [OPTIONS]
```

**Parameters**:
- `--path` (required): Repository path
- `--tenant` (required): Tenant ID
- `--repo` (optional): Repository ID
- `--commit` (optional): Commit SHA
- `--languages` (optional): Comma-separated list

### `code-update`

Update repository scan

**Usage**:
```bash
aosctl code-update --repo <REPO> --commit <COMMIT> --parent <PARENT> --tenant <TENANT>
```

**Parameters**:
- `--repo` (required): Repository ID
- `--commit` (required): Commit SHA
- `--parent` (required): Parent commit SHA
- `--tenant` (required): Tenant ID

### `code-list`

List registered repositories

**Usage**:
```bash
aosctl code-list --tenant <TENANT>
```

**Parameters**:
- `--tenant` (required): Tenant ID

### `code-status`

Get repository status

**Usage**:
```bash
aosctl code-status --repo <REPO> --tenant <TENANT>
```

**Parameters**:
- `--repo` (required): Repository ID
- `--tenant` (required): Tenant ID

---

## AOS File Operations

### `aos`

AOS adapter file operations (create, verify, info, convert)

**Usage**:
```bash
aosctl aos <SUBCOMMAND>
```

### `import`

Import an artifact bundle

**Usage**:
```bash
aosctl import <BUNDLE> [OPTIONS]
```

**Parameters**:
- `BUNDLE` (required): Bundle path
- `--no-verify` (optional): Skip signature verification

### `verify`

Verify a bundle

**Usage**:
```bash
aosctl verify <BUNDLE>
```

**Parameters**:
- `BUNDLE` (required): Bundle path

### `verify-adapter`

Verify a packaged adapter directory

**Usage**:
```bash
aosctl verify-adapter [OPTIONS]
```

**Parameters**:
- `--adapters-root` (optional): Adapters root directory (default: ./adapters)
- `--adapter-id` (optional): Adapter ID to verify

---

## Training & Quantization

### `quantize-qwen`

Quantize Qwen FP16 weights to int4 and write manifest

**Usage**:
```bash
aosctl quantize-qwen --input <INPUT> --output <OUTPUT> --model-name <NAME> [OPTIONS]
```

**Parameters**:
- `--input` (required): Input path (.safetensors file or directory containing them)
- `--output` (required): Output directory for .bin and manifest.json
- `--model-name` (required): Model name for manifest metadata
- `--block-size` (optional): Optional block size for stats (currently unused)
- `--manifest-only` (optional): Output manifest JSON to stdout

### `train`

Train a LoRA adapter

**Usage**:
```bash
aosctl train <ARGS>
```

**Examples**:
```bash
aosctl train --dataset-id dataset_123 --output adapters/my-adapter.aos --rank 16 --epochs 5
```

**Metrics & backend visibility**: Training runs record backend, backend_device, determinism_mode, and throughput (tokens/examples processed per second). You can view these in the UI training job detail page or fetch them via `GET /v1/training/jobs/<id>/metrics` for CLI/automation.

### `train-base-adapter`

Train base adapter from manifest

**Usage**:
```bash
aosctl train-base-adapter <ARGS>
```

---

## Utilities

### `completions`

Generate shell completion script

**Usage**:
```bash
aosctl completions <SHELL>
```

**Parameters**:
- `SHELL` (required): Shell type

### `explain`

Explain an error code or AosError variant

**Usage**:
```bash
aosctl explain <CODE>
```

**Parameters**:
- `CODE` (required): Error code (E3001) or AosError name (InvalidHash)

### `error-codes`

List all error codes

**Usage**:
```bash
aosctl error-codes [OPTIONS]
```

**Parameters**:
- `--json` (optional): Output JSON format

### `tutorial`

Interactive tutorial

**Usage**:
```bash
aosctl tutorial [OPTIONS]
```

**Parameters**:
- `--advanced` (optional): Run advanced tutorial
- `--ci` (optional): Non-interactive mode for CI

### `manual`

Display offline manual

**Usage**:
```bash
aosctl manual <ARGS>
```

---

## Exit Codes

- `0`: Success
- `1`: General error
- `2`: Invalid arguments
- `3`: Authentication/authorization failure
- `4`: Policy violation
- `5`: Job failed (scan, train, etc.)
- `6`: Gate failed (audit)

---

**Note**: This reference is auto-generated from the CLI codebase. For the latest information, use `aosctl --help` or `aosctl <command> --help`.

---

## See Also

- [Architecture Overview](ARCHITECTURE.md) - System architecture
- [Control Plane](ARCHITECTURE.md#architecture-components) - Control plane architecture
- [API Reference](API_REFERENCE.md) - REST API documentation
- [CLAUDE.md](../CLAUDE.md) - Complete developer reference

MLNavigator Inc 2025-12-08.
