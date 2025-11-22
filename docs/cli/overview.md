# AdapterOS CLI Overview

This directory contains documentation for all AdapterOS command-line interfaces.

## Which CLI Should I Use?

AdapterOS provides three primary command-line interfaces, each designed for specific use cases:

### For Operators & System Administration: `aosctl`

Use **`aosctl`** when you need to:
- Manage database migrations (`aosctl db migrate`)
- Deploy adapters to the system (`aosctl deploy adapters`)
- Perform verification tasks (`aosctl verify ...`)
- Run system maintenance (`aosctl maintenance ...`)
- Configure and manage the cluster
- Query system status and metrics

**Example workflows:**
```bash
# Initialize database
aosctl db migrate

# Deploy adapters
aosctl deploy adapters --adapters-root ./adapters

# Verify system integrity
aosctl verify adapters
aosctl verify determinism-loop

# System maintenance
aosctl maintenance gc-bundles
```

See: [aosctl.md](./AOSCTL.md) for complete reference.

---

### For Local Development & Testing: `aos-launch`

Use **`aos-launch`** when you need to:
- Start the full AdapterOS stack locally (backend + UI + menu bar)
- Run with automatic pre-flight checks
- Get port conflict detection and resolution
- Monitor service health automatically

**Example workflows:**
```bash
# Start all services with Metal backend (default)
./aos-launch

# Start with MLX backend
AOS_MLX_FFI_MODEL=/path/to/model ./aos-launch mlx

# Start only backend
./aos-launch backend
```

See: [aos-launch.md](./AOS-LAUNCH.md) for complete reference.

---

### For Local Service Control: `aos`

Use **`aos`** when you need to:
- Control local services (`start`, `stop`, `restart`, `status`)
- View service logs
- Perform quick service health checks

**Example workflows:**
```bash
# Start backend service
aos start backend

# Check status
aos status

# View logs
aos logs backend

# Restart services
aos restart
```

See: [aos.md](./AOS.md) for complete reference.

---

### For Developer Tasks: `cargo xtask`

Use **`cargo xtask`** when you need to:
- Generate SBOMs (`cargo xtask sbom`)
- Create determinism reports (`cargo xtask determinism-report`)
- Run verification tests (`cargo xtask verify-adapters`)
- Build datasets (`cargo xtask code2db-dataset`)
- Package LoRA weights (`cargo xtask pack-lora`)
- Train base adapters (`cargo xtask train-base-adapter`)

**Example workflows:**
```bash
# Generate SBOM
cargo xtask sbom

# Run adapter verification suite
cargo xtask verify-adapters --static-only

# Build training dataset
cargo xtask code2db-dataset --source-dir ./src --output dataset.jsonl
```

See: [xtask.md](./XTASK.md) for complete reference.

---

## Decision Tree

```
┌─ Need to manage production cluster/DB? ──────────> aosctl
│
├─ Need to start full local stack for development? ─> aos-launch
│
├─ Need to control local services only? ───────────> aos
│
└─ Need developer build/test automation? ──────────> cargo xtask
```

---

## Migration from Deprecated Commands

Many shell scripts have been deprecated in favor of the Rust CLIs. See the migration guide:

### Database Migrations
- **Old:** `./scripts/migrate.sh`
- **New:** `aosctl db migrate`

### Adapter Deployment
- **Old:** `./scripts/deploy_adapters.sh`
- **New:** `aosctl deploy adapters`

### Verification Commands
- **Old:** `aosctl verify-adapters`
- **New:** `aosctl verify adapters`
- **Old:** `aosctl telemetry-verify --bundle-dir ./var/telemetry`
- **New:** `aosctl verify telemetry --bundle-dir ./var/telemetry`

See [DEPRECATIONS.md](../../DEPRECATIONS.md) for the complete list.

---

## Related Documentation

- [aosctl.md](./AOSCTL.md) - Complete aosctl command reference
- [aos.md](./AOS.md) - aos service control reference
- [aos-launch.md](./AOS-LAUNCH.md) - aos-launch orchestration reference
- [xtask.md](./XTASK.md) - cargo xtask developer tasks reference
- [CONTRIBUTING.md](../../CONTRIBUTING.md) - Contribution guidelines
- [CLAUDE.md](../../CLAUDE.md) - Developer guide and architecture
