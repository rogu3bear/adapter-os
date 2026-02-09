//! API client infrastructure
//!
//! Provides typed HTTP client and SSE streaming for communicating
//! with the adapterOS backend.

pub mod client;
pub mod diagnostic_bundle;
pub mod error;
pub mod error_reporter;
pub mod reviews;
pub mod sse;
pub mod types;

// Re-export ApiClient and CSRF helper from client module
pub use client::{csrf_token_from_cookie, ApiClient};

// Re-export types from types module
pub use types::{
    AdapterScoreResponse, AddDocumentRequest, ApplyPolicyRequest, AuditChainEntry,
    AuditChainResponse, AuditLogEntry, AuditLogsQuery, AuditLogsResponse,
    ChainVerificationResponse, ChunkListResponse, ChunkResponse, CodePolicy,
    CollectionDetailResponse, CollectionDocumentInfo, CollectionListResponse, CollectionResponse,
    ComplianceAuditResponse, ComplianceControl, ComponentHealth, ComponentStatus,
    CreateCollectionRequest, CreateErrorAlertRuleRequest, CreateMonitoringRuleRequest,
    CreateStackRequest, CreateTrainingJobRequest, DatasetListResponse, DatasetPreviewResponse,
    DatasetResponse, DatasetSafetyCheckResult, DatasetStatisticsResponse, DetectedFeaturesResponse,
    DocumentListParams, DocumentListResponse, DocumentResponse, ErrorAlertHistoryListResponse,
    ErrorAlertHistoryResponse, ErrorAlertRuleResponse, ErrorAlertRulesListResponse,
    FederationAuditResponse, FileValidationError, GetCodePolicyResponse, HostChainSummary,
    InferenceRequest, InferenceTraceDetailResponse, InferenceTraceResponse, LoadAverageInfo,
    ModelArchitectureSummary, ModelListResponse, ModelWithStatsResponse, PolicyPackResponse,
    PolicyValidationResponse, PreprocessedCacheCountResponse, PreprocessedCacheEntry,
    PreprocessedCacheListResponse, ProcessAlertResponse, ProcessAnomalyResponse,
    ProcessCrashDumpResponse, ProcessDocumentResponse, ProcessHealthMetricResponse,
    ProcessLogResponse, ProcessMonitoringRuleResponse, ReadyzCheck, ReadyzChecks, ReadyzResponse,
    ResourceUsageInfo, RoutingCandidateResponse, RoutingDebugRequest, RoutingDebugResponse,
    RoutingDecisionChainResponse, RoutingDecisionResponse, RoutingDecisionsQuery,
    RoutingDecisionsResponse, SafetySignals, SearchResponse, SearchResultItem, ServiceStatus,
    StackResponse, SystemHealthResponse, SystemOverviewResponse, SystemReadyResponse,
    TimingBreakdown, TokenDecision, TraceEvent, TraceReceiptSummary, TraceSearchQuery,
    TrainingConfigRequest, UiInferenceTraceDetailResponse, UiTraceReceiptSummary,
    UpdateCodePolicyRequest, UpdateErrorAlertRuleRequest, UpdateStackRequest,
    ValidateAllFilesResponse, ValidateFileRequest, ValidateFileResponse, ValidatePolicyRequest,
    WorkflowType,
};

// Re-export types from adapteros-api-types via client module
pub use client::{
    ActivityEventResponse, AllModelsStatusResponse, AneMemoryStatus, ApiKeyInfo,
    ApiKeyListResponse, BaseModelStatusResponse, CanonicalRow, CreateApiKeyRequest,
    CreateApiKeyResponse, CreateRoutingRuleRequest, DatasetFileResponse, DatasetManifest,
    DatasetVersionsResponse, EmbeddingBenchmarkReport, EmbeddingBenchmarksQuery,
    EmbeddingBenchmarksResponse, InFlightAdaptersResponse, JsonlValidationDiagnostic,
    ListUsersResponse, ModelLoadStatus, ModelStatusResponse, RegisterRepositoryRequest,
    RegisterRepositoryResponse, RepositoryDetailResponse, RepositoryInfo, RepositoryListResponse,
    RevokeApiKeyResponse, RoutingRuleResponse, RoutingRulesResponse, ScanJobResponse,
    ScanRepositoryRequest, SeedModelRequest, SeedModelResponse, TenantListResponse, TenantSummary,
    UploadDatasetResponse, UserResponse, WorkerMetricsResponse,
};

pub use diagnostic_bundle::DiagnosticBundle;
pub use error::{ApiError, ApiResult};
pub use error_reporter::{report_error, report_error_with_toast, report_ui_panic};
pub use sse::{
    use_sse, use_sse_json, use_sse_json_events, use_sse_json_with_config, use_sse_with_config,
    CircuitBreakerConfig, SseConnection, SseEvent, SseState,
};

/// Error raised when the API base URL cannot be resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiBaseUrlError {
    Missing,
    Invalid(String),
}

impl std::fmt::Display for ApiBaseUrlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiBaseUrlError::Missing => write!(f, "API base URL is not configured"),
            ApiBaseUrlError::Invalid(msg) => write!(f, "Invalid API base URL: {msg}"),
        }
    }
}

impl std::error::Error for ApiBaseUrlError {}

/// Base URL for API requests (configured at runtime)
pub fn api_base_url_checked() -> Result<String, ApiBaseUrlError> {
    // Prefer a compile-time override when provided (e.g., via build env)
    if let Some(env_base) = option_env!("AOS_API_BASE_URL") {
        let trimmed = env_base.trim();
        if trimmed.is_empty() {
            return Err(ApiBaseUrlError::Missing);
        }
        return Ok(trimmed.to_string());
    }

    // Fallback to browser origin when available
    let candidate = web_sys::window()
        .and_then(|w| w.location().origin().ok())
        .unwrap_or_default();

    if candidate.is_empty() || candidate == "null" || candidate.starts_with("file://") {
        return Err(ApiBaseUrlError::Missing);
    }

    if !(candidate.starts_with("http://") || candidate.starts_with("https://")) {
        return Err(ApiBaseUrlError::Invalid(candidate));
    }

    Ok(candidate)
}

pub fn api_base_url() -> String {
    api_base_url_checked()
        .unwrap_or_else(|_| adapteros_api_types::defaults::DEFAULT_SERVER_URL.to_string())
}

/// UI build version for version skew detection against backend.
///
/// Automatically set from CARGO_PKG_VERSION by build.rs.
/// Can be overridden by setting AOS_UI_BUILD_VERSION before compilation.
pub fn ui_build_version() -> &'static str {
    option_env!("AOS_UI_BUILD_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"))
}
