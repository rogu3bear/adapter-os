# aosctl

**Status:** Active Rust CLI for system administration and operations
**Location:** `crates/adapteros-cli/src/main.rs` (and `app.rs`)
**Installation:** `cargo install --path crates/adapteros-cli` or included in release builds

## Purpose

`aosctl` is the primary command-line interface for AdapterOS system administration, operations, and cluster management. It provides comprehensive functionality for database management, adapter deployment, verification, maintenance, and monitoring.

## Command Categories

### Database Management
```bash
# Run database migrations
aosctl db migrate

# Verify migration signatures only
aosctl db migrate --verify-only

# Migrate registry database schema
aosctl registry-migrate --from-db deprecated/registry.db --to-db var/registry.db
```

### Verification Commands
```bash
# Verify all adapters (deliverables A-F)
aosctl verify adapters

# Verify determinism loop
aosctl verify determinism-loop

# Verify artifact bundle
aosctl verify bundle artifacts/adapters.zip

# Verify single adapter
aosctl verify adapter --adapters-root ./adapters --adapter-id demo_adapter

# Verify telemetry bundle chain
aosctl verify telemetry --bundle-dir ./var/telemetry

# Verify federation signatures
aosctl verify federation --bundle-dir ./var/telemetry --database ./var/cp.db
```

### Adapter Management
```bash
# List adapters
aosctl list-adapters
aosctl list-adapters --tier persistent

# Deploy adapters
aosctl deploy adapters --adapters-root ./adapters

# Pin adapter (prevent eviction)
aosctl adapter-pin --tenant tenant_a --adapter demo_adapter --ttl-hours 72

# Unpin adapter
aosctl adapter-unpin --tenant tenant_a --adapter demo_adapter

# List pinned adapters
aosctl adapter-list-pinned --tenant tenant_a
```

### System Status & Diagnostics
```bash
# System status
aosctl status adapters
aosctl status cluster
aosctl status tick
aosctl status memory

# Diagnostics
aosctl diag --profile system
aosctl diag --profile performance
```

### Maintenance & Operations
```bash
# Garbage collect bundles
aosctl maintenance gc-bundles --keep 10

# Other maintenance tasks
aosctl maintenance <task>
```

### Node & Cluster Management
```bash
# List nodes
aosctl node-list

# Verify nodes
aosctl node-verify --all

# Sync between nodes
aosctl node-sync verify --from node1 --to node2
```

### Policy Management
```bash
# List policies
aosctl policy list
aosctl policy list --implemented

# Explain policy
aosctl policy explain Egress

# Enforce policies
aosctl policy enforce --all --dry-run
aosctl policy enforce --pack Determinism
```

## Complete Reference

For the complete command reference with all options and examples, see:

**[crates/adapteros-cli/docs/aosctl_manual.md](../../crates/adapteros-cli/docs/aosctl_manual.md)**

The manual includes:
- Complete command syntax for all subcommands
- Detailed option descriptions
- Example workflows
- Configuration guidance
- Troubleshooting tips

## Quick Reference

### Common Operations

#### Initialize new tenant
```bash
aosctl init-tenant --id tenant_dev --uid 1000 --gid 1000
```

#### Deploy adapters to production
```bash
aosctl deploy adapters --adapters-root /var/adapters
```

#### Verify system integrity
```bash
aosctl verify adapters
aosctl verify determinism-loop
```

#### System maintenance
```bash
# Run DB migrations
aosctl db migrate

# Clean up old bundles
aosctl maintenance gc-bundles --keep 10

# Check cluster status
aosctl status cluster
```

## Global Flags

All `aosctl` commands support these global flags:

- `--json` - Output in JSON format
- `--quiet, -q` - Suppress non-essential output
- `--verbose, -v` - Enable verbose output

Example:
```bash
aosctl list-adapters --json
aosctl verify adapters --verbose
aosctl status cluster --quiet
```

## Configuration

`aosctl` reads configuration from:
1. Command-line flags (highest priority)
2. Environment variables
3. Configuration file (`configs/cp.toml`)
4. Defaults (lowest priority)

Common environment variables:
- `DATABASE_URL` - Database connection string
- `AOS_LOG` - Log level (trace, debug, info, warn, error)
- `RUST_LOG` - Rust-specific logging

## Telemetry

All `aosctl` operations emit structured telemetry events for observability and audit trails. Events include:
- Command name
- Tenant ID (if applicable)
- Success/failure status
- Error codes (on failure)
- Execution metadata

## Exit Codes

- **0** - Success
- **1** - General error
- **2** - Invalid arguments
- **3** - Database error
- **4** - Network error
- **5** - Permission denied

## When to Use aosctl

**Use `aosctl` for:**
- Production operations
- Database management
- System administration
- Cluster coordination
- Deployment workflows
- Compliance and verification
- Maintenance tasks

**Do NOT use `aosctl` for:**
- Local service control (use `aos` instead)
- Development automation (use `cargo xtask` instead)
- Quick local testing (use `aos-launch` instead)

## Migration from Deprecated Commands

Many shell scripts have been replaced by `aosctl` subcommands:

| Old Command | New Command |
|-------------|-------------|
| `./scripts/migrate.sh` | `aosctl db migrate` |
| `./scripts/deploy_adapters.sh` | `aosctl deploy adapters` |
| `./scripts/gc_bundles.sh` | `aosctl maintenance gc-bundles` |
| `aosctl verify-adapters` | `aosctl verify adapters` |
| `aosctl telemetry-verify` | `aosctl verify telemetry` |
| `aosctl federation-verify` | `aosctl verify federation` |

See [DEPRECATIONS.md](../../DEPRECATIONS.md) for the complete list.

## Related Commands

- [`aos`](./aos.md) - Local service control
- [`aos-launch`](./aos-launch.md) - Development orchestration
- [`cargo xtask`](./xtask.md) - Developer automation

## Source

For implementation details, see:
- Main CLI: `crates/adapteros-cli/src/main.rs`
- App structure: `crates/adapteros-cli/src/app.rs`
- Commands: `crates/adapteros-cli/src/commands/`
- Complete manual: `crates/adapteros-cli/docs/aosctl_manual.md`
