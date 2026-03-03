use adapteros_core::error_codes;
use axum::http::StatusCode;

#[derive(Debug, Clone)]
pub struct NormalizedErrorCode {
    pub primary: String,
    pub legacy: Option<String>,
    pub normalized: bool,
}

impl NormalizedErrorCode {
    fn unchanged(code: &str) -> Self {
        Self {
            primary: code.to_string(),
            legacy: None,
            normalized: false,
        }
    }

    fn mapped(primary: &'static str, legacy: &str) -> Self {
        let changed = primary != legacy;
        Self {
            primary: primary.to_string(),
            legacy: if changed {
                Some(legacy.to_string())
            } else {
                None
            },
            normalized: changed,
        }
    }
}

const CANONICAL_ERROR_CODES: &[&str] = &[
    "BAD_REQUEST",
    "VALIDATION_ERROR",
    "SERIALIZATION_ERROR",
    "PARSE_ERROR",
    "INVALID_HASH",
    "INVALID_CPID",
    "INVALID_MANIFEST",
    "ADAPTER_NOT_IN_MANIFEST",
    "ADAPTER_NOT_IN_EFFECTIVE_SET",
    "KERNEL_LAYOUT_MISMATCH",
    "CHAT_TEMPLATE_ERROR",
    "MISSING_FIELD",
    "INVALID_TENANT_ID",
    "INVALID_SESSION_ID",
    "INVALID_SEALED_DATA",
    "FEATURE_DISABLED",
    "PREFLIGHT_FAILED",
    "INCOMPATIBLE_SCHEMA_VERSION",
    "ADAPTER_BASE_MODEL_MISMATCH",
    "INCOMPATIBLE_BASE_MODEL",
    "UNSUPPORTED_BACKEND",
    "HASH_INTEGRITY_FAILURE",
    "VERSION_NOT_PROMOTABLE",
    "DETERMINISM_ERROR",
    "UNAUTHORIZED",
    "TOKEN_MISSING",
    "TOKEN_INVALID",
    "TOKEN_SIGNATURE_INVALID",
    "TOKEN_EXPIRED",
    "TOKEN_REVOKED",
    "INVALID_ISSUER",
    "INVALID_AUDIENCE",
    "INVALID_API_KEY",
    "SESSION_EXPIRED",
    "SESSION_LOCKED",
    "DEVICE_MISMATCH",
    "INVALID_CREDENTIALS",
    "FORBIDDEN",
    "PERMISSION_DENIED",
    "TENANT_ISOLATION_ERROR",
    "CSRF_ERROR",
    "INSUFFICIENT_ROLE",
    "MFA_REQUIRED",
    "POLICY_VIOLATION",
    "POLICY_ERROR",
    "SIGNATURE_REQUIRED",
    "SIGNATURE_INVALID",
    "REPO_ARCHIVED",
    "DETERMINISM_VIOLATION",
    "EGRESS_VIOLATION",
    "SSRF_BLOCKED",
    "ISOLATION_VIOLATION",
    "PERFORMANCE_VIOLATION",
    "ANOMALY_DETECTED",
    "SYSTEM_QUARANTINED",
    "ADAPTER_TENANT_MISMATCH",
    "INTEGRITY_VIOLATION",
    "CHECKPOINT_INTEGRITY_FAILED",
    "NOT_FOUND",
    "ADAPTER_NOT_FOUND",
    "MODEL_NOT_FOUND",
    "CACHE_ENTRY_NOT_FOUND",
    "REPO_NOT_FOUND",
    "VERSION_NOT_FOUND",
    "CONFLICT",
    "ADAPTER_HASH_MISMATCH",
    "ADAPTER_LAYER_HASH_MISMATCH",
    "POLICY_HASH_MISMATCH",
    "PROMOTION_ERROR",
    "MODEL_ACQUISITION_IN_PROGRESS",
    "DUPLICATE_REQUEST",
    "ADAPTER_IN_FLIGHT",
    "REPO_ALREADY_EXISTS",
    "REASONING_LOOP_DETECTED",
    "TOO_MANY_REQUESTS",
    "BACKPRESSURE",
    "THUNDERING_HERD_REJECTED",
    "CLIENT_CLOSED_REQUEST",
    "INTERNAL_ERROR",
    "EXPORT_FAILED",
    "DATABASE_ERROR",
    "CRYPTO_ERROR",
    "CONFIG_ERROR",
    "RAG_ERROR",
    "ROUTING_BYPASS",
    "REPLAY_ERROR",
    "MIGRATION_FILE_MISSING",
    "MIGRATION_CHECKSUM_MISMATCH",
    "SCHEMA_VERSION_MISMATCH",
    "SCHEMA_CONTRACT_VIOLATION",
    "RATE_LIMITER_NOT_CONFIGURED",
    "BAD_GATEWAY",
    "NETWORK_ERROR",
    "BASE_LLM_ERROR",
    "UDS_CONNECTION_FAILED",
    "INVALID_RESPONSE",
    "DOWNLOAD_FAILED",
    "SERVICE_UNAVAILABLE",
    "MEMORY_PRESSURE",
    "WORKER_NOT_RESPONDING",
    "CIRCUIT_BREAKER_OPEN",
    "CIRCUIT_BREAKER_HALF_OPEN",
    "HEALTH_CHECK_FAILED",
    "ADAPTER_NOT_LOADED",
    "ADAPTER_NOT_LOADABLE",
    "CACHE_BUDGET_EXCEEDED",
    "CPU_THROTTLED",
    "OUT_OF_MEMORY",
    "FD_EXHAUSTED",
    "THREAD_POOL_SATURATED",
    "GPU_UNAVAILABLE",
    "DISK_FULL",
    "TEMP_DIR_UNAVAILABLE",
    "CACHE_STALE",
    "CACHE_EVICTION",
    "STREAM_DISCONNECTED",
    "EVENT_GAP_DETECTED",
    "MODEL_NOT_READY",
    "NO_COMPATIBLE_WORKER",
    "WORKER_DEGRADED",
    "WORKER_ID_UNAVAILABLE",
    "GATEWAY_TIMEOUT",
    "REQUEST_TIMEOUT",
    "DEV_BYPASS_IN_RELEASE",
    "JWT_MODE_NOT_CONFIGURED",
    "API_KEY_MODE_NOT_CONFIGURED",
    "PAYLOAD_TOO_LARGE",
];

fn canonical_alias(code: &str) -> Option<&'static str> {
    match code {
        "INTERNAL_SERVER_ERROR" => Some(error_codes::INTERNAL_ERROR),
        "DB_ERROR" => Some(error_codes::DATABASE_ERROR),
        "WORKER_UNAVAILABLE" => Some(error_codes::SERVICE_UNAVAILABLE),
        "MODEL_PATH_MISSING" => Some(error_codes::NOT_FOUND),
        "MODEL_PATH_FORBIDDEN" => Some(error_codes::FORBIDDEN),
        "MODEL_COMPATIBILITY_FAILED" => Some(error_codes::VALIDATION_ERROR),
        "ROUTING_ERROR" | "ROUTING_CHAIN_ERROR" | "ADAPTER_FETCH_ERROR" => {
            Some(error_codes::INTERNAL_ERROR)
        }
        "VERIFICATION_FAILED" | "VERIFICATION_ERROR" => Some(error_codes::INTEGRITY_VIOLATION),
        "PROMOTION_FAILED" => Some(error_codes::PROMOTION_ERROR),
        "ROLLBACK_FAILED" => Some(error_codes::CONFLICT),
        "REPOSITORY_NOT_FOUND" => Some(error_codes::REPO_NOT_FOUND),
        "NODE_NOT_FOUND" => Some(error_codes::NOT_FOUND),
        "MISSING_TABLE" => Some(error_codes::SCHEMA_VERSION_MISMATCH),
        "PATH_TRAVERSAL" | "INVALID_PATH" => Some(error_codes::BAD_REQUEST),
        "TOKEN_ERROR" | "INVALID_TOKEN" => Some(error_codes::TOKEN_INVALID),
        "SESSION_INVALID" => Some(error_codes::UNAUTHORIZED),
        "TENANT_HEADER_MISSING" | "TENANT_ACCESS_DENIED" | "TENANT_MISMATCH" => {
            Some(error_codes::TENANT_ISOLATION_ERROR)
        }
        "SUPERVISOR_NOT_CONFIGURED" | "LIFECYCLE_ERROR" => Some(error_codes::SERVICE_UNAVAILABLE),
        "DATASET_NOT_FOUND" => Some(error_codes::NOT_FOUND),
        "DATASET_ERROR" | "TRAINING_ERROR" | "TRAINING_START_FAILED" => {
            Some(error_codes::INTERNAL_ERROR)
        }
        "RATE_LIMIT_EXCEEDED" => Some(error_codes::TOO_MANY_REQUESTS),
        "NOT_IMPLEMENTED" => Some(error_codes::FEATURE_DISABLED),
        _ => None,
    }
}

pub fn is_canonical_error_code(code: &str) -> bool {
    CANONICAL_ERROR_CODES.contains(&code)
}

pub fn canonical_code_for_status(status: StatusCode) -> &'static str {
    match status {
        StatusCode::BAD_REQUEST => error_codes::BAD_REQUEST,
        StatusCode::UNAUTHORIZED => error_codes::UNAUTHORIZED,
        StatusCode::FORBIDDEN => error_codes::FORBIDDEN,
        StatusCode::NOT_FOUND => error_codes::NOT_FOUND,
        StatusCode::CONFLICT => error_codes::CONFLICT,
        StatusCode::PAYLOAD_TOO_LARGE => error_codes::PAYLOAD_TOO_LARGE,
        StatusCode::TOO_MANY_REQUESTS => error_codes::TOO_MANY_REQUESTS,
        StatusCode::BAD_GATEWAY => error_codes::BAD_GATEWAY,
        StatusCode::SERVICE_UNAVAILABLE => error_codes::SERVICE_UNAVAILABLE,
        StatusCode::GATEWAY_TIMEOUT => error_codes::GATEWAY_TIMEOUT,
        _ if status.is_client_error() => error_codes::BAD_REQUEST,
        _ if status.is_server_error() => error_codes::INTERNAL_ERROR,
        _ => error_codes::INTERNAL_ERROR,
    }
}

pub fn normalize_error_code(code: &str) -> NormalizedErrorCode {
    if code.is_empty() {
        return NormalizedErrorCode::mapped(error_codes::INTERNAL_ERROR, "");
    }

    if is_canonical_error_code(code) {
        return NormalizedErrorCode::unchanged(code);
    }

    if let Some(mapped) = canonical_alias(code) {
        return NormalizedErrorCode::mapped(mapped, code);
    }

    NormalizedErrorCode::unchanged(code)
}

pub fn normalize_dynamic_error_code(code: &str, status: StatusCode) -> NormalizedErrorCode {
    let normalized = normalize_error_code(code);
    if normalized.normalized || is_canonical_error_code(&normalized.primary) {
        return normalized;
    }

    let fallback = canonical_code_for_status(status);
    NormalizedErrorCode::mapped(fallback, code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_passthrough() {
        let out = normalize_error_code("BAD_REQUEST");
        assert_eq!(out.primary, "BAD_REQUEST");
        assert!(!out.normalized);
    }

    #[test]
    fn alias_maps_to_canonical() {
        let out = normalize_error_code("INTERNAL_SERVER_ERROR");
        assert_eq!(out.primary, "INTERNAL_ERROR");
        assert_eq!(out.legacy.as_deref(), Some("INTERNAL_SERVER_ERROR"));
    }

    #[test]
    fn dynamic_unknown_falls_back_by_status() {
        let out = normalize_dynamic_error_code("SOME_RUNTIME_CODE", StatusCode::BAD_REQUEST);
        assert_eq!(out.primary, "BAD_REQUEST");
        assert_eq!(out.legacy.as_deref(), Some("SOME_RUNTIME_CODE"));
    }
}
