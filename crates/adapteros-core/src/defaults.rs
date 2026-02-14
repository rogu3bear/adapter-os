//! Canonical defaults for adapterOS.
//!
//! Centralizes configuration defaults to prevent drift across crates.

use crate::seed::SeedMode;

// =============================================================================
// Seed Defaults
// =============================================================================

/// Default seed derivation mode.
pub const DEFAULT_SEED_MODE: SeedMode = SeedMode::BestEffort;

// =============================================================================
// Router Defaults and Limits
// =============================================================================

/// Default number of adapters to select per token (k-sparse selection).
///
/// This matches the AOS_ROUTER_K_SPARSE environment variable default.
pub const DEFAULT_K_SPARSE: usize = 4;

/// Default entropy floor for gate values.
pub const DEFAULT_ENTROPY_FLOOR: f32 = 0.02;

/// Default gate quantization format (Q15 = 16-bit fixed-point).
pub const DEFAULT_GATE_QUANT_STR: &str = "q15";

/// Default sample tokens for full telemetry logging.
/// Per Telemetry Ruleset #9.
pub const DEFAULT_SAMPLE_TOKENS_FULL: usize = 128;

/// Default overhead budget percentage for router CPU.
pub const DEFAULT_OVERHEAD_BUDGET_PCT: f32 = 8.0;

/// Maximum allowed k-sparse value.
pub const MAX_K: usize = 8;

/// Default compression ratio for DIR (Deterministic Inference Runtime).
pub const DEFAULT_COMPRESSION_RATIO: f32 = 0.8;

/// Boost value added to priors for pinned adapters (CHAT-PIN-02).
///
/// Creates preference without exclusivity - pinned adapters are more likely
/// to be selected but non-pinned can still win with higher feature scores.
/// This value is added to the prior score for each pinned adapter before
/// the router's scoring algorithm runs.
pub const PINNED_BOOST: f32 = 0.3;

// =============================================================================
// Path Defaults
// =============================================================================
//
// IMPORTANT: The canonical var directory form is "var/" (NOT "./var/").
// All paths under var/ must use "var/..." without the leading "./".
// This is enforced project-wide. See docs/VAR_STRUCTURE.md for details.
//

/// Dev-only fixture path for the default local Llama-3.2-3B-Instruct-4bit model.
pub const DEV_MODEL_PATH: &str = "/var/models/Llama-3.2-3B-Instruct-4bit";

/// Dev-only fixture path for the default local manifest.
pub const DEV_MANIFEST_PATH: &str = "manifests/qwen7b-4bit-mlx-base-only.yaml";

/// Default cache root for base models (can be overridden via AOS_MODEL_CACHE_DIR).
pub const DEFAULT_MODEL_CACHE_ROOT: &str = "var/models";

/// Default embedding model path (can be overridden via AOS_EMBEDDING_MODEL_PATH).
pub const DEFAULT_EMBEDDING_MODEL_PATH: &str = "var/models/bge-small-en-v1.5";

/// Default base model identifier (can be overridden via AOS_BASE_MODEL_ID).
pub const DEFAULT_BASE_MODEL_ID: &str = "Llama-3.2-3B-Instruct-4bit";

/// Default Qwen2.5 int4 manifest directory.
pub const DEFAULT_QWEN_INT4_MANIFEST_DIR: &str = "artifacts/qwen2_5_7b_int4";

/// Default telemetry directory.
pub const DEFAULT_TELEMETRY_DIR: &str = "var/telemetry";

/// Default subdirectory under artifacts root for training reports.
pub const DEFAULT_TRAINING_REPORTS_SUBDIR: &str = "training-reports";

/// Default index root directory (per-tenant subdirs will be appended).
pub const DEFAULT_INDEX_ROOT: &str = "var/indices";

/// Default manifest cache directory.
pub const DEFAULT_MANIFEST_CACHE_DIR: &str = "var/manifest-cache";

/// Default adapters root directory.
pub const DEFAULT_ADAPTERS_ROOT: &str = "var/adapters";

/// Production worker socket root.
pub const DEFAULT_WORKER_SOCKET_PROD_ROOT: &str = "var/run/aos";

/// Development worker socket path.
pub const DEFAULT_WORKER_SOCKET_DEV: &str = "var/run/worker.sock";

/// Control plane worker socket default (training cancel path).
pub const DEFAULT_CP_WORKER_SOCKET: &str = "var/run/adapteros.sock";

/// Default status file path consumed by the menu bar app.
pub const DEFAULT_STATUS_PATH: &str = "var/run/adapteros_status.json";

/// Default supervisor signing key path.
pub const DEFAULT_SUPERVISOR_SIGNING_KEY_PATH: &str = "var/keys/supervisor_signing.key";

/// Default SQLite URL for the control plane database.
pub const DEFAULT_DB_PATH: &str = "sqlite://var/aos-cp.sqlite3";

// =============================================================================
// Network Defaults
// =============================================================================

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

// =============================================================================
// Network URL Constants (for use in clap default_value attributes)
// =============================================================================

/// Default control plane server URL string constant.
///
/// Use this in clap `#[arg(default_value = ...)]` attributes.
pub const DEFAULT_SERVER_URL: &str = "http://127.0.0.1:8080";

/// Default API base URL string constant.
///
/// Use this in clap `#[arg(default_value = ...)]` attributes.
pub const DEFAULT_API_URL: &str = "http://127.0.0.1:8080/api";

/// Default UI development server URL string constant.
pub const DEFAULT_UI_URL: &str = "http://127.0.0.1:3200";

/// Default KMS emulator host:port string constant.
pub const DEFAULT_KMS_EMULATOR_HOST: &str = "127.0.0.1:9011";

/// Default server bind address with port.
pub const DEFAULT_SERVER_ADDR: &str = "127.0.0.1:8080";

// =============================================================================
// Network URL Helpers (for runtime construction)
// =============================================================================

/// Returns the default control plane server URL (http://127.0.0.1:8080).
#[must_use]
pub fn default_server_url() -> String {
    DEFAULT_SERVER_URL.to_string()
}

/// Returns the default API base URL (http://127.0.0.1:8080/api).
#[must_use]
pub fn default_api_url() -> String {
    DEFAULT_API_URL.to_string()
}

/// Returns the default UI development server URL (http://127.0.0.1:3200).
#[must_use]
pub fn default_ui_url() -> String {
    DEFAULT_UI_URL.to_string()
}

/// Returns the default KMS emulator host:port for GCP KMS testing.
#[must_use]
pub fn default_kms_emulator_host() -> String {
    DEFAULT_KMS_EMULATOR_HOST.to_string()
}

// =============================================================================
// Backend Defaults
// =============================================================================

/// Default model backend.
///
/// "mlx" is the canonical production backend on Apple Silicon.
/// Note: CLI may show "auto" but the resolved default is "mlx".
pub const DEFAULT_MODEL_BACKEND: &str = "mlx";

// =============================================================================
// Logging Defaults
// =============================================================================

/// Default log level for the control plane and workers.
pub const DEFAULT_LOG_LEVEL: &str = "info";

// =============================================================================
// KV Storage Defaults
// =============================================================================

/// Default path for the KV (redb) database.
pub const DEFAULT_KV_PATH: &str = "var/aos-kv.redb";
