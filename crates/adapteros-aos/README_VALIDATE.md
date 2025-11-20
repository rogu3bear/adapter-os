# aos-validate

Comprehensive validation tool for AOS (Adapter Object Store) archive files. Replaces the Python `validate_aos.py` script with a production-ready Rust implementation.

## Overview

`aos-validate` performs deep validation of `.aos` archive files to ensure they are production-ready and comply with AdapterOS specifications. The tool validates file structure, manifest schema, semantic naming, cryptographic hashes, and tensor metadata.

## Installation

```bash
cargo build --release -p adapteros-aos --bin aos-validate
```

The binary will be available at `target/release/aos-validate`.

## Usage

### Basic Validation

```bash
aos-validate path/to/adapter.aos
```

### Verbose Mode (Show All Checks)

```bash
aos-validate path/to/adapter.aos --verbose
```

### JSON Output (CI/CD Integration)

```bash
aos-validate path/to/adapter.aos --json
```

### Performance Options

```bash
# Skip tensor validation (faster)
aos-validate path/to/adapter.aos --skip-tensors

# Skip BLAKE3 hash verification
aos-validate path/to/adapter.aos --skip-hash

# Combine options
aos-validate path/to/adapter.aos --skip-tensors --skip-hash --json
```

## Validation Checks

### Critical Checks (Must Pass)

1. **File Access** - File exists and is readable
2. **File Size** - Within valid range (16 bytes to 10 GB)
3. **Header Format** - Valid 8-byte header with correct offsets
4. **Manifest JSON** - Parseable JSON structure
5. **Manifest Schema** - Required fields present (`version`)
6. **Format Version** - Compatible version (2.0)
7. **LoRA Rank** - Valid range (1-256)
8. **File Integrity** - Size matches header offsets
9. **Safetensors Format** - Valid safetensors header
10. **Tensor Metadata** - All tensors have dtype, shape, data_offsets

### Optional Checks (Warnings)

1. **Semantic Naming** - Follows `{tenant}/{domain}/{purpose}/{revision}` convention
2. **LoRA Alpha** - Reasonable value (1-512)
3. **Target Modules** - At least one module specified
4. **BLAKE3 Hash** - Matches manifest hash if present

## Validation Output

### Human-Readable Format (Default)

```
AOS File Validation
======================================================================
File: adapters/creative-writer.aos

✓ VALIDATION PASSED

Summary
----------------------------------------------------------------------
  Total checks: 12
  ✓ Passed: 12
  File size: 1.23 MB

Validation Checks
----------------------------------------------------------------------
┌────────────────────┬────────┬──────────────────────────────────┐
│ Check              │ Status │ Result                           │
├────────────────────┼────────┼──────────────────────────────────┤
│ File Size          │ ✓      │ 1.23 MB                          │
│ Header Format      │ ✓      │ Valid (offset=1234, len=567)     │
│ Manifest JSON      │ ✓      │ Valid JSON                       │
│ Format Version     │ ✓      │ 2.0 (current)                    │
│ BLAKE3 Hash        │ ✓      │ Hash verified                    │
│ File Integrity     │ ✓      │ File structure is consistent     │
└────────────────────┴────────┴──────────────────────────────────┘
```

### JSON Format (--json)

```json
{
  "file_path": "adapters/creative-writer.aos",
  "valid": true,
  "summary": {
    "total_checks": 12,
    "passed": 12,
    "failed": 0,
    "warnings": 0,
    "file_size_bytes": 1289000,
    "manifest_valid": true,
    "weights_valid": true
  },
  "checks": [
    {
      "name": "File Access",
      "passed": true,
      "message": "File is readable",
      "severity": "info"
    },
    ...
  ],
  "errors": [],
  "warnings": []
}
```

## Exit Codes

- **0** - All validations passed
- **1** - One or more validations failed

## Validation Details

### File Structure Validation

The tool validates the AOS v2.0 archive format:

```
[0-3]    manifest_offset (u32, little-endian)
[4-7]    manifest_len (u32, little-endian)
[8...]   weights (safetensors format)
[offset] manifest (JSON)
```

### Manifest Schema

Required fields:
- `version` - Format version (must be "2.0")

Optional but validated if present:
- `adapter_id` - Semantic name validation
- `rank` - LoRA rank (1-256)
- `alpha` - LoRA alpha parameter
- `target_modules` - Array of module names
- `weights_hash` - BLAKE3 hash of weights section

### Semantic Naming Convention

Adapter IDs should follow the format:
```
{tenant}/{domain}/{purpose}/{revision}
```

Example: `shop-floor/hydraulics/troubleshooting/r042`

Rules:
- **Tenant**: 2-32 chars, lowercase alphanumeric + hyphens
- **Domain**: 2-48 chars, lowercase alphanumeric + hyphens
- **Purpose**: 2-64 chars, lowercase alphanumeric + hyphens
- **Revision**: `rNNN` format (e.g., r001, r042)
- No consecutive hyphens
- Reserved namespaces rejected:
  - Tenants: `system`, `admin`, `root`, `default`, `test`
  - Domains: `core`, `internal`, `deprecated`

### BLAKE3 Hash Verification

If `weights_hash` is present in the manifest, the tool:
1. Extracts the weights section (bytes 8 to manifest_offset)
2. Computes BLAKE3 hash of the weights data
3. Compares against the expected hash
4. Reports mismatch as a critical error

### Safetensors Format

Validates:
- Header length is within file bounds
- JSON header is parseable
- At least one tensor present
- All tensors have required fields:
  - `dtype` - Data type (F32, F16, etc.)
  - `shape` - Tensor dimensions
  - `data_offsets` - Byte offsets in file

## CI/CD Integration

Use JSON output for automated validation:

```bash
#!/bin/bash
# validate_artifacts.sh

for aos_file in dist/*.aos; do
  if ! aos-validate "$aos_file" --json > "${aos_file}.validation.json"; then
    echo "❌ Validation failed: $aos_file"
    cat "${aos_file}.validation.json"
    exit 1
  fi
  echo "✅ Validated: $aos_file"
done

echo "All artifacts validated successfully"
```

GitHub Actions example:

```yaml
- name: Validate AOS artifacts
  run: |
    cargo build --release --bin aos-validate
    for file in artifacts/*.aos; do
      ./target/release/aos-validate "$file" --json || exit 1
    done
```

## Performance

Validation speed (approximate):
- **Header + Manifest**: < 1ms
- **BLAKE3 Hash**: ~500 MB/s
- **Tensor Metadata**: ~100 MB/s

Use `--skip-tensors` and `--skip-hash` for faster validation when only structural checks are needed.

## Comparison with Python Version

### Advantages over `validate_aos.py`

1. **Performance**: 10-100x faster for large files
2. **Type Safety**: Compile-time guarantees
3. **Integration**: Uses existing AdapterOS types
4. **Maintainability**: Single codebase, no Python dependency
5. **Error Handling**: Structured error messages
6. **CI/CD Ready**: JSON output, proper exit codes

### Feature Parity

| Feature | Python | Rust |
|---------|--------|------|
| File structure validation | ✓ | ✓ |
| Manifest schema validation | ✓ | ✓ |
| Semantic naming validation | ✗ | ✓ |
| BLAKE3 hash verification | ✓ | ✓ |
| Rank/Alpha validation | ✓ | ✓ |
| Tensor metadata validation | ✓ | ✓ |
| JSON output | ✗ | ✓ |
| Verbose mode | ✗ | ✓ |
| Performance options | ✗ | ✓ |

## Examples

### Valid Adapter

```bash
$ aos-validate adapters/shop-floor-hydraulics-troubleshooting-r042.aos

✓ VALIDATION PASSED
  Total checks: 15
  ✓ Passed: 15
  File size: 1.23 MB
```

### Invalid Adapter (Naming)

```bash
$ aos-validate adapters/bad-name.aos

✗ VALIDATION FAILED
  Total checks: 12
  ✓ Passed: 11
  ⚠ Warnings: 1

Warnings
----------------------------------------------------------------------
  ⚠ Invalid semantic name: Invalid adapter name format
```

### Hash Mismatch

```bash
$ aos-validate adapters/corrupted.aos

✗ VALIDATION FAILED

Errors
----------------------------------------------------------------------
  ✗ BLAKE3 hash mismatch
  ✗ Hash mismatch
    Expected: cfa57a8adf876f43, Got: 1234567890abcdef
```

## Related Tools

- **aos-info** - Display adapter metadata
- **aos-verify** - Deep verification with detailed checks
- **aos-create** - Create new AOS archives
- **aos-analyze** - Performance analysis

## References

- [AOS v2.0 Format Specification](../PARSER_IMPLEMENTATION.md)
- [Semantic Naming Guide](../../docs/ADAPTER_TAXONOMY.md)
- [BLAKE3 Hash Standard](https://github.com/BLAKE3-team/BLAKE3)
- [Safetensors Format](https://github.com/huggingface/safetensors)

## Copyright

© 2025 JKCA / James KC Auchterlonie. All rights reserved.
