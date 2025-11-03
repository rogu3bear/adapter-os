# AOS Format 2.0: Memory-Mappable Single-File Adapter Format

**Status**: ✅ **IMPLEMENTED** (January 2025)  
**Implementation**: `crates/adapteros-single-file-adapter/src/aos2_format.rs` and `aos2_packager.rs`  
**Note**: AOS 2.0 format is fully integrated and works alongside ZIP format with automatic format detection via `crates/adapteros-single-file-adapter/src/format_detector.rs`. Use `aosctl aos create --format aos2` to create AOS 2.0 files, or `aosctl aos convert` to migrate from ZIP.

## 🎯 **The Problem with ZIP**

Current `.aos` files use ZIP compression, which creates these issues:

1. **Decompression Overhead**: Every access requires decompression
2. **No Zero-Copy Loading**: Weights can't be memory-mapped
3. **Sequential Access**: Can't seek to components efficiently
4. **Memory Pressure**: Full decompression loads everything
5. **Not ML-Optimized**: Inference needs fast, direct weight access

## 🚀 **AOS 2.0: Memory-Mappable Architecture**

### **File Structure: Fixed-Offset Sections**

```
┌─────────────────────────────────────────────────────────────┐
│ AOS Header (256 bytes, fixed)                               │
│ - Magic: "AOS\x01\x00"                                       │
│ - Version: 2                                                │
│ - Total size                                                │
│ - Section offsets & sizes                                   │
│ - Checksums & signatures                                    │
└─────────────────────────────────────────────────────────────┘
┌─────────────────────────────────────────────────────────────┐
│ Weights Section (memory-mappable, page-aligned)            │
│ - Positive weights (safetensors, compressed/uncompressed)  │
│ - Negative weights (safetensors, compressed/uncompressed)  │
│ - Combined weights (safetensors, compressed/uncompressed)  │
└─────────────────────────────────────────────────────────────┘
┌─────────────────────────────────────────────────────────────┐
│ Metadata Section (compressed, accessed on-demand)          │
│ - Manifest JSON                                             │
│ - Training config TOML                                      │
│ - Lineage info JSON                                         │
│ - Training examples JSONL (optional)                        │
└─────────────────────────────────────────────────────────────┘
┌─────────────────────────────────────────────────────────────┐
│ Signatures Section (fixed size)                             │
│ - Ed25519 signature                                          │
│ - Public key                                                 │
│ - Certificate chain (optional)                               │
└─────────────────────────────────────────────────────────────┘
```

### **Key Innovations**

#### **1. Memory-Mappable Weights**
```rust
// Zero-copy weight loading
let weights = unsafe {
    mmap_adapter.weights_section(WeightsKind::Positive)?
};

// Direct GPU transfer without decompression
gpu_buffer.copy_from_mmap(weights)?;
```

#### **2. Lazy Metadata Loading**
```rust
// Load only manifest first (fast validation)
let manifest = adapter.manifest()?;

// Load training data only when needed
let training_data = adapter.training_data()?;
```

#### **3. Streaming Access Patterns**
```rust
// ML inference pattern: weights first, metadata later
let adapter = AosAdapter::load_weights_only(path)?;

// Later, if needed for analysis
let metadata = adapter.load_metadata()?;
```

#### **4. Compression Options per Section**
- **Weights**: Optional Zstandard compression (fast decompression)
- **Metadata**: LZ4 for small sections
- **Training Data**: Zstandard for large datasets

## 🏗️ **Implementation Strategy**

### **Phase 1: Core Format**
```rust
#[repr(C)]
struct AosHeader {
    magic: [u8; 8],        // "AOS2\x00\x00\x00\x00"
    version: u32,          // 2
    total_size: u64,
    weights_offset: u64,
    weights_size: u64,
    metadata_offset: u64,
    metadata_size: u64,
    signatures_offset: u64,
    signatures_size: u64,
    checksum: [u8; 32],    // BLAKE3
    signature: [u8; 64],   // Ed25519
}

struct AosAdapter {
    mmap: Mmap,
    header: AosHeader,
    weights_cache: Mutex<Option<Arc<WeightGroups>>>,
    metadata_cache: Mutex<Option<Arc<Metadata>>>,
}
```

### **Phase 2: Advanced Features**
- **Delta Updates**: Efficient adapter evolution
- **Partial Loading**: Load only needed weight groups
- **Concurrent Access**: Multiple processes can share mmap
- **Version Compatibility**: Backward-compatible format evolution

## 🎯 **Benefits Over ZIP**

| Feature | ZIP Approach | AOS 2.0 |
|---------|-------------|---------|
| Weight Loading | Decompress → Copy | mmap → Direct GPU transfer |
| Memory Usage | Full file in RAM | Page-fault loading |
| Access Speed | Sequential | Random access O(1) |
| Concurrent Use | Copy-on-read | Shared memory |
| GPU Transfer | Host→GPU copy | Direct GPU mapping |
| Metadata Access | Decompress all | Lazy loading |

## 🚀 **Migration Path**

**Option A: Hybrid Support**
- Keep ZIP loader for existing .aos files
- Add new format for new adapters
- Automatic format detection

**Option B: Conversion Tools**
```bash
# Convert existing .aos to new format (now available)
aosctl aos convert --input old_adapter.aos --output new_adapter.aos2 --format aos2

# Create new AOS 2.0 adapter
aosctl aos create --source weights.safetensors --output adapter.aos2 --format aos2

# In-place migration (format version upgrade)
aosctl aos migrate adapter.aos
```

## 🎪 **The Vision**

**AOS 2.0 becomes the "SQLite of ML adapters":**
- **Memory-mappable** for zero-copy loading
- **Concurrent access** from multiple processes
- **Transactional updates** for adapter evolution
- **Query-able metadata** without full loading
- **Streaming training data** for analysis

This isn't just a file format - it's an **ML artifact database** optimized for inference performance.

---

**What do you think? Should we implement AOS 2.0?** 🚀
