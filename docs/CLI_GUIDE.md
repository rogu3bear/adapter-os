# adapterOS CLI Guide

**Purpose**: Comprehensive guide to understanding and using the adapterOS CLI (`aosctl`)
**Last Updated**: January 2026
**Version**: 0.12.0

---

## Table of Contents

- [Overview](#overview)
- [Architectural Layers](#architectural-layers)
- [Command Mapping](#command-mapping)
- [When to Use Which Layer](#when-to-use-which-layer)
- [Common Workflows](#common-workflows)
- [Quick Reference](#quick-reference)
- [Complete Command Reference](#complete-command-reference)
  - [Tenant Management](#tenant-management)
  - [Adapter Management](#adapter-management)
  - [Adapter Lifecycle Management](#adapter-lifecycle-management)
  - [Dataset Management](#dataset-management)
  - [Database Management](#database-management)
  - [Storage Management](#storage-management)
  - [Preflight Checks](#preflight-checks)
  - [Cluster Management](#cluster-management)
  - [Model Operations](#model-operations)
  - [Telemetry and Auditing](#telemetry--auditing)
  - [Policy Management](#policy-management)
  - [Serving and Inference](#serving--inference)
  - [Development and Testing](#development--testing)
  - [System Administration](#system-administration)
  - [Code Intelligence](#code-intelligence)
  - [AOS File Operations](#aos-file-operations)
  - [Training and Quantization](#training--quantization)
  - [Utilities](#utilities)
- [Codebase Adapter Scope](#codebase-adapter-scope)
- [Alias Change Gating](#alias-change-gating)
- [Command Aliases](#command-aliases)

---

## Overview

The adapterOS CLI (`aosctl`) provides access to different layers of the adapterOS system. Understanding which layer each command operates on is crucial for choosing the right tool for your task.

### Key Concepts

- **Layers**: Different parts of the system with different purposes and access patterns
- **State**: Persistent (database) vs Runtime (memory)
- **Access**: Direct (database) vs API (HTTP) vs Runtime (UDS socket)

---

## Architectural Layers

adapterOS CLI commands operate at five distinct architectural layers:

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

### `adapter seal`

Seal an adapter into a cryptographically secure container

**Usage**:
```bash
aosctl adapter seal [OPTIONS]
```

**Parameters**:
- `--input-bundle` (required): Path to adapter bundle directory
- `--signing-key` (required): Path to Ed25519 signing key (.pem file)
- `--output` (required): Output path for .sealed.aos file
- `--metadata` (optional): JSON metadata string
- `--name` (optional): Adapter name override

**Examples**:
```bash
# Seal an adapter with signing key
aosctl adapter seal \
  --input-bundle ./my-adapter/ \
  --signing-key ./sealing-key.pem \
  --output my-adapter.sealed.aos \
  --metadata '{"version": "1.0", "author": "research-team"}'

# Seal with custom name
aosctl adapter seal \
  --input-bundle ./adapter-bundle/ \
  --signing-key ./key.pem \
  --output sealed.aos \
  --name "production-model-v1"
```

### `adapter load-sealed`

Load and verify a sealed adapter

**Usage**:
```bash
aosctl adapter load-sealed <SEALED_FILE> [OPTIONS]
```

**Parameters**:
- `SEALED_FILE` (required): Path to .sealed.aos file
- `--trusted-key` (required): Path to trusted public key or hex string
- `--trusted-key-hex` (optional): Hex-encoded Ed25519 public key
- `--tenant` (optional): Target tenant ID
- `--json` (optional): Output in JSON format

**Examples**:
```bash
# Load with public key file
aosctl adapter load-sealed model.sealed.aos --trusted-key trusted.pub

# Load with hex-encoded key
aosctl adapter load-sealed model.sealed.aos \
  --trusted-key-hex "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a"

# Load for specific tenant
aosctl adapter load-sealed model.sealed.aos \
  --trusted-key trusted.pub \
  --tenant production-tenant

# JSON output for scripting
aosctl adapter load-sealed model.sealed.aos \
  --trusted-key trusted.pub \
  --json > load_result.json
```

### `adapter verify-seal`

Verify sealed adapter integrity without loading

**Usage**:
```bash
aosctl adapter verify-seal <SEALED_FILE> [OPTIONS]
```

**Parameters**:
- `SEALED_FILE` (required): Path to .sealed.aos file
- `--trusted-key` (required): Path to trusted public key or hex string
- `--trusted-key-hex` (optional): Hex-encoded Ed25519 public key
- `--json` (optional): Output in JSON format

**Examples**:
```bash
# Verify seal integrity
aosctl adapter verify-seal model.sealed.aos --trusted-key trusted.pub

# Verify with hex key and JSON output
aosctl adapter verify-seal model.sealed.aos \
  --trusted-key-hex "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a" \
  --json
```

### `keygen create-sealing-key`

Generate Ed25519 keypair for adapter sealing

**Usage**:
```bash
aosctl keygen create-sealing-key [OPTIONS]
```

**Parameters**:
- `--name` (required): Key name identifier
- `--protected` (optional): Require passphrase for private key

**Examples**:
```bash
# Create unprotected keypair
aosctl keygen create-sealing-key --name production-sealing

# Create protected keypair
aosctl keygen create-sealing-key --name secure-sealing --protected
```

### `keygen export-public`

Export public key for distribution

**Usage**:
```bash
aosctl keygen export-public --key <KEY_NAME>
```

**Parameters**:
- `--key` (required): Key name to export

**Examples**:
```bash
# Export public key for sharing
aosctl keygen export-public --key production-sealing > production.pub
```

### `receipt verify`

Verify cryptographic receipt integrity

**Usage**:
```bash
aosctl receipt verify [OPTIONS]
```

**Parameters**:
- `--digest` (optional): Receipt digest to verify
- `--trace-id` (optional): Trace ID to lookup receipt
- `--input-tokens` (optional): Expected input tokens (comma-separated)
- `--allow-equipment-mismatch` (optional): Allow equipment profile mismatch
- `--json` (optional): Output in JSON format

**Examples**:
```bash
# Verify receipt by digest
aosctl receipt verify --digest b3abc123...

# Verify with input validation
aosctl receipt verify \
  --digest b3abc123... \
  --input-tokens 123,456,789

# Verify by trace ID
aosctl receipt verify --trace-id trace-456

# JSON output for automation
aosctl receipt verify --digest b3abc123... --json
```

### `receipt list`

List cryptographic receipts

**Usage**:
```bash
aosctl receipt list [OPTIONS]
```

**Parameters**:
- `--tenant` (optional): Filter by tenant ID
- `--since` (optional): Filter receipts since date (ISO 8601)
- `--limit` (optional): Maximum results (default: 50)
- `--json` (optional): Output in JSON format

**Examples**:
```bash
# List recent receipts
aosctl receipt list

# List receipts for specific tenant
aosctl receipt list --tenant production

# List receipts since date
aosctl receipt list --since 2026-01-01

# JSON output for processing
aosctl receipt list --tenant dev --json > receipts.json
```

### `receipt inspect`

Inspect receipt details and metadata

**Usage**:
```bash
aosctl receipt inspect <RECEIPT_DIGEST> [OPTIONS]
```

**Parameters**:
- `RECEIPT_DIGEST` (required): Receipt digest to inspect
- `--verbose` (optional): Show detailed information
- `--json` (optional): Output in JSON format

**Examples**:
```bash
# Basic receipt inspection
aosctl receipt inspect b3abc123...

# Detailed inspection with all fields
aosctl receipt inspect b3abc123... --verbose

# JSON output for analysis
aosctl receipt inspect b3abc123... --json > receipt_details.json
```

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

### `adapter update-lifecycle`

Update adapter lifecycle state

**Usage**:
```bash
aosctl adapter update-lifecycle <ADAPTER_ID> <STATE> [OPTIONS]
```

**Parameters**:
- `ADAPTER_ID` (required): Adapter ID
- `STATE` (required): New lifecycle state (`draft`, `training`, `ready`, `active`, `deprecated`, `retired`, `failed`)
- `--tenant` (optional): Tenant ID (defaults to 'default')

**Examples**:
```bash
# Deprecate an adapter
aosctl adapter update-lifecycle adapter-1 deprecated

# Activate an adapter for a specific tenant
aosctl adapter update-lifecycle adapter-1 active --tenant dev

# Retire an adapter
aosctl adapter update-lifecycle adapter-1 retired --json
```

### `adapter lifecycle-transition`

Transition adapter lifecycle state and record history/version updates

**Usage**:
```bash
aosctl adapter lifecycle-transition <ADAPTER_ID> <STATE> [OPTIONS]
```

**Parameters**:
- `ADAPTER_ID` (required): Adapter ID
- `STATE` (required): New lifecycle state (draft, training, ready, active, deprecated, retired, failed)
- `--reason` (optional): Reason for the transition (default: manual)
- `--initiated-by` (optional): Who initiated the transition (default: aosctl)

**Examples**:
```bash
# Move adapter to ready state
aosctl adapter lifecycle-transition adapter-1 ready

# Promote with a recorded reason
aosctl adapter lifecycle-transition adapter-1 active --reason "Promotion"

# Deprecate with an explicit initiator
aosctl adapter lifecycle-transition adapter-1 deprecated --initiated-by ci
```

### `adapter versions`

List adapter versions for a repository (control plane)

**Usage**:
```bash
aosctl adapter versions <REPO_ID> [OPTIONS]
```

**Parameters**:
- `REPO_ID` (required): Repository ID
- `--base-url` (optional): Control plane base URL (default: http://127.0.0.1:8080)
- `--json` (optional): Output JSON format

**Examples**:
```bash
aosctl adapter versions repo-123
aosctl adapter versions repo-123 --json
```

### `adapter promote-version`

Promote an adapter version (control plane)

**Usage**:
```bash
aosctl adapter promote-version <REPO_ID> <VERSION_ID> [OPTIONS]
```

**Parameters**:
- `REPO_ID` (required): Repository ID
- `VERSION_ID` (required): Version ID
- `--base-url` (optional): Control plane base URL (default: http://127.0.0.1:8080)
- `--json` (optional): Output JSON format

**Examples**:
```bash
aosctl adapter promote-version repo-123 ver-456
aosctl adapter promote-version repo-123 ver-456 --json
```

### `adapter rollback-version`

Roll back a repository branch to a previous version

**Usage**:
```bash
aosctl adapter rollback-version <REPO_ID> [OPTIONS]
```

**Parameters**:
- `REPO_ID` (required): Repository ID
- `--branch` (optional): Branch to roll back (default: main)
- `--version-id` (optional): Target version ID (required unless server chooses last good)
- `--base-url` (optional): Control plane base URL (default: http://127.0.0.1:8080)
- `--json` (optional): Output JSON format

**Examples**:
```bash
aosctl adapter rollback-version repo-123 --version-id ver-456 --branch main
aosctl adapter rollback-version repo-123 --branch main --json
```

### `adapter export`

Export adapter to a .aos file (PRD-ART-01)

**Usage**:
```bash
aosctl adapter export <ADAPTER_ID> [OPTIONS]
```

**Parameters**:
- `ADAPTER_ID` (required): Adapter ID to export
- `--out`, `-o` (optional): Output file path (default: ./{adapter_id}.aos)
- `--base-url` (optional): Control plane base URL (default: http://127.0.0.1:8080/api)

**Examples**:
```bash
aosctl adapter export adapter-1
aosctl adapter export adapter-1 -o ./exported.aos
aosctl adapter export adapter-1 --out path/to/file.aos
```

### `adapter import`

Import adapter from a .aos file (PRD-ART-01)

**Usage**:
```bash
aosctl adapter import <PATH> --tenant <TENANT> [OPTIONS]
```

**Parameters**:
- `PATH` (required): Path to .aos file
- `--tenant` (required): Tenant ID
- `--auto-load` (optional): Auto-load adapter after import
- `--base-url` (optional): Control plane base URL (default: http://127.0.0.1:8080/api)

**Examples**:
```bash
aosctl adapter import ./my-adapter.aos --tenant dev
aosctl adapter import ./adapter.aos --tenant dev --auto-load
```

### `adapter swap`

Hot-swap adapters in running worker

**Usage**:
```bash
aosctl adapter swap <ADAPTER_ID> [OPTIONS]
```

**Parameters**:
- `ADAPTER_ID` (required): Adapter ID to activate on the worker
- `--server-url` (optional): Control plane base URL (default: http://127.0.0.1:8080)
- `--timeout` (optional): Timeout in seconds to wait for readiness (default: 30)

**Examples**:
```bash
aosctl adapter swap adapter-1
aosctl adapter swap adapter-1 --server-url http://localhost:8080
aosctl adapter swap adapter-1 --timeout 60
```

### `adapter inspect`

Inspect an .aos archive (header, segments, manifest metadata)

**Usage**:
```bash
aosctl adapter inspect <PATH>
```

**Parameters**:
- `PATH` (required): Path to .aos file

**Examples**:
```bash
aosctl adapter inspect ./my-adapter.aos
```

### `adapter lineage`

Show adapter lineage tree (ancestors and descendants)

**Usage**:
```bash
aosctl adapter lineage <ADAPTER_ID> [OPTIONS]
```

**Parameters**:
- `ADAPTER_ID` (required): Adapter ID
- `--json` (optional): Output JSON format
- `--tree` (optional): Display as tree (ASCII art)

**Examples**:
```bash
aosctl adapter lineage adapter-1
aosctl adapter lineage adapter-1 --json
aosctl adapter lineage adapter-1 --tree
```

### `adapter evict`

Evict adapter from memory

**Usage**:
```bash
aosctl adapter evict <ADAPTER_ID> [OPTIONS]
```

**Parameters**:
- `ADAPTER_ID` (required): Adapter ID
- `--tenant` (optional): Tenant ID
- `--reason` (optional): Reason for eviction (for audit trail)

**Examples**:
```bash
aosctl adapter evict adapter-1
aosctl adapter evict adapter-1 --tenant dev --reason "Low activation"
```

### `adapter codebase ingest`

Ingest a codebase repository and train an adapter from its contents. This command extracts knowledge from code repositories for specialized code intelligence adapters.

**Usage**:
```bash
aosctl adapter codebase ingest --repo <PATH_OR_URL> [OPTIONS]
```

**Parameters**:
- `--repo` (required): Repository path or git URL
- `--repo-slug` (optional): Repository slug for adapter naming and provenance (auto-derived if not provided)
- `--override-repo-slug` (optional): Override repository slug in scope metadata (auto-derived if not provided)
- `--adapter-id` (optional): Adapter ID override (defaults to code.\<repo_slug\>.\<commit\>)
- `--project-name` (optional): Logical project name for metadata
- `--repo-id` (optional): Registry repo identifier override
- `--branch` (optional): Git branch to use (defaults to current branch)
- `--commit` (optional): Git commit SHA to use (defaults to current commit)
- `--output-dir` (optional): Output directory for .aos artifacts (default: ./adapters)
- `--base-model` (optional): Base model name for metadata (default: qwen2.5-7b)
- `--max-symbols` (optional): Maximum symbols to sample per repo (default: 64)
- `--include-private` (optional): Include private symbols in dataset
- `--positive-weight` (optional): Positive sample weight (default: 1.0)
- `--abstention-weight` (optional): Weight for abstention samples (default: 0.5). Uses `sample_role: "abstention"` metadata.
- `--skip-register` (optional): Skip registry registration
- `--tier` (optional): Registry tier (default: 1)
- `--seed` (optional): Deterministic seed override
- `--fixed-timestamp` (optional): Fixed timestamp for reproducible builds (ISO 8601)
- `--stable-ordering` (optional): Enforce stable ordering for deterministic hashing/sorting
- `--strict-determinism` (optional): Enable strict determinism checks
- `--trace-seeds` (optional): Trace seed derivations for debugging

**Scope Configuration**:
- `--include-paths` (optional): Paths to include (comma-separated, e.g., "src/,lib/")
- `--exclude-paths` (optional): Paths to exclude (comma-separated, e.g., "tests/,vendor/")
- `--include-extensions` (optional): File extensions to include (comma-separated, e.g., "rs,py,ts")
- `--exclude-extensions` (optional): File extensions to exclude (comma-separated, e.g., "md,txt,json")
- `--scan-root` (optional): Scan root paths within the repository (can be specified multiple times)
- `--remote-url` (optional): Remote URL override for provenance tracking

**Scope Override Options** (metadata-only overrides):
- `--repo-name` (optional): Override repository name
- `--override-repo-slug` (optional): Override repository slug for naming/provenance
- `--override-branch` (optional): Override branch name
- `--override-commit` (optional): Override commit SHA
- `--override-scan-root` (optional): Override primary scan root path for scope metadata
- `--override-remote-url` (optional): Override remote URL

**Streaming Configuration**:
- `--stream` (optional): Enable streaming progress output
- `--stream-format` (optional): Stream output format: json or text (default: text)
- `--stream-interval` (optional): Minimum interval between stream events in milliseconds

**Session Configuration**:
- `--session-id` (optional): Session ID for correlating ingestion workflows (auto-generated when `--session-name`, `--session-tags`, or `--stream` is set)
- `--session-name` (optional): Human-readable session name
- `--session-tags` (optional): Session tags for categorization (comma-separated)

**Examples**:
```bash
# Ingest a local repository
aosctl adapter codebase ingest --repo /path/to/repo

# Ingest specific directories with custom slug
aosctl adapter codebase ingest --repo /path/to/repo \
    --repo-slug my_project \
    --scan-root src --scan-root lib

# Ingest with path and extension filters
aosctl adapter codebase ingest --repo /path/to/repo \
    --include-paths src/,lib/ \
    --exclude-paths tests/,vendor/ \
    --include-extensions rs,py,ts

# Ingest with streaming progress
aosctl adapter codebase ingest --repo /path/to/repo \
    --stream --stream-format json

# Ingest from a specific branch
aosctl adapter codebase ingest --repo /path/to/repo \
    --branch feature/new-api

# Deterministic ingestion for CI/CD
aosctl adapter codebase ingest --repo /path/to/repo \
    --seed 42 \
    --fixed-timestamp "2025-01-15T10:00:00Z" \
    --stable-ordering --strict-determinism --trace-seeds

# Ingest with session metadata
aosctl adapter codebase ingest --repo /path/to/repo \
    --session-name nightly \
    --session-tags ci,nightly
```

### `adapter train-from-code`

Train an adapter directly from a repository. Combines code ingestion and training into a single command.

**Usage**:
```bash
aosctl adapter train-from-code --repo <PATH_OR_URL> [OPTIONS]
```

**Parameters**:
- `--repo` (required): Repository path or git URL
- `--adapter-id` (optional): Adapter ID (defaults to code.\<repo\>.\<commit\>)
- `--project-name` (optional): Logical project name for metadata
- `--repo-id` (optional): Registry repo identifier override
- `--output-dir` (optional): Output directory for .aos artifacts (default: ./adapters)
- `--base-model` (optional): Base model name for metadata (default: qwen2.5-7b)
- `--max-symbols` (optional): Maximum symbols to sample per repo (default: 64)
- `--include-private` (optional): Include private symbols in dataset
- `--positive-weight` (optional): Positive sample weight (default: 1.0)
- `--abstention-weight` (optional): Weight for abstention samples (default: 0.5). Uses `sample_role: "abstention"` metadata.
- `--skip-register` (optional): Skip registry registration
- `--tier` (optional): Registry tier (default: 1)
- `--seed` (optional): Deterministic seed override

**Scope Override Options** (see [Scope Override Options](#scope-override-options)):
- `--repo-name` (optional): Override repository name
- `--override-repo-slug` (optional): Override repository slug for naming/provenance
- `--override-branch` (optional): Override branch name
- `--override-commit` (optional): Override commit SHA
- `--override-scan-root` (optional): Override scan root path
- `--override-remote-url` (optional): Override remote URL

**Examples**:
```bash
# Train adapter from local repository
aosctl adapter train-from-code --repo /path/to/repo

# Train with custom adapter ID
aosctl adapter train-from-code --repo /path/to/repo \
    --adapter-id my_custom_adapter

# Train with scope overrides for CI/CD
aosctl adapter train-from-code --repo /path/to/repo \
    --override-branch main \
    --override-commit abc123

# Train including private symbols
aosctl adapter train-from-code --repo /path/to/repo \
    --include-private --max-symbols 128
```

### `adapter verify-gpu`

Verify GPU buffer integrity for loaded adapters

**Usage**:
```bash
aosctl adapter verify-gpu [OPTIONS]
```

**Parameters**:
- `--tenant` (optional): Tenant ID
- `--adapter` (optional): Specific adapter ID to verify (verifies all if omitted)
- `--socket` (optional): UDS socket path (default: /var/run/aos/aos.sock)
- `--timeout` (optional): Timeout in milliseconds (default: 10000)

**Examples**:
```bash
aosctl adapter verify-gpu
aosctl adapter verify-gpu --tenant dev
aosctl adapter verify-gpu --adapter adapter-1 --tenant dev
```

### `adapter list-pinned`

List pinned adapters for a tenant

**Usage**:
```bash
aosctl adapter list-pinned --tenant <TENANT>
```

**Parameters**:
- `--tenant`, `-t` (required): Tenant ID

**Examples**:
```bash
aosctl adapter list-pinned --tenant dev
aosctl adapter list-pinned --tenant dev --json
```

---

## Dataset Management

### `dataset create`

Create a dataset identity from documents/collections (no local files)

**Usage**:
```bash
aosctl dataset create [OPTIONS]
```

**Parameters**:
- `--name` (optional): Dataset name
- `--dataset-type` (optional): Dataset type (freeform, e.g. training, evaluation)
- `--purpose` (optional): Purpose (e.g. chat-finetune, safety-eval)
- `--source-location` (optional): Source location hint (e.g. bucket path or URL)
- `--tags` (optional): Comma-separated tags
- `--document-id` (optional): Single document ID
- `--document-ids` (optional): Multiple document IDs
- `--collection-id` (optional): Collection ID
- `--description` (optional): Optional description

**Examples**:
```bash
# Create from a single document
aosctl dataset create --document-id doc-123 --name reviews

# Create from multiple documents
aosctl dataset create --document-ids doc-1 doc-2 --name combined

# Create from a collection
aosctl dataset create --collection-id coll-9 --name coll_ds
```

### `dataset ingest`

Ingest local files into a new dataset version

**Usage**:
```bash
aosctl dataset ingest <FILES...> [OPTIONS]
```

**Parameters**:
- `FILES` (required): Files to upload (JSONL/CSV/TXT)
- `--dataset-id` (optional): Existing dataset ID (omit to create new)
- `--format` (optional): Format hint (jsonl, csv, txt, patches)
- `--name` (optional): Dataset name when creating
- `--description` (optional): Description

**Examples**:
```bash
# Ingest a single file
aosctl dataset ingest ./data/train.jsonl

# Ingest multiple files with format hint
aosctl dataset ingest ./data/*.jsonl --format jsonl

# Ingest into an existing dataset
aosctl dataset ingest ./data/new.jsonl --dataset-id ds-123

# JSON output
aosctl dataset ingest ./data/train.jsonl --json
```

### Dataset Root Configuration

Dataset storage roots are resolved in this order: `AOS_DATASETS_DIR`, config `paths.datasets_root`, then the default `var/datasets`. The resolved path is canonicalized and validated (must not be under `/tmp` or system directories), so prefer an absolute, persistent path.

### `dataset list`

List datasets with validation/trainability state

**Usage**:
```bash
aosctl dataset list [OPTIONS]
```

**Parameters**:
- `--trust-state` (optional): Filter by trust_state (allowed, allowed_with_warning, blocked, needs_approval)
- `--name` (optional): Filter by dataset name substring (case-insensitive)
- `--limit` (optional): Limit results (default: 50)

**Examples**:
```bash
aosctl dataset list
aosctl dataset list --trust-state allowed
aosctl dataset list --name "reviews" --json
```

### `dataset versions`

List dataset versions for a dataset

**Usage**:
```bash
aosctl dataset versions <DATASET_ID>
```

**Parameters**:
- `DATASET_ID` (required): Dataset ID

**Examples**:
```bash
aosctl dataset versions ds-123
aosctl dataset versions ds-123 --json
```

### `dataset show`

Show manifest/validation/trust for a dataset version

**Usage**:
```bash
aosctl dataset show <DATASET_VERSION_ID>
```

**Parameters**:
- `DATASET_VERSION_ID` (required): Dataset version ID

**Examples**:
```bash
aosctl dataset show dsv-abc123
aosctl dataset show dsv-abc123 --json
```

### `dataset validate`

Trigger validation for a dataset (creates/uses latest version)

**Usage**:
```bash
aosctl dataset validate [OPTIONS]
```

**Parameters**:
- `--dataset-id` (optional): Dataset ID (validated via API)
- `--dataset-version-id` (optional): Dataset version ID (resolved to dataset ID)

**Examples**:
```bash
aosctl dataset validate --dataset-id ds-123
aosctl dataset validate --dataset-version-id dsv-abc123
```

---

## Database Management

### `db migrate`

Run database migrations

**Usage**:
```bash
aosctl db migrate [OPTIONS]
```

**Parameters**:
- `--db-path` (optional): Database path (defaults to DATABASE_URL or ./var/aos-cp.sqlite3)
- `--verify-only` (optional): Verify signatures only (don't run migrations)

**Examples**:
```bash
# Run migrations on default database
aosctl db migrate

# Run migrations on custom database
aosctl db migrate --db-path ./var/custom.db

# Verify signatures only (don't run migrations)
aosctl db migrate --verify-only
```

### `db unlock`

Clear a stuck migration lock and reset WAL/shm files

**Usage**:
```bash
aosctl db unlock [OPTIONS]
```

**Parameters**:
- `--db-path` (optional): Database path (defaults to DATABASE_URL or ./var/aos-cp.sqlite3)

**Examples**:
```bash
# Unlock default database
aosctl db unlock

# Unlock custom database
aosctl db unlock --db-path ./var/custom.db
```

### `db reset`

Reset database (DEVELOPMENT ONLY - destroys all data)

**Usage**:
```bash
aosctl db reset [OPTIONS]
```

**Parameters**:
- `--db-path` (optional): Database path (defaults to DATABASE_URL or ./var/aos-cp.sqlite3)
- `--force` (optional): Skip confirmation prompt

**Examples**:
```bash
# Reset default database
aosctl db reset

# Reset custom database
aosctl db reset --db-path ./var/custom.db

# Skip confirmation prompt (dangerous!)
aosctl db reset --force
```

**Warning**: This command DELETES the database file and recreates it with all migrations. All data will be PERMANENTLY LOST. Only use in development environments.

### `db seed-fixtures`

Reset and seed deterministic test fixtures (development only)

**Usage**:
```bash
aosctl db seed-fixtures [OPTIONS]
```

**Parameters**:
- `--db-path` (optional): Database path (defaults to DATABASE_URL or ./var/aos-cp.sqlite3)
- `--skip-reset` (optional): Skip removing the database file before seeding
- `--chat` (optional): Include a starter chat session + single message (default: true)

**Examples**:
```bash
# Reset DB and seed deterministic fixtures
aosctl db seed-fixtures

# Seed without dropping existing DB
aosctl db seed-fixtures --skip-reset

# Seed without chat history
aosctl db seed-fixtures --skip-reset --no-chat
```

### `db health`

Health check for migration signatures and DB integrity

**Usage**:
```bash
aosctl db health [OPTIONS]
```

**Parameters**:
- `--db-path` (optional): Database path (defaults to DATABASE_URL or ./var/aos-cp.sqlite3)
- `--json` (optional): Emit JSON instead of human-readable output

### `db verify-seed`

Verify seeded demo fixtures exist (development only)

**Usage**:
```bash
aosctl db verify-seed [OPTIONS]
```

**Parameters**:
- `--db-path` (optional): Database path (defaults to DATABASE_URL or ./var/aos-cp.sqlite3)
- `--tenant-id` (optional): Tenant to verify (defaults to tenant-test)

**Examples**:
```bash
# Verify default demo seed (tenant-test)
aosctl db verify-seed

# Verify custom database
aosctl db verify-seed --db-path ./var/custom.db

# Verify a different tenant id
aosctl db verify-seed --tenant-id tenant-test
```

### `db repair-bootstrap`

Validate and repair system bootstrap state

**Usage**:
```bash
aosctl db repair-bootstrap [OPTIONS]
```

**Parameters**:
- `--db-path` (optional): Database path (defaults to DATABASE_URL or ./var/aos-cp.sqlite3)
- `--dry-run` (optional): Check only, don't repair
- `--json` (optional): Output JSON instead of human-readable

**Examples**:
```bash
# Check bootstrap state (dry-run)
aosctl db repair-bootstrap --dry-run

# Repair bootstrap state if needed
aosctl db repair-bootstrap

# Check/repair custom database
aosctl db repair-bootstrap --db-path ./var/custom.db
```

This command validates that the system tenant and core policies are properly seeded. Issues detected include:
- Missing system tenant
- Missing core policies (egress, determinism, isolation, evidence)
- KV/SQL inconsistency for system tenant

---

## Storage Management

### `storage mode`

Show current storage mode

**Usage**:
```bash
aosctl storage mode [OPTIONS]
```

**Parameters**:
- `--db-path` (optional): Database path (defaults to DATABASE_URL or ./var/aos-cp.sqlite3)

**Examples**:
```bash
# Show current storage mode
aosctl storage mode

# Show mode with JSON output
aosctl storage mode --json
```

### `storage set-mode`

Set storage mode

Changes the storage backend mode. Valid modes:
- `sql_only`: SQL backend only (default)
- `dual_write`: Write to both SQL and KV, read from SQL (validation phase)
- `kv_primary`: Write to both SQL and KV, read from KV (cutover phase)
- `kv_only`: KV backend only (full migration complete)

**Usage**:
```bash
aosctl storage set-mode <MODE> [OPTIONS]
```

**Parameters**:
- `MODE` (required): Storage mode (sql_only, dual_write, kv_primary, kv_only)
- `--db-path` (optional): Database path (defaults to DATABASE_URL or ./var/aos-cp.sqlite3)
- `--kv-path` (optional): KV database path (default: ./var/aos-kv.redb)
- `--init-kv` (optional): Initialize KV backend if not exists

**Examples**:
```bash
# Enable dual-write mode for validation
aosctl storage set-mode dual_write

# Switch to KV-primary mode for cutover
aosctl storage set-mode kv_primary

# Complete migration to KV-only mode
aosctl storage set-mode kv_only

# Revert to SQL-only mode
aosctl storage set-mode sql_only

# Set mode with custom database path
aosctl storage set-mode dual_write --db-path ./var/custom.db --kv-path ./var/custom.redb
```

### `storage migrate`

Migrate data from SQL to KV backend

**Usage**:
```bash
aosctl storage migrate [OPTIONS]
```

**Parameters**:
- `--db-path` (optional): Database path (defaults to DATABASE_URL or ./var/aos-cp.sqlite3)
- `--kv-path` (optional): KV database path (default: ./var/aos-kv.redb)
- `--dry-run` (optional): Show what would be migrated without making changes
- `--verify` (optional): Verify consistency after migration
- `--force` (optional): Force migration even if KV backend already has data
- `--tenant` (optional): Migrate a single tenant only
- `--batch-size` (optional): Batch size for migrations (default: 100)
- `--resume` (optional): Resume from checkpoint (requires --checkpoint-path)
- `--checkpoint-path` (optional): Path to checkpoint file (default: ./var/aos-migrate.checkpoint.json)
- `--domains` (optional): Comma-separated domains (adapters, tenants, stacks, plans, auth_sessions, runtime_sessions, rag_artifacts)

**Examples**:
```bash
# Migrate all data from SQL to KV
aosctl storage migrate

# Migrate with custom paths
aosctl storage migrate --db-path ./var/aos-cp.sqlite3 --kv-path ./var/aos-kv.redb

# Dry run to preview migration
aosctl storage migrate --dry-run

# Migrate with verification
aosctl storage migrate --verify
```

### `storage verify`

Verify consistency between SQL and KV backends

**Usage**:
```bash
aosctl storage verify [OPTIONS]
```

**Parameters**:
- `--db-path` (optional): Database path (defaults to DATABASE_URL or ./var/aos-cp.sqlite3)
- `--kv-path` (optional): KV database path (default: ./var/aos-kv.redb)
- `--adapters-only` (optional): Verify adapters only
- `--tenants-only` (optional): Verify tenants only
- `--stacks-only` (optional): Verify stacks only
- `--repair` (optional): Repair detected drift by re-migrating domains SQL -> KV
- `--domains` (optional): Comma-separated domains to verify/repair (default: all supported)
- `--fail-on-drift` (optional): Exit with non-zero if drift is detected

**Examples**:
```bash
# Verify consistency between backends
aosctl storage verify

# Verify with custom paths
aosctl storage verify --db-path ./var/aos-cp.sqlite3 --kv-path ./var/aos-kv.redb

# Verify specific entities
aosctl storage verify --adapters-only
aosctl storage verify --tenants-only
aosctl storage verify --stacks-only
```

### `storage validate-consistency`

Validate and optionally repair consistency for a tenant

**Usage**:
```bash
aosctl storage validate-consistency --tenant <TENANT> [OPTIONS]
```

**Parameters**:
- `--tenant` (required): Tenant ID to validate
- `--db-path` (optional): Database path (defaults to DATABASE_URL or ./var/aos-cp.sqlite3)
- `--kv-path` (optional): KV database path (default: ./var/aos-kv.redb)
- `--repair` (optional): Repair drift by syncing SQL -> KV

**Examples**:
```bash
aosctl storage validate-consistency --tenant default --repair
aosctl storage validate-consistency --tenant default --db-path ./var/aos-cp.sqlite3 --kv-path ./var/aos-kv.redb
```

### `storage kv-isolation-scan`

Trigger a KV isolation scan via the control plane API

**Usage**:
```bash
aosctl storage kv-isolation-scan [OPTIONS]
```

**Parameters**:
- `--server-url` (optional): Control plane base URL (default: http://localhost:8080)
- `--token` (optional): Bearer token (overrides stored login)
- `--sample-rate` (optional): Override sample rate (0.0 - 1.0)
- `--max-findings` (optional): Override maximum findings
- `--hash-seed` (optional): Override hash seed for sampling
- `--timeout` (optional): HTTP timeout in seconds (default: 15)

### `storage kv-isolation-health`

Fetch last KV isolation scan health via the control plane API

**Usage**:
```bash
aosctl storage kv-isolation-health [OPTIONS]
```

**Parameters**:
- `--server-url` (optional): Control plane base URL (default: http://localhost:8080)
- `--token` (optional): Bearer token (overrides stored login)
- `--timeout` (optional): HTTP timeout in seconds (default: 10)

### `storage kv status`

Show KV readiness and cutover checklist

**Usage**:
```bash
aosctl storage kv status [OPTIONS]
```

**Parameters**:
- `--db-path` (optional): Database path
- `--kv-path` (optional): KV database path (default: ./var/aos-kv.redb)
- `--tenant` (optional): Tenant to include in checksum evidence

### `storage kv cutover`

Attempt cutover to KV (kv_primary or kv_only) with gating

**Usage**:
```bash
aosctl storage kv cutover [OPTIONS]
```

**Parameters**:
- `--to` (optional): Target mode (kv_primary or kv_only, default: kv_only)
- `--db-path` (optional): Database path
- `--kv-path` (optional): KV database path (default: ./var/aos-kv.redb)

### `storage kv rollback`

Roll back to dual_write mode

**Usage**:
```bash
aosctl storage kv rollback [OPTIONS]
```

**Parameters**:
- `--db-path` (optional): Database path
- `--kv-path` (optional): KV database path (default: ./var/aos-kv.redb)

---

## Preflight Checks

### `preflight`

Pre-flight system readiness checker for adapterOS

Provides comprehensive environment verification before launching the server:
- Model availability and configuration
- Database initialization and migrations
- Required directories and files
- Environment variables
- Backend availability (CoreML, Metal, MLX)
- System resources

**Usage**:
```bash
aosctl preflight [OPTIONS]
```

**Parameters**:
- `--fix`, `-f` (optional): Fix issues automatically where possible (interactive mode)
- `--fix-force` (optional): Fix all issues without confirmation (dangerous - use with caution)
- `--safe-only` (optional): Only apply safe fixes (no user confirmation required)
- `--database-url` (optional): Database path to check (defaults to AOS_DATABASE_URL env var or var/aos-cp.sqlite3)
- `--model-path` (optional): Model path to check (overrides AOS_MODEL_CACHE_DIR/AOS_BASE_MODEL_ID resolver)
- `--skip-backends` (optional): Skip backend availability checks
- `--skip-resources` (optional): Skip resource checks (memory, disk)

**Examples**:
```bash
# Run preflight checks
aosctl preflight

# Run preflight with auto-fix in interactive mode
aosctl preflight --fix

# Auto-fix all issues without confirmation (use with caution)
aosctl preflight --fix --fix-force

# Only apply safe fixes
aosctl preflight --fix --safe-only

# Skip backend and resource checks
aosctl preflight --skip-backends --skip-resources
```

**Check Categories**:

1. **Model Availability**: Verifies base model exists and is properly configured
2. **Database**: Checks database initialization, migrations, and bootstrap state
3. **Directories**: Validates required directories exist
4. **Environment Variables**: Checks required environment variables are set
5. **Backends**: Tests CoreML, Metal, and MLX backend availability
6. **System Resources**: Validates memory and disk availability

**Fix Modes**:

- **Interactive** (`--fix`): Prompts before applying each fix
- **Force** (`--fix --fix-force`): Applies all fixes without confirmation
- **Safe Only** (`--fix --safe-only`): Only applies fixes that don't require confirmation

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

### `chat`

Interactive chat and prompt inference commands.

**Usage**:
```bash
aosctl chat <SUBCOMMAND>
```

**Subcommands**:
- `interactive` - Start an interactive chat session
- `prompt` - Run a single prompt inference

### Local Chat Mode (No Server Required)

The `chat` command supports a local mode that runs inference directly without requiring a running server. This is useful for quick testing and development.

**Prerequisites**:
- CLI built with `multi-backend` feature: `cargo build --release -p adapteros-cli --features multi-backend`
- A model directory with `tokenizer.json` (e.g., `./var/models/Qwen2.5-7B-Instruct`)

**Commands**:

```bash
# Interactive local chat
aosctl chat interactive --local --model-path ./var/models/Qwen2.5-7B-Instruct

# Single prompt local inference
aosctl chat prompt --text "Hello" --local --model-path ./var/models/Qwen2.5-7B-Instruct

# With custom parameters
aosctl chat interactive --local \
  --model-path ./var/models/Qwen2.5-7B-Instruct \
  --temperature 0.8 \
  --max-tokens 1024
```

**Flags**:

| Flag | Description | Default |
|------|-------------|---------|
| `--local` | Enable local mode (no server) | false |
| `--model-path <PATH>` | Path to model directory | Required when `--local` |

**Environment Variables**:

| Variable | Description |
|----------|-------------|
| `AOS_MODEL_PATH` | Default model path for local mode |

**Limitations**:
- Local mode does not support adapter stacks (LoRA routing)
- No session persistence or history
- Requires `multi-backend` feature flag

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

Bootstrap adapterOS installation

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

Run diagnostics and manage diagnostic bundles

**Usage**:
```bash
aosctl diag run [OPTIONS]
aosctl diag export --trace-id <TRACE_ID> -o <PATH> [OPTIONS]
aosctl diag verify <BUNDLE> [OPTIONS]
```

**Run Parameters**:
- `--profile` (optional): Diagnostic profile: system, tenant, or full (default: full)
- `--tenant` (optional): Tenant ID for tenant-specific checks
- `--json` (optional): Output JSON format
- `--bundle` (optional): Create a local diagnostic bundle at the given path
- `--system` (optional): System checks only
- `--tenant-only` (optional): Tenant checks only
- `--full` (optional): Full diagnostics (default)

**Export Parameters**:
- `--trace-id` (required): Trace ID to export
- `-o, --output` (required): Output file path
- `--format` (optional): Bundle format: `tar.zst` or `zip` (default: `tar.zst`)
- `--include-evidence` (optional): Include evidence payloads (requires token)
- `--evidence-token` (optional): Evidence authorization token
- `--base-url` (optional): API base URL (default: `http://127.0.0.1:8080`)

**Verify Parameters**:
- `--verbose` (optional): Verbose output

**Examples**:
```bash
# Run local diagnostics
aosctl diag run --full

# Export signed bundle from server
aosctl diag export --trace-id trace-abc123 -o bundle.tar.zst

# Verify bundle offline
aosctl diag verify bundle.tar.zst --verbose
```

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

## Codebase Adapter Scope

Adapters can be scoped to specific codebases to enable targeted code intelligence. When training adapters from code repositories, scope metadata is automatically captured and embedded in the adapter manifest, including a stable `codebase_scope` identifier used for uniqueness and dataset scoping.

### Scope Fields

Codebase scope is defined by the following fields in the adapter manifest and manifest metadata:

| Field | Description | Example |
|-------|-------------|---------|
| `scope_repo` | Repository name or identifier | `my-org/my-repo` |
| `scope_branch` | Branch name at training time | `main`, `feature/xyz` |
| `scope_commit` | Commit SHA at training time | `a1b2c3d4...` |
| `scope_scan_root` | Primary scan root path used during ingestion | `src/api` |
| `scope_remote_url` | Remote URL of the repository | `https://github.com/org/repo` |
| `repo_slug` | Normalized repository slug used for adapter naming | `my_repo` |
| `scope_repo_id` | Normalized repository identifier used for scope (includes scan root for subdir scans) | `github.com/org/repo` |
| `codebase_scope` | Stable repository identifier (normalized repo ID; mirrors `scope_repo_id`) | `github.com/org/repo` |

Additional provenance keys are recorded in adapter metadata:
- `codebase_scope`: Stable repository identifier derived from the repo ID/slug
- `repo_commit`: Commit SHA captured at ingestion time (raw git SHA)
- `repo_root_path`: Absolute repository root path (always the repo root)
- `repo_path`: Repository path used for ingestion (may match scan root)
- `scan_root_path`: Absolute primary scan root path
- `scan_root_relative`: Repo-relative scan root (empty when scanning the repo root)
- `scan_roots`: Ordered list of scan root entries captured during ingestion

`scope_scan_root` uses the repo-relative path when available.
For codebase adapters, the manifest `scope` field mirrors `adapter_scope_id` and
defaults to `scope_repo_id` (normalized repo identifier, with scan-root suffix
when scanning subdirectories). `repo_slug` is still used for adapter naming.
When `--stable-ordering` or `--strict-determinism` is enabled, `repo_root_path`,
`scan_root_path`, and `scan_root_relative` are normalized to repo-relative values
(`.` when scanning the repo root) to keep metadata deterministic across machines.

### Viewing Scope Information

Use `adapter inspect` to view scope metadata in a packaged `.aos` file:

```bash
# Inspect adapter scope and metadata
aosctl adapter inspect ./my-adapter.aos
```

The output includes scope information showing:
```
Scope Information
  Repository: my-org/my-repo
  Branch: main
  Commit: a1b2c3d4e5f6...
  Scan Root: src/api
  Remote URL: https://github.com/my-org/my-repo
```

### Scope in Training

When training adapters from code, scope is automatically derived from:

1. The repository being ingested
2. The scan roots specified during ingestion
3. The semantic naming taxonomy (domain/group/scope/operation)

Example workflow:
```bash
# Initialize code repository for training
aosctl code-init --path /path/to/repo --tenant dev

# Train adapter (scope derived from repository)
aosctl train --dataset-id dataset_123 --output adapters/my-adapter.aos

# Verify scope metadata
aosctl adapter inspect adapters/my-adapter.aos
```

### Scan Roots

For complex projects (monorepos, multi-module projects), multiple scan roots may be recorded. Each scan root captures:

- **path**: Absolute or relative path to the scan root directory
- **label**: Optional label describing the root's role (e.g., "main", "lib", "tests")
- **file_count**: Number of files processed from this scan root
- **byte_count**: Total bytes ingested from this scan root
- **content_hash**: BLAKE3 hash of the scan root's content at ingestion time
- **scanned_at**: Timestamp when this scan root was processed

Scan roots are persisted with dataset metadata and embedded in adapter manifests. `--override-scan-root` updates the primary scan root metadata only (it does not change the actual scan inputs).

Example manifest with multiple scan roots:
```json
{
  "scan_roots": [
    {"path": "src/api", "label": "main", "file_count": 150},
    {"path": "libs/core", "label": "lib", "file_count": 80},
    {"path": "tests", "label": "tests", "file_count": 45}
  ]
}
```

### Scope Path Derivation

The `scope_path` field provides a derived hierarchical path following the pattern:
`domain/group/scope/operation`

This path is used for:
- Organizing adapters in the registry
- Filtering adapters by domain or scope
- Semantic routing decisions

### Scope Override Options

When training adapters from codebases, you can override auto-detected git metadata for deterministic training. These options are particularly useful in CI/CD pipelines where the git state may not reflect the intended training context. The options apply to both `adapter codebase ingest` and `adapter train-from-code`.

| CLI Flag | Description | Auto-detected From |
|----------|-------------|-------------------|
| `--repo-name` | Repository name for provenance | Directory name |
| `--override-repo-slug` | Repository slug for naming/provenance | Repo name (normalized) |
| `--override-branch` | Branch name for scope metadata | `git rev-parse --abbrev-ref HEAD` |
| `--override-commit` | Commit SHA for versioning | `git rev-parse HEAD` |
| `--override-scan-root` | Primary scan root path (metadata only) | Git repository root |
| `--override-remote-url` | Remote URL for provenance | `git remote get-url origin` |

Note: `--repo-slug` affects adapter naming and the fallback scope id when no repo-id/remote-url is available. `--override-repo-slug` only updates scope metadata (it does not change the adapter ID unless `--repo-slug` is also set).
Note: `--branch` and `--commit` populate scope metadata when `--override-branch` and `--override-commit` are not provided.

**Examples**:
```bash
# Override branch for CI builds (e.g., when in detached HEAD state)
aosctl adapter train-from-code --repo /path/to/repo \
    --override-branch main

# Override commit for reproducible builds
aosctl adapter train-from-code --repo /path/to/repo \
    --override-commit a1b2c3d4e5f6

# Full override for air-gapped environments
aosctl adapter train-from-code --repo /path/to/repo \
    --repo-name my_org_repo \
    --override-branch release/v2.0 \
    --override-commit abc123def456 \
    --override-repo-slug my_org_repo \
    --override-remote-url https://github.com/my-org/my-repo
```

**Use Cases**:
- **CI/CD Pipelines**: Override branch when running in detached HEAD state
- **Reproducible Builds**: Pin to specific commit SHA for deterministic training
- **Air-gapped Environments**: Provide remote URL when git remote is not accessible
- **Multi-repo Projects**: Override repo name for consistent adapter naming across clones

---

## Alias Change Gating

Adapter aliases (semantic names) are gated based on lifecycle state to prevent accidental changes to production adapters.

### Lifecycle State Rules

| Lifecycle State | Alias Updates | Behavior |
|-----------------|---------------|----------|
| `draft` | Allowed | Mutable state - aliases can be freely changed |
| `training` | Allowed | Mutable state - aliases can be changed during training |
| `ready` | Blocked by default | Transitional state - requires explicit allow-ready override |
| `active` | Blocked | Production state - alias changes are rejected |
| `deprecated` | Blocked | Production state - alias changes are rejected |
| `retired` | Blocked | Terminal state - no modifications allowed |
| `failed` | Blocked | Terminal state - no modifications allowed |

### Gating Behavior

When attempting to change an alias for an adapter in a protected state, the CLI will:

1. **Draft/Training**: Apply the change immediately
2. **Ready**: Block by default unless an explicit allow-ready override is enabled
3. **Active/Deprecated/Retired/Failed**: Reject the change with an error message

When `ready` is explicitly allowed, the CLI runs alias swap preflight checks to ensure the target adapter is deployable before applying the change.

**Example Error Messages**:
```
Error: Cannot update alias for adapter 'my-adapter' in 'active' state.
Adapters in production states (active, deprecated) are immutable.
To change the alias, first transition the adapter to 'draft' state.
```

**Rationale**:
- Prevents accidental breakage of production routing
- Ensures audit trail integrity
- Maintains semantic name stability for active adapters
- Allows flexibility during development and testing phases

---

## Command Aliases

Several commands have shorter aliases for convenience. Aliases are fully equivalent to their canonical forms.

### Available Aliases

| Canonical Command | Alias | Description |
|-------------------|-------|-------------|
| `aosctl adapter` | `aosctl adapters` | Adapter management commands |
| `aosctl stack` | `aosctl stacks` | Adapter stack management |
| `aosctl node` | `aosctl nodes` | Node management commands |

### Usage Examples

```bash
# These are equivalent:
aosctl adapter list
aosctl adapters list

# These are equivalent:
aosctl stack create my-stack
aosctl stacks create my-stack

# These are equivalent:
aosctl node list
aosctl nodes list
```

### Alias Behavior

- Aliases are shown in `--help` output as "visible aliases"
- Tab completion works with both the canonical command and alias
- All subcommands and options work identically with either form
- Log output and telemetry use the canonical command name

### Discovering Aliases

To see available command aliases, run:

```bash
aosctl --help
```

Aliases appear in parentheses next to command names:
```
Commands:
  adapter (adapters)    Adapter management commands
  stack (stacks)        Adapter stack management
  node (nodes)          Node management commands
```

---

## Codebase Adapter Lifecycle and Provenance

This section covers the complete workflow for codebase adapters, including dataset provenance, lifecycle management, and activation gating.

### Operator Runbook (Codebase Training)

Use `aosctl adapter codebase ingest` for codebase training runs. Operators should:

- Enable streaming with `--stream` (and `--stream-format json` for log ingestion). A session ID is auto-generated when streaming or when `--session-name`/`--session-tags` is set.
- Capture the `.aos` output path and dataset hash from CLI output; `aosctl adapter inspect <path>` shows `dataset_id`, `dataset_version_ids`, `codebase_scope`, and scope metadata.
- Use `aosctl dataset show <dataset_version_id>` to view row counts, splits, validation, and trust state.
- When registration is enabled (default), each prompt/response pair is persisted in `codebase_dataset_rows` with `repo_slug`, `repo_identifier`, commit info, file paths, and `training_config_hash` in `metadata_json` for traceability.

### Dataset Category Metadata

When training adapters from codebases, datasets are automatically categorized to enable canonical storage and reproducibility tracking:

| Category | Description | Source |
|----------|-------------|--------|
| `codebase` | Derived from code repositories | `train-from-code` command |
| `metrics` | System metrics and telemetry | Automated ingestion |
| `synthetic` | Generated/augmented data | Synthetic data pipelines |
| `upload` | User-uploaded datasets | Manual upload |
| `patches` | Code patches and diffs | Patch extraction |
| `general` | General-purpose training data | Mixed sources |

Category metadata is embedded in the adapter manifest and used for:
- Organizing datasets in canonical storage paths
- Filtering adapters by training data source
- Reproducibility and audit trails

For codebase datasets, scope changes (repo-scope filters or scan-root changes) create new dataset records with a `scope-<hash>` suffix in the dataset name and storage path.
Each commit ingest creates a new dataset record named `{repo_slug}-{short_commit}` (plus the scope suffix when applicable). Use `aosctl dataset list --name <repo_slug>` to find dataset IDs and `aosctl dataset versions <DATASET_ID>` to inspect per-commit versions.

### Canonical Dataset Storage

Datasets are stored using content-addressable paths for deduplication and integrity:

```
{datasets_root}/
├── files/                          # Standard dataset files
│   └── {workspace_id}/
│       └── {dataset_id}/
│           ├── manifest.json
│           ├── data.jsonl
│           └── samples/
│               └── {sample_file}
├── canonical/                      # Content-addressable storage
│   └── {category}/                 # e.g., codebase, metrics
│       └── {hash_prefix}/          # First 2 chars of hash
│           └── {content_hash}/     # Full BLAKE3 hash
│               └── canonical.jsonl
└── temp/                           # Temporary uploads
```

For codebase ingestion, the canonical dataset artifact is stored at
`canonical/codebase/{hash_prefix}/{dataset_hash_b3}/canonical.jsonl` and the
dataset metadata records both `dataset_hash_b3` (sample-derived) and
`dataset_file_hash_b3` (exact JSONL bytes).

Sample artifacts (previews, stats, or inspection outputs) are stored under
`files/{workspace_id}/{dataset_id}/samples/{sample_file}` and, when tied to a dataset
version, `files/{workspace_id}/{dataset_id}/versions/{version_id}/samples/{sample_file}`.

**Benefits**:
- Automatic deduplication across training runs
- Integrity verification via BLAKE3 hashes
- Easy reproducibility with content-addressed references

### Training Snapshot Provenance

When adapters are trained, a training snapshot is recorded for reproducibility:

```bash
# View training snapshot for an adapter
aosctl adapter snapshot --adapter-id my-adapter
```

Snapshot includes:
- **Dataset ID**: Reference to the training dataset
- **Dataset Version**: Exact version used for training
- **Dataset Hash (BLAKE3)**: Content hash for integrity verification
- **Documents JSON**: List of documents with individual hashes
- **Chunking Config**: Exact chunking parameters used

### Lifecycle and Activation Gating

Before an adapter can be activated (transitioned to `active` state), preflight checks must pass:

```bash
# Run preflight checks for an adapter
aosctl preflight --adapter my-adapter

# Gate activation (blocks if checks fail)
aosctl adapter activate my-adapter
```

**Activation Requirements**:

| Check | Description | Bypassable |
|-------|-------------|------------|
| Adapter Exists | Adapter must exist in registry | No |
| Tenant Isolation | Adapter tenant must match requested tenant (when specified) | Yes |
| Lifecycle State | Must be in `ready` or `active` state (`training` allowed if configured) | No |
| File Path Set | .aos file path must be configured | No |
| File Exists | .aos file must exist on disk | No |
| File Hash Set | .aos file hash must be computed | No |
| Content Hash Set | `content_hash_b3` must be recorded for reproducibility | No |
| Training Evidence | Training snapshot evidence must exist | No |
| File Integrity | .aos file must be readable and valid | No |
| No Conflicts | No other active adapters for same repo/branch | Yes |
| System Mode | System must not be in maintenance mode | Yes |

**Bypass Options** (for emergency deployments):

```bash
# Bypass preflight with explicit flag (requires confirmation)
aosctl adapter activate my-adapter --bypass-preflight

# Hotfix deployment (implicitly bypasses preflight)
aosctl adapter activate my-adapter --hotfix
```

### Complete Codebase Adapter Workflow

End-to-end workflow for training and activating a codebase adapter:

```bash
# 1. Initialize code repository
aosctl code-init --path /path/to/repo --tenant dev

# 2. Train adapter from code (creates dataset with 'codebase' category)
aosctl adapter train-from-code --repo /path/to/repo \
    --output adapters/my-adapter.aos \
    --category codebase

# 3. Register adapter (moves to 'draft' state)
aosctl adapter register adapters/my-adapter.aos

# 4. Run preflight checks (required before activation)
aosctl preflight --adapter my-adapter

# 5. Transition to ready state (after preflight passes)
aosctl adapter lifecycle-transition my-adapter ready

# 6. Activate adapter (production traffic)
aosctl adapter activate my-adapter

# 7. Verify activation
aosctl adapter info my-adapter --show-lifecycle
```

### Lifecycle State Transitions

```
Draft → Training → Ready → Active → Deprecated → Retired
  ↘         ↘        ↘       ↘  ↖ (rollback)     ↗
   └────────┴────────┴───────┴──► Failed
```

**Key Rules**:
- `Ready → Active` requires preflight checks to pass
- `Active → Ready` is allowed for rollback scenarios
- `Active → Deprecated` requires confirmation for non-ephemeral adapters
- Ephemeral adapters skip `Deprecated` and go directly to `Retired`
- `Retired` and `Failed` are terminal states

---

## See Also

- [Architecture Overview](ARCHITECTURE.md) - System architecture
- [Control Plane](ARCHITECTURE.md#architecture-components) - Control plane architecture
- [API Reference](API_REFERENCE.md) - REST API documentation
- [AGENTS.md](../AGENTS.md) - Complete developer reference

MLNavigator Inc January 2026.
