//! # AdapterOS Service Supervisor
//!
//! Production-ready service supervisor with proper process management,
//! JWT authentication, and comprehensive monitoring.
//!
//! ## Features
//!
//! - **Secure Authentication**: JWT tokens with Ed25519 signing
//! - **Process Supervision**: Proper process lifecycle management with restart policies
//! - **Health Monitoring**: Comprehensive health checks beyond basic HTTP
//! - **Circuit Breakers**: Fault tolerance for service operations
//! - **Configuration Management**: External YAML configuration files
//! - **Metrics & Logging**: Structured logging and Prometheus metrics
//! - **Async Architecture**: Tokio-based with proper concurrency

pub mod auth;
pub mod config;
pub mod error;
pub mod health;
pub mod metrics;
pub mod process;
pub mod server;
pub mod service;
pub mod supervisor;

pub use auth::{AuthService, Claims};
pub use config::SupervisorConfig;
pub use error::{SupervisorError, Result};
pub use health::HealthMonitor;
pub use metrics::init_metrics;
pub use server::SupervisorServer;
pub use service::{ManagedService, ServiceStatus};
pub use supervisor::ServiceSupervisor;
