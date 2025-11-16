//! AOS 2.0 Archive Format
//!
//! Single-file adapter archive format with manifest + weights.
//!
//! ## Format Specification
//!
//! ```text
//! [0-3]    manifest_offset (u32, little-endian)
//! [4-7]    manifest_len (u32, little-endian)
//! [offset] manifest (JSON metadata)
//! [offset] weights (safetensors format)
//! ```

// Loader requires Metal - optional feature
#[cfg(feature = "mmap")]
pub mod aos2_implementation;

pub mod aos2_writer;

#[cfg(feature = "mmap")]
pub use aos2_implementation::{AOS2Loader, AOS2Manifest, LoadedAdapter};

pub use aos2_writer::{AOS2Writer, WriteOptions};
