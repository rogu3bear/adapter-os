# AOS (AdapterOS Single-file) Format Specification

## Overview

The `.aos` format is a single-file binary archive format for packaging LoRA adapters in AdapterOS. It provides a compact, self-contained format for distributing adapter models with zero-copy memory-mapped loading.

## Binary Structure

The AOS format uses a 64-byte header for optimal memory alignment and cache efficiency:

```
+--------+--------+------------------------------------------+
| Offset | Size   | Field                                    |
+--------+--------+------------------------------------------+
| 0      | 4      | Magic bytes: "AOS\x00"                   |
| 4      | 4      | Flags (u32 LE, reserved)                 |
| 8      | 8      | Weights offset (u64 LE)                  |
| 16     | 8      | Weights size (u64 LE)                    |
| 24     | 8      | Manifest offset (u64 LE)                 |
| 32     | 8      | Manifest size (u64 LE)                   |
| 40     | 24     | Reserved (padding to 64 bytes)           |
+--------+--------+------------------------------------------+
| 64     | N      | Weights data (SafeTensors or Q15)        |
| 64+N   | M      | Manifest (JSON metadata)                 |
+--------+--------+------------------------------------------+
```

### Header Fields (64 bytes)

| Field | Offset | Size | Type | Description |
|-------|--------|------|------|-------------|
| `magic` | 0 | 4 | bytes | Magic identifier `AOS\x00` |
| `flags` | 4 | 4 | u32 LE | Reserved for future use (must be 0) |
| `weights_offset` | 8 | 8 | u64 LE | Byte offset where weights begin |
| `weights_size` | 16 | 8 | u64 LE | Size of weights data in bytes |
| `manifest_offset` | 24 | 8 | u64 LE | Byte offset where manifest begins |
| `manifest_size` | 32 | 8 | u64 LE | Size of manifest JSON in bytes |
| `reserved` | 40 | 24 | bytes | Reserved padding (must be zeros) |

### Design Rationale

- **64-byte alignment**: Matches CPU cache line size for optimal memory access
- **8-byte fields**: Natural alignment for 64-bit systems, supports files up to 16 EB
- **Magic bytes**: Enables reliable format detection
- **Reserved fields**: Future-proofs the format without breaking compatibility

### Weights Section

Starts at `weights_offset` (typically byte 64) with size `weights_size`. Contains adapter weights in one of:

1. **SafeTensors format** (recommended for compatibility)
2. **Q15 quantized format** (optimized for Metal kernels)

Tensor naming convention:
- `lora_a.{module}` - A matrix for each target module
- `lora_b.{module}` - B matrix for each target module

### Manifest Section

JSON metadata starting at `manifest_offset` with size `manifest_size`. Must be valid UTF-8.

## Manifest Schema

```json
{
  "adapter_id": "tenant-a/domain/purpose/r001",
  "name": "Human-readable adapter name",
  "version": "1.0.0",
  "rank": 16,
  "alpha": 32.0,
  "base_model": "qwen2.5-7b",
  "target_modules": ["q_proj", "k_proj", "v_proj", "o_proj"],
  "category": "code",
  "tier": "persistent",
  "created_at": "2025-01-18T12:00:00Z",
  "weights_hash": "blake3_64char_hex_string",
  "training_config": {
    "rank": 16,
    "alpha": 32.0,
    "learning_rate": 0.0005,
    "batch_size": 8,
    "epochs": 4,
    "hidden_dim": 3584
  },
  "metadata": {
    "description": "Optional description",
    "use_cases": ["code completion", "debugging"]
  }
}
```

### Required Fields

| Field | Type | Description |
|-------|------|-------------|
| `adapter_id` | string | Semantic name: `{tenant}/{domain}/{purpose}/{revision}` |
| `name` | string | Human-readable display name |
| `version` | string | Semantic version (e.g., "1.0.0") |
| `rank` | integer | LoRA rank (typically 8-32) |
| `alpha` | float | LoRA scaling factor (typically 2x rank) |
| `base_model` | string | Base model identifier |
| `target_modules` | array | List of model layers for adapter application |
| `created_at` | string | ISO 8601 timestamp |
| `weights_hash` | string | BLAKE3 hash of weights data (64 hex chars) |

### Optional Fields

| Field | Type | Description |
|-------|------|-------------|
| `category` | string | Adapter category (code, documentation, creative) |
| `tier` | string | Lifecycle tier (persistent, ephemeral) |
| `training_config` | object | Training hyperparameters |
| `metadata` | object | Additional key-value pairs |

## Weight Formats

### SafeTensors Format

Standard SafeTensors format with tensors named:
- `lora_a.q_proj`, `lora_b.q_proj` (Query projection)
- `lora_a.k_proj`, `lora_b.k_proj` (Key projection)
- `lora_a.v_proj`, `lora_b.v_proj` (Value projection)
- `lora_a.o_proj`, `lora_b.o_proj` (Output projection)

### Q15 Quantized Format

For Metal kernel optimization, weights quantized to signed 16-bit integers:
- Range: -32768 to 32767
- Scale: +/-1.0 maps to +/-32767
- Dequantization: `float_value = q15_value / 32767.0`

## Creating AOS Files

### Rust Example

```rust
use std::io::Write;

const AOS_MAGIC: [u8; 4] = *b"AOS\x00";
const HEADER_SIZE: u64 = 64;

fn create_aos_file(
    weights_data: &[u8],
    manifest_json: &[u8],
    output_path: &std::path::Path,
) -> std::io::Result<()> {
    let mut file = std::fs::File::create(output_path)?;

    let weights_offset = HEADER_SIZE;
    let weights_size = weights_data.len() as u64;
    let manifest_offset = weights_offset + weights_size;
    let manifest_size = manifest_json.len() as u64;

    // Write 64-byte header
    file.write_all(&AOS_MAGIC)?;                            // 0-3: magic
    file.write_all(&0u32.to_le_bytes())?;                   // 4-7: flags
    file.write_all(&weights_offset.to_le_bytes())?;         // 8-15: weights_offset
    file.write_all(&weights_size.to_le_bytes())?;           // 16-23: weights_size
    file.write_all(&manifest_offset.to_le_bytes())?;        // 24-31: manifest_offset
    file.write_all(&manifest_size.to_le_bytes())?;          // 32-39: manifest_size
    file.write_all(&[0u8; 24])?;                            // 40-63: reserved

    // Write weights
    file.write_all(weights_data)?;

    // Write manifest
    file.write_all(manifest_json)?;

    Ok(())
}
```

### Implementation References

- Writer: `crates/adapteros-aos/src/writer.rs`
- Loader: `crates/adapteros-aos/src/mmap_loader.rs`
- Implementation: `crates/adapteros-aos/src/implementation.rs`
- Packager: `crates/adapteros-lora-worker/src/training/packager.rs`

## Loading AOS Files

### Rust Example

```rust
const AOS_MAGIC: [u8; 4] = *b"AOS\x00";

struct AosHeader {
    flags: u32,
    weights_offset: u64,
    weights_size: u64,
    manifest_offset: u64,
    manifest_size: u64,
}

fn load_aos_header(data: &[u8]) -> Result<AosHeader, &'static str> {
    if data.len() < 64 {
        return Err("File too small for AOS header");
    }

    // Verify magic bytes
    if &data[0..4] != &AOS_MAGIC {
        return Err("Invalid AOS magic bytes");
    }

    Ok(AosHeader {
        flags: u32::from_le_bytes(data[4..8].try_into().unwrap()),
        weights_offset: u64::from_le_bytes(data[8..16].try_into().unwrap()),
        weights_size: u64::from_le_bytes(data[16..24].try_into().unwrap()),
        manifest_offset: u64::from_le_bytes(data[24..32].try_into().unwrap()),
        manifest_size: u64::from_le_bytes(data[32..40].try_into().unwrap()),
    })
}
```

### Memory Mapping (Zero-Copy)

The format is optimized for memory-mapped loading:
1. Map file into memory
2. Parse 64-byte header
3. Access weights directly via pointer offset
4. Parse manifest JSON from mapped memory

```rust
use memmap2::Mmap;

fn load_aos_mmap(path: &std::path::Path) -> std::io::Result<(Mmap, AosHeader)> {
    let file = std::fs::File::open(path)?;
    let mmap = unsafe { Mmap::map(&file)? };
    let header = load_aos_header(&mmap).map_err(|e|
        std::io::Error::new(std::io::ErrorKind::InvalidData, e)
    )?;
    Ok((mmap, header))
}
```

## Hash Verification

The `weights_hash` field must contain the BLAKE3 hash of the weights data:

```rust
use blake3::Hasher;

fn verify_weights_hash(weights_data: &[u8], expected_hash: &str) -> bool {
    let computed = blake3::hash(weights_data);
    computed.to_hex().as_str() == expected_hash
}
```

## File Size Limits

| Limit | Value | Notes |
|-------|-------|-------|
| Maximum file size | 16 EB | Limited by u64 offsets |
| Recommended maximum | 500 MB | For reasonable load times |
| Typical size | 100 KB - 10 MB | Standard LoRA adapters |
| Header size | 64 bytes | Fixed, cache-line aligned |

## Format Detection

Detect AOS format by checking magic bytes:

```rust
fn detect_aos_format(data: &[u8]) -> bool {
    if data.len() < 4 {
        return false;
    }
    &data[0..4] == b"AOS\x00"
}
```

## Security Considerations

### Hash Verification

Always verify `weights_hash` before using weights:
- Use BLAKE3 for fast, secure hashing
- Reject files with mismatched hashes
- Log verification failures for audit

### Size Validation

Validate sizes before memory allocation:
```rust
fn validate_aos_header(header: &AosHeader, file_size: u64) -> Result<(), &'static str> {
    if header.weights_offset + header.weights_size > file_size {
        return Err("Weights extend beyond file");
    }
    if header.manifest_offset + header.manifest_size > file_size {
        return Err("Manifest extends beyond file");
    }
    if header.manifest_size > 10 * 1024 * 1024 {
        return Err("Manifest too large (>10MB)");
    }
    Ok(())
}
```

### Manifest Validation

- Validate all required fields are present
- Check semantic naming conventions
- Sanitize string inputs

## Examples

### Minimal Manifest

```json
{
  "adapter_id": "default/test/example/r001",
  "name": "Example Adapter",
  "version": "1.0.0",
  "rank": 16,
  "alpha": 32.0,
  "base_model": "qwen2.5-7b",
  "target_modules": ["q_proj", "k_proj", "v_proj", "o_proj"],
  "created_at": "2025-01-18T12:00:00Z",
  "weights_hash": "e7a75704bc81a1427ea880e1425e67c6b97367da0634f687a43ed45a37d63e29"
}
```

### Full Manifest with Metadata

```json
{
  "adapter_id": "acme-corp/engineering/code-review/r001",
  "name": "Code Review Assistant",
  "version": "2.1.0",
  "rank": 16,
  "alpha": 32.0,
  "base_model": "qwen2.5-7b",
  "target_modules": ["q_proj", "k_proj", "v_proj", "o_proj"],
  "category": "code",
  "tier": "persistent",
  "created_at": "2025-01-18T12:00:00Z",
  "weights_hash": "e7a75704bc81a1427ea880e1425e67c6b97367da0634f687a43ed45a37d63e29",
  "training_config": {
    "rank": 16,
    "alpha": 32.0,
    "learning_rate": 0.0005,
    "batch_size": 8,
    "epochs": 4,
    "hidden_dim": 3584,
    "dropout": 0.1
  },
  "metadata": {
    "description": "Optimized for code review and analysis tasks",
    "use_cases": [
      "Pull request review",
      "Code quality assessment",
      "Bug detection",
      "Security vulnerability scanning"
    ],
    "training_examples": 10000,
    "validation_accuracy": 0.92
  }
}
```

## CLI Integration

```bash
# Package adapter to .aos file
aosctl adapter package ./weights.safetensors --manifest manifest.json -o adapter.aos

# Validate .aos file
aosctl adapter validate adapter.aos

# Show .aos file info
aosctl adapter info adapter.aos

# Extract manifest
aosctl adapter manifest adapter.aos
```

## Integration Points

The AOS format integrates with:
- **Lifecycle Management**: `adapteros-lora-lifecycle` for state transitions
- **Hot-Swap**: `adapteros-aos/hot_swap.rs` for live adapter replacement
- **Memory Mapping**: `adapteros-aos/mmap_loader.rs` for zero-copy loading
- **Metal Kernels**: Direct GPU VRAM transfer for Q15 weights
- **Database Registration**: Content-addressed storage via BLAKE3 hash
- **Federation**: Peer-to-peer adapter distribution

## References

- Writer: `crates/adapteros-aos/src/writer.rs`
- Implementation: `crates/adapteros-aos/src/implementation.rs`
- Memory-mapped loader: `crates/adapteros-aos/src/mmap_loader.rs`
- Training packager: `crates/adapteros-lora-worker/src/training/packager.rs`
- Architecture overview: `docs/architecture/aos_filetype_architecture.md`

---

**Last Updated**: 2025-11-22
