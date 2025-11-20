# aos-create: Native Rust AOS Archive Creator

**Location:** `/Users/star/Dev/aos/crates/adapteros-aos/src/bin/aos-create.rs`

**Purpose:** Native Rust tool for creating AOS 2.0 binary archives from adapter directories. This is the production-ready implementation for the AdapterOS ecosystem.

---

## Overview

The `aos-create` tool packages adapter directories containing manifest.json and weights.safetensors into the AOS 2.0 binary archive format. This replaces the Python implementation with a native Rust binary that integrates seamlessly with the AdapterOS build system.

## Installation

```bash
# Build the binary
cargo build --release -p adapteros-aos --bin aos-create

# The binary will be available at:
# ./target/release/aos-create
```

## Usage

### Basic Usage

```bash
# Create .aos archive from adapter directory
aos-create adapters/code_lang_v1/ -o code-assistant.aos

# Verbose output
aos-create adapters/my_adapter/ -o my_adapter.aos -v

# Verify after creation
aos-create adapters/my_adapter/ -o my_adapter.aos --verify

# Dry run (preview without creating)
aos-create adapters/my_adapter/ -o my_adapter.aos --dry-run
```

### Advanced Usage

```bash
# Override adapter ID (semantic naming)
aos-create adapters/my_adapter/ -o my_adapter.aos \
  --adapter-id tenant-a/engineering/code-review/r001

# Use default output location (adapters/<dirname>.aos)
aos-create adapters/my_adapter/
```

## Command-Line Options

| Option | Short | Description |
|--------|-------|-------------|
| `<INPUT_DIR>` | - | Input directory containing manifest.json and weights.safetensors |
| `--output <FILE>` | `-o` | Output .aos file path |
| `--format <FORMAT>` | `-f` | Archive format (binary only) [default: binary] |
| `--adapter-id <ID>` | - | Override adapter ID (semantic naming: tenant/domain/purpose/revision) |
| `--verify` | - | Verify the created .aos file |
| `--dry-run` | - | Dry run - preview without creating file |
| `--verbose` | `-v` | Verbose output |
| `--help` | `-h` | Print help |
| `--version` | `-V` | Print version |

## Features

### 1. Manifest Processing

- **Flexible schema:** Optional fields with sensible defaults
- **Semantic naming:** Auto-generates adapter IDs in `tenant/domain/purpose/revision` format
- **Validation:** Checks manifest structure and required fields
- **Hash synchronization:** Ensures training_config rank/alpha match top-level values

### 2. AOS 2.0 Binary Format

Creates archives with the following structure:

```
[0-3]    manifest_offset (u32, little-endian)
[4-7]    manifest_len (u32, little-endian)
[8...]   weights (safetensors format)
[offset] manifest (JSON)
```

### 3. BLAKE3 Hashing

- Uses BLAKE3 for cryptographic hashing of weights
- Stores hash in manifest for verification
- Compatible with existing AOS verification tools

### 4. Verification

- Optional `--verify` flag validates created archives
- Checks:
  - Header integrity
  - Hash verification (BLAKE3)
  - Manifest parsing
  - Format version

### 5. Dry Run Mode

Preview operations without creating files:

```bash
$ aos-create adapters/creative_writer/ -o /tmp/test.aos --dry-run
🔍 Dry run - would create:
   Output: /tmp/test.aos
   Adapter ID: default/general/adapter/r001
   Rank: 12
   Alpha: 24
   Weights size: 1.75 MB
   Hash: f8c15973e1bad2ff...
```

## Manifest Schema

### Required Files

Input directory must contain:
- `manifest.json` - Adapter metadata
- `weights.safetensors` - LoRA weights in safetensors format

### Manifest Structure

```json
{
  "version": "1.0.0",
  "rank": 16,
  "alpha": 32.0,
  "base_model": "qwen2.5-7b",
  "training_config": {
    "rank": 16,
    "alpha": 32.0,
    "learning_rate": 0.0005,
    "batch_size": 8,
    "epochs": 4,
    "hidden_dim": 3584
  },
  "created_at": "2025-01-19T12:00:00Z",
  "metadata": {}
}
```

**Fields:**
- `version` - Adapter version (default: "1.0.0")
- `rank` - LoRA rank (default: 16)
- `alpha` - LoRA alpha (default: 32.0)
- `base_model` - Base model identifier (default: "qwen2.5-7b")
- `target_modules` - List of modules (default: ["q_proj", "k_proj", "v_proj", "o_proj"])
- `created_at` - ISO 8601 timestamp (auto-generated if missing)
- `training_config` - Optional training parameters
- `metadata` - Optional additional metadata

**Auto-generated fields:**
- `format_version` - Always set to 2 for AOS 2.0
- `adapter_id` - Generated from name or overridden via CLI
- `weights_hash` - BLAKE3 hash of weights

## Examples

### Example 1: Basic Packaging

```bash
$ aos-create adapters/code_lang_v1/ -o code-assistant.aos -v
INFO 📦 Packaging adapters/code_lang_v1/
INFO Weights hash: e7a75704bc81a142...
INFO Adapter ID: default/general/adapter/r001
INFO Rank: 16, Alpha: 32
INFO Base model: qwen2.5-7b
INFO Writing AOS 2.0 archive
INFO ✅ Created code-assistant.aos
INFO    Size: 1.75 MB
INFO    Hash: e7a75704bc81a142...
INFO    ID: default/general/adapter/r001
INFO    Rank: 16
```

### Example 2: Custom Adapter ID

```bash
$ aos-create adapters/code_lang_v1/ -o code-assistant.aos \
  --adapter-id tenant-a/engineering/code-review/r001 \
  --verify -v

INFO Adapter ID: tenant-a/engineering/code-review/r001
INFO ✅ Created code-assistant.aos
INFO 🔍 Verifying code-assistant.aos
INFO ✅ Valid .aos file
INFO    Format version: 2
INFO    Adapter ID: tenant-a/engineering/code-review/r001
INFO    Weights size: 1.75 MB
INFO    Hash verified: e7a75704bc81a142...
```

### Example 3: Integration with Training Pipeline

```bash
# Train adapter
cargo run --bin aosctl -- train \
  --dataset my-dataset \
  --output adapters/my_adapter/

# Package into .aos
aos-create adapters/my_adapter/ \
  --adapter-id tenant-a/ml/my-adapter/r001 \
  --verify

# Register with system
cargo run --bin aosctl -- register \
  --path adapters/my-adapter.aos
```

## Compatibility

### Format Compatibility

The `aos-create` tool produces standard AOS 2.0 binary archives:

- ✅ AOS 2.0 binary format specification
- ✅ Compatible manifest schema
- ✅ BLAKE3 hashing for integrity
- ✅ Verifiable with `aos-info` and `aos-verify` tools
- ✅ Works with existing training pipeline
- ✅ Compatible with AdapterOS lifecycle management

### Usage Examples

```bash
# Create .aos file from adapter directory
aos-create adapters/my_adapter/ -o output.aos -v

# Using cargo (if binary not installed)
cargo run --bin aos-create -- adapters/my_adapter/ -o output.aos -v

# Create with custom manifest
aos-create --weights weights.safetensors --manifest manifest.json -o output.aos
```

## Testing

### Unit Tests

```bash
cargo test -p adapteros-aos --bin aos-create
```

**Test coverage:**
- Manifest generation and validation
- BLAKE3 hashing
- Adapter ID generation
- Semantic naming validation

### Integration Tests

```bash
# Create test archive
aos-create adapters/code_lang_v1/ -o /tmp/test.aos --verify

# Verify with aos-info
aos-info /tmp/test.aos

# Compare with Python version
python scripts/create_aos_adapter.py adapters/code_lang_v1/ -o /tmp/python.aos
diff <(aos-info /tmp/test.aos) <(aos-info /tmp/python.aos)
```

## Performance

The Rust implementation offers several advantages:

- **Faster execution:** Native binary vs Python interpreter
- **Memory efficient:** Streaming I/O for large files
- **Type safety:** Compile-time checks prevent runtime errors
- **Better integration:** Part of the Rust build system

Benchmark comparison (code_lang_v1 adapter, 1.75 MB):

| Tool | Time | Notes |
|------|------|-------|
| Python | ~0.15s | With fallback SHA256 |
| Rust | ~0.05s | Native BLAKE3 |

## Error Handling

The tool provides clear error messages:

```bash
# Missing manifest
$ aos-create adapters/invalid/
Error: Validation error: Missing manifest.json in adapters/invalid

# Invalid adapter ID format
$ aos-create adapters/my_adapter/ --adapter-id invalid-format
Error: Validation error: adapter_id must follow tenant/domain/purpose/revision format

# Hash mismatch during verification
$ aos-create adapters/corrupted/ --verify
Error: Validation error: Hash mismatch: computed abc123... != stored def456...
```

## Future Enhancements

Planned features:

1. **ZIP format support:** Alternative to binary format for compatibility
2. **Compression:** Optional weight compression
3. **Signature support:** Ed25519 signing of archives
4. **Batch processing:** Create multiple archives in one command
5. **Template support:** Predefined manifest templates

## References

- [AOS 2.0 Format Specification](/Users/star/Dev/aos/docs/AOS_V2_ACTUAL_FORMAT.md)
- [Training Pipeline](/Users/star/Dev/aos/docs/TRAINING_PIPELINE.md)
- [Adapter Packager](/Users/star/Dev/aos/crates/adapteros-lora-worker/src/training/packager.rs)
- [AOS2Writer Implementation](/Users/star/Dev/aos/crates/adapteros-aos/src/aos2_writer.rs)

---

**Copyright:** © 2025 JKCA / James KC Auchterlonie. All rights reserved.
**Last Updated:** 2025-11-19
**Maintained by:** James KC Auchterlonie
