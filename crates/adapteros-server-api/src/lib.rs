//! adapterOS Server API Layer
//!
//! This crate provides the REST API layer for the adapterOS control plane server.
//! It handles HTTP routing, authentication, middleware, and request processing.
//!
//! # Architecture
//!
//! The API layer is built on Axum and follows a modular structure:
//!
//! ```text
//! Request -> Middleware -> Route Handler -> Service Layer -> Database
//!              |               |                |
//!              v               v                v
//!         - Auth          - handlers/      - AppState
//!         - Rate limit    - validation     - Db / KvDb
//!         - Request ID    - api_error      - Telemetry
//! ```
//!
//! # Key Components
//!
//! - **Routes** (`routes.rs`): Axum router builder with all API endpoints
//! - **Handlers** (`handlers/`): Request handlers organized by domain (adapters, chat, etc.)
//! - **Middleware** (`middleware.rs`, `middleware_security.rs`): Auth, rate limiting, tracing
//! - **State** (`state.rs`): `AppState` shared across all handlers
//! - **Auth** (`auth.rs`, `auth_common.rs`): JWT validation, dev bypass, claims extraction
//!
//! # Request Flow
//!
//! 1. Request arrives at Axum router
//! 2. Middleware stack processes request (auth, rate limit, request ID)
//! 3. Route handler extracts parameters and validates input
// This tool call is just a placeholder to check file content in next step
// I realized I should view the server-api structure first.ion
//! 4. Handler calls service layer or database directly
//! 5. Response is serialized and returned
//!
//! # Authentication
//!
//! Two modes are supported:
//! - **Production**: JWT Bearer token validation via `Claims`
//! - **Development**: Bypass via `AOS_DEV_NO_AUTH=1` or `security.dev_bypass = true`
//!
//! Use `is_dev_bypass_enabled()` to check current mode.
//!
//! # Error Handling
//!
//! All errors flow through `ApiError` which implements `IntoResponse`.
//! Errors are logged with correlation via request ID and return appropriate HTTP status codes.
//!
//! # SSE Streaming
//!
//! Server-Sent Events are supported via `SseEventManager` for:
//! - Chat streaming responses
//! - Training progress updates
//! - Real-time telemetry
//!
//! # Health Checks
//!
//! - `/healthz`: Liveness probe (always returns 200 if process is running)
//! - `/readyz`: Readiness probe (checks database connectivity)
//! - `/system/ready`: Detailed system readiness with component status

#![allow(unused_imports)]
#![allow(clippy::items_after_test_module)]
#![allow(clippy::unnecessary_to_owned)]
#![allow(clippy::field_reassign_with_default)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::await_holding_lock)]
#![allow(clippy::bool_assert_comparison)]
#![allow(clippy::useless_vec)]
#![allow(clippy::single_component_path_imports)]
#![allow(deprecated)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::result_large_err)]

pub mod adapter_helpers;
pub mod api_error;
pub mod audit_helper;
pub mod auth;
pub mod auth_common;
pub mod backpressure;
pub mod boot_state;
pub mod cab_workflow;
pub mod caching;
pub mod chat_context;
pub mod chat_session_config;
pub mod citations;
pub mod config;
pub mod determinism_context;
pub mod embedding_resilience;
pub mod event_bus;
pub mod execution_profile;
pub mod handlers;
pub mod health;
pub mod idempotency;
pub mod inference_cache;
pub mod inference_core;
pub mod inference_state_tracker;
pub mod ip_extraction;
pub mod kv_isolation;
pub mod lifecycle;
pub mod live_data;
pub mod load_coordinator;
pub mod mfa;
pub mod middleware;
pub mod middleware_security;
pub mod model_status;
pub mod operation_tracker;
pub mod pause_tracker;
pub mod permissions;
pub mod plugin_registry;
pub mod prefix_resolver;
pub mod rate_limit;
pub mod reconciler;
pub mod request_id;
pub mod request_tracker;
pub mod routes;
pub mod runtime_mode;
pub mod security;
pub mod session_tokens;
pub mod self_hosting;
pub mod services;
pub mod settings_loader;
pub mod signing;
pub mod sse;
pub mod state;
pub mod storage_reconciler;
pub mod storage_usage;
pub mod supervisor_client;
pub mod telemetry;
pub mod telemetry_ext;
pub mod types;
pub mod uds_client;
pub mod uds_metrics;
pub mod validation;
pub mod worker_capabilities;
pub mod worker_health;

pub use auth::{is_dev_bypass_enabled, set_dev_bypass_from_config, Claims};
pub use event_bus::EventBus;
pub use load_coordinator::{LoadCoordinator, LoadCoordinatorMetrics};
pub use plugin_registry::PluginRegistry;

pub use config::PathsConfig;
pub use inference_core::InferenceCore;
pub use lifecycle::{
    LifecycleContext, LifecycleHook, LifecycleHookRegistry, LifecyclePhase, ShutdownConfig,
    ShutdownCoordinator, ShutdownError, ShutdownProgress, ShutdownStatus,
};
pub use state::{ApiConfig, AppState, CryptoState};
pub use telemetry::{
    spawn_telemetry_workers, SpanStatus, TelemetryWorkerConfig, TraceBuffer, TraceEvent,
    TraceSearchQuery,
};
pub use telemetry_ext::StackMetadataExt;
pub use types::*;
pub use uds_client::{
    enter_routed_context, exit_routed_context, run_with_routing_context, UdsClient, UdsClientError,
};
pub use worker_health::{
    HealthConfig, WorkerHealthMonitor, WorkerHealthStatus, WorkerHealthSummary,
};

// SSE event management for reliable streaming with replay support
pub use sse::{SseEvent, SseEventManager, SseRingBuffer, SseStreamType};

// Export the router builder function
pub use routes::build as create_app;

// Inference cache for semantic request deduplication
pub use inference_cache::{
    CachedInferenceResult, CachedInferenceResultBuilder, InferenceCache, InferenceCacheConfig,
    InferenceCacheKey, InferenceCacheStats,
};

// HTTP utilities from adapteros-api
pub mod http;
