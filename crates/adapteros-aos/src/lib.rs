//! AOS Archive Format
//!
//! Unified single-file adapter archive format with manifest + weights.
//!
//! ## Format Specification (64-byte header + segment index)
//!
//! ```text
//! | Offset | Size | Field                               |
//! |--------|------|-------------------------------------|
//! | 0      | 4    | Magic: "AOS2"                       |
//! | 4      | 4    | Flags (bit 0 = has index)           |
//! | 8      | 8    | Index offset (u64 LE)               |
//! | 16     | 8    | Index size (u64 LE)                 |
//! | 24     | 8    | Manifest offset (u64 LE)            |
//! | 32     | 8    | Manifest size (u64 LE)              |
//! | 40     | 24   | Reserved                            |
//! | 64     | ...  | Index entries (80 bytes each)       |
//! | ...    | ...  | Segments (weights payloads)         |
//! | ...    | ...  | Manifest (JSON metadata)            |
//! ```
//!
//! See `docs/AOS_FORMAT.md` for full specification.

#[cfg(feature = "mmap")]
pub mod cache;
#[cfg(feature = "mmap")]
pub mod hot_swap;
#[cfg(feature = "mmap")]
pub mod implementation;
#[cfg(feature = "mmap")]
pub mod manager;
pub mod metrics;
pub mod writer;

#[cfg(feature = "mmap")]
pub use implementation::{
    AosLoader, AosManifest, LoadedAdapter, TrainingConfigManifest,
    HEADER_SIZE as LOADER_HEADER_SIZE,
};
#[cfg(feature = "mmap")]
pub use manager::{AosManager, AosManagerBuilder};
pub use writer::{
    compute_scope_hash, open_aos, parse_segments, select_segment, AosFileView, AosHeader,
    AosWriter, BackendTag, SegmentDescriptor, SegmentView, WriteOptions, AOS_MAGIC, HAS_INDEX_FLAG,
    HEADER_SIZE, INDEX_ENTRY_SIZE,
};
