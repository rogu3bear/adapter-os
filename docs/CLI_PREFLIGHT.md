# AdapterOS Pre-Flight Checker

## Overview

The `aosctl preflight` command provides comprehensive environment verification before launching the AdapterOS server. It checks all critical components and provides actionable guidance for fixing issues.

## Quick Start

```bash
# Run pre-flight checks
cargo run -p adapteros-cli -- preflight

# Or when CLI is built:
./target/release/aosctl preflight

# Run checks and attempt auto-fix
aosctl preflight --fix

# Recommended: Check before launching server
aosctl preflight && cargo run -p adapteros-server-api
```

## What It Checks

### 1. Model Availability ✓
- **Checks:**
  - Model directory exists
  - Required files present (`config.json`, `tokenizer.json`)
  - Model weights (`.safetensors` or `.bin` files)
- **Fix:** `make download-model` or `./scripts/download-model.sh`

### 2. Database Status ✓
- **Checks:**
  - Database file exists
  - Database connection works
  - Migrations applied
- **Fix:** `cargo run -p adapteros-cli -- db migrate`

### 3. Required Directories ✓
- **Checks:**
  - `var/` - Runtime data
  - `var/logs/` - Logs
  - `var/bundles/` - Telemetry
  - `var/keys/` - Cryptographic keys
- **Fix:** Auto-created or `mkdir -p var/{logs,bundles,keys}`

### 4. Environment Variables ✓
- **Checks:**
  - `AOS_DATABASE_URL` or `DATABASE_URL`
  - `AOS_MODEL_PATH` or `AOS_MLX_FFI_MODEL`
  - `AOS_MODEL_BACKEND`
- **Fix:** Set in `.env` file or export

### 5. Backend Availability ✓
- **Checks:**
  - CoreML: Swift compiler (`swiftc`)
  - Metal: Metal compiler (`xcrun metal`)
  - MLX: MLX library (optional)
- **Fix:** `xcode-select --install` (macOS) or `pip install mlx` (MLX)

### 6. System Resources ✓
- **Checks:**
  - Available disk space
  - System memory (recommend 8GB+)
- **Fix:** Free up space/add RAM

## Command Options

```bash
aosctl preflight [OPTIONS]

Options:
  -f, --fix              Fix issues automatically where possible
      --database-url <URL>   Database path (default: from env)
      --model-path <PATH>    Model path (default: from env)
      --skip-backends        Skip backend availability checks (faster)
      --skip-resources       Skip resource checks (faster)
  -h, --help             Print help
```

## Usage Examples

### Basic Check
```bash
$ aosctl preflight

🚀 Running AdapterOS preflight checks...

╔════════════════════════╦════════╦═══════════════════════════════════════╗
║ Check                  ║ Status ║ Message                               ║
╠════════════════════════╬════════╬═══════════════════════════════════════╣
║ Model                  ║ ✓      ║ Model ready at ./models/qwen2.5-7b... ║
║ Database               ║ ✓      ║ Database ready at var/aos-cp.sqlite3  ║
║ Directory: var         ║ ✓      ║ Runtime data directory exists         ║
║ Directory: var/logs    ║ ✓      ║ Log directory exists                  ║
║ Env: DATABASE_URL      ║ ✓      ║ Database URL configured               ║
║ Env: MODEL_PATH        ║ ✓      ║ Model path configured                 ║
║ Backend: CoreML        ║ ✓      ║ Swift compiler available - CoreML...  ║
║ Backend: Metal         ║ ✓      ║ Metal compiler available - Metal...   ║
║ Backend: MLX           ║ ⚠      ║ MLX library not found - using stub... ║
║ System Memory          ║ ✓      ║ 16GB RAM available                    ║
╚════════════════════════╩════════╩═══════════════════════════════════════╝

📊 Summary: 10 checks run
✅ All checks passed - system ready to launch!
```

### With Failures
```bash
$ aosctl preflight

🚀 Running AdapterOS preflight checks...

╔════════════════════════╦════════╦═══════════════════════════════════════╗
║ Check                  ║ Status ║ Message                               ║
╠════════════════════════╬════════╬═══════════════════════════════════════╣
║ Model Directory        ║ ✗      ║ Model directory not found: ./models...║
║ Database               ║ ✗      ║ Database not initialized: var/aos-... ║
║ Backend: CoreML        ║ ⚠      ║ Swift compiler not found - CoreML...  ║
╚════════════════════════╩════════╩═══════════════════════════════════════╝

📊 Summary: 3 checks run
❌ 2 critical failures
⚠️  1 warnings

💡 Suggested fixes:

  Model Directory:
    $ make download-model  # or: ./scripts/download-model.sh

  Database:
    $ cargo run -p adapteros-cli -- db migrate

  Backend: CoreML:
    $ xcode-select --install
```

### Check Specific Model
```bash
$ aosctl preflight --model-path ./custom-models/llama-3

🚀 Running AdapterOS preflight checks...
# Checks ./custom-models/llama-3 instead of default path
```

### Fast Check (Skip Backends)
```bash
$ aosctl preflight --skip-backends --skip-resources

# Only checks: model, database, directories, env vars
# Faster for quick validation
```

## Integration with Launch Scripts

### Method 1: Shell Script
```bash
#!/bin/bash
# launch.sh

echo "Checking system readiness..."
if ! aosctl preflight; then
    echo "❌ Pre-flight checks failed - fix issues before launching"
    exit 1
fi

echo "✅ System ready - launching server..."
cargo run --release -p adapteros-server-api
```

### Method 2: One-Liner
```bash
# Only launch if preflight passes
aosctl preflight && cargo run -p adapteros-server-api
```

### Method 3: Makefile Integration
```makefile
# Add to Makefile
preflight: ## Check system readiness before launch
	cargo run -p adapteros-cli -- preflight

serve: preflight ## Run server (with preflight check)
	cargo run --release -p adapteros-server-api
```

## Exit Codes

| Exit Code | Meaning |
|-----------|---------|
| 0 | All checks passed (or only warnings) |
| 1 | One or more critical failures |

Use in scripts:
```bash
if aosctl preflight; then
    echo "Ready to launch"
else
    echo "Fix issues first"
    exit 1
fi
```

## Comparison with `doctor` Command

| Feature | `preflight` | `doctor` |
|---------|-------------|----------|
| **Purpose** | Pre-launch environment check | Running server health check |
| **When to use** | Before starting server | After server is running |
| **Checks** | File system, config, backends | HTTP endpoints, components |
| **Requires server** | No | Yes (server must be running) |
| **Auto-fix** | Yes (--fix flag) | No |

**Workflow:**
1. `aosctl preflight` - Check environment is ready
2. Launch server - `cargo run -p adapteros-server-api`
3. `aosctl doctor` - Verify server health

## Common Issues and Fixes

### Issue: Model Not Found
```
❌ Model directory not found: ./models/qwen2.5-7b-mlx
```
**Fix:**
```bash
make download-model
# or
./scripts/download-model.sh
```

### Issue: Database Not Initialized
```
❌ Database not initialized: var/aos-cp.sqlite3
```
**Fix:**
```bash
cargo run -p adapteros-cli -- db migrate
```

### Issue: Swift Compiler Missing
```
⚠️  Swift compiler not found - CoreML backend may be degraded
```
**Fix:**
```bash
xcode-select --install
```

### Issue: Missing Directories
```
⚠️  Runtime data directory not found (will be auto-created)
```
**Fix:**
```bash
mkdir -p var/{logs,bundles,keys}
# Or just run the server - directories auto-created
```

## Developer Notes

### File Location
`crates/adapteros-cli/src/commands/preflight.rs`

### Dependencies
- `sqlx` - Database connection checking
- `reqwest` - (Not used in preflight, only in doctor)
- `comfy_table` - Formatted output tables
- `clap` - CLI argument parsing

### Adding New Checks

To add a new check to the preflight system:

1. Create a check function in `preflight.rs`:
```rust
async fn check_my_feature(cmd: &PreflightCommand) -> CheckResult {
    // Your check logic
    if feature_ok {
        CheckResult::pass("Feature Name", "Feature is ready")
    } else {
        CheckResult::fail(
            "Feature Name",
            "Feature not configured",
            Some("fix-command-here".to_string())
        )
    }
}
```

2. Add to the `run()` function:
```rust
results.push(check_my_feature(&cmd).await);
```

3. Test:
```bash
cargo run -p adapteros-cli -- preflight
```

## See Also

- [QUICKSTART.md](../QUICKSTART.md) - Getting started guide
- [ENVIRONMENT_SETUP.md](ENVIRONMENT_SETUP.md) - Environment configuration
- [CLI Reference](CLI_REFERENCE.md) - All CLI commands
- [download-model.sh](../scripts/download-model.sh) - Model download script

---

**Copyright:** © 2025 JKCA / James KC Auchterlonie. All rights reserved.
