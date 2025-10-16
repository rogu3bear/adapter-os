use crate::{types::*, AdapterOSClient};
use anyhow::{Context, Result};

pub struct NativeClient {
    base_url: String,
    client: reqwest::Client,
}

impl AdapterOSClient for NativeClient {
    // Health & Auth
    async fn health(&self) -> Result<HealthResponse> {
        let url = format!("{}/healthz", self.base_url);
        let resp = self.client.get(&url).send().await?;
        resp.json().await.context("Failed to parse health response")
    }

    async fn login(&self, req: LoginRequest) -> Result<LoginResponse> {
        let url = format!("{}/v1/auth/login", self.base_url);
        let resp = self.client.post(&url).json(&req).send().await?;
        resp.json().await.context("Failed to parse login response")
    }

    async fn logout(&self) -> Result<()> {
        let url = format!("{}/v1/auth/logout", self.base_url);
        self.client.post(&url).send().await?;
        Ok(())
    }

    async fn me(&self) -> Result<UserInfoResponse> {
        let url = format!("{}/v1/auth/me", self.base_url);
        let resp = self.client.get(&url).send().await?;
        resp.json().await.context("Failed to parse user info")
    }

    // Tenants
    async fn list_tenants(&self) -> Result<Vec<TenantResponse>> {
        let url = format!("{}/v1/tenants", self.base_url);
        let resp = self.client.get(&url).send().await?;
        resp.json().await.context("Failed to parse tenants")
    }

    async fn create_tenant(&self, req: CreateTenantRequest) -> Result<TenantResponse> {
        let url = format!("{}/v1/tenants", self.base_url);
        let resp = self.client.post(&url).json(&req).send().await?;
        resp.json().await.context("Failed to parse tenant response")
    }

    // Adapters
    async fn list_adapters(&self) -> Result<Vec<AdapterResponse>> {
        let url = format!("{}/v1/adapters", self.base_url);
        let resp = self.client.get(&url).send().await?;
        resp.json().await.context("Failed to parse adapters")
    }

    async fn register_adapter(&self, req: RegisterAdapterRequest) -> Result<AdapterResponse> {
        let url = format!("{}/v1/adapters", self.base_url);
        let resp = self.client.post(&url).json(&req).send().await?;
        resp.json()
            .await
            .context("Failed to parse adapter response")
    }

    async fn evict_adapter(&self, adapter_id: &str) -> Result<()> {
        let url = format!("{}/v1/adapters/{}/evict", self.base_url, adapter_id);
        self.client.post(&url).send().await?;
        Ok(())
    }

    async fn pin_adapter(&self, adapter_id: &str, pinned: bool) -> Result<()> {
        let url = format!("{}/v1/adapters/{}/pin", self.base_url, adapter_id);
        let req_body = serde_json::json!({ "pinned": pinned });
        self.client.post(&url).json(&req_body).send().await?;
        Ok(())
    }

    // Memory Management
    async fn get_memory_usage(&self) -> Result<MemoryUsageResponse> {
        let url = format!("{}/v1/memory/usage", self.base_url);
        let resp = self.client.get(&url).send().await?;
        resp.json().await.context("Failed to parse memory usage")
    }

    // Training
    async fn start_adapter_training(
        &self,
        req: StartTrainingRequest,
    ) -> Result<TrainingSessionResponse> {
        let url = format!("{}/v1/training/sessions", self.base_url);
        let resp = self.client.post(&url).json(&req).send().await?;
        resp.json()
            .await
            .context("Failed to parse training session response")
    }

    async fn get_training_session(&self, session_id: &str) -> Result<TrainingSessionResponse> {
        let url = format!("{}/v1/training/sessions/{}", self.base_url, session_id);
        let resp = self.client.get(&url).send().await?;
        resp.json()
            .await
            .context("Failed to parse training session")
    }

    async fn list_training_sessions(&self) -> Result<Vec<TrainingSessionResponse>> {
        let url = format!("{}/v1/training/sessions", self.base_url);
        let resp = self.client.get(&url).send().await?;
        resp.json()
            .await
            .context("Failed to parse training sessions")
    }

    // Telemetry
    async fn get_telemetry_events(&self, filters: TelemetryFilters) -> Result<Vec<TelemetryEvent>> {
        let mut url = format!("{}/v1/telemetry/events", self.base_url);
        let mut params = Vec::new();

        if let Some(limit) = filters.limit {
            params.push(format!("limit={}", limit));
        }
        if let Some(tenant_id) = filters.tenant_id {
            params.push(format!("tenant_id={}", tenant_id));
        }
        if let Some(user_id) = filters.user_id {
            params.push(format!("user_id={}", user_id));
        }
        if let Some(start_time) = filters.start_time {
            params.push(format!("start_time={}", start_time));
        }
        if let Some(end_time) = filters.end_time {
            params.push(format!("end_time={}", end_time));
        }
        if let Some(event_type) = filters.event_type {
            params.push(format!("event_type={}", event_type));
        }
        if let Some(level) = filters.level {
            params.push(format!("level={}", level));
        }

        if !params.is_empty() {
            url.push('?');
            url.push_str(&params.join("&"));
        }

        let resp = self.client.get(&url).send().await?;
        resp.json()
            .await
            .context("Failed to parse telemetry events")
    }

    // Nodes
    async fn list_nodes(&self) -> Result<Vec<NodeResponse>> {
        let url = format!("{}/v1/nodes", self.base_url);
        let resp = self.client.get(&url).send().await?;
        resp.json().await.context("Failed to parse nodes")
    }

    async fn register_node(&self, req: RegisterNodeRequest) -> Result<NodeResponse> {
        let url = format!("{}/v1/nodes/register", self.base_url);
        let resp = self.client.post(&url).json(&req).send().await?;
        resp.json().await.context("Failed to parse node response")
    }

    // Plans
    async fn list_plans(&self, tenant_id: Option<String>) -> Result<Vec<PlanResponse>> {
        let mut url = format!("{}/v1/plans", self.base_url);
        if let Some(tenant_id) = tenant_id {
            url.push_str(&format!("?tenant_id={}", tenant_id));
        }
        let resp = self.client.get(&url).send().await?;
        resp.json().await.context("Failed to parse plans")
    }

    async fn build_plan(&self, req: BuildPlanRequest) -> Result<JobResponse> {
        let url = format!("{}/v1/plans/build", self.base_url);
        let resp = self.client.post(&url).json(&req).send().await?;
        resp.json()
            .await
            .context("Failed to parse build plan response")
    }

    // Workers
    async fn list_workers(&self, tenant_id: Option<String>) -> Result<Vec<WorkerResponse>> {
        let mut url = format!("{}/v1/workers", self.base_url);
        if let Some(tenant_id) = tenant_id {
            url.push_str(&format!("?tenant_id={}", tenant_id));
        }
        let resp = self.client.get(&url).send().await?;
        resp.json().await.context("Failed to parse workers")
    }

    async fn spawn_worker(&self, req: SpawnWorkerRequest) -> Result<()> {
        let url = format!("{}/v1/workers/spawn", self.base_url);
        self.client.post(&url).json(&req).send().await?;
        Ok(())
    }

    // CP Operations
    async fn promote_cp(&self, req: PromoteCPRequest) -> Result<PromotionResponse> {
        let url = format!("{}/v1/cp/promote", self.base_url);
        let resp = self.client.post(&url).json(&req).send().await?;
        resp.json()
            .await
            .context("Failed to parse promotion response")
    }

    async fn promotion_gates(&self, cpid: String) -> Result<PromotionGatesResponse> {
        let url = format!("{}/v1/cp/gates/{}", self.base_url, cpid);
        let resp = self.client.get(&url).send().await?;
        resp.json().await.context("Failed to parse promotion gates")
    }

    async fn rollback_cp(&self, req: RollbackCPRequest) -> Result<RollbackResponse> {
        let url = format!("{}/v1/cp/rollback", self.base_url);
        let resp = self.client.post(&url).json(&req).send().await?;
        resp.json()
            .await
            .context("Failed to parse rollback response")
    }

    // Jobs
    async fn list_jobs(&self, tenant_id: Option<String>) -> Result<Vec<JobResponse>> {
        let mut url = format!("{}/v1/jobs", self.base_url);
        if let Some(tenant_id) = tenant_id {
            url.push_str(&format!("?tenant_id={}", tenant_id));
        }
        let resp = self.client.get(&url).send().await?;
        resp.json().await.context("Failed to parse jobs")
    }

    // Models
    async fn import_model(&self, req: ImportModelRequest) -> Result<()> {
        let url = format!("{}/v1/models/import", self.base_url);
        self.client.post(&url).json(&req).send().await?;
        Ok(())
    }

    // Policies
    async fn list_policies(&self) -> Result<Vec<PolicyPackResponse>> {
        let url = format!("{}/v1/policies", self.base_url);
        let resp = self.client.get(&url).send().await?;
        resp.json().await.context("Failed to parse policies")
    }

    async fn get_policy(&self, cpid: String) -> Result<PolicyPackResponse> {
        let url = format!("{}/v1/policies/{}", self.base_url, cpid);
        let resp = self.client.get(&url).send().await?;
        resp.json().await.context("Failed to parse policy")
    }

    async fn validate_policy(
        &self,
        req: ValidatePolicyRequest,
    ) -> Result<PolicyValidationResponse> {
        let url = format!("{}/v1/policies/validate", self.base_url);
        let resp = self.client.post(&url).json(&req).send().await?;
        resp.json()
            .await
            .context("Failed to parse policy validation")
    }

    async fn apply_policy(&self, req: ApplyPolicyRequest) -> Result<PolicyPackResponse> {
        let url = format!("{}/v1/policies/apply", self.base_url);
        let resp = self.client.post(&url).json(&req).send().await?;
        resp.json()
            .await
            .context("Failed to parse policy application")
    }

    // Telemetry Bundles
    async fn list_telemetry_bundles(&self) -> Result<Vec<TelemetryBundleResponse>> {
        let url = format!("{}/v1/telemetry/bundles", self.base_url);
        let resp = self.client.get(&url).send().await?;
        resp.json()
            .await
            .context("Failed to parse telemetry bundles")
    }

    // Code Intelligence
    async fn register_repo(&self, req: RegisterRepoRequest) -> Result<RepoResponse> {
        let url = format!("{}/v1/repos/register", self.base_url);
        let resp = self.client.post(&url).json(&req).send().await?;
        resp.json()
            .await
            .context("Failed to parse repo registration")
    }

    async fn scan_repo(&self, req: ScanRepoRequest) -> Result<JobResponse> {
        let url = format!("{}/v1/repos/scan", self.base_url);
        let resp = self.client.post(&url).json(&req).send().await?;
        resp.json().await.context("Failed to parse repo scan")
    }

    async fn list_repos(&self) -> Result<Vec<RepoResponse>> {
        let url = format!("{}/v1/repos", self.base_url);
        let resp = self.client.get(&url).send().await?;
        resp.json().await.context("Failed to parse repos")
    }

    async fn list_adapters_by_tenant(&self, tenant_id: String) -> Result<ListAdaptersResponse> {
        let url = format!("{}/v1/adapters?tenant_id={}", self.base_url, tenant_id);
        let resp = self.client.get(&url).send().await?;
        resp.json().await.context("Failed to parse adapters list")
    }

    async fn get_adapter_activations(&self) -> Result<Vec<ActivationData>> {
        let url = format!("{}/v1/adapters/activations", self.base_url);
        let resp = self.client.get(&url).send().await?;
        resp.json()
            .await
            .context("Failed to parse adapter activations")
    }

    async fn create_commit_delta(&self, req: CommitDeltaRequest) -> Result<CommitDeltaResponse> {
        let url = format!("{}/v1/repos/commit-delta", self.base_url);
        let resp = self.client.post(&url).json(&req).send().await?;
        resp.json().await.context("Failed to parse commit delta")
    }

    async fn get_commit_details(
        &self,
        repo_id: String,
        commit: String,
    ) -> Result<CommitDetailsResponse> {
        let url = format!("{}/v1/repos/{}/commits/{}", self.base_url, repo_id, commit);
        let resp = self.client.get(&url).send().await?;
        resp.json().await.context("Failed to parse commit details")
    }

    // Routing Inspector
    async fn extract_router_features(
        &self,
        req: RouterFeaturesRequest,
    ) -> Result<RouterFeaturesResponse> {
        let url = format!("{}/v1/router/features", self.base_url);
        let resp = self.client.post(&url).json(&req).send().await?;
        resp.json().await.context("Failed to parse router features")
    }

    async fn score_adapters(&self, req: ScoreAdaptersRequest) -> Result<ScoreAdaptersResponse> {
        let url = format!("{}/v1/router/score", self.base_url);
        let resp = self.client.post(&url).json(&req).send().await?;
        resp.json().await.context("Failed to parse adapter scores")
    }

    // Patch Lab
    async fn propose_patch(&self, req: ProposePatchRequest) -> Result<ProposePatchResponse> {
        let url = format!("{}/v1/patches/propose", self.base_url);
        let resp = self.client.post(&url).json(&req).send().await?;
        resp.json().await.context("Failed to parse patch proposal")
    }

    async fn validate_patch(&self, req: ValidatePatchRequest) -> Result<ValidatePatchResponse> {
        let url = format!("{}/v1/patches/validate", self.base_url);
        let resp = self.client.post(&url).json(&req).send().await?;
        resp.json()
            .await
            .context("Failed to parse patch validation")
    }

    async fn apply_patch(&self, req: ApplyPatchRequest) -> Result<ApplyPatchResponse> {
        let url = format!("{}/v1/patches/apply", self.base_url);
        let resp = self.client.post(&url).json(&req).send().await?;
        resp.json()
            .await
            .context("Failed to parse patch application")
    }

    // Code Policy
    async fn get_code_policy(&self) -> Result<GetCodePolicyResponse> {
        let url = format!("{}/v1/code-policy", self.base_url);
        let resp = self.client.get(&url).send().await?;
        resp.json().await.context("Failed to parse code policy")
    }

    async fn update_code_policy(&self, req: UpdateCodePolicyRequest) -> Result<()> {
        let url = format!("{}/v1/code-policy", self.base_url);
        self.client.put(&url).json(&req).send().await?;
        Ok(())
    }

    // Metrics Dashboard
    async fn get_code_metrics(&self, req: CodeMetricsRequest) -> Result<CodeMetricsResponse> {
        let url = format!("{}/v1/metrics/code", self.base_url);
        let resp = self.client.post(&url).json(&req).send().await?;
        resp.json().await.context("Failed to parse code metrics")
    }

    async fn compare_metrics(&self, req: CompareMetricsRequest) -> Result<CompareMetricsResponse> {
        let url = format!("{}/v1/metrics/compare", self.base_url);
        let resp = self.client.post(&url).json(&req).send().await?;
        resp.json()
            .await
            .context("Failed to parse metrics comparison")
    }
}

impl NativeClient {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            client: reqwest::Client::new(),
        }
    }
}
