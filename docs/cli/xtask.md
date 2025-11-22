# cargo xtask

**Status:** Active developer automation tool
**Location:** `xtask/src/main.rs`
**Usage:** `cargo xtask <task>`

## Purpose

`cargo xtask` provides developer-focused automation tasks for building, testing, and packaging AdapterOS. These tasks are intended for development workflows and should NOT be used in production.

## Available Tasks

### `cargo xtask sbom`

Generate Software Bill of Materials (SBOM) from project dependencies.

```bash
cargo xtask sbom
```

**Output:** SBOM in standard format (SPDX/CycloneDX)
**Use case:** Compliance, security audits, dependency tracking

---

### `cargo xtask determinism-report`

Generate build reproducibility and determinism report.

```bash
cargo xtask determinism-report
```

**Checks:**
- Build reproducibility across runs
- Compiler determinism
- Dependency version pinning
- Randomness sources

**Use case:** Ensuring reproducible builds for security and audit purposes

---

### `cargo xtask verify-adapters`

Verify all adapter deliverables (Phases A-F) are complete and pass quality gates.

```bash
# Full verification
cargo xtask verify-adapters

# Static checks only (no server startup)
cargo xtask verify-adapters --static-only

# JSON output for CI
cargo xtask verify-adapters --json

# Dry run
cargo xtask verify-adapters --dry-run
```

**Options:**
- `--static-only` - Skip runtime checks (server start/stop)
- `--artifacts <dir>` - Output directory for verification artifacts (default: `target/verify`)
- `--fail-on-regression` - Fail if performance regressions detected
- `--timeout <seconds>` - Timeout for entire verification (default: 300)
- `--no-gpu` - Skip GPU counter checks

**Checks:**
- Baseline: Environment, dependencies, compilation
- Agent A: Kernel & Determinism
- Agent B: Backend & Control Plane
- Agent C: Adapters & Routing
- Agent D: UI/UX/Observability
- Agent E: Testing/Deployment/Compliance
- Agent F: Adapter Lifecycle & TTL

**Exit codes:**
- 0 - All checks passed
- 1 - One or more checks failed

---

### `cargo xtask verify-artifacts`

Verify and sign release artifacts.

```bash
cargo xtask verify-artifacts
```

**Use case:** Pre-release artifact validation

---

### `cargo xtask openapi-docs`

Generate OpenAPI documentation in Markdown format.

```bash
cargo xtask openapi-docs
```

**Output:** Markdown documentation from OpenAPI specs
**Use case:** API documentation generation

---

### `cargo xtask code2db-dataset`

Build JSON training dataset for code→database tasks.

```bash
# Basic usage
cargo xtask code2db-dataset \
  --source-dir ./crates/adapteros-db \
  --output dataset.jsonl

# With custom options
cargo xtask code2db-dataset \
  --source-dir ./src \
  --output training/dataset.jsonl \
  --include-tests \
  --max-examples 10000
```

**Options:**
- `--source-dir <dir>` - Source code directory to analyze
- `--output <file>` - Output JSONL file path
- `--include-tests` - Include test files in dataset
- `--max-examples <n>` - Maximum number of examples to generate

**Use case:** Training dataset generation for code understanding models

---

### `cargo xtask pack-lora`

Quantize and package trained LoRA weights into `.aos` archive format.

```bash
# Package weights with default settings
cargo xtask pack-lora \
  --weights trained_weights.safetensors \
  --manifest adapter_manifest.json \
  --output adapter.aos

# With custom quantization
cargo xtask pack-lora \
  --weights trained_weights.safetensors \
  --manifest adapter_manifest.json \
  --output adapter.aos \
  --quantization q15
```

**Options:**
- `--weights <file>` - Input safetensors weights file
- `--manifest <file>` - Adapter manifest JSON
- `--output <file>` - Output .aos file path
- `--quantization <type>` - Quantization method (q15, q8, etc.)

**Use case:** Packaging trained adapters for deployment

---

### `cargo xtask train-base-adapter`

Train a base adapter from a training manifest.

```bash
# Train with defaults
cargo xtask train-base-adapter

# Custom manifest and tokenizer
cargo xtask train-base-adapter \
  --manifest training/datasets/my_manifest.json \
  --tokenizer models/qwen2.5-7b-mlx/tokenizer.json

# Output as .aos file
cargo xtask train-base-adapter \
  --output-format aos \
  --adapter-id my_adapter
```

**Options:**
- `--manifest <file>` - Training manifest path
- `--tokenizer <file>` - Tokenizer configuration
- `--output-format <format>` - Output format (safetensors, aos)
- `--adapter-id <id>` - Adapter identifier

**Use case:** Training base adapters for fine-tuning

---

### `cargo xtask build`

Custom build workflow (development only).

```bash
cargo xtask build
```

**Note:** Currently a placeholder. Use `cargo build` for actual builds.

---

### `cargo xtask test`

Run full test suite with custom orchestration.

```bash
cargo xtask test
```

**Note:** Currently a placeholder. Use `cargo test` for actual testing.

---

## When to Use xtask vs aosctl

**Use `cargo xtask` for:**
- Development workflows
- Building and packaging
- Generating reports (SBOM, determinism)
- Testing and verification in development
- Dataset creation
- Adapter training

**Use `aosctl` for:**
- Production operations
- Database migrations
- Cluster management
- Deployment
- System maintenance

**Rule of thumb:** If it's for development/CI/testing → xtask. If it's for production/ops → aosctl.

## Examples

### Generate SBOM for compliance review
```bash
cargo xtask sbom > sbom.json
```

### Verify adapters before committing
```bash
cargo xtask verify-adapters --static-only
```

### Create training dataset from codebase
```bash
cargo xtask code2db-dataset \
  --source-dir ./crates/adapteros-db \
  --output training/db_dataset.jsonl
```

### Package trained adapter for deployment
```bash
cargo xtask pack-lora \
  --weights ./trained/adapter.safetensors \
  --manifest ./trained/manifest.json \
  --output ./adapters/my_adapter.aos
```

### Full verification pipeline (CI)
```bash
# Run all checks with JSON output
cargo xtask verify-adapters --json > verification.json

# Check exit code
if [ $? -eq 0 ]; then
  echo "All checks passed"
else
  echo "Verification failed"
  exit 1
fi
```

## Adding New Tasks

To add a new xtask:

1. Add match arm in `xtask/src/main.rs`
2. Create a new module in `xtask/src/<task_name>.rs`
3. Implement the task logic
4. Update help text in `print_help()`
5. Document in this file

Example:
```rust
// In main.rs
match task.as_deref() {
    Some("my-task") => my_task::run()?,
    // ...
}

// In xtask/src/my_task.rs
pub fn run() -> Result<()> {
    println!("Running my task...");
    // Task implementation
    Ok(())
}
```

## Related Commands

- [`aosctl`](./AOSCTL.md) - Production system administration
- [`aos`](./AOS.md) - Local service control
- [`aos-launch`](./AOS-LAUNCH.md) - Development orchestration

## Source

For implementation details, see: `xtask/src/main.rs:1-134`
