use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Structured failure codes for smoke-test visibility and UI surfacing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FailureCode {
    MigrationInvalid,
    ModelLoadFailed,
    OutOfMemory,
    TraceWriteFailed,
    ReceiptMismatch,
    PolicyDivergence,
    BackendFallback,
    TenantAccessDenied,
    /// KV cache quota exceeded - tenant has exhausted KV cache allocation
    KvQuotaExceeded,
    /// Worker is at capacity and cannot accept more requests (retryable)
    WorkerOverloaded,

    // Boot-specific failure codes
    /// Database is unreachable during boot
    BootDbUnreachable,
    /// Database migration failed during boot
    BootMigrationFailed,
    /// Seed derivation or initialization failed during boot
    BootSeedFailed,
    /// No workers registered or available during boot
    BootNoWorkers,
    /// No models loaded or available during boot
    BootNoModels,
    /// Dependency timeout during boot
    BootDependencyTimeout,
    /// Background task failed to spawn during boot
    BootBackgroundTaskFailed,
    /// Configuration invalid during boot
    BootConfigInvalid,
}

impl FailureCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            FailureCode::MigrationInvalid => "MIGRATION_INVALID",
            FailureCode::ModelLoadFailed => "MODEL_LOAD_FAILED",
            FailureCode::OutOfMemory => "OUT_OF_MEMORY",
            FailureCode::TraceWriteFailed => "TRACE_WRITE_FAILED",
            FailureCode::ReceiptMismatch => "RECEIPT_MISMATCH",
            FailureCode::PolicyDivergence => "POLICY_DIVERGENCE",
            FailureCode::BackendFallback => "BACKEND_FALLBACK",
            FailureCode::TenantAccessDenied => "TENANT_ACCESS_DENIED",
            FailureCode::KvQuotaExceeded => "KV_QUOTA_EXCEEDED",
            FailureCode::WorkerOverloaded => "WORKER_OVERLOADED",
            FailureCode::BootDbUnreachable => "BOOT_DB_UNREACHABLE",
            FailureCode::BootMigrationFailed => "BOOT_MIGRATION_FAILED",
            FailureCode::BootSeedFailed => "BOOT_SEED_FAILED",
            FailureCode::BootNoWorkers => "BOOT_NO_WORKERS",
            FailureCode::BootNoModels => "BOOT_NO_MODELS",
            FailureCode::BootDependencyTimeout => "BOOT_DEPENDENCY_TIMEOUT",
            FailureCode::BootBackgroundTaskFailed => "BOOT_BACKGROUND_TASK_FAILED",
            FailureCode::BootConfigInvalid => "BOOT_CONFIG_INVALID",
        }
    }

    pub fn parse_code(code: &str) -> Option<Self> {
        match code {
            "MIGRATION_INVALID" => Some(FailureCode::MigrationInvalid),
            "MODEL_LOAD_FAILED" => Some(FailureCode::ModelLoadFailed),
            "OUT_OF_MEMORY" => Some(FailureCode::OutOfMemory),
            "TRACE_WRITE_FAILED" => Some(FailureCode::TraceWriteFailed),
            "RECEIPT_MISMATCH" => Some(FailureCode::ReceiptMismatch),
            "POLICY_DIVERGENCE" => Some(FailureCode::PolicyDivergence),
            "BACKEND_FALLBACK" => Some(FailureCode::BackendFallback),
            "TENANT_ACCESS_DENIED" => Some(FailureCode::TenantAccessDenied),
            "KV_QUOTA_EXCEEDED" => Some(FailureCode::KvQuotaExceeded),
            "WORKER_OVERLOADED" => Some(FailureCode::WorkerOverloaded),
            "BOOT_DB_UNREACHABLE" => Some(FailureCode::BootDbUnreachable),
            "BOOT_MIGRATION_FAILED" => Some(FailureCode::BootMigrationFailed),
            "BOOT_SEED_FAILED" => Some(FailureCode::BootSeedFailed),
            "BOOT_NO_WORKERS" => Some(FailureCode::BootNoWorkers),
            "BOOT_NO_MODELS" => Some(FailureCode::BootNoModels),
            "BOOT_DEPENDENCY_TIMEOUT" => Some(FailureCode::BootDependencyTimeout),
            "BOOT_BACKGROUND_TASK_FAILED" => Some(FailureCode::BootBackgroundTaskFailed),
            "BOOT_CONFIG_INVALID" => Some(FailureCode::BootConfigInvalid),
            _ => None,
        }
    }
}
