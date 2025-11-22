# Single-File Adapter Format (.aos)

<!-- ============================================================================
AOS COORDINATION HEADER
============================================================================
File: docs/training/aos_adapters.md
Phase: 3 - Advanced Features (Documentation)
Assigned: Intern H (Documentation Team)
Status: Complete - Documentation implemented
Dependencies: SingleFileAdapter, CLI commands, UI components
Last Updated: 2024-01-15

COORDINATION NOTES:
- This file affects: User documentation, API documentation, training guides
- Changes require: Updates when SingleFileAdapter format changes
- Testing needed: Documentation accuracy tests, user workflow validation
- CLI Impact: Documents CLI command usage and examples
- UI Impact: Documents UI component usage and workflows
- Database Impact: Documents database schema and migration procedures
============================================================================ -->

The `.aos` format provides a self-contained adapter package that includes:
- LoRA weights (safetensors format)
- Training data (JSONL format)
- Configuration (TOML format)
- Lineage tracking (JSON format)
- Cryptographic signatures (Ed25519)

## Benefits

- **Self-contained**: All adapter components in a single file
- **Portable**: Easy to share and deploy across environments
- **Versioned**: Built-in lineage tracking and version management
- **Signed**: Cryptographic integrity verification
- **Efficient**: ZIP compression reduces file size
- **Compatible**: Works seamlessly with existing AdapterOS infrastructure

## Creating .aos Files

### From Existing Adapter

```bash
# Create from packaged adapter directory
aosctl aos create \
  --source adapters/code_lang_v1/weights.safetensors \
  --output code_lang_v1.aos \
  --adapter-id code_lang_v1 \
  --version 1.0.0 \
  --training-data training/datasets/base/code/adapteros/positive.jsonl \
  --config training/configs/base_adapter.toml

# Create with signing
aosctl aos create \
  --source adapters/code_lang_v1/weights.safetensors \
  --output code_lang_v1.aos \
  --adapter-id code_lang_v1 \
  --version 1.0.0 \
  --sign
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

## Loading .aos Files

### Into Registry

```bash
# Load and register adapter
aosctl aos load --path code_lang_v1.aos

# Load with custom adapter ID
aosctl aos load \
  --path code_lang_v1.aos \
  --adapter-id my_custom_id
```

### Via Lifecycle Manager

```rust
use adapteros_lora_lifecycle::LifecycleManager;

// Load .aos adapter
lifecycle.load_aos_adapter(0, "code_lang_v1.aos").await?;
```

## Verifying .aos Files

### Command Line

```bash
# Verify integrity
aosctl aos verify --path code_lang_v1.aos

# Verify with JSON output
aosctl aos verify --path code_lang_v1.aos --format json
```

### Programmatic

```rust
use adapteros_single_file_adapter::SingleFileAdapterValidator;

let result = SingleFileAdapterValidator::validate("code_lang_v1.aos").await?;
if result.is_valid {
    println!("Adapter is valid");
} else {
    for error in result.errors {
        println!("Error: {}", error);
    }
}
```

## Extracting Components

```bash
# Extract all components
aosctl aos extract \
  --path code_lang_v1.aos \
  --output-dir extracted/

# Extract specific components
aosctl aos extract \
  --path code_lang_v1.aos \
  --output-dir extracted/ \
  --components weights,training_data,lineage
```

## File Structure

The `.aos` file is a ZIP container with the following structure:

```
code_lang_v1.aos (ZIP container)
├── manifest.json          # Adapter metadata
├── weights.safetensors    # LoRA weights
├── training_data.jsonl    # Training examples
├── config.toml           # Training configuration
├── lineage.json          # Evolution history
└── signature.sig         # Cryptographic signature (optional)
```

### Manifest Format

```json
{
  "adapter_id": "code_lang_v1",
  "version": "1.0.0",
  "rank": 16,
  "alpha": 32.0,
  "base_model": "qwen2.5-7b",
  "category": "code",
  "scope": "global",
  "tier": "persistent",
  "target_modules": ["q_proj", "k_proj", "v_proj", "o_proj", "gate_proj", "up_proj", "down_proj"],
  "created_at": "2025-01-15T10:30:00Z",
  "weights_hash": "e7a75704bc81a1427ea880e1425e67c6b97367da0634f687a43ed45a37d63e29",
  "training_data_hash": "3f8a9c7e2b1d5f6a8c4e9d7b2a5f8c3e1d4b7a9c6e2f5b8d1a4c7e9b3f6a8c2e",
  "metadata": {}
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

### Versioning

Use semantic versioning for adapter versions:
- **Major version**: Breaking changes to adapter behavior
- **Minor version**: New training data or capabilities
- **Patch version**: Bug fixes or minor improvements

```bash
# Version 1.0.0 - Initial release
aosctl aos create --version 1.0.0 ...

# Version 1.1.0 - Added new training examples
aosctl aos create --version 1.1.0 ...

# Version 2.0.0 - Changed base model
aosctl aos create --version 2.0.0 ...
```

### Signing

Always sign production adapters:

```bash
aosctl aos create \
  --source adapters/prod_adapter/weights.safetensors \
  --output prod_adapter.aos \
  --sign
```

### Storage

Organize .aos files by version:

```
adapters/
├── code_lang_v1/
│   ├── 1.0.0.aos
│   ├── 1.1.0.aos
│   └── latest.aos -> 1.1.0.aos
├── rust_framework/
│   ├── 1.0.0.aos
│   └── latest.aos -> 1.0.0.aos
└── tenant_001_codebase/
    ├── 1.0.0.aos
    ├── 1.1.0.aos
    └── latest.aos -> 1.1.0.aos
```

## Integration with Existing Systems

### AdapterLoader

The `.aos` format integrates seamlessly with the existing `AdapterLoader`:

```rust
// Load .aos adapter
let mut loader = AdapterLoader::new(PathBuf::from("./adapters"));
let handle = loader.load_aos_adapter(0, "code_lang_v1.aos").await?;
```

### Registry

Register .aos adapters in the database:

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

Use with lifecycle manager:

```rust
// Load adapter
lifecycle.load_aos_adapter(0, "code_lang_v1.aos").await?;

// Adapter is now managed like any other adapter
lifecycle.promote_adapter(0).await?;
lifecycle.demote_adapter(0).await?;
```

## Troubleshooting

### Verification Failures

If verification fails:

1. Check file integrity:
   ```bash
   aosctl aos verify --path adapter.aos --format json
   ```

2. Extract and inspect components:
   ```bash
   aosctl aos extract --path adapter.aos --output-dir debug/
   ```

3. Validate manifest:
   ```bash
   cat debug/manifest.json | jq .
   ```

### Loading Errors

If loading fails:

1. Check adapter compatibility with base model
2. Verify weights format (must be safetensors)
3. Ensure training data is valid JSONL
4. Check lineage version compatibility

### Performance Issues

If loading is slow:

1. Check file size (should be < 500MB)
2. Ensure adequate disk space for extraction
3. Consider using SSD for adapter storage
4. Monitor memory usage during loading

## Examples

See [examples/aos_usage.rs](../../examples/aos_usage.rs) for complete usage examples.

## References

- [Training the AdapterOS Base Code Adapter](base_adapter.md)
- [Adapter Lifecycle Management](../database-schema/workflows/ADAPTER-LIFECYCLE.md)
- [Registry Schema](../database-schema/SCHEMA-DIAGRAM.md)
