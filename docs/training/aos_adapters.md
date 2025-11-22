# Single-File Adapter Format (.aos)

<!-- ============================================================================
AOS COORDINATION HEADER
============================================================================
File: docs/training/aos_adapters.md
Phase: 3 - Advanced Features (Documentation)
Assigned: Intern H (Documentation Team)
Status: Complete - Documentation implemented
Dependencies: SingleFileAdapter, CLI commands, UI components
Last Updated: 2025-11-02

COORDINATION NOTES:
- This file affects: User documentation, API documentation, training guides
- Changes require: Updates when SingleFileAdapter format changes
- Testing needed: Documentation accuracy tests, user workflow validation
- CLI Impact: Documents CLI command usage and examples
- UI Impact: Documents UI component usage and workflows
- Database Impact: Documents database schema and migration procedures
============================================================================ -->

The `.aos` format provides a self-contained adapter package that includes:
- LoRA weights (safetensors in v1, structured binary sections in v2)
- Training data (JSONL format, compressed in v2)
- Configuration (TOML captured in metadata)
- Lineage tracking (JSON format)
- Cryptographic signatures (Ed25519)
- Weight-group metadata (manifest + disk info)

## Format Versions

AdapterOS maintains two interoperable `.aos` revisions:

| Version | Container | Default Tooling | Primary Use Case |
| ------- | --------- | ---------------- | ---------------- |
| **v1 (ZIP)** | ZIP archive with discrete files (`manifest.json`, `weights.safetensors`, etc.) | `SingleFileAdapterPackager`, existing CLI commands | Backwards compatibility with legacy pipelines |
| **v2 (AOS 2.0)** | Fixed-layout binary with 256-byte header + aligned sections | `Aos2Packager`, `Aos2Adapter` loader | Memory-mapped loading, zero-copy weights, faster verification |

Both versions share the `.aos` extension. `SingleFileAdapterLoader` and the runtime CLI auto-detect which format is on disk via the file header.

## Benefits

- **Self-contained**: All adapter components in a single file
- **Portable**: Easy to share and deploy across environments
- **Versioned**: Built-in lineage tracking and version management
- **Signed**: Cryptographic integrity verification
- **Efficient**: v1 uses ZIP compression; v2 stores metadata with zstd and keeps weights mmap-friendly
- **Mmap-ready**: v2 exposes aligned sections for zero-copy loading when `LoadOptions::use_mmap` is enabled
- **Compatible**: Works seamlessly with existing AdapterOS infrastructure, loaders, and registries

## Creating .aos Files

### Format selection

- Use **v1 (ZIP)** when you need parity with older pipelines or tools that expect discrete files inside the archive. This remains the default for `aosctl aos create` and `cargo xtask train-*` helpers.
- Use **v2 (AOS 2.0)** when deploying to environments that benefit from memory-mapped weights or when you want deterministic section layouts for audit/logging.

Both packagers produce artifacts that can be stored and verified by the same registry APIs.

### From existing adapter (ZIP v1)

```bash
# Create from packaged adapter directory (ZIP-based v1)
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

### From training pipeline (ZIP v1)

```bash
# Train and package in one step (ZIP-based v1)
cargo xtask train-base-adapter \
  --manifest training/datasets/base/code/adapteros/manifest.json \
  --tokenizer models/qwen2.5-7b-mlx/tokenizer.json \
  --output-format aos \
  --output code_lang_v1.aos \
  --adapter-id code_lang_v1
```

### Programmatic packaging (AOS 2.0)

```rust,no_run
use adapteros_single_file_adapter::{
    Aos2Packager, Aos2PackageOptions, SingleFileAdapter,
};

# async fn package(adapter: &SingleFileAdapter) -> adapteros_core::Result<()> {
let options = Aos2PackageOptions {
    compress_metadata: true,
    compress_weights: false,
    compression_level: 5,
    include_combined_weights: true,
};

Aos2Packager::save_with_options(adapter, "code_lang_v2.aos", options).await?;
# Ok(())
# }
```

`Aos2Packager` aligns sections on page boundaries, emits a 256-byte header, and preserves signing information in a dedicated signatures section.

## Loading .aos Files

`SingleFileAdapterLoader` auto-detects the file header, so the same code path works for both v1 (ZIP) and v2 (AOS 2.0). When `LoadOptions::use_mmap` is `true`, the loader:
- maps v2 files directly with zero-copy weight access,
- falls back to the mmap ZIP loader for v1 files when possible.

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

### Direct loader API

```rust,no_run
use adapteros_single_file_adapter::{LoadOptions, SingleFileAdapterLoader};

# async fn load(path: &str) -> adapteros_core::Result<()> {
let options = LoadOptions {
    skip_verification: false,
    skip_signature_check: false,
    use_mmap: true,
};

let adapter = SingleFileAdapterLoader::load_with_options(path, options).await?;
println!("Loaded format v{}", adapter.manifest.format_version);
# Ok(())
# }
```

## Verifying .aos Files

Verification uses the same format detection logic, so the validator works with ZIP and AOS 2.0 artifacts without extra flags.

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

## Detecting format version

The crate exports `detect_format` to help tooling branch on layout-specific logic (e.g., analytics or migration scripts):

```rust,no_run
use adapteros_single_file_adapter::{detect_format, FormatVersion};

match detect_format("code_lang_v2.aos")? {
    FormatVersion::AosV2 => println!("memory-mappable"),
    FormatVersion::ZipV1 => println!("legacy ZIP"),
}
```

## Extracting Components

Extraction utilities (`aosctl aos extract`, registry tooling) detect the format version and decode either layout transparently.

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

Both format versions contain the same logical artifacts (manifest, weights, lineage, config, training data, signatures). Only the outer container changes.

### Format v1 (ZIP layout)

```
code_lang_v1.aos (ZIP container)
├── manifest.json          # Adapter metadata
├── weights.safetensors    # LoRA weights
├── training_data.jsonl    # Training examples
├── config.toml            # Training configuration
├── lineage.json           # Evolution history
├── signature.sig          # Cryptographic signature (optional)
└── weight_groups.json     # Disk metadata (optional, newer builds only)
```

### Format v2 (AOS 2.0 layout)

```
code_lang_v2.aos (AOS 2.0 binary)
┌───────────────────────────────┐
│ Header (256 bytes)            │ magic, version, section offsets, checksum
├───────────────────────────────┤
│ Weights section               │ serialized `Aos2Weights`, optional zstd
├───────────────────────────────┤
│ Metadata section              │ zstd-compressed JSON (`Aos2Metadata`)
├───────────────────────────────┤
│ Signatures section            │ JSON payload, empty when unsigned
└───────────────────────────────┘
```

The header aligns subsequent sections to the system page size so the loader can memory-map weights without copying.

### Manifest Format (shared)

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

### Lineage Format (shared)

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

### Metadata bundle (v2)

AOS 2.0 stores manifest, config, lineage, training data, and signature inside a single JSON structure before compression:

```json
{
  "manifest": { "adapter_id": "code_lang_v2", "version": "1.1.0" },
  "config": { "rank": 16, "hidden_dim": 4096 },
  "lineage": { "parent_version": "1.0.0", "mutations": [] },
  "training_data": [
    { "input": "...", "output": "...", "tags": ["positive"] }
  ],
  "signature": {
    "key_id": "ed25519:prod-signing",
    "signature": "base64-encoded"
  }
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

### Format rollout

- Default to AOS 2.0 for new deployments once all runtime nodes are on the latest loader release.
- Keep ZIP-based artifacts for legacy consumers until they have validated the new binary layout.
- Record the `manifest.format_version` in release notes so downstream services can enforce allowlists.

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
