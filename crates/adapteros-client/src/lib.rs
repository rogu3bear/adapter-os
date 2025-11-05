pub mod types;

#[cfg(not(target_arch = "wasm32"))]
pub mod native;

#[cfg(not(target_arch = "wasm32"))]
pub mod uds;

#[cfg(target_arch = "wasm32")]
pub mod wasm;

use adapteros_api_types::telemetry::TelemetryBundleResponse;
use anyhow::Result;
// First export API types
pub use adapteros_api_types::*;
// Then export client-specific types (these will override API types where they conflict)
pub use types::*;

#[cfg(not(target_arch = "wasm32"))]
pub use native::NativeClient as DefaultClient;

#[cfg(target_arch = "wasm32")]
pub use wasm::WasmClient as DefaultClient;

// Re-export UDS client for convenience
#[cfg(not(target_arch = "wasm32"))]
pub use uds::{ConnectionPool, Signal, UdsClient, UdsClientError};

/// Unified client trait for all AdapterOS API access
///
/// # Citations
/// - CONTRIBUTING.md L118-122: "Follow Rust naming conventions", "Use `cargo clippy` for linting"
/// - Policy Pack #1 (Egress): "MUST NOT open listening TCP ports; use Unix domain sockets only"
pub trait AdapterOSClient {
    // Health & Auth
    fn health(&self) -> impl std::future::Future<Output = Result<HealthResponse>> + Send;
    fn login(
        &self,
        req: LoginRequest,
    ) -> impl std::future::Future<Output = Result<LoginResponse>> + Send;
    fn logout(&self) -> impl std::future::Future<Output = Result<()>> + Send;
    fn me(&self) -> impl std::future::Future<Output = Result<UserInfoResponse>> + Send;

    // Tenants
    fn list_tenants(&self)
        -> impl std::future::Future<Output = Result<Vec<TenantResponse>>> + Send;
    fn create_tenant(
        &self,
        req: CreateTenantRequest,
    ) -> impl std::future::Future<Output = Result<TenantResponse>> + Send;

    // Adapters
    fn list_adapters(
        &self,
    ) -> impl std::future::Future<Output = Result<Vec<AdapterResponse>>> + Send;
    fn register_adapter(
        &self,
        req: RegisterAdapterRequest,
    ) -> impl std::future::Future<Output = Result<AdapterResponse>> + Send;
    fn evict_adapter(
        &self,
        adapter_id: &str,
    ) -> impl std::future::Future<Output = Result<()>> + Send;
    fn pin_adapter(
        &self,
        adapter_id: &str,
        pinned: bool,
    ) -> impl std::future::Future<Output = Result<()>> + Send;

    // Memory Management
    fn get_memory_usage(
        &self,
    ) -> impl std::future::Future<Output = Result<MemoryUsageResponse>> + Send;

    // Training
    fn start_adapter_training(
        &self,
        req: StartTrainingRequest,
    ) -> impl std::future::Future<Output = Result<TrainingSessionResponse>> + Send;
    fn get_training_session(
        &self,
        session_id: &str,
    ) -> impl std::future::Future<Output = Result<TrainingSessionResponse>> + Send;
    fn list_training_sessions(
        &self,
    ) -> impl std::future::Future<Output = Result<Vec<TrainingSessionResponse>>> + Send;

    // Telemetry
    fn get_telemetry_events(
        &self,
        filters: TelemetryFilters,
    ) -> impl std::future::Future<Output = Result<Vec<TelemetryEvent>>> + Send;

    // Nodes
    fn list_nodes(&self) -> impl std::future::Future<Output = Result<Vec<NodeResponse>>> + Send;
    fn register_node(
        &self,
        req: RegisterNodeRequest,
    ) -> impl std::future::Future<Output = Result<NodeResponse>> + Send;

    // Plans
    fn list_plans(
        &self,
        tenant_id: Option<String>,
    ) -> impl std::future::Future<Output = Result<Vec<PlanResponse>>> + Send;
    fn build_plan(
        &self,
        req: BuildPlanRequest,
    ) -> impl std::future::Future<Output = Result<JobResponse>> + Send;

    // Workers
    fn list_workers(
        &self,
        tenant_id: Option<String>,
    ) -> impl std::future::Future<Output = Result<Vec<WorkerResponse>>> + Send;
    fn spawn_worker(
        &self,
        req: SpawnWorkerRequest,
    ) -> impl std::future::Future<Output = Result<()>> + Send;

    // CP Operations
    fn promote_cp(
        &self,
        req: PromoteCPRequest,
    ) -> impl std::future::Future<Output = Result<PromotionResponse>> + Send;
    fn promotion_gates(
        &self,
        cpid: String,
    ) -> impl std::future::Future<Output = Result<PromotionGatesResponse>> + Send;
    fn rollback_cp(
        &self,
        req: RollbackCPRequest,
    ) -> impl std::future::Future<Output = Result<RollbackResponse>> + Send;

    // Jobs
    fn list_jobs(
        &self,
        tenant_id: Option<String>,
    ) -> impl std::future::Future<Output = Result<Vec<JobResponse>>> + Send;

    // Models
    fn import_model(
        &self,
        req: ImportModelRequest,
    ) -> impl std::future::Future<Output = Result<()>> + Send;

    // Policies
    fn list_policies(
        &self,
    ) -> impl std::future::Future<Output = Result<Vec<PolicyPackResponse>>> + Send;
    fn get_policy(
        &self,
        cpid: String,
    ) -> impl std::future::Future<Output = Result<PolicyPackResponse>> + Send;
    fn validate_policy(
        &self,
        req: ValidatePolicyRequest,
    ) -> impl std::future::Future<Output = Result<PolicyValidationResponse>> + Send;
    fn apply_policy(
        &self,
        req: ApplyPolicyRequest,
    ) -> impl std::future::Future<Output = Result<PolicyPackResponse>> + Send;

    // Telemetry Bundles
    fn list_telemetry_bundles(
        &self,
    ) -> impl std::future::Future<Output = Result<Vec<TelemetryBundleResponse>>> + Send;

    // Code Intelligence
    fn register_repo(
        &self,
        req: RegisterRepoRequest,
    ) -> impl std::future::Future<Output = Result<RepoResponse>> + Send;
    fn scan_repo(
        &self,
        req: ScanRepoRequest,
    ) -> impl std::future::Future<Output = Result<JobResponse>> + Send;
    fn list_repos(&self) -> impl std::future::Future<Output = Result<Vec<RepoResponse>>> + Send;
    fn list_adapters_by_tenant(
        &self,
        tenant_id: String,
    ) -> impl std::future::Future<Output = Result<ListAdaptersResponse>> + Send;
    fn get_adapter_activations(
        &self,
    ) -> impl std::future::Future<Output = Result<Vec<ActivationData>>> + Send;
    fn create_commit_delta(
        &self,
        req: CommitDeltaRequest,
    ) -> impl std::future::Future<Output = Result<CommitDeltaResponse>> + Send;
    fn get_commit_details(
        &self,
        repo_id: String,
        commit: String,
    ) -> impl std::future::Future<Output = Result<CommitDetailsResponse>> + Send;

    // Routing Inspector
    fn extract_router_features(
        &self,
        req: RouterFeaturesRequest,
    ) -> impl std::future::Future<Output = Result<RouterFeaturesResponse>> + Send;
    fn score_adapters(
        &self,
        req: ScoreAdaptersRequest,
    ) -> impl std::future::Future<Output = Result<ScoreAdaptersResponse>> + Send;

    // Patch Lab
    fn propose_patch(
        &self,
        req: ProposePatchRequest,
    ) -> impl std::future::Future<Output = Result<ProposePatchResponse>> + Send;
    fn validate_patch(
        &self,
        req: ValidatePatchRequest,
    ) -> impl std::future::Future<Output = Result<ValidatePatchResponse>> + Send;
    fn apply_patch(
        &self,
        req: ApplyPatchRequest,
    ) -> impl std::future::Future<Output = Result<ApplyPatchResponse>> + Send;

    // Code Policy
    fn get_code_policy(
        &self,
    ) -> impl std::future::Future<Output = Result<GetCodePolicyResponse>> + Send;
    fn update_code_policy(
        &self,
        req: UpdateCodePolicyRequest,
    ) -> impl std::future::Future<Output = Result<()>> + Send;

    // Metrics Dashboard
    fn get_code_metrics(
        &self,
        req: CodeMetricsRequest,
    ) -> impl std::future::Future<Output = Result<CodeMetricsResponse>> + Send;
    fn compare_metrics(
        &self,
        req: CompareMetricsRequest,
    ) -> impl std::future::Future<Output = Result<CompareMetricsResponse>> + Send;
}
