//! API client infrastructure
//!
//! Provides typed HTTP client and SSE streaming for communicating
//! with the AdapterOS backend.

pub mod client;
pub mod error;
pub mod sse;

pub use client::{
    AdapterScoreResponse, ApiClient, AuditChainEntry, AuditChainResponse, AuditLogEntry,
    AuditLogsQuery, AuditLogsResponse, ChainVerificationResponse, CodePolicy,
    CollectionDetailResponse, CollectionDocumentInfo, CollectionListResponse, CollectionResponse,
    ComplianceAuditResponse, ComplianceControl, CreateCollectionRequest, CreateStackRequest,
    FederationAuditResponse, GetCodePolicyResponse, HostChainSummary, InferenceRequest,
    ProcessAlertResponse, ProcessAnomalyResponse, ProcessCrashDumpResponse,
    ProcessHealthMetricResponse, ProcessLogResponse, ProcessMonitoringRuleResponse,
    PublishAdapterRequest, RegisterRepositoryRequest, RepositoryAdapter, RepositoryDetailResponse,
    RepositoryListResponse, RepositoryResponse, RepositoryVersion, RoutingCandidateResponse,
    RoutingDebugRequest, RoutingDebugResponse, RoutingDecisionResponse, RoutingDecisionsQuery,
    RoutingDecisionsResponse, ScanStatusResponse, StackResponse, UpdateCodePolicyRequest,
    UpdateStackRequest, WorkerMetricsResponse, WorkflowType,
};
pub use error::{ApiError, ApiResult};
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
