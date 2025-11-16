<<<<<<< HEAD
//! Memory-mapped .aos file loading with caching and hot-swap support
//!
//! This crate provides file-level management for .aos (Adapter OS) files,
//! offering efficient memory-mapped loading, LRU caching, and atomic hot-swapping.
//!
//! ## Architecture
//!
//! - **File-level cache layer**: Manages memory-mapped files (separate from VRAM management)
//! - **Composable modules**: Each module can be used independently
//! - **Optional unified API**: `AosManager` provides a convenient builder pattern
//! - **Extended metrics**: Performance telemetry for observability
//!
//! ## Usage
//!
//! ### Using Individual Modules
//!
//! ```rust,no_run
//! use adapteros_aos::MmapAdapterLoader;
//! use std::path::Path;
//!
//! # async fn example() -> adapteros_core::Result<()> {
//! let loader = MmapAdapterLoader::new();
//! let adapter = loader.load(Path::new("adapter.aos")).await?;
//! # Ok(())
//! # }
//! ```
//!
//! ### Using Unified Manager
//!
//! ```rust,no_run
//! use adapteros_aos::AosManager;
//! use std::path::Path;
//!
//! # async fn example() -> adapteros_core::Result<()> {
//! let manager = AosManager::builder()
//!     .with_cache(1024 * 1024 * 1024) // 1GB cache
//!     .with_hot_swap()
//!     .build()?;
//!
//! let adapter = manager.load(Path::new("adapter.aos")).await?;
//! # Ok(())
//! # }
//! ```

pub mod cache;
pub mod hot_swap;
pub mod manager;
pub mod metrics;
pub mod mmap_loader;

pub use cache::{AdapterCache, CacheConfig};
pub use hot_swap::{HotSwapManager, SwapOperation};
pub use manager::{AosManager, AosManagerBuilder};
pub use metrics::{CacheMetrics, LoadMetrics, SwapMetrics};
pub use mmap_loader::{MmapAdapter, MmapAdapterLoader};
=======
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
>>>>>>> integration-branch
