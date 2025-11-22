//! AOS Archive Format
//!
//! Unified single-file adapter archive format with manifest + weights.
//!
//! ## Format Specification (v3.0, 64-byte header)
//!
//! ```text
//! | Offset | Size | Field                              |
//! |--------|------|------------------------------------|
//! | 0      | 8    | Magic: "AOS3\x00\x00\x00\x00"      |
//! | 8      | 4    | Version (u32 LE) = 3               |
//! | 12     | 4    | Flags (u32 LE, reserved)           |
//! | 16     | 8    | Total file size (u64 LE)           |
//! | 24     | 8    | Weights offset (u64 LE)            |
//! | 32     | 8    | Weights size (u64 LE)              |
//! | 40     | 8    | Manifest offset (u64 LE)           |
//! | 48     | 8    | Manifest size (u64 LE)             |
//! | 56     | 8    | Reserved (padding)                 |
//! | 64     | N    | Weights data (SafeTensors)         |
//! | 64+N   | M    | Manifest (JSON metadata)           |
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
#[cfg(feature = "mmap")]
pub mod mmap_loader;
pub mod writer;

#[cfg(feature = "mmap")]
pub use implementation::{
    AosLoader, AosManifest, LoadedAdapter, TrainingConfigManifest,
    AOS3_MAGIC, AOS_VERSION, HEADER_SIZE as LOADER_HEADER_SIZE,
};
#[cfg(feature = "mmap")]
pub use manager::{AosManager, AosManagerBuilder};
#[cfg(feature = "mmap")]
pub use mmap_loader::{MmapAdapter, MmapAdapterLoader};
pub use writer::{AosHeader, AosWriter, WriteOptions, AOS_MAGIC, AOS_VERSION as WRITER_VERSION, HEADER_SIZE};
