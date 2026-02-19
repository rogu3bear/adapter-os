//! Model Server for Shared Model Loading
//!
//! This crate provides a dedicated server process that loads a base model once
//! and serves forward pass requests to multiple workers via gRPC over UDS.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                    Control Plane                                 │
//! └───────────────────────────┬─────────────────────────────────────┘
//!                             │
//!           ┌─────────────────┼─────────────────┐
//!           ▼                 ▼                 ▼
//!    ┌────────────┐    ┌────────────┐    ┌────────────┐
//!    │  Worker A  │    │  Worker B  │    │  Worker C  │
//!    │ (adapters) │    │ (adapters) │    │ (adapters) │
//!    └─────┬──────┘    └─────┬──────┘    └─────┬──────┘
//!          │                 │                 │
//!          └─────────────────┼─────────────────┘
//!                            ▼
//!                  ┌──────────────────┐
//!                  │   Model Server   │
//!                  │ (aos-model-srv)  │
//!                  │                  │
//!                  │  Loaded Model    │
//!                  │  KV Cache Mgr    │
//!                  └──────────────────┘
//! ```
//!
//! ## Memory Savings
//!
//! With N workers, memory usage is reduced from N × model_size to:
//! - 1 × model_size (in Model Server)
//! - N × adapter_size (in Workers, for cold adapters)
//! - Shared KV cache (in Model Server)
//!
//! For a 7B model (~14GB) with 3 workers: 42GB → ~14.3GB (~65% reduction)
//!
//! ## Hybrid Adapter Strategy
//!
//! - **Hot adapters** (>10% activation): Cached in Model Server, fused before returning logits
//! - **Cold adapters** (<10% activation): Loaded per-worker, applied to base logits locally
//!
//! ## Usage
//!
//! ```bash
//! # Start model server
//! aos-model-srv --model-path /var/models/Llama-3.2-3B-Instruct-4bit \
//!               --socket-path var/run/aos-model-srv.sock
//!
//! # Workers connect via config
//! [model_server]
//! enabled = true
//! socket_path = "var/run/aos-model-srv.sock"
//! ```

#![cfg_attr(not(feature = "mlx"), allow(unused_imports))]

pub mod activation_tracker;
pub mod adapter_cache;
pub mod config;
pub mod forward;
pub mod kv_cache;
pub mod server;

// Re-export generated protobuf types
pub mod proto {
    // Include the generated protobuf code
    // This will be populated by build.rs
    include!("generated/adapteros.model_server.rs");
}

pub use config::ModelServerConfig;
pub use server::{ModelServer, ModelServerStartupStatus};

/// Version of the model server protocol
pub const PROTOCOL_VERSION: &str = "1.0.0";

/// Default socket path for the model server
pub const DEFAULT_SOCKET_PATH: &str = "var/run/aos-model-srv.sock";

/// Default hot adapter threshold (activation rate percentage)
pub const DEFAULT_HOT_ADAPTER_THRESHOLD: f64 = 0.10; // 10%

/// Maximum number of hot adapters to cache in model server
pub const MAX_HOT_ADAPTERS: usize = 8;
