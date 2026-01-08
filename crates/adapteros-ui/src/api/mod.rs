//! API client infrastructure
//!
//! Provides typed HTTP client and SSE streaming for communicating
//! with the AdapterOS backend.

pub mod client;
pub mod error;
pub mod error_reporter;
pub mod sse;

pub use client::{
    AdapterScoreResponse, ApiClient, ApiKeyInfo, ApiKeyListResponse, AuditChainEntry,
    AuditChainResponse, AuditLogEntry, AuditLogsQuery, AuditLogsResponse,
    ChainVerificationResponse, CodePolicy, CollectionDetailResponse, CollectionDocumentInfo,
    CollectionListResponse, CollectionResponse, ColumnStatistics, ComplianceAuditResponse,
    ComplianceControl, CreateApiKeyRequest, CreateApiKeyResponse, CreateCollectionRequest,
    CreateStackRequest, DatasetListResponse, DatasetResponse, DatasetStatisticsResponse,
    FederationAuditResponse, GetCodePolicyResponse, HostChainSummary, InferenceRequest,
    ListUsersResponse, ProcessAlertResponse, ProcessAnomalyResponse, ProcessCrashDumpResponse,
    ProcessHealthMetricResponse, ProcessLogResponse, ProcessMonitoringRuleResponse,
    PublishAdapterRequest, RegisterRepositoryRequest, RepositoryAdapter, RepositoryDetailResponse,
    RepositoryListResponse, RepositoryResponse, RepositoryVersion, RevokeApiKeyResponse,
    RoutingCandidateResponse, RoutingDebugRequest, RoutingDebugResponse, RoutingDecisionResponse,
    RoutingDecisionsQuery, RoutingDecisionsResponse, ScanStatusResponse, SplitStatistics,
    StackResponse, UpdateCodePolicyRequest, UpdateStackRequest, UserResponse,
    WorkerMetricsResponse, WorkflowType,
};
pub use error::{ApiError, ApiResult};
pub use error_reporter::report_error;
pub use sse::{
    use_sse, use_sse_json, use_sse_json_events, use_sse_json_with_config, use_sse_with_config,
    CircuitBreakerConfig, SseConnection, SseEvent, SseState,
};

/// Base URL for API requests (configured at runtime)
pub fn api_base_url() -> String {
    // In development, use the same origin
    // In production, this could be configured via environment
    web_sys::window()
        .and_then(|w| w.location().origin().ok())
        .unwrap_or_else(|| String::from("http://localhost:8080"))
}
