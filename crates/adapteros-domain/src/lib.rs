//! Domain Adapter Layer for AdapterOS
//!
//! This module provides high-level domain-specific abstractions that translate
//! deterministic tensor operations into domain-specific functions (text, vision,
//! telemetry) while maintaining full reproducibility guarantees.
//!
//! # Determinism Guarantees
//!
//! All domain adapters behave as pure, traceable morphisms:
//! - Identical input → identical output, byte-for-byte
//! - No RNG or dropout
//! - Canonical input/output ordering
//! - All operations logged into BLAKE3 trace
//! - Numerical drift (ε) reported in trace metadata

#![allow(unused_imports)]
#![allow(unused_variables)]
#![allow(unused_assignments)]
#![allow(dead_code)]
#![allow(clippy::manual_clamp)]
#![allow(clippy::collapsible_if)]

//! # Architecture
//!
//! ```text
//! External Data ─→ Domain Adapters ─→ Deterministic Core ─→ Output
//!                   (Text/Vision)      (Executor/Graph)
//! ```
//!
//! # Example Usage
//!
//! ```rust,no_run
//! use adapteros_domain::{DomainAdapter, TextAdapter};
//! use adapteros_deterministic_exec::{DeterministicExecutor, ExecutorConfig};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let config = ExecutorConfig::default();
//! let mut executor = DeterministicExecutor::new(config);
//!
//! let mut text_adapter = TextAdapter::load("path/to/manifest.toml")?;
//! text_adapter.prepare(&mut executor);
//!
//! // Domain adapter automatically uses deterministic operations
//! # let input_tensor = adapteros_numerics::noise::Tensor::new(vec![1.0], vec![1]);
//! let input_data = adapteros_domain::TensorData::new(input_tensor, "f32".to_string());
//! let output = text_adapter.forward(&input_data);
//! # Ok(())
//! # }
//! ```

pub mod adapter;
pub mod error;
pub mod manifest;
pub mod telemetry;
pub mod text;
pub mod vision;

pub use adapter::{DomainAdapter, DomainAdapterMetadata, TensorData};
/// Backward compatibility alias
#[deprecated(since = "0.12.0", note = "Use DomainAdapterMetadata instead")]
pub type AdapterMetadata = DomainAdapterMetadata;
pub use error::{DomainAdapterError, Result};
pub use manifest::{load_manifest, AdapterManifest};
pub use telemetry::TelemetryAdapter;
pub use text::TextAdapter;
pub use vision::VisionAdapter;

pub use adapteros_core::B3Hash;
/// Re-export common types for convenience
pub use adapteros_numerics::noise::Tensor;
pub use adapteros_trace::{Event, TraceBundle};
