# AOS (AdapterOS Single-file) Format Specification

## Overview

The `.aos` format is a single-file binary archive for LoRA adapters. AOS2 is a single format with internal segmentation so that one artifact can carry backend-specific payloads (canonical, mlx, metal, coreml) while keeping manifest and audit invariants intact.

## Binary Structure

**Header (64 bytes, little endian)**

| Offset | Size | Field | Description |
|--------|------|-------|-------------|
| 0 | 4 | `magic` | ASCII `"AOS2"` (0x41 0x4F 0x53 0x32) |
| 4 | 4 | `flags` | Bit 0 = `HAS_INDEX` (must be set) |
| 8 | 8 | `index_offset` | Byte offset of the segment index (typically 64) |
| 16 | 8 | `index_size` | Size of the segment index in bytes (80 * entry_count) |
| 24 | 8 | `manifest_offset` | Byte offset of manifest JSON |
| 32 | 8 | `manifest_size` | Size of manifest JSON |
| 40 | 24 | `reserved` | Zero (non-zero = corrupted) |

**Segment index entry (80 bytes each)**

| Offset | Size | Field | Notes |
|--------|------|-------|-------|
| 0 | 4 | `segment_id` | u32 |
| 4 | 2 | `backend_tag` | u16 (0=canonical, 1=mlx, 2=metal, 3=coreml) |
| 6 | 2 | `reserved0` | Zero |
| 8 | 8 | `offset` | Byte offset of payload |
| 16 | 8 | `len` | Payload length in bytes |
| 24 | 16 | `scope_hash` | BLAKE3-128 of UTF-8 `scope_path` (all zero if absent) |
| 40 | 32 | `weights_hash` | BLAKE3 of the payload bytes |
| 72 | 8 | `reserved1` | Zero |

All index entries immediately follow the 64-byte header; payloads are tightly packed after the index, and the manifest sits last.

### Validity rules

- Header magic is `"AOS2"`; there is one UMA container format (no public v1/v2 split).
- `HAS_INDEX` (bit 0) **must** be set; reserved header bytes must be zero.
- Every archive **must** include a segment index and at least one **canonical** segment (`backend_tag=0`).
- Manifest **must** include `metadata["scope_path"]`; each segment stores `scope_hash = blake3_128(scope_path)`.
- `weights_hash` in every index entry is the BLAKE3 of that payload; mismatch means corruption.
- Offsets/lengths must keep index and segments before the manifest and within file bounds.

### Segments and backend selection

- Canonical segment is mandatory; optional backend segments (`mlx`, `metal`, `coreml`) may coexist.
- Loader selection is canonical-first; backend-aware selection may be added later.
- Segment hashes are verified on load; mismatch -> `SegmentHashMismatch`.

### Scope hierarchy

Logical scope is expressed as `domain / group / scope / operation`.  
`scope_path` is required in `manifest.metadata["scope_path"]` and is hashed (BLAKE3-128) into each segment’s `scope_hash`.

## Manifest

Stored as JSON at `manifest_offset` with size `manifest_size`. Core fields remain:

Required: `adapter_id`, `version`, `rank`, `alpha`, `base_model`, `target_modules`, `weights_hash` (BLAKE3 hex of canonical segment), `created_at`.  
Optional: `name`, `category`, `tier`, `training_config`, `per_layer_hashes`, `metadata` (include `scope_path`, `domain`, `group`, `operation` when available), `lora_tier`, `scope`.

## Writing AOS2 (Rust)

```rust
use adapteros_aos::{AosWriter, BackendTag};

let mut writer = AosWriter::new();
writer.add_segment(BackendTag::Canonical, Some("domain/group/scope/op".into()), weights_bytes)?;
writer.write_archive("adapter.aos", &manifest_json)?;
```

## Loading AOS2 (Rust)

```rust
use adapteros_aos::{open_aos, BackendTag};

let data = std::fs::read("adapter.aos")?;
let view = open_aos(&data)?;
let canonical = view.segments
    .iter()
    .find(|s| s.backend_tag == BackendTag::Canonical)
    .ok_or("missing canonical segment")?;
let manifest: serde_json::Value = serde_json::from_slice(view.manifest_bytes)?;
// `canonical.payload` holds safetensors bytes
```

## Validation and security

- Reject files with bad magic, non-zero reserved bytes, missing `HAS_INDEX`, out-of-bounds ranges, or misaligned index sizes (corrupted / retrain).
- Recompute BLAKE3 per segment and compare to `weights_hash` (fails fast on mismatch).
- Manifest `weights_hash` should match the canonical segment payload.
- Per-layer hashes (if provided) remain enforced by runtime loader.

## Detection

`&bytes[0..4] == b"AOS2"` is sufficient for format detection. Older headers are treated as “corrupted / needs retrain.”

## File size guidance

- Header: 64 bytes fixed  
- Index: `80 * segment_count` bytes  
- Typical adapter payloads: 100 KB – 10 MB  
- Offsets use `u64` (theoretical 16 EB maximum)

## References

- Writer: `crates/adapteros-aos/src/writer.rs`  
- Core loader: `crates/adapteros-aos/src/implementation.rs`  
- Runtime loader: `crates/adapteros-lora-lifecycle/src/loader.rs`  
- Packager: `crates/adapteros-lora-worker/src/training/packager.rs`

Last Updated: 2025-12-08  
MLNavigator Inc 2025-12-08.
