# AOS (AdapterOS Single-file) Format Specification v3.0

**Document Type**: Proposed Specification (NOT YET IMPLEMENTED)
**Version**: 3.0
**Date**: 2025-01-19
**Author**: AdapterOS Team
**Status**: PROPOSAL ONLY - See docs/AOS_FORMAT_REALITY.md for actual implementation

---

## ⚠️ IMPORTANT: This is a PROPOSAL, not reality

**Current implementation** uses v2 ZIP-based format with JSON-serialized weights.
**See `docs/AOS_FORMAT_REALITY.md`** for actual working code and real file formats.

This document describes a **proposed future format** that has NOT been implemented.

---

## Executive Summary (Proposed)

AOS v3.0 would extend the v2.0 format to support MPLoRA (Multi-Path Low-Rank Adaptation) with shared bottleneck architecture, enabling efficient multi-adapter systems with up to 50% memory reduction compared to standard LoRA.

### Proposed Key Enhancements

1. **MPLoRA Support**: Shared down-projection matrices across multiple adapters
2. **Enhanced Header**: Extended 32-byte header with version detection
3. **Module Mapping Table**: Explicit module-to-layer mapping
4. **Integrity Verification**: Per-tensor and file-level checksums
5. **Backward Compatibility**: Graceful fallback to v2.0 when needed

### Reality Check

**None of these features exist in the codebase.** Current v2 format:
- Uses ZIP archives with JSON-serialized weights (NOT safetensors)
- Has no tensor tables or checksums
- Does not support MPLoRA
- See `crates/adapteros-single-file-adapter/src/packager.rs` for actual implementation

---

## Reality Check: Current v2 Implementation

**Before reading this proposal**, understand what actually exists:

### Actual v2 ZIP Format (Production)

```
.aos file (ZIP archive, ~2-3KB compressed)
├── manifest.json (Deflate-9, ~200 bytes)
├── weights_positive.safetensors (JSON!, not safetensors, ~2KB)
├── weights_negative.safetensors (JSON!, not safetensors, ~2KB)
├── config.toml (Deflate-9, ~150 bytes)
├── lineage.json (Deflate-9, ~100 bytes)
└── signature.sig (Optional)
```

### Actual v2 Binary Format (Experimental)

```
[0-7]     Simple header (manifest_offset: u32, manifest_len: u32)
[8...]    Weights JSON (not safetensors!)
[offset]  Manifest JSON
```

**No tensor tables, no checksums, no MPLoRA support.**

---

## Proposed Binary Structure (v3.0 - NOT IMPLEMENTED)

### Proposed Overall Layout

```
[Bytes 0-31]     Extended Header (32 bytes)
[Bytes 32...]    Tensor Data Section
[manifest_offset...] JSON Manifest
```

### Extended Header (32 bytes)

```c
struct AOS3Header {
    // Magic & Version (8 bytes)
    uint32_t magic;              // [0-3]   0x41534F33 ('AOS3')
    uint16_t major_version;      // [4-5]   3
    uint16_t minor_version;      // [6-7]   0

    // Offsets (16 bytes)
    uint32_t manifest_offset;    // [8-11]  Byte offset to JSON manifest
    uint32_t manifest_len;       // [12-15] Length of JSON manifest
    uint32_t tensor_table_offset;// [16-19] Offset to tensor table
    uint32_t tensor_table_len;   // [20-23] Length of tensor table

    // Checksums (8 bytes)
    uint32_t header_checksum;    // [24-27] CRC32 of bytes 0-23
    uint32_t reserved;           // [28-31] Reserved for future use
};
```

All multi-byte values are stored in **little-endian** format.

---

## Tensor Table Structure

Located at `tensor_table_offset`, the tensor table provides a directory of all tensors in the file:

```c
struct TensorTableEntry {
    char name[64];          // Null-terminated tensor name
    uint32_t offset;        // Byte offset from file start
    uint32_t size;          // Size in bytes
    uint16_t dtype;         // Data type (0=f32, 1=f16, 2=Q15)
    uint16_t ndims;         // Number of dimensions
    uint32_t shape[4];      // Shape (up to 4D)
    uint32_t checksum;      // CRC32 of tensor data
    uint32_t alignment;     // Alignment requirement (64 for GPU)
};
```

### Tensor Naming Convention

#### Standard LoRA (Backward Compatible)
- `lora_a.{module}` - Down-projection matrix for module
- `lora_b.{module}` - Up-projection matrix for module

#### MPLoRA (Shared Bottleneck)
- `shared_a.{layer}` - Shared down-projection for layer
- `adapter_{id}.b.{module}` - Adapter-specific up-projection
- `gates.{adapter_id}` - Q15 quantized gates (optional)

Example tensor names:
- `shared_a.layer_0` - Shared bottleneck for layer 0
- `adapter_001.b.q_proj` - Adapter 1's up-projection for query
- `adapter_002.b.k_proj` - Adapter 2's up-projection for key

---

## JSON Manifest Schema

### Version 3.0 Schema

```json
{
  "format_version": 3,
  "format_features": ["mplora", "q15", "checksums"],

  // Adapter Identification
  "adapter_id": "tenant/domain/purpose/revision",
  "name": "Human-readable name",
  "version": "1.0.0",
  "created_at": "2025-01-19T12:00:00Z",

  // Architecture Configuration
  "architecture": {
    "type": "mplora",              // "standard" or "mplora"
    "base_model": "qwen2.5-7b",
    "hidden_size": 3584,
    "intermediate_size": 18944,
    "num_attention_heads": 28,
    "num_key_value_heads": 4,
    "num_hidden_layers": 28
  },

  // LoRA Configuration
  "lora_config": {
    "rank": 16,
    "alpha": 32.0,
    "dropout": 0.0,
    "target_modules": ["q_proj", "k_proj", "v_proj", "o_proj"],
    "modules_to_save": []
  },

  // MPLoRA Specific (optional)
  "mplora_config": {
    "shared_rank": 16,
    "num_adapters": 4,
    "shared_bottleneck": true,
    "orthogonal_constraint": 0.1,
    "compression_ratio": 2.0
  },

  // Module Mapping
  "module_mapping": {
    "q_proj": {
      "layer_indices": [0, 1, 2, 3, 4, 5, 6, 7],
      "weight_shape": [3584, 3584],
      "lora_shape": [16, 3584]
    },
    "k_proj": {
      "layer_indices": [0, 1, 2, 3, 4, 5, 6, 7],
      "weight_shape": [512, 3584],
      "lora_shape": [16, 512]
    },
    "v_proj": {
      "layer_indices": [0, 1, 2, 3, 4, 5, 6, 7],
      "weight_shape": [512, 3584],
      "lora_shape": [16, 512]
    },
    "o_proj": {
      "layer_indices": [0, 1, 2, 3, 4, 5, 6, 7],
      "weight_shape": [3584, 3584],
      "lora_shape": [16, 3584]
    }
  },

  // Checksums
  "checksums": {
    "file_hash": "blake3:e7a75704...",     // Full file hash
    "weights_hash": "blake3:a1b2c3d4...",   // Weights section only
    "manifest_hash": "blake3:f5e4d3c2...",  // This manifest
    "algorithm": "blake3"
  },

  // Training Metadata (optional)
  "training_config": {
    "learning_rate": 0.0005,
    "batch_size": 8,
    "num_epochs": 4,
    "gradient_accumulation_steps": 1,
    "warmup_ratio": 0.1,
    "weight_decay": 0.01
  },

  // Metadata
  "metadata": {
    "description": "Adapter description",
    "use_cases": ["code review", "documentation"],
    "tags": ["code", "technical"],
    "license": "Apache-2.0"
  }
}
```

---

## Data Type Encodings

### Supported Data Types

| dtype | Value | Description | Size/element |
|-------|-------|-------------|--------------|
| f32   | 0     | 32-bit float (IEEE-754) | 4 bytes |
| f16   | 1     | 16-bit float (IEEE-754) | 2 bytes |
| Q15   | 2     | 16-bit signed int (-32768 to 32767) | 2 bytes |
| Q8    | 3     | 8-bit signed int (-128 to 127) | 1 byte |
| Q4    | 4     | 4-bit packed (2 per byte) | 0.5 bytes |

### Q15 Quantization

Q15 format maps floating-point values to 16-bit integers:
- Range: [-1.0, 1.0] → [-32768, 32767]
- Quantization: `q15_value = round(float_value * 32767.0)`
- Dequantization: `float_value = q15_value / 32767.0`

---

## Versioning and Compatibility

### Version Detection

```rust
fn detect_aos_version(header: &[u8; 32]) -> AosVersion {
    // Check magic number
    let magic = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);

    if magic == 0x41534F33 { // 'AOS3'
        let major = u16::from_le_bytes([header[4], header[5]]);
        let minor = u16::from_le_bytes([header[6], header[7]]);
        return AosVersion::V3(major, minor);
    }

    // Check for v2.0 format (no magic, offsets at bytes 0-7)
    let manifest_offset = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
    let manifest_len = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);

    if manifest_offset > 8 && manifest_offset < 0x10000000 &&
       manifest_len > 0 && manifest_len < 0x1000000 {
        return AosVersion::V2;
    }

    AosVersion::Unknown
}
```

### Backward Compatibility Rules

1. **Reading v2.0 files**: Loaders MUST support v2.0 format
2. **Writing v3.0 files**: Writers MAY include v2.0 compatibility mode
3. **Feature detection**: Use `format_features` array for optional features
4. **Graceful degradation**: Missing MPLoRA config falls back to standard LoRA

---

## Rust Parser Implementation

### Core Parser Structure

```rust
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

#[derive(Debug)]
pub struct AOS3Parser {
    header: AOS3Header,
    tensor_table: Vec<TensorTableEntry>,
    manifest: serde_json::Value,
}

#[derive(Debug)]
pub struct AOS3Header {
    pub magic: u32,
    pub major_version: u16,
    pub minor_version: u16,
    pub manifest_offset: u32,
    pub manifest_len: u32,
    pub tensor_table_offset: u32,
    pub tensor_table_len: u32,
    pub header_checksum: u32,
}

#[derive(Debug)]
pub struct TensorTableEntry {
    pub name: String,
    pub offset: u32,
    pub size: u32,
    pub dtype: DataType,
    pub shape: Vec<u32>,
    pub checksum: u32,
}

#[derive(Debug)]
pub enum DataType {
    F32 = 0,
    F16 = 1,
    Q15 = 2,
    Q8 = 3,
    Q4 = 4,
}

impl AOS3Parser {
    pub fn parse_file<P: AsRef<Path>>(path: P) -> Result<Self, AosError> {
        let mut file = std::fs::File::open(path)?;

        // Read and validate header
        let header = Self::read_header(&mut file)?;

        // Read tensor table
        let tensor_table = Self::read_tensor_table(&mut file, &header)?;

        // Read manifest
        let manifest = Self::read_manifest(&mut file, &header)?;

        Ok(Self {
            header,
            tensor_table,
            manifest,
        })
    }

    fn read_header<R: Read>(reader: &mut R) -> Result<AOS3Header, AosError> {
        let mut header_bytes = [0u8; 32];
        reader.read_exact(&mut header_bytes)?;

        // Parse fields
        let magic = u32::from_le_bytes([
            header_bytes[0], header_bytes[1],
            header_bytes[2], header_bytes[3]
        ]);

        // Validate magic
        if magic != 0x41534F33 {
            return Err(AosError::InvalidFormat("Invalid AOS3 magic"));
        }

        let header = AOS3Header {
            magic,
            major_version: u16::from_le_bytes([header_bytes[4], header_bytes[5]]),
            minor_version: u16::from_le_bytes([header_bytes[6], header_bytes[7]]),
            manifest_offset: u32::from_le_bytes([
                header_bytes[8], header_bytes[9],
                header_bytes[10], header_bytes[11]
            ]),
            manifest_len: u32::from_le_bytes([
                header_bytes[12], header_bytes[13],
                header_bytes[14], header_bytes[15]
            ]),
            tensor_table_offset: u32::from_le_bytes([
                header_bytes[16], header_bytes[17],
                header_bytes[18], header_bytes[19]
            ]),
            tensor_table_len: u32::from_le_bytes([
                header_bytes[20], header_bytes[21],
                header_bytes[22], header_bytes[23]
            ]),
            header_checksum: u32::from_le_bytes([
                header_bytes[24], header_bytes[25],
                header_bytes[26], header_bytes[27]
            ]),
        };

        // Verify header checksum
        let computed_checksum = Self::compute_crc32(&header_bytes[0..24]);
        if computed_checksum != header.header_checksum {
            return Err(AosError::ChecksumMismatch);
        }

        Ok(header)
    }

    fn read_tensor_table<R: Read + Seek>(
        reader: &mut R,
        header: &AOS3Header
    ) -> Result<Vec<TensorTableEntry>, AosError> {
        reader.seek(SeekFrom::Start(header.tensor_table_offset as u64))?;

        let num_entries = header.tensor_table_len / std::mem::size_of::<TensorTableEntry>() as u32;
        let mut entries = Vec::with_capacity(num_entries as usize);

        for _ in 0..num_entries {
            let entry = Self::read_tensor_entry(reader)?;
            entries.push(entry);
        }

        Ok(entries)
    }

    fn read_tensor_entry<R: Read>(reader: &mut R) -> Result<TensorTableEntry, AosError> {
        let mut name_bytes = [0u8; 64];
        reader.read_exact(&mut name_bytes)?;

        let name = std::str::from_utf8(&name_bytes)?
            .trim_end_matches('\0')
            .to_string();

        let mut buffer = [0u8; 4];

        reader.read_exact(&mut buffer)?;
        let offset = u32::from_le_bytes(buffer);

        reader.read_exact(&mut buffer)?;
        let size = u32::from_le_bytes(buffer);

        let mut dtype_buffer = [0u8; 2];
        reader.read_exact(&mut dtype_buffer)?;
        let dtype = DataType::from_u16(u16::from_le_bytes(dtype_buffer))?;

        reader.read_exact(&mut dtype_buffer)?;
        let ndims = u16::from_le_bytes(dtype_buffer);

        let mut shape = Vec::with_capacity(ndims as usize);
        for _ in 0..ndims {
            reader.read_exact(&mut buffer)?;
            shape.push(u32::from_le_bytes(buffer));
        }

        // Skip unused shape slots
        for _ in ndims..4 {
            reader.read_exact(&mut buffer)?;
        }

        reader.read_exact(&mut buffer)?;
        let checksum = u32::from_le_bytes(buffer);

        reader.read_exact(&mut buffer)?;
        let _alignment = u32::from_le_bytes(buffer);

        Ok(TensorTableEntry {
            name,
            offset,
            size,
            dtype,
            shape,
            checksum,
        })
    }

    fn read_manifest<R: Read + Seek>(
        reader: &mut R,
        header: &AOS3Header
    ) -> Result<serde_json::Value, AosError> {
        reader.seek(SeekFrom::Start(header.manifest_offset as u64))?;

        let mut manifest_bytes = vec![0u8; header.manifest_len as usize];
        reader.read_exact(&mut manifest_bytes)?;

        let manifest = serde_json::from_slice(&manifest_bytes)?;

        Ok(manifest)
    }

    pub fn load_tensor<R: Read + Seek>(
        &self,
        reader: &mut R,
        name: &str
    ) -> Result<Vec<f32>, AosError> {
        let entry = self.tensor_table
            .iter()
            .find(|e| e.name == name)
            .ok_or(AosError::TensorNotFound(name.to_string()))?;

        reader.seek(SeekFrom::Start(entry.offset as u64))?;

        match entry.dtype {
            DataType::F32 => {
                let num_elements: u32 = entry.shape.iter().product();
                let mut buffer = vec![0f32; num_elements as usize];

                // Read raw bytes
                let byte_buffer = unsafe {
                    std::slice::from_raw_parts_mut(
                        buffer.as_mut_ptr() as *mut u8,
                        entry.size as usize
                    )
                };
                reader.read_exact(byte_buffer)?;

                // Verify checksum
                let computed = Self::compute_crc32(byte_buffer);
                if computed != entry.checksum {
                    return Err(AosError::TensorChecksumMismatch(name.to_string()));
                }

                Ok(buffer)
            }
            DataType::Q15 => {
                let num_elements: u32 = entry.shape.iter().product();
                let mut q15_buffer = vec![0i16; num_elements as usize];

                // Read Q15 values
                let byte_buffer = unsafe {
                    std::slice::from_raw_parts_mut(
                        q15_buffer.as_mut_ptr() as *mut u8,
                        entry.size as usize
                    )
                };
                reader.read_exact(byte_buffer)?;

                // Verify checksum
                let computed = Self::compute_crc32(byte_buffer);
                if computed != entry.checksum {
                    return Err(AosError::TensorChecksumMismatch(name.to_string()));
                }

                // Dequantize to f32
                let buffer: Vec<f32> = q15_buffer
                    .iter()
                    .map(|&q| q as f32 / 32767.0)
                    .collect();

                Ok(buffer)
            }
            _ => Err(AosError::UnsupportedDataType),
        }
    }

    fn compute_crc32(data: &[u8]) -> u32 {
        // CRC32 implementation or use crc32fast crate
        crc32fast::hash(data)
    }
}

// Error types
#[derive(Debug)]
pub enum AosError {
    InvalidFormat(&'static str),
    ChecksumMismatch,
    TensorNotFound(String),
    TensorChecksumMismatch(String),
    UnsupportedDataType,
    IoError(std::io::Error),
}

impl From<std::io::Error> for AosError {
    fn from(e: std::io::Error) -> Self {
        AosError::IoError(e)
    }
}
```

---

## Example: Parsing creative-writer.aos

### Reading Adapter Weights

```rust
use std::path::Path;

fn load_creative_writer_adapter() -> Result<(), AosError> {
    // Parse the file
    let parser = AOS3Parser::parse_file("adapters/creative-writer.aos")?;

    // Check format version
    assert_eq!(parser.header.major_version, 3);

    // Extract adapter configuration
    let adapter_id = parser.manifest["adapter_id"].as_str().unwrap();
    let rank = parser.manifest["lora_config"]["rank"].as_u64().unwrap();
    let alpha = parser.manifest["lora_config"]["alpha"].as_f64().unwrap();

    println!("Loaded adapter: {}", adapter_id);
    println!("Rank: {}, Alpha: {}", rank, alpha);

    // Check if MPLoRA is enabled
    if let Some(mplora) = parser.manifest.get("mplora_config") {
        let shared_rank = mplora["shared_rank"].as_u64().unwrap();
        println!("MPLoRA enabled with shared rank: {}", shared_rank);

        // Load shared down-projection matrix
        let mut file = std::fs::File::open("adapters/creative-writer.aos")?;
        let shared_a = parser.load_tensor(&mut file, "shared_a.layer_0")?;
        println!("Loaded shared bottleneck: {} parameters", shared_a.len());
    }

    // Load adapter-specific up-projections
    let target_modules = parser.manifest["lora_config"]["target_modules"]
        .as_array()
        .unwrap();

    for module in target_modules {
        let module_name = module.as_str().unwrap();

        // Try MPLoRA naming first
        let tensor_name = format!("adapter_001.b.{}", module_name);
        let mut file = std::fs::File::open("adapters/creative-writer.aos")?;

        match parser.load_tensor(&mut file, &tensor_name) {
            Ok(weights) => {
                println!("Loaded {}: {} parameters", tensor_name, weights.len());
            }
            Err(_) => {
                // Fallback to standard LoRA naming
                let fallback_name = format!("lora_b.{}", module_name);
                let weights = parser.load_tensor(&mut file, &fallback_name)?;
                println!("Loaded {} (v2 compat): {} parameters",
                         fallback_name, weights.len());
            }
        }
    }

    Ok(())
}
```

---

## Error Handling

### Error Categories

1. **Format Errors**
   - Invalid magic number
   - Unsupported version
   - Malformed header

2. **Integrity Errors**
   - Header checksum mismatch
   - Tensor checksum mismatch
   - File hash mismatch

3. **Compatibility Errors**
   - Missing required fields
   - Unsupported data types
   - Shape mismatches

### Error Recovery

```rust
fn load_with_fallback<P: AsRef<Path>>(path: P) -> Result<Adapter, AosError> {
    // Try v3.0 parser first
    match AOS3Parser::parse_file(&path) {
        Ok(parser) => {
            // Load as v3.0
            load_v3_adapter(parser)
        }
        Err(AosError::InvalidFormat(_)) => {
            // Fallback to v2.0
            println!("Falling back to v2.0 parser");
            load_v2_adapter(&path)
        }
        Err(e) => Err(e),
    }
}
```

---

## Security Considerations

### Validation Requirements

1. **Size Limits**
   - Maximum file size: 4GB (32-bit offsets)
   - Maximum tensor size: 1GB
   - Maximum manifest size: 10MB

2. **Checksum Verification**
   - ALWAYS verify header checksum
   - ALWAYS verify tensor checksums before GPU upload
   - OPTIONALLY verify file hash for critical adapters

3. **Memory Safety**
   - Validate all offsets before seeking
   - Check tensor sizes match shape declarations
   - Ensure alignment requirements are met

### Safe Loading Pattern

```rust
fn safe_load_adapter<P: AsRef<Path>>(path: P) -> Result<Adapter, AosError> {
    let metadata = std::fs::metadata(&path)?;

    // Check file size
    if metadata.len() > 4_294_967_296 { // 4GB
        return Err(AosError::FileTooLarge);
    }

    // Memory-map with safety checks
    let file = std::fs::File::open(&path)?;
    let mmap = unsafe { memmap2::Mmap::map(&file)? };

    // Validate before parsing
    if mmap.len() < 32 {
        return Err(AosError::FileTooSmall);
    }

    // Parse with checksums enabled
    let parser = AOS3Parser::parse_file(&path)?;

    // Verify all checksums
    for entry in &parser.tensor_table {
        verify_tensor_checksum(&mmap, entry)?;
    }

    Ok(load_adapter(parser)?)
}
```

---

## Migration from v2.0 to v3.0

### Conversion Tool

```rust
fn convert_v2_to_v3<P: AsRef<Path>>(
    input: P,
    output: P,
    enable_mplora: bool
) -> Result<(), AosError> {
    // Load v2.0 file
    let v2_data = load_v2_file(&input)?;

    // Create v3.0 header
    let mut header = AOS3Header {
        magic: 0x41534F33,
        major_version: 3,
        minor_version: 0,
        manifest_offset: 0, // Will be calculated
        manifest_len: 0,    // Will be calculated
        tensor_table_offset: 32, // Right after header
        tensor_table_len: 0, // Will be calculated
        header_checksum: 0,  // Will be calculated
    };

    // Build tensor table
    let mut tensor_table = Vec::new();
    let mut current_offset = 32; // Start after header

    for (name, tensor) in v2_data.tensors {
        let entry = TensorTableEntry {
            name: name.clone(),
            offset: current_offset,
            size: tensor.size_bytes(),
            dtype: tensor.dtype(),
            shape: tensor.shape().to_vec(),
            checksum: compute_crc32(&tensor.data()),
        };

        tensor_table.push(entry);
        current_offset += tensor.size_bytes();

        // Align to 64 bytes for GPU
        current_offset = (current_offset + 63) & !63;
    }

    // Update manifest for v3.0
    let mut manifest = v2_data.manifest.clone();
    manifest["format_version"] = json!(3);
    manifest["format_features"] = json!(["checksums"]);

    if enable_mplora {
        // Add MPLoRA configuration
        manifest["mplora_config"] = json!({
            "shared_rank": 16,
            "shared_bottleneck": true,
            "compression_ratio": 2.0
        });

        // Convert weights to MPLoRA format
        convert_to_mplora_weights(&mut tensor_table)?;
    }

    // Write v3.0 file
    write_v3_file(&output, header, tensor_table, manifest)?;

    Ok(())
}
```

---

## Performance Considerations

### Alignment Requirements

- **GPU Access**: 64-byte alignment for tensor data
- **Cache Lines**: 64-byte alignment for frequently accessed data
- **Memory Mapping**: Page-aligned (4KB) for optimal mmap performance

### Zero-Copy Loading

```rust
fn zero_copy_load<P: AsRef<Path>>(path: P) -> Result<MmapAdapter, AosError> {
    let file = std::fs::File::open(&path)?;
    let mmap = unsafe { memmap2::Mmap::map(&file)? };

    // Parse header without copying
    let header = unsafe {
        &*(mmap.as_ptr() as *const AOS3Header)
    };

    // Validate magic and version
    if header.magic != 0x41534F33 {
        return Err(AosError::InvalidFormat("Bad magic"));
    }

    // Create tensor views without copying
    let tensor_views = create_tensor_views(&mmap, header)?;

    Ok(MmapAdapter {
        mmap: Arc::new(mmap),
        header: *header,
        tensors: tensor_views,
    })
}
```

---

## Future Extensions (v3.1+)

### Planned Features

1. **Compression Support**
   - Zstandard compression for weights
   - LZ4 for fast decompression
   - Compression field in header

2. **Streaming Support**
   - Progressive loading via HTTP range requests
   - Chunked tensor loading
   - Lazy evaluation

3. **Distributed Adapters**
   - Multi-file adapter sets
   - Federated loading
   - Cross-adapter references

4. **Enhanced Metadata**
   - Provenance tracking
   - Digital signatures (Ed25519)
   - License enforcement

---

## Appendix A: Module Mapping Examples

### Qwen2.5 Architecture

```json
{
  "module_mapping": {
    "q_proj": {
      "layer_pattern": "model.layers.{}.self_attn.q_proj",
      "layer_indices": [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27],
      "weight_shape": [3584, 3584],
      "lora_shape": [16, 3584]
    },
    "k_proj": {
      "layer_pattern": "model.layers.{}.self_attn.k_proj",
      "layer_indices": [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27],
      "weight_shape": [512, 3584],
      "lora_shape": [16, 512]
    },
    "v_proj": {
      "layer_pattern": "model.layers.{}.self_attn.v_proj",
      "layer_indices": [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27],
      "weight_shape": [512, 3584],
      "lora_shape": [16, 512]
    },
    "o_proj": {
      "layer_pattern": "model.layers.{}.self_attn.o_proj",
      "layer_indices": [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27],
      "weight_shape": [3584, 3584],
      "lora_shape": [16, 3584]
    }
  }
}
```

---

## Appendix B: Test Vectors

### Minimal Valid v3.0 File

```hex
# Header (32 bytes)
41 53 4F 33  # Magic 'AOS3'
03 00        # Major version 3
00 00        # Minor version 0
20 01 00 00  # Manifest offset: 288
64 00 00 00  # Manifest length: 100
20 00 00 00  # Tensor table offset: 32
00 01 00 00  # Tensor table length: 256
XX XX XX XX  # Header checksum (CRC32 of bytes 0-23)
00 00 00 00  # Reserved

# Tensor table follows...
# Tensor data follows...
# JSON manifest at offset 288...
```

---

## References

1. AOS v2.0 Specification: `docs/AOS_FORMAT.md`
2. MPLoRA Patent Architecture: `docs/PATENT_MPLORA_ARCHITECTURE.md`
3. SafeTensors Format: https://github.com/huggingface/safetensors
4. CRC32 Algorithm: https://en.wikipedia.org/wiki/Cyclic_redundancy_check
5. IEEE-754 Standard: https://standards.ieee.org/standard/754-2019.html

---

## Change Log

| Version | Date | Changes |
|---------|------|---------|
| 3.0 | 2025-01-19 | Initial v3.0 specification with MPLoRA support |

---

**Copyright**: © 2025 AdapterOS Project. All rights reserved.