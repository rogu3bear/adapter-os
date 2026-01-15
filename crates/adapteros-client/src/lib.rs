#![allow(async_fn_in_trait)]

pub mod types;

#[cfg(not(target_arch = "wasm32"))]
pub mod native;

#[cfg(not(target_arch = "wasm32"))]
pub mod uds;

#[cfg(target_arch = "wasm32")]
pub mod wasm;

use anyhow::Result;
pub use types::*;

// Re-export telemetry types needed by trait
pub use adapteros_api_types::telemetry::{
    ApiTelemetryEvent as TelemetryEvent, TelemetryBundleResponse,
};

#[cfg(not(target_arch = "wasm32"))]
pub use native::NativeClient as DefaultClient;

#[cfg(target_arch = "wasm32")]
pub use wasm::WasmClient as DefaultClient;

// Re-export UDS client for convenience
#[cfg(not(target_arch = "wasm32"))]
pub use uds::{CancelTrainingResponse, ConnectionPool, Signal, UdsClient, UdsClientError};

/// Unified client trait for all adapterOS API access
///
/// # Citations
/// - CONTRIBUTING.md L118-122: "Follow Rust naming conventions", "Use `cargo clippy` for linting"
/// - Policy Pack #1 (Egress): "MUST NOT open listening TCP ports; use Unix domain sockets only"
pub trait adapterOSClient {
    // Health & Auth
    async fn health(&self) -> Result<HealthResponse>;
    async fn login(&self, req: LoginRequest) -> Result<LoginResponse>;
    async fn logout(&self) -> Result<()>;
    async fn me(&self) -> Result<UserInfoResponse>;

    // Tenants
    async fn list_tenants(&self) -> Result<Vec<TenantResponse>>;
    async fn create_tenant(&self, req: CreateTenantRequest) -> Result<TenantResponse>;

    // Adapters
    async fn list_adapters(&self) -> Result<Vec<AdapterResponse>>;
    async fn register_adapter(&self, req: RegisterAdapterRequest) -> Result<AdapterResponse>;
    async fn evict_adapter(&self, adapter_id: &str) -> Result<()>;
    async fn pin_adapter(&self, adapter_id: &str, pinned: bool) -> Result<()>;

    // Memory Management
    async fn get_memory_usage(&self) -> Result<MemoryUsageResponse>;

    // Training
    async fn start_adapter_training(
        &self,
        req: StartTrainingRequest,
    ) -> Result<TrainingSessionResponse>;
    async fn get_training_session(&self, session_id: &str) -> Result<TrainingSessionResponse>;
    async fn list_training_sessions(&self) -> Result<Vec<TrainingSessionResponse>>;

    // Telemetry
    async fn get_telemetry_events(&self, filters: TelemetryFilters) -> Result<Vec<TelemetryEvent>>;

    // Nodes
    async fn list_nodes(&self) -> Result<Vec<NodeResponse>>;
    async fn register_node(&self, req: RegisterNodeRequest) -> Result<NodeResponse>;

    // Plans
    async fn list_plans(&self, tenant_id: Option<String>) -> Result<Vec<PlanResponse>>;
    async fn build_plan(&self, req: BuildPlanRequest) -> Result<JobResponse>;

    // Workers
    async fn list_workers(&self, tenant_id: Option<String>) -> Result<Vec<WorkerResponse>>;
    async fn spawn_worker(&self, req: SpawnWorkerRequest) -> Result<()>;

    // CP Operations
    async fn promote_cp(&self, req: PromoteCPRequest) -> Result<PromotionResponse>;
    async fn promotion_gates(&self, cpid: String) -> Result<PromotionGatesResponse>;
    async fn rollback_cp(&self, req: RollbackCPRequest) -> Result<RollbackResponse>;

    // Jobs
    async fn list_jobs(&self, tenant_id: Option<String>) -> Result<Vec<JobResponse>>;

    // Models
    async fn import_model(&self, req: ImportModelRequest) -> Result<()>;

    // Policies
    async fn list_policies(&self) -> Result<Vec<PolicyPackResponse>>;
    async fn get_policy(&self, cpid: String) -> Result<PolicyPackResponse>;
    async fn validate_policy(&self, req: ValidatePolicyRequest)
        -> Result<PolicyValidationResponse>;
    async fn apply_policy(&self, req: ApplyPolicyRequest) -> Result<PolicyPackResponse>;

    // Telemetry Bundles
    async fn list_telemetry_bundles(&self) -> Result<Vec<TelemetryBundleResponse>>;

    // Code Intelligence
    async fn register_repo(&self, req: RegisterRepoRequest) -> Result<RepoResponse>;
    async fn scan_repo(&self, req: ScanRepoRequest) -> Result<JobResponse>;
    async fn list_repos(&self) -> Result<Vec<RepoResponse>>;
    async fn list_adapters_by_tenant(&self, tenant_id: String) -> Result<ListAdaptersResponse>;
    async fn get_adapter_activations(&self) -> Result<Vec<ActivationData>>;
    async fn create_commit_delta(&self, req: CommitDeltaRequest) -> Result<CommitDeltaResponse>;
    async fn get_commit_details(
        &self,
        repo_id: String,
        commit: String,
    ) -> Result<CommitDetailsResponse>;

    // Patch Lab
    async fn propose_patch(&self, req: ProposePatchRequest) -> Result<ProposePatchResponse>;
    async fn validate_patch(&self, req: ValidatePatchRequest) -> Result<ValidatePatchResponse>;
    async fn apply_patch(&self, req: ApplyPatchRequest) -> Result<ApplyPatchResponse>;

    // Code Policy
    async fn get_code_policy(&self) -> Result<GetCodePolicyResponse>;
    async fn update_code_policy(&self, req: UpdateCodePolicyRequest) -> Result<()>;

    // Metrics Dashboard
    async fn get_code_metrics(&self, req: CodeMetricsRequest) -> Result<CodeMetricsResponse>;
    async fn compare_metrics(&self, req: CompareMetricsRequest) -> Result<CompareMetricsResponse>;
}
