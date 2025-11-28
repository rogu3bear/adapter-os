//! AOS Archive Format
//!
//! Unified single-file adapter archive format with manifest + weights.
//!
//! ## Format Specification (64-byte header)
//!
//! ```text
//! | Offset | Size | Field                              |
//! |--------|------|------------------------------------|
//! | 0      | 4    | Magic: "AOS\x00"                   |
//! | 4      | 4    | Flags (u32 LE, reserved)           |
//! | 8      | 8    | Weights offset (u64 LE)            |
//! | 16     | 8    | Weights size (u64 LE)              |
//! | 24     | 8    | Manifest offset (u64 LE)           |
//! | 32     | 8    | Manifest size (u64 LE)             |
//! | 40     | 24   | Reserved (padding)                 |
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
pub mod writer;

#[cfg(feature = "mmap")]
pub use implementation::{
    AosLoader, AosManifest, LoadedAdapter, TrainingConfigManifest,
    HEADER_SIZE as LOADER_HEADER_SIZE,
};
#[cfg(feature = "mmap")]
pub use manager::{AosManager, AosManagerBuilder};
pub use writer::{AosHeader, AosWriter, WriteOptions, AOS_MAGIC, HEADER_SIZE};
