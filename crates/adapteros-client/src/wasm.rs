use crate::{types::*, CpClient};
use anyhow::{Context, Result};
use gloo_net::http::Request;

pub struct WasmClient {
    base_url: String,
}

impl CpClient for WasmClient {
    fn new(base_url: String) -> Self {
        Self { base_url }
    }

    async fn health(&self) -> Result<HealthResponse> {
        let url = format!("{}/healthz", self.base_url);
        let resp = Request::get(&url).send().await?;
        resp.json().await.context("Failed to parse health response")
    }

    async fn login(&self, req: LoginRequest) -> Result<LoginResponse> {
        let url = format!("{}/v1/auth/login", self.base_url);
        let resp = Request::post(&url).json(&req)?.send().await?;
        resp.json().await.context("Failed to parse login response")
    }

    async fn logout(&self) -> Result<()> {
        let url = format!("{}/v1/auth/logout", self.base_url);
        Request::post(&url).send().await?;
        Ok(())
    }

    async fn me(&self) -> Result<UserInfoResponse> {
        let url = format!("{}/v1/auth/me", self.base_url);
        let resp = Request::get(&url).send().await?;
        resp.json().await.context("Failed to parse user info")
    }

    async fn list_tenants(&self) -> Result<Vec<TenantResponse>> {
        let url = format!("{}/v1/tenants", self.base_url);
        let resp = Request::get(&url).send().await?;
        resp.json().await.context("Failed to parse tenants")
    }

    async fn create_tenant(&self, req: CreateTenantRequest) -> Result<TenantResponse> {
        let url = format!("{}/v1/tenants", self.base_url);
        let resp = Request::post(&url).json(&req)?.send().await?;
        resp.json().await.context("Failed to parse tenant response")
    }

    async fn list_nodes(&self) -> Result<Vec<NodeResponse>> {
        let url = format!("{}/v1/nodes", self.base_url);
        let resp = Request::get(&url).send().await?;
        resp.json().await.context("Failed to parse nodes")
    }

    async fn register_node(&self, req: RegisterNodeRequest) -> Result<NodeResponse> {
        let url = format!("{}/v1/nodes/register", self.base_url);
        let resp = Request::post(&url).json(&req)?.send().await?;
        resp.json().await.context("Failed to parse node response")
    }

    async fn list_plans(&self, tenant_id: Option<String>) -> Result<Vec<PlanResponse>> {
        let mut url = format!("{}/v1/plans", self.base_url);
        if let Some(tid) = tenant_id {
            url.push_str(&format!("?tenant_id={}", tid));
        }
        let resp = Request::get(&url).send().await?;
        resp.json().await.context("Failed to parse plans")
    }

    async fn build_plan(&self, req: BuildPlanRequest) -> Result<JobResponse> {
        let url = format!("{}/v1/plans/build", self.base_url);
        let resp = Request::post(&url).json(&req)?.send().await?;
        resp.json().await.context("Failed to parse job response")
    }

    async fn list_workers(&self, tenant_id: Option<String>) -> Result<Vec<WorkerResponse>> {
        let mut url = format!("{}/v1/workers", self.base_url);
        if let Some(tid) = tenant_id {
            url.push_str(&format!("?tenant_id={}", tid));
        }
        let resp = Request::get(&url).send().await?;
        resp.json().await.context("Failed to parse workers")
    }

    async fn spawn_worker(&self, req: SpawnWorkerRequest) -> Result<()> {
        let url = format!("{}/v1/workers/spawn", self.base_url);
        Request::post(&url).json(&req)?.send().await?;
        Ok(())
    }

    async fn promote_cp(&self, req: PromoteCPRequest) -> Result<PromotionResponse> {
        let url = format!("{}/v1/cp/promote", self.base_url);
        let resp = Request::post(&url).json(&req)?.send().await?;
        resp.json()
            .await
            .context("Failed to parse promotion response")
    }

    async fn promotion_gates(&self, cpid: String) -> Result<PromotionGatesResponse> {
        let url = format!("{}/v1/cp/promotion-gates/{}", self.base_url, cpid);
        let resp = Request::get(&url).send().await?;
        resp.json().await.context("Failed to parse gates response")
    }

    async fn rollback_cp(&self, req: RollbackCPRequest) -> Result<RollbackResponse> {
        let url = format!("{}/v1/cp/rollback", self.base_url);
        let resp = Request::post(&url).json(&req)?.send().await?;
        resp.json()
            .await
            .context("Failed to parse rollback response")
    }

    async fn list_jobs(&self, tenant_id: Option<String>) -> Result<Vec<JobResponse>> {
        let mut url = format!("{}/v1/jobs", self.base_url);
        if let Some(tid) = tenant_id {
            url.push_str(&format!("?tenant_id={}", tid));
        }
        let resp = Request::get(&url).send().await?;
        resp.json().await.context("Failed to parse jobs")
    }

    async fn import_model(&self, req: ImportModelRequest) -> Result<()> {
        let url = format!("{}/v1/models/import", self.base_url);
        Request::post(&url).json(&req)?.send().await?;
        Ok(())
    }

    async fn list_policies(&self) -> Result<Vec<PolicyPackResponse>> {
        let url = format!("{}/v1/policies", self.base_url);
        let resp = Request::get(&url).send().await?;
        resp.json().await.context("Failed to parse policies")
    }

    async fn get_policy(&self, cpid: String) -> Result<PolicyPackResponse> {
        let url = format!("{}/v1/policies/{}", self.base_url, cpid);
        let resp = Request::get(&url).send().await?;
        resp.json().await.context("Failed to parse policy")
    }

    async fn validate_policy(
        &self,
        req: ValidatePolicyRequest,
    ) -> Result<PolicyValidationResponse> {
        let url = format!("{}/v1/policies/validate", self.base_url);
        let resp = Request::post(&url).json(&req)?.send().await?;
        resp.json()
            .await
            .context("Failed to parse validation response")
    }

    async fn apply_policy(&self, req: ApplyPolicyRequest) -> Result<PolicyPackResponse> {
        let url = format!("{}/v1/policies/apply", self.base_url);
        let resp = Request::post(&url).json(&req)?.send().await?;
        resp.json().await.context("Failed to parse policy response")
    }

    async fn list_telemetry_bundles(&self) -> Result<Vec<TelemetryBundleResponse>> {
        let url = format!("{}/v1/telemetry/bundles", self.base_url);
        let resp = Request::get(&url).send().await?;
        resp.json()
            .await
            .context("Failed to parse telemetry bundles")
    }

    // Code Intelligence

    async fn register_repo(&self, req: RegisterRepoRequest) -> Result<RepoResponse> {
        let url = format!("{}/v1/code/register-repo", self.base_url);
        let resp = Request::post(&url).json(&req)?.send().await?;
        resp.json().await.context("Failed to parse repo response")
    }

    async fn scan_repo(&self, req: ScanRepoRequest) -> Result<JobResponse> {
        let url = format!("{}/v1/code/scan", self.base_url);
        let resp = Request::post(&url).json(&req)?.send().await?;
        resp.json()
            .await
            .context("Failed to parse scan job response")
    }

    async fn list_repos(&self) -> Result<Vec<RepoResponse>> {
        let url = format!("{}/v1/code/repos", self.base_url);
        let resp = Request::get(&url).send().await?;
        resp.json().await.context("Failed to parse repos")
    }

    async fn list_adapters_by_tenant(&self, tenant_id: String) -> Result<ListAdaptersResponse> {
        let url = format!("{}/v1/code/adapters?tenant_id={}", self.base_url, tenant_id);
        let resp = Request::get(&url).send().await?;
        resp.json().await.context("Failed to parse adapters")
    }

    async fn get_adapter_activations(&self) -> Result<Vec<ActivationData>> {
        let url = format!("{}/v1/code/adapters/activations", self.base_url);
        let resp = Request::get(&url).send().await?;
        resp.json().await.context("Failed to parse activation data")
    }

    async fn create_commit_delta(&self, req: CommitDeltaRequest) -> Result<CommitDeltaResponse> {
        let url = format!("{}/v1/code/commit-delta", self.base_url);
        let resp = Request::post(&url).json(&req)?.send().await?;
        resp.json()
            .await
            .context("Failed to parse commit delta response")
    }

    async fn get_commit_details(
        &self,
        repo_id: String,
        commit: String,
    ) -> Result<CommitDetailsResponse> {
        let url = format!("{}/v1/code/commits/{}/{}", self.base_url, repo_id, commit);
        let resp = Request::get(&url).send().await?;
        resp.json().await.context("Failed to parse commit details")
    }

    async fn evict_adapter(&self, adapter_id: String) -> Result<()> {
        let url = format!("{}/v1/code/adapters/{}/evict", self.base_url, adapter_id);
        Request::post(&url).send().await?;
        Ok(())
    }

    // Patch Lab

    async fn propose_patch(&self, req: ProposePatchRequest) -> Result<ProposePatchResponse> {
        let url = format!("{}/v1/patch/propose", self.base_url);
        let resp = Request::post(&url).json(&req)?.send().await?;
        resp.json().await.context("Failed to parse patch proposal")
    }

    async fn validate_patch(&self, req: ValidatePatchRequest) -> Result<ValidatePatchResponse> {
        let url = format!("{}/v1/patch/validate", self.base_url);
        let resp = Request::post(&url).json(&req)?.send().await?;
        resp.json()
            .await
            .context("Failed to parse patch validation")
    }

    async fn apply_patch(&self, req: ApplyPatchRequest) -> Result<ApplyPatchResponse> {
        let url = format!("{}/v1/patch/apply", self.base_url);
        let resp = Request::post(&url).json(&req)?.send().await?;
        resp.json()
            .await
            .context("Failed to parse patch application")
    }

    // Code Policy - STUB (endpoint not implemented in API)
    async fn get_code_policy(&self) -> Result<GetCodePolicyResponse> {
        Err(anyhow::anyhow!("Code policy endpoint not available"))
    }

    async fn update_code_policy(&self, _req: UpdateCodePolicyRequest) -> Result<()> {
        Err(anyhow::anyhow!("Code policy endpoint not available"))
    }

    // Metrics Dashboard - STUB (endpoints not implemented in API)
    async fn get_code_metrics(&self, _req: CodeMetricsRequest) -> Result<CodeMetricsResponse> {
        Err(anyhow::anyhow!("Code metrics endpoint not available"))
    }

    async fn compare_metrics(&self, _req: CompareMetricsRequest) -> Result<CompareMetricsResponse> {
        Err(anyhow::anyhow!("Metrics comparison endpoint not available"))
    }
}
