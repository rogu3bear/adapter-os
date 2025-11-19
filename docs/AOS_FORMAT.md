# AOS (AdapterOS Single-file) Format Specification

## Overview

The `.aos` format is a single-file archive format for packaging LoRA adapters in AdapterOS. It provides a compact, self-contained, and versioned format for distributing adapter models.

## Format Version

Current version: **2.0**

## Binary Structure

The AOS 2.0 format uses a simple binary layout optimized for memory-mapped loading:

```
[Bytes 0-3]    manifest_offset (u32, little-endian)
[Bytes 4-7]    manifest_len (u32, little-endian)
[Bytes 8...]   weights_data (safetensors format or Q15 quantized)
[manifest_offset...] manifest (JSON metadata)
```

### Header (8 bytes)

- **manifest_offset** (4 bytes): Byte offset where the manifest JSON begins
- **manifest_len** (4 bytes): Length of the manifest JSON in bytes

Both values are stored as unsigned 32-bit integers in little-endian format.

### Weights Section

Starts at byte 8 and continues until `manifest_offset`. Contains the adapter weights in one of two formats:

1. **SafeTensors format** (for compatibility)
2. **Q15 quantized format** (for Metal kernels)

The weights include:
- `lora_a` matrices for each target module
- `lora_b` matrices for each target module

### Manifest Section

JSON metadata stored at the tail of the file, starting at `manifest_offset` with length `manifest_len`.

## Manifest Schema

```json
{
  "format_version": 2,
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
  "weights_hash": "blake3_hash_hex",
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

- **format_version**: Must be 2 for AOS 2.0
- **adapter_id**: Semantic name following `{tenant}/{domain}/{purpose}/{revision}` pattern
- **name**: Human-readable display name
- **version**: Semantic version (e.g., "1.0.0")
- **rank**: LoRA rank (typically 8-32)
- **alpha**: LoRA scaling factor (typically 2x rank)
- **base_model**: Base model identifier
- **target_modules**: List of model layers to apply adapter to
- **created_at**: ISO 8601 timestamp
- **weights_hash**: BLAKE3 hash of weights data (hex-encoded)

### Optional Fields

- **category**: Adapter category (code, documentation, creative, etc.)
- **tier**: Lifecycle tier (persistent, ephemeral)
- **training_config**: Training hyperparameters
- **metadata**: Additional key-value pairs

## Weight Formats

### SafeTensors Format

Standard SafeTensors format with tensors named:
- `lora_a.{module}` - A matrix for each target module
- `lora_b.{module}` - B matrix for each target module

Example tensor names:
- `lora_a.q_proj`, `lora_b.q_proj` (Query projection)
- `lora_a.k_proj`, `lora_b.k_proj` (Key projection)
- `lora_a.v_proj`, `lora_b.v_proj` (Value projection)
- `lora_a.o_proj`, `lora_b.o_proj` (Output projection)

### Q15 Quantized Format

For Metal kernel optimization, weights can be quantized to signed 16-bit integers:
- Range: -32768 to 32767
- Scale: ±1.0 maps to ±32767
- Dequantization: `float_value = q15_value / 32767.0`

## Creating AOS Files

### Python Example

```python
import struct
import json
import hashlib

def create_aos_file(weights_data, manifest, output_path):
    # Serialize manifest to JSON
    manifest_json = json.dumps(manifest, indent=2).encode('utf-8')

    # Calculate offsets
    header_size = 8
    weights_offset = header_size
    manifest_offset = weights_offset + len(weights_data)
    manifest_len = len(manifest_json)

    # Write file
    with open(output_path, 'wb') as f:
        # Write header
        f.write(struct.pack('<II', manifest_offset, manifest_len))

        # Write weights
        f.write(weights_data)

        # Write manifest
        f.write(manifest_json)
```

### Rust Example

See `crates/adapteros-aos/src/aos2_writer.rs` for the reference implementation.

## Loading AOS Files

### Python Example

```python
import struct
import json

def load_aos_file(file_path):
    with open(file_path, 'rb') as f:
        # Read header
        manifest_offset, manifest_len = struct.unpack('<II', f.read(8))

        # Read weights
        weights_size = manifest_offset - 8
        weights_data = f.read(weights_size)

        # Read manifest
        manifest_json = f.read(manifest_len)
        manifest = json.loads(manifest_json)

    return weights_data, manifest
```

### Memory Mapping (Zero-Copy)

The format is designed for efficient memory-mapped loading:
1. Map file into memory
2. Parse 8-byte header to find manifest location
3. Parse manifest JSON
4. Access weights directly via memory mapping

## Hash Verification

The `weights_hash` field in the manifest should be the BLAKE3 hash of the weights data:

```python
import blake3

def verify_aos_file(file_path):
    weights_data, manifest = load_aos_file(file_path)

    # Compute hash of weights
    computed_hash = blake3.blake3(weights_data).hexdigest()

    # Compare with manifest
    return computed_hash == manifest.get('weights_hash')
```

## File Size Limits

- Maximum file size: 4 GB (due to 32-bit offsets)
- Typical size: 100 KB - 10 MB for LoRA adapters
- Recommended maximum: 500 MB

## Compatibility

### Version Detection

Check the `format_version` field in the manifest:
- Version 1: Legacy ZIP-based format (deprecated)
- Version 2: Current binary format (this specification)

### Backward Compatibility

Loaders should check the format version and handle appropriately:
```python
if manifest['format_version'] != 2:
    raise ValueError(f"Unsupported format version: {manifest['format_version']}")
```

## Security Considerations

### Hash Verification

Always verify the `weights_hash` to detect corruption or tampering:
- Use BLAKE3 for fast, secure hashing
- Reject files with mismatched hashes

### Size Validation

Validate sizes before allocation:
- Check that `manifest_offset + manifest_len <= file_size`
- Limit maximum file size to prevent DoS

### Manifest Validation

- Validate all required fields are present
- Check semantic naming conventions
- Verify version compatibility

## Examples

### Minimal Manifest

```json
{
  "format_version": 2,
  "adapter_id": "default/test/example/r001",
  "name": "Example Adapter",
  "version": "1.0.0",
  "rank": 16,
  "alpha": 32.0,
  "base_model": "qwen2.5-7b",
  "target_modules": ["q_proj", "k_proj", "v_proj", "o_proj"],
  "created_at": "2025-01-18T12:00:00Z",
  "weights_hash": "0123456789abcdef..."
}
```

### Full Manifest with Metadata

```json
{
  "format_version": 2,
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

## Tools and Utilities

### Command Line

```bash
# Package adapter directory to .aos file
python scripts/create_aos_adapter.py adapters/my_adapter/ -o my_adapter.aos

# Validate .aos file
python scripts/validate_aos.py my_adapter.aos

# Extract manifest
python -c "import struct, json; f=open('adapter.aos','rb'); o,l=struct.unpack('<II',f.read(8)); f.seek(o); print(json.loads(f.read(l)))"
```

### Integration

The AOS format integrates with:
- AdapterOS lifecycle management
- Metal kernel execution
- Database registration
- Hot-swap mechanisms

## References

- Implementation: `crates/adapteros-aos/src/aos2_writer.rs`
- Loader: `crates/adapteros-aos/src/aos2_implementation.rs`
- Packager: `crates/adapteros-lora-worker/src/training/packager.rs`
- Tests: `test_data/adapters/*.aos`