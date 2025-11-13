# AdapterOS CLI Guide

**Purpose**: Comprehensive guide to understanding and using the AdapterOS CLI (`aosctl`)  
**Last Updated**: 2025-01-15  
**Version**: alpha-v0.04-unstable

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

**What it is**: PostgreSQL/SQLite database with full tenant management and lifecycle tracking.

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

## See Also

- [Architecture Overview](architecture.md) - System architecture
- [Control Plane](control-plane.md) - Control plane API documentation
- [Architecture Index](ARCHITECTURE_INDEX.md) - Complete architecture reference

