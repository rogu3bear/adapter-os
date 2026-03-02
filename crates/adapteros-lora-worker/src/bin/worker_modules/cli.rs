use adapteros_core::AosError;
use clap::Parser;

/// adapterOS Inference Worker
#[derive(Parser, Debug)]
#[command(name = "aos-worker")]
#[command(about = "adapterOS inference worker with UDS communication")]
pub struct Args {
    /// Tenant ID for this worker
    #[arg(long, env = "TENANT_ID", default_value = "default")]
    pub tenant_id: String,

    /// Plan ID for this worker
    #[arg(long, env = "PLAN_ID", default_value = "dev")]
    pub plan_id: String,

    /// UDS socket path for communication
    /// Standard production path: var/run/aos/{tenant_id}/worker.sock
    /// Development path: var/run/worker.sock (relative to cwd)
    #[arg(long, env = "AOS_WORKER_SOCKET")]
    pub uds_path: Option<std::path::PathBuf>,

    /// Manifest hash (preferred) to fetch/verify
    #[arg(long, env = "AOS_MANIFEST_HASH")]
    pub manifest_hash: Option<String>,

    /// Path to manifest YAML/JSON file (fallback when hash fetch is unavailable)
    #[arg(long, env = "AOS_WORKER_MANIFEST")]
    pub manifest: Option<std::path::PathBuf>,

    /// Path to model directory (auto-discovered from AOS_MODEL_PATH)
    #[arg(long, env = "AOS_MODEL_PATH")]
    pub model_path: Option<std::path::PathBuf>,

    /// Path to tokenizer JSON file (auto-discovered from AOS_TOKENIZER_PATH or model directory)
    #[arg(long, env = "AOS_TOKENIZER_PATH")]
    pub tokenizer: Option<std::path::PathBuf>,

    /// Backend choice (auto, metal, coreml, mlx, mock [debug-only])
    #[arg(long, default_value = "auto")]
    pub backend: String,

    /// Pin the base model in cache for the worker lifetime
    #[arg(long, default_value_t = false)]
    pub pin_base_model: bool,

    /// Pin budget in bytes for base model residency
    #[arg(long)]
    pub pin_budget_bytes: Option<u64>,

    /// Pin conflict mode when pin limit is reached (shadow|enforce)
    #[arg(long, value_parser = ["shadow", "enforce"])]
    pub pin_conflict_mode: Option<String>,

    /// Adapter cache budget in bytes
    #[arg(long, env = "AOS_ADAPTER_CACHE_BYTES")]
    pub adapter_cache_bytes: Option<u64>,

    /// Worker ID (auto-generated if not provided)
    #[arg(long, env = "WORKER_ID")]
    pub worker_id: Option<String>,

    /// Maximum number of tokens allowed per inference request.
    /// Acts as a hard worker-level ceiling over the manifest's policy.
    #[arg(long, env = "AOS_MAX_TOKENS_LIMIT")]
    pub max_tokens_limit: Option<usize>,

    /// Thread pool size for handling concurrent worker tasks
    #[arg(long, env = "AOS_LIMIT_THREAD_POOL_SIZE")]
    pub thread_pool_size: Option<usize>,

    /// Maximum total process memory allowed (MB)
    #[arg(long, env = "AOS_LIMIT_MAX_TOTAL_MEMORY_MB")]
    pub max_total_memory_mb: Option<u64>,

    /// Jitter factor (0.0 - 1.0) for randomizing retry hints
    #[arg(long, env = "AOS_LIMIT_JITTER_FACTOR")]
    pub jitter_factor: Option<f64>,

    /// Base retry hint in milliseconds for throttling clients
    #[arg(long, env = "AOS_LIMIT_BASE_RETRY_HINT_MS")]
    pub base_retry_hint_ms: Option<u64>,

    /// Maximum retry hint in milliseconds for throttling clients
    #[arg(long, env = "AOS_LIMIT_MAX_RETRY_HINT_MS")]
    pub max_retry_hint_ms: Option<u64>,

    /// Control plane URL for fatal error reporting
    #[arg(long, env = "AOS_CP_URL", default_value = "http://127.0.0.1:18080")]
    pub cp_url: String,
    /// Enable backend coordinator (primary + fallback) for runtime failover
    #[arg(long, env = "AOS_COORDINATOR_ENABLED", default_value_t = false)]
    pub coordinator_enabled: bool,

    /// Enable strict mode (fail-closed boot)
    /// When enabled:
    /// - Worker public key must exist (var/keys/worker_signing.pub)
    /// - Tokens from CP are required for all requests
    #[arg(long, env = "AOS_STRICT")]
    pub strict: bool,
}

/// Exit codes for worker process control
///
/// These codes determine restart behavior:
/// - 0: Graceful shutdown (don't restart)
/// - 1: Config/validation error (don't restart - requires manual fix)
/// - 2: Transient error (restart with backoff)
/// - 3: Fatal error (don't restart - requires investigation)
pub const EXIT_SUCCESS: i32 = 0;
pub const EXIT_CONFIG_ERROR: i32 = 1;
pub const EXIT_TRANSIENT_ERROR: i32 = 2;
pub const EXIT_FATAL_ERROR: i32 = 3;

/// Determine exit code based on error type
pub fn error_to_exit_code(err: &AosError) -> i32 {
    match err {
        // Config/validation errors should not restart (need manual fix)
        AosError::Config(_) | AosError::Validation(_) => EXIT_CONFIG_ERROR,

        // Network/transient errors should restart with backoff
        AosError::Network(_) | AosError::Timeout { .. } => EXIT_TRANSIENT_ERROR,

        // Fatal errors (internal, cache corruption, etc.) should not restart
        AosError::Internal(_) | AosError::CacheCorruption { .. } => EXIT_FATAL_ERROR,

        // Default: treat as transient for unknown error types
        _ => EXIT_TRANSIENT_ERROR,
    }
}

pub fn is_prod_runtime() -> bool {
    match std::env::var("AOS_RUNTIME_MODE") {
        Ok(mode) => matches!(mode.to_lowercase().as_str(), "prod" | "production"),
        Err(_) => false,
    }
}
