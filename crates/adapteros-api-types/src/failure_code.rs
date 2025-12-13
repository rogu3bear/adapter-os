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
        }
    }

    pub fn from_str(code: &str) -> Option<Self> {
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
            _ => None,
        }
    }
}
