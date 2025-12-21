//! Control Plane Client for AdapterOS Workers
//!
//! This crate provides a unified HTTP client abstraction for worker-to-control-plane
//! communication. It consolidates all CP calls (registration, status, heartbeat, fatal)
//! into a single typed API surface.
//!
//! # Design
//!
//! - **Async-first**: Uses `reqwest` for all normal operations
//! - **Sync helper**: Separate `report_fatal_sync()` for panic hook context
//! - **Typed API**: All requests/responses use shared types from `adapteros-api-types`
//! - **Retry support**: Exponential backoff for transient failures
//! - **Auth-ready**: Prepared for future token/mTLS authentication
//!
//! # Example
//!
//! ```ignore
//! use adapteros_cp_client::{ControlPlaneClient, ClientConfig};
//! use adapteros_api_types::workers::WorkerRegistrationRequest;
//!
//! let config = ClientConfig::builder()
//!     .base_url("http://127.0.0.1:8080")
//!     .build()?;
//!
//! let client = ControlPlaneClient::new(config)?;
//!
//! let response = client.register(WorkerRegistrationRequest {
//!     worker_id: "worker-123".to_string(),
//!     // ...
//! }).await?;
//! ```

mod client;
mod config;
mod error;
mod retry;
pub mod sync_helper;

pub use client::ControlPlaneClient;
pub use config::{ClientConfig, ClientConfigBuilder, HeartbeatFailurePolicy};
pub use error::{Result, WorkerCpError};

// Re-export worker types from adapteros-api-types for convenience
pub use adapteros_api_types::workers::{
    WorkerFatalRequest, WorkerFatalResponse, WorkerHeartbeatRequest, WorkerHeartbeatResponse,
    WorkerRegistrationRequest, WorkerRegistrationResponse, WorkerStatusNotification,
    WorkerStatusResponse,
};
