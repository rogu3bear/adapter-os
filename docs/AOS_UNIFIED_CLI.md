# AOS Unified CLI Tool

**Copyright:** © 2025 JKCA / James KC Auchterlonie. All rights reserved.

## Overview

The `aos-unified` binary is a comprehensive command-line tool that consolidates all AOS operations into a single interface. It replaces the need for multiple separate binaries (`aos-analyze`, `aos-validate`, `aos-create`, `aos-info`, `aos-verify`) and also includes service management capabilities.

## Installation

Build the unified CLI tool:

```bash
cargo build --release -p adapteros-aos --bin aos-unified
```

The binary will be available at `target/release/aos-unified`.

Optionally, create a symlink for easier access:

```bash
ln -s $(pwd)/target/release/aos-unified /usr/local/bin/aos-unified
```

## Command Structure

```
aos-unified <SUBCOMMAND> [OPTIONS]

SUBCOMMANDS:
    analyze   Analyze AOS file structure and contents
    validate  Validate AOS file integrity and compliance
    create    Create AOS archive from directory
    info      Display AOS file information
    verify    Deep verification of AOS files
    service   Service management commands (start, stop, restart, status, logs)
    help      Print this message or the help of the given subcommand(s)
```

## Global Options

All subcommands support these global options:

- `-v, --verbose` - Enable verbose output (detailed logging and additional information)
- `-q, --quiet` - Suppress non-essential output (errors only)
- `--json` - Output as JSON where applicable (for programmatic consumption)
- `-h, --help` - Print help information
- `-V, --version` - Print version information

## AOS Archive Commands

### analyze

Analyze AOS file structure and display detailed information about the archive format, weights, and manifest.

**Usage:**
```bash
aos-unified analyze <FILE> [OPTIONS]
```

**Examples:**
```bash
# Basic analysis
aos-unified analyze adapters/creative-writer.aos

# Verbose output
aos-unified analyze adapters/creative-writer.aos --verbose

# JSON output for automation
aos-unified analyze adapters/creative-writer.aos --json
```

**Output includes:**
- File size and structure breakdown
- Header information (manifest offset, length)
- Weights format detection (JSON vs SafeTensors)
- Tensor information (names, shapes, dtypes, sizes)
- Complete manifest JSON
- Hex dump of file header
- Validation warnings and errors

---

### validate

Validate AOS file integrity, format compliance, and production readiness.

**Usage:**
```bash
aos-unified validate <FILE> [OPTIONS]
```

**Options:**
- `--skip-tensors` - Skip tensor data validation (faster)
- `--skip-hash` - Skip BLAKE3 hash verification

**Examples:**
```bash
# Full validation
aos-unified validate adapters/creative-writer.aos

# Quick validation (skip tensor checks)
aos-unified validate adapters/creative-writer.aos --skip-tensors

# CI/CD integration (JSON output)
aos-unified validate adapters/*.aos --json
```

**Validation checks:**
- File structure integrity
- Header format validation
- Manifest schema validation
- Semantic naming convention (tenant/domain/purpose/revision)
- Rank and alpha parameter ranges
- BLAKE3 hash verification
- Tensor metadata consistency
- File size limits

**Exit codes:**
- `0` - All validations passed
- `1` - One or more validations failed

---

### create

Create AOS archive from a directory containing `manifest.json` and `weights.safetensors`.

**Usage:**
```bash
aos-unified create <INPUT_DIR> [OPTIONS]
```

**Options:**
- `-o, --output <FILE>` - Output .aos file path (default: `adapters/<dirname>.aos`)
- `--adapter-id <ID>` - Override adapter ID (format: `tenant/domain/purpose/revision`)
- `--verify` - Verify the created .aos file after creation
- `--dry-run` - Preview without creating file

**Examples:**
```bash
# Create archive with default output path
aos-unified create input_dir/

# Specify output path
aos-unified create input_dir/ -o my-adapter.aos

# Override adapter ID and verify
aos-unified create input_dir/ --adapter-id tenant-a/ml/classifier/r001 --verify

# Dry run to preview
aos-unified create input_dir/ --dry-run --verbose
```

**Input directory structure:**
```
input_dir/
├── manifest.json          # Adapter manifest
└── weights.safetensors    # LoRA weights in SafeTensors format
```

**Manifest fields:**
```json
{
  "format_version": 2,
  "adapter_id": "tenant/domain/purpose/revision",
  "version": "1.0.0",
  "rank": 16,
  "alpha": 32.0,
  "base_model": "qwen2.5-7b",
  "weights_hash": "computed-blake3-hash",
  "metadata": {}
}
```

---

### info

Display concise information about an AOS archive file.

**Usage:**
```bash
aos-unified info <FILE> [OPTIONS]
```

**Options:**
- `--full-manifest` - Show complete manifest JSON
- `--checksums` - Show tensor data checksums

**Examples:**
```bash
# Basic info
aos-unified info adapters/creative-writer.aos

# Show full manifest
aos-unified info adapters/creative-writer.aos --full-manifest

# JSON output
aos-unified info adapters/creative-writer.aos --json
```

**Output includes:**
- File path and total size
- Format version
- Archive structure (header, weights, manifest sections)
- Manifest summary (version, adapter_id, rank, alpha, base_model)
- Tensor count and sizes (if available)

---

### verify

Deep verification of AOS files with comprehensive integrity checks.

**Usage:**
```bash
aos-unified verify <FILE> [OPTIONS]
```

**Options:**
- `--skip-tensors` - Skip tensor data validation (faster)

**Examples:**
```bash
# Full verification
aos-unified verify adapters/creative-writer.aos

# Quick verification
aos-unified verify adapters/creative-writer.aos --skip-tensors

# Verbose output
aos-unified verify adapters/creative-writer.aos --verbose
```

**Verification checks:**
- File access and readability
- Header format and bounds
- Manifest JSON parsing
- Manifest schema validation
- Tensor metadata validation
- File integrity (size consistency)
- SafeTensors format validation

**Exit codes:**
- `0` - Verification passed
- `1` - Verification failed

---

## Service Management Commands

The `service` subcommand provides management for AdapterOS backend services, UI, and menu bar application.

### service start

Start a service (backend, UI, or menu bar).

**Usage:**
```bash
aos-unified service start [SERVICE] [OPTIONS]
```

**Arguments:**
- `SERVICE` - Service to start: `backend` (default), `ui`, `menubar`

**Options:**
- `--config <PATH>` - Path to configuration file (default: `configs/aos.toml`)
- `--dry-run` - Preview without starting

**Examples:**
```bash
# Start backend service
aos-unified service start backend

# Start UI dev server
aos-unified service start ui

# Start menu bar app (macOS only)
aos-unified service start menubar

# Dry run
aos-unified service start backend --dry-run
```

---

### service stop

Stop a running service.

**Usage:**
```bash
aos-unified service stop [SERVICE] [OPTIONS]
```

**Options:**
- `--dry-run` - Preview without stopping

**Examples:**
```bash
# Stop backend
aos-unified service stop backend

# Stop UI
aos-unified service stop ui

# Stop all services
aos-unified service stop backend
aos-unified service stop ui
aos-unified service stop menubar
```

---

### service restart

Restart a service (stop then start).

**Usage:**
```bash
aos-unified service restart [SERVICE] [OPTIONS]
```

**Options:**
- `--config <PATH>` - Path to configuration file (default: `configs/aos.toml`)

**Examples:**
```bash
# Restart backend
aos-unified service restart backend

# Restart UI
aos-unified service restart ui
```

---

### service status

Show status of all services.

**Usage:**
```bash
aos-unified service status [OPTIONS]
```

**Examples:**
```bash
# Show status
aos-unified service status

# JSON output for automation
aos-unified service status --json
```

**Output:**
```
backend: running (pid=12345)
ui: running (pid=12346)
menu-bar: stopped
```

**JSON output:**
```json
{
  "ts": "2025-01-19T12:00:00Z",
  "component": "aos",
  "services": [
    {
      "service": "backend",
      "status": "running",
      "pid": 12345
    },
    {
      "service": "ui",
      "status": "running",
      "pid": 12346
    },
    {
      "service": "menu-bar",
      "status": "stopped",
      "pid": null
    }
  ]
}
```

---

### service logs

Show recent logs for a service.

**Usage:**
```bash
aos-unified service logs [SERVICE] [OPTIONS]
```

**Examples:**
```bash
# Show backend logs
aos-unified service logs backend

# Show UI logs
aos-unified service logs ui

# JSON output
aos-unified service logs backend --json
```

**Log files:**
- Backend: `server.log`
- UI: `ui-dev.log`
- Menu bar: `menu-bar.log`

---

## Common Workflows

### Development Workflow

1. **Start services:**
   ```bash
   aos-unified service start backend
   aos-unified service start ui
   ```

2. **Check status:**
   ```bash
   aos-unified service status
   ```

3. **View logs:**
   ```bash
   aos-unified service logs backend
   ```

4. **Restart after changes:**
   ```bash
   aos-unified service restart backend
   ```

### Archive Creation Workflow

1. **Prepare directory:**
   ```
   my-adapter/
   ├── manifest.json
   └── weights.safetensors
   ```

2. **Create archive:**
   ```bash
   aos-unified create my-adapter/ -o adapters/my-adapter.aos --verify
   ```

3. **Validate archive:**
   ```bash
   aos-unified validate adapters/my-adapter.aos --verbose
   ```

4. **Inspect archive:**
   ```bash
   aos-unified info adapters/my-adapter.aos --full-manifest
   ```

### CI/CD Integration

**Archive validation in CI:**
```bash
#!/bin/bash
for file in adapters/*.aos; do
  aos-unified validate "$file" --json || exit 1
done
```

**Automated archive creation:**
```bash
#!/bin/bash
aos-unified create input/ -o output.aos --verify --json > result.json
if [ $? -eq 0 ]; then
  echo "Archive created successfully"
else
  echo "Archive creation failed"
  exit 1
fi
```

---

## Comparison with Individual Binaries

| Operation | Old Command | New Unified Command |
|-----------|-------------|---------------------|
| Analyze | `aos-analyze file.aos` | `aos-unified analyze file.aos` |
| Validate | `aos-validate file.aos` | `aos-unified validate file.aos` |
| Create | `aos-create dir/ -o file.aos` | `aos-unified create dir/ -o file.aos` |
| Info | `aos-info file.aos` | `aos-unified info file.aos` |
| Verify | `aos-verify file.aos` | `aos-unified verify file.aos` |
| Start service | `aos start backend` | `aos-unified service start backend` |
| Check status | `aos status` | `aos-unified service status` |

---

## Migration Guide

### For Users of Individual Binaries

The individual binaries (`aos-analyze`, `aos-validate`, etc.) continue to work and are maintained for backward compatibility. However, the unified CLI provides:

1. **Consistency** - Single interface for all operations
2. **Discoverability** - All commands visible via `--help`
3. **Global options** - Consistent `--verbose`, `--quiet`, `--json` flags
4. **Future-proof** - New features added to unified binary first

### Transition Path

1. **Phase 1 (Current):** Both individual and unified binaries available
2. **Phase 2 (Recommended):** Switch to unified binary for new workflows
3. **Phase 3 (Future):** Individual binaries deprecated (with migration notices)

### Shell Aliases for Compatibility

Add to `.bashrc` or `.zshrc`:

```bash
alias aos-analyze='aos-unified analyze'
alias aos-validate='aos-unified validate'
alias aos-create='aos-unified create'
alias aos-info='aos-unified info'
alias aos-verify='aos-unified verify'
```

---

## Troubleshooting

### Common Issues

**Issue:** `aos-unified: command not found`
**Solution:** Ensure binary is in PATH or use full path to binary

**Issue:** Service fails to start
**Solution:** Check logs with `aos-unified service logs <service>` and verify configuration

**Issue:** Archive creation fails with "Missing manifest.json"
**Solution:** Ensure input directory contains required files (`manifest.json` and `weights.safetensors`)

**Issue:** Validation fails with hash mismatch
**Solution:** Weights may have been modified after manifest creation. Recreate archive with `aos-unified create`

### Debug Mode

Enable verbose logging for debugging:

```bash
# Archive operations
RUST_LOG=debug aos-unified analyze file.aos --verbose

# Service operations
RUST_LOG=debug aos-unified service start backend --verbose
```

---

## Implementation Notes

**Binary location:** `crates/adapteros-aos/src/bin/aos-unified.rs`

**Features:**
- Single binary for all AOS operations
- Consistent CLI interface with clap
- Unified logging with tracing
- JSON output support for automation
- Service PID tracking in `var/` directory
- Process management with SIGTERM signals

**Architecture:**
- Archive commands call simplified versions of individual binary logic
- Service commands reuse patterns from original `aos.rs`
- Shared error handling via `adapteros-core::AosError`
- Async runtime via tokio

---

## See Also

- [AOS Archive Format Specification](ARCHITECTURE_PATTERNS.md)
- [Training Pipeline Documentation](TRAINING_PIPELINE.md)
- [AdapterOS Developer Guide](../CLAUDE.md)

---

**Maintained by:** James KC Auchterlonie
**Last Updated:** 2025-01-19
