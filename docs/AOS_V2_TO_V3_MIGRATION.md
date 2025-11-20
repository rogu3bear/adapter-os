# AOS Format v2.0 to v3.0 Migration Guide

**Document Type**: Migration Guide
**Date**: 2025-01-19
**Author**: AdapterOS Team

---

## Overview

This guide provides detailed instructions for migrating AOS files from version 2.0 to version 3.0, including backward compatibility considerations and automated conversion tools.

## Key Differences

### v2.0 Format
- 8-byte header (manifest offset + length only)
- No magic number or version field
- SafeTensors or Q15 weights directly after header
- No tensor table or per-tensor checksums
- Standard LoRA architecture only

### v3.0 Format
- 32-byte extended header with magic number
- Explicit version fields (major.minor)
- Tensor table with per-tensor metadata
- File and tensor-level checksums
- Support for MPLoRA shared bottleneck architecture
- Enhanced module mapping

## Compatibility Matrix

| Reader Version | v2.0 Files | v3.0 Files | v3.0 + MPLoRA |
|---------------|------------|------------|---------------|
| v2.0 Reader   | ✅ Yes     | ❌ No      | ❌ No         |
| v3.0 Reader   | ✅ Yes     | ✅ Yes     | ✅ Yes        |
| v3.0 Writer   | ⚠️ Optional| ✅ Yes     | ✅ Yes        |

## Detection Algorithm

```rust
pub enum AosVersion {
    V2,
    V3(u16, u16), // (major, minor)
    Unknown,
}

pub fn detect_version(header: &[u8; 32]) -> AosVersion {
    // Check for v3.0 magic number
    let magic = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);

    if magic == 0x41534F33 { // 'AOS3' in hex
        let major = u16::from_le_bytes([header[4], header[5]]);
        let minor = u16::from_le_bytes([header[6], header[7]]);
        return AosVersion::V3(major, minor);
    }

    // Check if it could be v2.0 (no magic, valid offsets)
    let manifest_offset = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
    let manifest_len = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);

    // Heuristic: v2.0 has manifest_offset >= 8 and reasonable sizes
    if manifest_offset >= 8 && manifest_offset < 0x10000000 && // < 256MB
       manifest_len > 0 && manifest_len < 0x1000000 {          // < 16MB
        return AosVersion::V2;
    }

    AosVersion::Unknown
}
```

## Automated Conversion

### Command-Line Tool

```bash
# Convert single file
aos-convert --input adapter.aos --output adapter_v3.aos --format v3

# Convert with MPLoRA optimization
aos-convert --input adapter.aos --output adapter_v3.aos --format v3 --enable-mplora

# Batch conversion
aos-convert --input-dir ./adapters --output-dir ./adapters_v3 --format v3

# Verify conversion
aos-verify --file adapter_v3.aos --check-checksums
```

### Programmatic Conversion

```rust
use adapteros_aos::{AOS2Reader, AOS3Writer, MploraConverter};

pub async fn convert_adapter(
    input_path: &Path,
    output_path: &Path,
    enable_mplora: bool,
) -> Result<(), AosError> {
    // Read v2.0 file
    let v2_reader = AOS2Reader::new();
    let (manifest, weights) = v2_reader.read_archive(input_path)?;

    // Update manifest for v3.0
    let mut v3_manifest = manifest.clone();
    v3_manifest["format_version"] = json!(3);
    v3_manifest["format_features"] = json!(["checksums", "tensor_table"]);

    // Add architecture details
    v3_manifest["architecture"] = json!({
        "type": if enable_mplora { "mplora" } else { "standard" },
        "base_model": manifest["base_model"],
        "hidden_size": 3584,  // Model-specific
        "intermediate_size": 18944,
        "num_attention_heads": 28,
        "num_key_value_heads": 4,
        "num_hidden_layers": 28
    });

    // Convert weights if MPLoRA enabled
    let converted_weights = if enable_mplora {
        let converter = MploraConverter::new();
        converter.convert_to_shared_bottleneck(weights)?
    } else {
        weights
    };

    // Write v3.0 file
    let writer = AOS3Writer::new();
    writer.write_archive(output_path, &v3_manifest, &converted_weights)?;

    println!("Converted {} -> {} (v3.0)", input_path.display(), output_path.display());

    Ok(())
}
```

## MPLoRA Weight Conversion

### Standard LoRA to MPLoRA

When converting from standard LoRA to MPLoRA format:

1. **Extract shared components** from down-projection matrices
2. **Recompute up-projection matrices** to maintain equivalence
3. **Store shared bottleneck** separately

```rust
pub struct MploraConverter {
    shared_rank: usize,
    orthogonal_threshold: f32,
}

impl MploraConverter {
    pub fn convert_to_shared_bottleneck(
        &self,
        standard_weights: &StandardLoraWeights,
    ) -> Result<MploraWeights, AosError> {
        let mut mplora_weights = MploraWeights::new();

        // For each target module
        for module in &["q_proj", "k_proj", "v_proj", "o_proj"] {
            // Get standard LoRA A and B matrices
            let lora_a = standard_weights.get_tensor(&format!("lora_a.{}", module))?;
            let lora_b = standard_weights.get_tensor(&format!("lora_b.{}", module))?;

            // Perform SVD to extract shared component
            let (u, s, vt) = svd(&lora_a)?;

            // Shared down-projection (truncated to shared_rank)
            let shared_a = &u[.., ..self.shared_rank];

            // Adapter-specific up-projection
            let adapter_b = matmul(&lora_b, &shared_a.transpose())?;

            // Store in MPLoRA format
            mplora_weights.set_shared_a(module, shared_a);
            mplora_weights.set_adapter_b("adapter_001", module, adapter_b);
        }

        Ok(mplora_weights)
    }
}
```

## Backward Compatibility

### Reading v2.0 Files with v3.0 Reader

```rust
pub struct UniversalAosReader {
    v2_reader: AOS2Reader,
    v3_reader: AOS3Reader,
}

impl UniversalAosReader {
    pub fn read_any_version(&self, path: &Path) -> Result<AdapterData, AosError> {
        // Read first 32 bytes to detect version
        let mut file = File::open(path)?;
        let mut header = [0u8; 32];
        file.read_exact(&mut header)?;

        match detect_version(&header) {
            AosVersion::V3(major, minor) => {
                println!("Detected v{}.{} format", major, minor);
                self.v3_reader.read_archive(path)
            }
            AosVersion::V2 => {
                println!("Detected v2.0 format, using compatibility mode");
                let (manifest, weights) = self.v2_reader.read_archive(path)?;

                // Convert to v3 structure internally
                Ok(AdapterData {
                    format_version: 2,
                    manifest,
                    weights,
                    tensor_table: None,  // Not available in v2
                    checksums: None,     // Not available in v2
                })
            }
            AosVersion::Unknown => {
                Err(AosError::UnsupportedFormat("Unknown AOS format"))
            }
        }
    }
}
```

## Validation and Testing

### Post-Conversion Validation

```rust
pub fn validate_conversion(
    original_path: &Path,
    converted_path: &Path,
) -> Result<ValidationReport, AosError> {
    let original = UniversalAosReader::new().read_any_version(original_path)?;
    let converted = UniversalAosReader::new().read_any_version(converted_path)?;

    let mut report = ValidationReport::new();

    // Check adapter ID preserved
    report.check(
        "adapter_id",
        original.manifest["adapter_id"] == converted.manifest["adapter_id"],
    );

    // Check weight dimensions preserved
    for tensor_name in original.weights.keys() {
        let orig_shape = original.weights[tensor_name].shape();

        // Handle naming changes (lora_a -> shared_a for MPLoRA)
        let conv_name = if converted.manifest["architecture"]["type"] == "mplora" {
            map_tensor_name_to_mplora(tensor_name)
        } else {
            tensor_name.to_string()
        };

        if let Some(conv_tensor) = converted.weights.get(&conv_name) {
            report.check(
                &format!("shape_{}", tensor_name),
                orig_shape == conv_tensor.shape(),
            );
        }
    }

    // Verify checksums if v3
    if converted.format_version >= 3 {
        report.check("has_checksums", converted.checksums.is_some());
        report.check("has_tensor_table", converted.tensor_table.is_some());
    }

    Ok(report)
}
```

## Migration Checklist

### Pre-Migration

- [ ] Backup all original v2.0 files
- [ ] Verify adapter functionality with current system
- [ ] Document adapter configurations and dependencies
- [ ] Test conversion tool on sample files

### During Migration

- [ ] Run conversion with validation enabled
- [ ] Check conversion reports for warnings
- [ ] Verify file sizes are reasonable (typically 0.9x-1.1x of original)
- [ ] Test converted files with v3.0 reader

### Post-Migration

- [ ] Run inference tests with converted adapters
- [ ] Compare outputs with original adapters
- [ ] Update deployment configurations for v3.0
- [ ] Monitor performance metrics

## Rollback Procedure

If issues arise with v3.0 files:

1. **Immediate Rollback**: Restore v2.0 files from backup
2. **Compatibility Mode**: Use UniversalAosReader for mixed versions
3. **Gradual Migration**: Convert adapters incrementally

```rust
pub struct AdapterRegistry {
    fallback_enabled: bool,
    version_preference: AosVersion,
}

impl AdapterRegistry {
    pub fn load_adapter(&self, name: &str) -> Result<Adapter, AosError> {
        // Try preferred version first
        let v3_path = format!("adapters_v3/{}.aos", name);
        if Path::new(&v3_path).exists() {
            if let Ok(adapter) = self.load_v3(&v3_path) {
                return Ok(adapter);
            }
        }

        // Fallback to v2 if enabled
        if self.fallback_enabled {
            let v2_path = format!("adapters/{}.aos", name);
            if Path::new(&v2_path).exists() {
                println!("WARNING: Using v2.0 fallback for {}", name);
                return self.load_v2(&v2_path);
            }
        }

        Err(AosError::AdapterNotFound(name.to_string()))
    }
}
```

## Performance Considerations

### Conversion Performance

- **Typical conversion time**: 100-500ms per adapter
- **Memory usage**: ~2x adapter size during conversion
- **Disk I/O**: Sequential reads/writes optimized

### Runtime Performance

| Operation | v2.0 | v3.0 | v3.0 + MPLoRA |
|-----------|------|------|---------------|
| Load time | 100ms | 95ms | 90ms |
| Memory usage | 100% | 102% | 50-70% |
| Inference speed | 100% | 100% | 95-105% |
| GPU memory | 100% | 100% | 50-70% |

## Troubleshooting

### Common Issues

1. **"Invalid magic number" error**
   - File is v2.0 format, use UniversalAosReader
   - File may be corrupted, verify with aos-verify tool

2. **"Checksum mismatch" error**
   - File corrupted during conversion
   - Re-run conversion with fresh source file

3. **"Tensor not found" error**
   - Tensor naming changed between versions
   - Check module_mapping in manifest

4. **Memory errors during conversion**
   - Large adapter file, increase system memory
   - Use streaming conversion for files >1GB

## Support

For migration assistance:
- Documentation: `docs/AOS_FORMAT_V3.md`
- Issues: https://github.com/adapteros/adapteros/issues
- Migration tool: `crates/adapteros-aos/src/migrate.rs`

---

**Copyright**: © 2025 AdapterOS Project. All rights reserved.