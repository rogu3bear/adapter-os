//! Network defaults for adapterOS.
//!
//! This module provides canonical network defaults that work in both server
//! and WASM contexts. When the `server` feature is enabled, these are re-exported
//! from `adapteros_core::defaults`. For WASM builds, they are defined directly
//! here (and must be kept in sync with adapteros-core).
//!
//! # Usage
//!
//! ```rust
//! use adapteros_api_types::defaults::{DEFAULT_SERVER_URL, DEFAULT_API_URL};
//! ```

// =============================================================================
// Server builds: re-export from adapteros-core (single source of truth)
// =============================================================================

#[cfg(feature = "server")]
pub use adapteros_core::defaults::{
    DEFAULT_API_URL, DEFAULT_CODEGRAPH_PORT, DEFAULT_KMS_EMULATOR_HOST, DEFAULT_KMS_EMULATOR_PORT,
    DEFAULT_LOCALSTACK_PORT, DEFAULT_MINIMAL_UI_PORT, DEFAULT_MODEL_SERVER_ADDR,
    DEFAULT_MODEL_SERVER_PORT, DEFAULT_NODE_PORT, DEFAULT_PANEL_PORT, DEFAULT_PORT_PANE_BASE,
    DEFAULT_POSTGRES_PORT, DEFAULT_PROMETHEUS_PORT, DEFAULT_SERVER_ADDR, DEFAULT_SERVER_HOST,
    DEFAULT_SERVER_PORT, DEFAULT_SERVER_URL, DEFAULT_TELEMETRY_PORT, DEFAULT_UI_PORT,
    DEFAULT_UI_URL, DEFAULT_VAULT_PORT,
};

// =============================================================================
// WASM builds: define constants directly (must match adapteros_core::defaults)
// =============================================================================

#[cfg(not(feature = "server"))]
pub mod wasm_defaults {
    //! Network defaults for WASM builds.
    //!
    //! **Important**: These values MUST match `adapteros_core::defaults`.
    //! If the canonical values change, update these to match.

    /// Default base offset for the local port pane.
    pub const DEFAULT_PORT_PANE_BASE: u16 = 18080;

    /// Default server port for the control plane HTTP API.
    pub const DEFAULT_SERVER_PORT: u16 = 18080;

    /// Default UI development server port.
    pub const DEFAULT_UI_PORT: u16 = 18081;

    /// Default service supervisor panel port.
    pub const DEFAULT_PANEL_PORT: u16 = 18082;

    /// Default node agent port.
    pub const DEFAULT_NODE_PORT: u16 = 18083;

    /// Default Prometheus/metrics port.
    pub const DEFAULT_PROMETHEUS_PORT: u16 = 18084;

    /// Default model server port.
    pub const DEFAULT_MODEL_SERVER_PORT: u16 = 18085;

    /// Default codegraph frontend dev-server port.
    pub const DEFAULT_CODEGRAPH_PORT: u16 = 18086;

    /// Default minimal static UI test lane port.
    pub const DEFAULT_MINIMAL_UI_PORT: u16 = 18087;

    /// Default server bind address.
    pub const DEFAULT_SERVER_HOST: &str = "127.0.0.1";

    /// Default HashiCorp Vault port (for secret management).
    pub const DEFAULT_VAULT_PORT: u16 = 18089;

    /// Default OpenTelemetry collector port.
    pub const DEFAULT_TELEMETRY_PORT: u16 = 18088;

    /// Default GCP KMS emulator port (for local development/testing).
    pub const DEFAULT_KMS_EMULATOR_PORT: u16 = 18090;

    /// Default Postgres port for local development.
    pub const DEFAULT_POSTGRES_PORT: u16 = 18091;

    /// Default LocalStack port for local development.
    pub const DEFAULT_LOCALSTACK_PORT: u16 = 18092;

    /// Default control plane server URL string constant.
    pub const DEFAULT_SERVER_URL: &str = "http://127.0.0.1:18080";

    /// Default API base URL string constant.
    pub const DEFAULT_API_URL: &str = "http://127.0.0.1:18080/api";

    /// Default UI development server URL string constant.
    pub const DEFAULT_UI_URL: &str = "http://127.0.0.1:18081";

    /// Default KMS emulator host:port string constant.
    pub const DEFAULT_KMS_EMULATOR_HOST: &str = "127.0.0.1:18090";

    /// Default model server host:port string constant.
    pub const DEFAULT_MODEL_SERVER_ADDR: &str = "http://127.0.0.1:18085";

    /// Default server bind address with port.
    pub const DEFAULT_SERVER_ADDR: &str = "127.0.0.1:18080";
}

#[cfg(not(feature = "server"))]
pub use wasm_defaults::*;
