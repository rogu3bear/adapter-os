# Single-File Adapter Format (.aos)

<!-- ============================================================================
AOS COORDINATION HEADER
============================================================================
File: docs/training/aos_adapters.md
Phase: 3 - Advanced Features (Documentation)
Assigned: Intern H (Documentation Team)
Status: Complete - Documentation implemented
Dependencies: SingleFileAdapter, CLI commands, UI components
Last Updated: 2025-11-22

COORDINATION NOTES:
- This file affects: User documentation, API documentation, training guides
- Changes require: Updates when SingleFileAdapter format changes
- Testing needed: Documentation accuracy tests, user workflow validation
- CLI Impact: Documents CLI command usage and examples
- UI Impact: Documents UI component usage and workflows
- Database Impact: Documents database schema and migration procedures
============================================================================ -->

The `.aos` format provides a self-contained adapter package that includes:
- LoRA weights (SafeTensors or Q15 quantized)
- Training data (JSONL format)
- Configuration (TOML captured in metadata)
- Lineage tracking (JSON format)
- Cryptographic signatures (Ed25519)
- Weight-group metadata (manifest + disk info)

## Format Specification

The `.aos` format uses a unified 64-byte header with cache-aligned layout for optimal zero-copy loading:

| Offset | Size | Field | Purpose |
| ------ | ---- | ----- | ------- |
| 0-3 | 4 bytes | Magic: `AOS\x00` | Format identifier |
| 4-7 | 4 bytes | Flags | Reserved (u32 LE) |
| 8-15 | 8 bytes | Weights offset | Position of weight data (u64 LE) |
| 16-23 | 8 bytes | Weights size | Size of weight data (u64 LE) |
| 24-31 | 8 bytes | Manifest offset | Position of manifest JSON (u64 LE) |
| 32-39 | 8 bytes | Manifest size | Size of manifest JSON (u64 LE) |
| 40-63 | 24 bytes | Reserved | Padding/future use |

## Benefits

- **Self-contained**: All adapter components in a single file
- **Portable**: Easy to share and deploy across environments
- **Versioned**: Built-in lineage tracking and version management
- **Signed**: Cryptographic integrity verification (Ed25519)
- **Efficient**: Binary format with optional weight quantization
- **Mmap-ready**: 64-byte aligned header for zero-copy memory-mapped loading
- **Compatible**: Works seamlessly with existing AdapterOS infrastructure

## Creating .aos Files

### Command Line

```bash
# Package adapter to .aos file
aosctl adapter package ./weights.safetensors --manifest manifest.json -o adapter.aos

# Create with signing
aosctl adapter package \
  ./weights.safetensors \
  --manifest manifest.json \
  --sign \
  -o adapter.aos
```

### From Training Pipeline

```bash
# Train and package in one step
cargo xtask train-base-adapter \
  --manifest training/datasets/base/code/adapteros/manifest.json \
  --tokenizer models/qwen2.5-7b-mlx/tokenizer.json \
  --output-format aos \
  --output code_lang_v1.aos \
  --adapter-id code_lang_v1
```

### Programmatic (Rust)

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

    // Write 64-byte header (cache-aligned)
    file.write_all(&AOS_MAGIC)?;                           // 0-3: magic
    file.write_all(&0u32.to_le_bytes())?;                  // 4-7: flags
    file.write_all(&weights_offset.to_le_bytes())?;        // 8-15: weights_offset
    file.write_all(&weights_size.to_le_bytes())?;          // 16-23: weights_size
    file.write_all(&manifest_offset.to_le_bytes())?;       // 24-31: manifest_offset
    file.write_all(&manifest_size.to_le_bytes())?;         // 32-39: manifest_size
    file.write_all(&[0u8; 24])?;                           // 40-63: reserved

    // Write weights and manifest
    file.write_all(weights_data)?;
    file.write_all(manifest_json)?;

    Ok(())
}
```

## Loading .aos Files

The loader validates `.aos` files by checking magic bytes and reading the 64-byte header.

### Command Line

```bash
# Load and register adapter
aosctl adapter load --path code_lang_v1.aos

# Load with custom adapter ID
aosctl adapter load \
  --path code_lang_v1.aos \
  --adapter-id my_custom_id
```

### Via Lifecycle Manager

```rust
use adapteros_lora_lifecycle::LifecycleManager;

// Load .aos adapter
lifecycle.load_aos_adapter(0, "code_lang_v1.aos").await?;
```

### Direct Loader API

```rust
use adapteros_aos::AosLoader;

let adapter = AosLoader::load(path)
    .await?;

println!("Adapter: {}", adapter.manifest.adapter_id);
println!("Weights size: {} bytes", adapter.weights.len());
```

## Verifying .aos Files

### Command Line

```bash
# Verify integrity
aosctl adapter validate adapter.aos

# Verify with JSON output
aosctl adapter validate adapter.aos --format json
```

### Programmatic

```rust
use adapteros_aos::AosLoader;

match AosLoader::load("adapter.aos").await {
    Ok(adapter) => {
        println!("Adapter is valid");
        println!("ID: {}", adapter.manifest.adapter_id);
    }
    Err(e) => {
        println!("Error: {}", e);
    }
}
```

## Format Detection

The loader validates the `.aos` format by checking magic bytes at file start:

```rust
fn is_valid_aos_file(data: &[u8]) -> bool {
    if data.len() < 64 {
        return false;
    }

    // Check magic bytes "AOS\x00"
    &data[0..4] == b"AOS\x00"
}
```

The header provides all metadata needed for zero-copy loading via memory mapping.

## Extracting Components

```bash
# Extract all components
aosctl adapter extract \
  --path adapter.aos \
  --output-dir extracted/

# Extract specific components
aosctl adapter extract \
  --path adapter.aos \
  --output-dir extracted/ \
  --components weights,manifest
```

## File Structure

### Binary Layout (64-byte cache-aligned header)

```
adapter.aos
+--------+--------+------------------------------------------+
| Offset | Size   | Field                                    |
+--------+--------+------------------------------------------+
| 0-3    | 4      | Magic bytes: "AOS\x00"                   |
| 4-7    | 4      | Flags (u32 LE, reserved)                 |
| 8-15   | 8      | Weights offset (u64 LE)                  |
| 16-23  | 8      | Weights size (u64 LE)                    |
| 24-31  | 8      | Manifest offset (u64 LE)                 |
| 32-39  | 8      | Manifest size (u64 LE)                   |
| 40-63  | 24     | Reserved (padding)                       |
+--------+--------+------------------------------------------+
| 64+    | N      | Weights (SafeTensors or Q15)             |
| offset | M      | Manifest (JSON metadata)                 |
+--------+--------+------------------------------------------+
```

### Manifest Format

```json
{
  "adapter_id": "tenant-a/engineering/code-review/r001",
  "name": "Code Review Assistant",
  "version": "1.0.0",
  "rank": 16,
  "alpha": 32.0,
  "base_model": "qwen2.5-7b",
  "category": "code",
  "tier": "persistent",
  "target_modules": ["q_proj", "k_proj", "v_proj", "o_proj"],
  "created_at": "2025-01-15T10:30:00Z",
  "weights_hash": "e7a75704bc81a1427ea880e1425e67c6b97367da0634f687a43ed45a37d63e29",
  "training_config": {
    "rank": 16,
    "alpha": 32.0,
    "learning_rate": 0.0005,
    "batch_size": 8
  },
  "metadata": {
    "description": "Optimized for code review tasks"
  }
}
```

### Lineage Format

```json
{
  "adapter_id": "code_lang_v1",
  "version": "1.0.0",
  "parent_version": null,
  "parent_hash": null,
  "mutations": [],
  "quality_delta": 0.0,
  "created_at": "2025-01-15T10:30:00Z"
}
```

## Best Practices

### Signing

Always sign production adapters:

```bash
aosctl adapter package \
  ./weights.safetensors \
  --manifest manifest.json \
  --sign \
  -o prod_adapter.aos
```

### Adapter Versioning

Use semantic versioning in the manifest:
- **Major version**: Breaking changes to adapter behavior
- **Minor version**: New training data or capabilities
- **Patch version**: Bug fixes or minor improvements

Keep the adapter ID stable while incrementing the version field in the manifest.

### Storage

Organize .aos files by version:

```
adapters/
  code_lang_v1/
    1.0.0.aos
    1.1.0.aos
    latest.aos -> 1.1.0.aos
  rust_framework/
    1.0.0.aos
    latest.aos -> 1.0.0.aos
  tenant_001_codebase/
    1.0.0.aos
    1.1.0.aos
    latest.aos -> 1.1.0.aos
```

## Integration with Existing Systems

### AdapterLoader

```rust
let mut loader = AdapterLoader::new(PathBuf::from("./adapters"));
let handle = loader.load_aos_adapter(0, "adapter.aos").await?;
```

### Registry

```rust
use adapteros_registry::{AosAdapterMetadata, AosAdapterRegistry};

let metadata = AosAdapterMetadata::new(
    "code_lang_v1".to_string(),
    "adapters/code_lang_v1.aos".to_string(),
    "e7a75704...".to_string(),
    Some(100),
    Some("1.0.0".to_string()),
    true,
);

registry.register_aos_adapter(metadata)?;
```

### Lifecycle Management

```rust
lifecycle.load_aos_adapter(0, "adapter.aos").await?;
lifecycle.promote_adapter(0).await?;
lifecycle.demote_adapter(0).await?;
```

## Troubleshooting

### Verification Failures

1. Check file integrity:
   ```bash
   aosctl adapter validate adapter.aos --format json
   ```

2. Extract and inspect components:
   ```bash
   aosctl adapter extract --path adapter.aos --output-dir debug/
   ```

3. Validate manifest:
   ```bash
   cat debug/manifest.json | jq .
   ```

### Loading Errors

1. Check adapter compatibility with base model
2. Verify weights format (SafeTensors or Q15)
3. Check lineage version compatibility

### Performance Issues

1. Check file size (should be < 500MB)
2. Ensure adequate disk space for extraction
3. Consider using SSD for adapter storage
4. Monitor memory usage during loading

## References

- [AOS Format Specification](../AOS_FORMAT.md)
- [Training the AdapterOS Base Code Adapter](base_adapter.md)
- [Adapter Lifecycle Management](../database-schema/workflows/ADAPTER-LIFECYCLE.md)
- [Registry Schema](../database-schema/SCHEMA-DIAGRAM.md)
