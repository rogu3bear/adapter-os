//! HTTP middleware for adapterOS telemetry
//!
//! Provides reusable middleware components for API logging across all servers.
//!
//! # Usage
//!
//! ```rust,ignore
//! use adapteros_telemetry::middleware::api_logger_layer;
//!
//! let app = Router::new()
//!     .route("/health", get(health))
//!     .layer(api_logger_layer());
//! ```

pub mod api_logger;

pub use api_logger::{
    api_error_logger_layer, api_error_only_middleware, api_logger_layer, api_logger_middleware,
};
