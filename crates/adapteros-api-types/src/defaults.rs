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
    DEFAULT_API_URL, DEFAULT_KMS_EMULATOR_HOST, DEFAULT_KMS_EMULATOR_PORT, DEFAULT_SERVER_ADDR,
    DEFAULT_SERVER_HOST, DEFAULT_SERVER_PORT, DEFAULT_SERVER_URL, DEFAULT_TELEMETRY_PORT,
    DEFAULT_UI_PORT, DEFAULT_UI_URL, DEFAULT_VAULT_PORT,
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

    /// Default server port for the control plane HTTP API.
    pub const DEFAULT_SERVER_PORT: u16 = 8080;

    /// Default UI development server port.
    pub const DEFAULT_UI_PORT: u16 = 3200;

    /// Default server bind address.
    pub const DEFAULT_SERVER_HOST: &str = "127.0.0.1";

    /// Default HashiCorp Vault port (for secret management).
    pub const DEFAULT_VAULT_PORT: u16 = 8200;

    /// Default OpenTelemetry collector port.
    pub const DEFAULT_TELEMETRY_PORT: u16 = 4317;

    /// Default GCP KMS emulator port (for local development/testing).
    pub const DEFAULT_KMS_EMULATOR_PORT: u16 = 9011;

    /// Default control plane server URL string constant.
    pub const DEFAULT_SERVER_URL: &str = "http://127.0.0.1:8080";

    /// Default API base URL string constant.
    pub const DEFAULT_API_URL: &str = "http://127.0.0.1:8080/api";

    /// Default UI development server URL string constant.
    pub const DEFAULT_UI_URL: &str = "http://127.0.0.1:3200";

    /// Default KMS emulator host:port string constant.
    pub const DEFAULT_KMS_EMULATOR_HOST: &str = "127.0.0.1:9011";

    /// Default server bind address with port.
    pub const DEFAULT_SERVER_ADDR: &str = "127.0.0.1:8080";
}

#[cfg(not(feature = "server"))]
pub use wasm_defaults::*;
