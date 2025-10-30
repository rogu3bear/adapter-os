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
