//! Canonical defaults for AdapterOS.
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

/// Dev-only fixture path for the default local Qwen2.5-7B-Instruct-4bit model.
pub const DEV_MODEL_PATH: &str = "./var/model-cache/models/qwen2.5-7b-mlx";

/// Dev-only fixture path for the default local Qwen2.5-7B-Instruct-4bit manifest (config.json).
pub const DEV_MANIFEST_PATH: &str = "./var/models/Qwen2.5-7B-Instruct-4bit/config.json";

/// Default cache root for base models (can be overridden via AOS_MODEL_CACHE_DIR).
pub const DEFAULT_MODEL_CACHE_ROOT: &str = "./var/model-cache/models";

/// Default embedding model path (can be overridden via AOS_EMBEDDING_MODEL_PATH).
pub const DEFAULT_EMBEDDING_MODEL_PATH: &str = "./var/model-cache/models/bge-small-en-v1.5";

/// Default base model identifier (can be overridden via AOS_BASE_MODEL_ID).
pub const DEFAULT_BASE_MODEL_ID: &str = "qwen2.5-7b-mlx";

/// Default Qwen2.5 int4 manifest directory.
pub const DEFAULT_QWEN_INT4_MANIFEST_DIR: &str = "artifacts/qwen2_5_7b_int4";

/// Default telemetry directory.
pub const DEFAULT_TELEMETRY_DIR: &str = "./var/telemetry";

/// Default index root directory (per-tenant subdirs will be appended).
pub const DEFAULT_INDEX_ROOT: &str = "./var/indices";

/// Default manifest cache directory.
pub const DEFAULT_MANIFEST_CACHE_DIR: &str = "./var/manifest-cache";

/// Default adapters root directory.
pub const DEFAULT_ADAPTERS_ROOT: &str = "./var/adapters";

/// Production worker socket root.
pub const DEFAULT_WORKER_SOCKET_PROD_ROOT: &str = "./var/run/aos";

/// Development worker socket path.
pub const DEFAULT_WORKER_SOCKET_DEV: &str = "./var/run/worker.sock";

/// Control plane worker socket default (training cancel path).
pub const DEFAULT_CP_WORKER_SOCKET: &str = "./var/run/adapteros.sock";

/// Default status file path consumed by the menu bar app.
pub const DEFAULT_STATUS_PATH: &str = "./var/run/adapteros_status.json";

/// Default supervisor signing key path.
pub const DEFAULT_SUPERVISOR_SIGNING_KEY_PATH: &str = "var/keys/supervisor_signing.key";

/// Default SQLite URL for the control plane database.
pub const DEFAULT_DB_PATH: &str = "sqlite://var/aos-cp.sqlite3";
