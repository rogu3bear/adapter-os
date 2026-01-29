//! HTTP API client
//!
//! Typed HTTP client for the adapterOS REST API.

use super::{api_base_url, ApiError, ApiResult};
use gloo_net::http::{Request, RequestBuilder};
use serde::{de::DeserializeOwned, Serialize};
use web_sys::RequestCredentials;
use std::sync::{Arc, RwLock};
use urlencoding::encode;

pub use adapteros_api_types::dataset_domain::CanonicalRow;
pub use adapteros_api_types::code_repositories::{
    RegisterRepositoryRequest, RegisterRepositoryResponse, RepositoryDetailResponse,
    RepositoryInfo, RepositoryListResponse, ScanJobResponse, ScanRepositoryRequest,
};
pub use adapteros_api_types::activity::ActivityEventResponse;
pub use adapteros_api_types::training::{
    DatasetFileResponse, DatasetVersionsResponse, JsonlValidationDiagnostic,
};
pub use adapteros_api_types::{DatasetManifest, UploadDatasetResponse};
// Consolidated types from shared crate
pub use adapteros_api_types::admin::{ListUsersResponse, UserResponse};
pub use adapteros_api_types::api_keys::{
    ApiKeyInfo, ApiKeyListResponse, CreateApiKeyRequest, CreateApiKeyResponse, RevokeApiKeyResponse,
};
pub use adapteros_api_types::model_status::ModelLoadStatus;
pub use adapteros_api_types::models::{
    AllModelsStatusResponse, AneMemoryStatus, BaseModelStatusResponse, ModelStatusResponse,
    SeedModelRequest, SeedModelResponse,
};
pub use adapteros_api_types::routing::{
    CreateRoutingRuleRequest, RoutingRuleResponse, RoutingRulesResponse,
};
pub use adapteros_api_types::embeddings::{
    EmbeddingBenchmarkReport, EmbeddingBenchmarksQuery, EmbeddingBenchmarksResponse,
};
pub use adapteros_api_types::workers::WorkerMetricsResponse;

#[cfg(target_arch = "wasm32")]
fn csrf_token_from_cookie() -> Option<String> {
    use wasm_bindgen::JsCast;
    web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.dyn_into::<web_sys::HtmlDocument>().ok())
        .and_then(|d| d.cookie().ok())
        .and_then(|cookies| {
            for cookie in cookies.split(';') {
                let cookie = cookie.trim();
                if let Some(token) = cookie.strip_prefix("csrf_token=") {
                    return Some(token.to_string());
                }
            }
            None
        })
}

#[cfg(not(target_arch = "wasm32"))]
fn csrf_token_from_cookie() -> Option<String> {
    None
}

/// HTTP API client for adapterOS backend
#[derive(Clone)]
pub struct ApiClient {
    base_url: String,
    auth_token: Arc<RwLock<Option<String>>>,
}

impl Default for ApiClient {
    fn default() -> Self {
        Self::new()
    }
}

impl ApiClient {
    /// Create a new API client
    pub fn new() -> Self {
        Self {
            base_url: api_base_url(),
            auth_token: Arc::new(RwLock::new(Self::load_token())),
        }
    }

    /// Create client with custom base URL
    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            auth_token: Arc::new(RwLock::new(Self::load_token())),
        }
    }

    /// Initialize in-memory auth state.
    ///
    /// Note: Auth tokens are managed via httpOnly cookies, not localStorage.
    /// This method returns None as the initial state; auth status is
    /// tracked in-memory via `set_auth_status`.
    fn load_token() -> Option<String> {
        None
    }

    /// Set in-memory authentication token state.
    ///
    /// Note: This only updates in-memory state for tracking auth status.
    /// Actual authentication is handled via httpOnly cookies set by the server.
    /// No localStorage persistence is performed.
    pub fn set_token(&self, token: Option<String>) {
        if let Ok(mut guard) = self.auth_token.write() {
            *guard = token;
        }
    }

    /// Check if client has an auth token
    pub fn is_authenticated(&self) -> bool {
        self.auth_token
            .read()
            .ok()
            .map(|t| t.is_some())
            .unwrap_or(false)
    }

    /// Mark client as authenticated (for httpOnly cookie auth)
    ///
    /// With httpOnly cookies, the browser handles auth automatically.
    /// This sets a placeholder to track authenticated state locally.
    pub fn set_auth_status(&self, authenticated: bool) {
        if authenticated {
            // Set a placeholder to indicate authenticated state
            self.set_token(Some("cookie_auth".to_string()));
        } else {
            self.set_token(None);
        }
    }

    /// Clear authentication status
    ///
    /// Clears local auth state. Server-side logout should also be called
    /// to clear httpOnly cookies.
    pub fn clear_auth_status(&self) {
        self.set_token(None);
    }

    /// Build a request with common headers
    fn request(&self, method: &str, path: &str) -> RequestBuilder {
        let url = format!("{}{}", self.base_url, path);
        let req = match method {
            "GET" => Request::get(&url),
            "POST" => Request::post(&url),
            "PUT" => Request::put(&url),
            "DELETE" => Request::delete(&url),
            "PATCH" => Request::patch(&url),
            _ => Request::get(&url),
        };

        // Include credentials (cookies) with all requests for httpOnly cookie auth
        let mut req = req
            .credentials(RequestCredentials::Include)
            .header("Content-Type", "application/json");

        if matches!(method, "POST" | "PUT" | "PATCH" | "DELETE") {
            if let Some(token) = csrf_token_from_cookie() {
                req = req.header("X-CSRF-Token", &token);
            }
        }

        // Only add Authorization header for real Bearer tokens, not the cookie_auth placeholder
        if let Some(token) = self.auth_token.read().ok().and_then(|t| t.clone()) {
            if token != "cookie_auth" {
                return req.header("Authorization", &format!("Bearer {}", token));
            }
        }
        req
    }

    /// Perform a GET request
    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> ApiResult<T> {
        let response = self.request("GET", path).send().await?;
        self.handle_response(response).await
    }

    /// Perform a GET request and return the text body
    pub async fn get_text(&self, path: &str) -> ApiResult<String> {
        let response = self.request("GET", path).send().await?;
        if response.ok() {
            response
                .text()
                .await
                .map_err(|e| ApiError::Serialization(e.to_string()))
        } else {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            Err(ApiError::from_response(status, &text))
        }
    }

    /// Perform a POST request with JSON body
    pub async fn post<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> ApiResult<T> {
        let response = self.request("POST", path).json(body)?.send().await?;
        self.handle_response(response).await
    }

    /// Perform a POST request without response body
    pub async fn post_no_response<B: Serialize>(&self, path: &str, body: &B) -> ApiResult<()> {
        let response = self.request("POST", path).json(body)?.send().await?;

        if response.ok() {
            Ok(())
        } else {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            Err(ApiError::from_response(status, &text))
        }
    }

    /// Perform a POST request without body, returning a response
    pub async fn post_empty<T: DeserializeOwned>(&self, path: &str) -> ApiResult<T> {
        let response = self.request("POST", path).send().await?;
        self.handle_response(response).await
    }

    /// Perform a PUT request with JSON body
    pub async fn put<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> ApiResult<T> {
        let response = self.request("PUT", path).json(body)?.send().await?;
        self.handle_response(response).await
    }

    /// Perform a DELETE request
    pub async fn delete(&self, path: &str) -> ApiResult<()> {
        let response = self.request("DELETE", path).send().await?;

        if response.ok() {
            Ok(())
        } else {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            Err(ApiError::from_response(status, &text))
        }
    }

    /// Perform a DELETE request and deserialize response
    pub async fn delete_with_response<T: DeserializeOwned>(&self, path: &str) -> ApiResult<T> {
        let response = self.request("DELETE", path).send().await?;
        self.handle_response(response).await
    }

    /// Handle response and deserialize JSON
    async fn handle_response<T: DeserializeOwned>(
        &self,
        response: gloo_net::http::Response,
    ) -> ApiResult<T> {
        if response.ok() {
            let json = response.json().await?;
            Ok(json)
        } else {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            Err(ApiError::from_response(status, &text))
        }
    }
}

// ============================================================================
// Domain-specific API methods
// ============================================================================

impl ApiClient {
    // --- Health ---

    /// Check backend liveness
    pub async fn health(&self) -> ApiResult<adapteros_api_types::HealthResponse> {
        self.get("/healthz").await
    }

    /// Check backend readiness
    pub async fn ready(&self) -> ApiResult<adapteros_api_types::HealthResponse> {
        self.get("/readyz").await
    }

    // --- Auth ---

    /// Login with credentials
    pub async fn login(
        &self,
        username: &str,
        password: &str,
    ) -> ApiResult<adapteros_api_types::LoginResponse> {
        #[derive(Serialize)]
        struct LoginRequest<'a> {
            username: &'a str,
            password: &'a str,
        }

        self.post("/v1/auth/login", &LoginRequest { username, password })
            .await
    }

    /// Get current user info
    pub async fn me(&self) -> ApiResult<adapteros_api_types::UserInfoResponse> {
        self.get("/v1/auth/me").await
    }

    /// Logout
    pub async fn logout(&self) -> ApiResult<()> {
        self.post_no_response("/v1/auth/logout", &serde_json::json!({}))
            .await
    }

    // --- Admin ---

    /// List users (Admin role required)
    pub async fn list_users(
        &self,
        page: Option<i64>,
        page_size: Option<i64>,
    ) -> ApiResult<ListUsersResponse> {
        let mut url = "/v1/admin/users".to_string();
        let mut params = Vec::new();
        if let Some(p) = page {
            params.push(format!("page={}", p));
        }
        if let Some(ps) = page_size {
            params.push(format!("page_size={}", ps));
        }
        if !params.is_empty() {
            url.push('?');
            url.push_str(&params.join("&"));
        }
        self.get(&url).await
    }

    // --- API Keys ---

    /// List API keys for the current tenant
    pub async fn list_api_keys(&self) -> ApiResult<ApiKeyListResponse> {
        self.get("/v1/api-keys").await
    }

    /// Create a new API key
    pub async fn create_api_key(
        &self,
        request: &CreateApiKeyRequest,
    ) -> ApiResult<CreateApiKeyResponse> {
        self.post("/v1/api-keys", request).await
    }

    /// Revoke an API key
    pub async fn revoke_api_key(&self, id: &str) -> ApiResult<RevokeApiKeyResponse> {
        self.delete_with_response(&format!("/v1/api-keys/{}", id))
            .await
    }

    // --- Adapters ---

    /// List adapters
    pub async fn list_adapters(&self) -> ApiResult<Vec<adapteros_api_types::AdapterResponse>> {
        self.get("/v1/adapters").await
    }

    /// Get adapter details
    pub async fn get_adapter(&self, id: &str) -> ApiResult<adapteros_api_types::AdapterResponse> {
        self.get(&format!("/v1/adapters/{}", id)).await
    }

    // --- System ---

    /// Get system status
    pub async fn system_status(&self) -> ApiResult<adapteros_api_types::SystemStatusResponse> {
        self.get("/v1/system/status").await
    }

    /// Get system overview (includes active sessions, workers, resources)
    pub async fn get_system_overview(&self) -> ApiResult<SystemOverviewResponse> {
        self.get("/v1/system/overview").await
    }

    // --- Workers ---

    /// List workers
    pub async fn list_workers(&self) -> ApiResult<Vec<adapteros_api_types::WorkerResponse>> {
        self.get("/v1/workers").await
    }

    /// Get worker details
    pub async fn get_worker(&self, id: &str) -> ApiResult<adapteros_api_types::WorkerResponse> {
        self.get(&format!("/v1/workers/{}", id)).await
    }

    /// Spawn a new worker
    pub async fn spawn_worker(
        &self,
        request: &adapteros_api_types::SpawnWorkerRequest,
    ) -> ApiResult<adapteros_api_types::WorkerResponse> {
        self.post("/v1/workers/spawn", request).await
    }

    /// Drain a worker (gracefully stop accepting new requests)
    pub async fn drain_worker(&self, id: &str) -> ApiResult<()> {
        self.post_no_response(&format!("/v1/workers/{}/drain", id), &serde_json::json!({}))
            .await
    }

    /// Stop a worker
    pub async fn stop_worker(&self, id: &str) -> ApiResult<()> {
        self.post_no_response(&format!("/v1/workers/{}/stop", id), &serde_json::json!({}))
            .await
    }

    /// Get worker metrics
    pub async fn get_worker_metrics(&self, id: &str) -> ApiResult<WorkerMetricsResponse> {
        self.get(&format!("/v1/workers/{}/metrics", id)).await
    }

    // --- Nodes ---

    /// List nodes
    pub async fn list_nodes(&self) -> ApiResult<Vec<adapteros_api_types::NodeResponse>> {
        self.get("/v1/nodes").await
    }

    // --- Metrics ---

    /// Get system metrics
    pub async fn system_metrics(&self) -> ApiResult<adapteros_api_types::SystemMetricsResponse> {
        self.get("/v1/metrics/system").await
    }

    // --- Training ---

    /// List training jobs with optional filtering
    pub async fn list_training_jobs(
        &self,
        params: Option<&adapteros_api_types::TrainingListParams>,
    ) -> ApiResult<adapteros_api_types::TrainingJobListResponse> {
        let path = match params {
            Some(p) => {
                let mut query_parts = Vec::new();
                if let Some(ref status) = p.status {
                    query_parts.push(format!("status={}", encode(status)));
                }
                if let Some(page) = p.page {
                    query_parts.push(format!("page={}", page));
                }
                if let Some(page_size) = p.page_size {
                    query_parts.push(format!("page_size={}", page_size));
                }
                if let Some(ref adapter_name) = p.adapter_name {
                    query_parts.push(format!("adapter_name={}", encode(adapter_name)));
                }
                if let Some(ref template_id) = p.template_id {
                    query_parts.push(format!("template_id={}", encode(template_id)));
                }
                if let Some(ref dataset_id) = p.dataset_id {
                    query_parts.push(format!("dataset_id={}", encode(dataset_id)));
                }
                if query_parts.is_empty() {
                    "/v1/training/jobs".to_string()
                } else {
                    format!("/v1/training/jobs?{}", query_parts.join("&"))
                }
            }
            None => "/v1/training/jobs".to_string(),
        };
        self.get(&path).await
    }

    /// Get training job details
    pub async fn get_training_job(
        &self,
        id: &str,
    ) -> ApiResult<adapteros_api_types::TrainingJobResponse> {
        self.get(&format!("/v1/training/jobs/{}", id)).await
    }

    /// Cancel a training job
    pub async fn cancel_training_job(&self, id: &str) -> ApiResult<()> {
        self.post_no_response(
            &format!("/v1/training/jobs/{}/cancel", id),
            &serde_json::json!({}),
        )
        .await
    }

    /// Create a new training job
    pub async fn create_training_job(
        &self,
        request: &CreateTrainingJobRequest,
    ) -> ApiResult<adapteros_api_types::TrainingJobResponse> {
        self.post("/v1/training/jobs", request).await
    }

    /// Inspect CoreML preprocessing cache status for a dataset/model pair
    pub async fn get_preprocess_status(
        &self,
        request: &adapteros_api_types::PreprocessStatusRequest,
    ) -> ApiResult<adapteros_api_types::PreprocessStatusResponse> {
        self.post("/v1/training/preprocessing/status", request)
            .await
    }

    /// Get training logs for a job
    pub async fn get_training_logs(&self, job_id: &str) -> ApiResult<Vec<String>> {
        self.get(&format!("/v1/training/jobs/{}/logs", job_id))
            .await
    }

    /// Get training metrics time-series for a job
    pub async fn get_training_metrics(
        &self,
        job_id: &str,
    ) -> ApiResult<adapteros_api_types::TrainingMetricsListResponse> {
        self.get(&format!("/v1/training/jobs/{}/metrics", job_id))
            .await
    }

    /// Get backend readiness for training (CoreML/Metal/MLX availability)
    pub async fn get_training_backend_readiness(
        &self,
    ) -> ApiResult<adapteros_api_types::TrainingBackendReadinessResponse> {
        self.get("/v1/training/backend-readiness").await
    }

    // --- Models ---

    /// List all models with stats
    pub async fn list_models(&self) -> ApiResult<ModelListResponse> {
        self.get("/v1/models").await
    }

    /// List all models status
    pub async fn list_models_status(&self) -> ApiResult<AllModelsStatusResponse> {
        self.get("/v1/models/status/all").await
    }

    /// Get model status by ID
    pub async fn get_model(&self, id: &str) -> ApiResult<ModelStatusResponse> {
        self.get(&format!("/v1/models/{}/status", id)).await
    }

    /// Import a new model
    pub async fn seed_model(&self, request: &SeedModelRequest) -> ApiResult<SeedModelResponse> {
        self.post("/v1/models/import", request).await
    }

    /// Load a model into memory
    pub async fn load_model(&self, id: &str) -> ApiResult<ModelStatusResponse> {
        self.post_empty(&format!("/v1/models/{}/load", id)).await
    }

    /// Unload a model from memory
    pub async fn unload_model(&self, id: &str) -> ApiResult<ModelStatusResponse> {
        self.post_empty(&format!("/v1/models/{}/unload", id)).await
    }

    // --- Stacks ---

    /// List adapter stacks
    pub async fn list_stacks(&self) -> ApiResult<Vec<StackResponse>> {
        self.get("/v1/adapter-stacks").await
    }

    /// Get stack by ID
    pub async fn get_stack(&self, id: &str) -> ApiResult<StackResponse> {
        self.get(&format!("/v1/adapter-stacks/{}", id)).await
    }

    /// Create a new adapter stack
    pub async fn create_stack(&self, request: &CreateStackRequest) -> ApiResult<StackResponse> {
        self.post("/v1/adapter-stacks", request).await
    }

    /// Activate an adapter stack
    pub async fn activate_stack(&self, id: &str) -> ApiResult<serde_json::Value> {
        self.post(
            &format!("/v1/adapter-stacks/{}/activate", id),
            &serde_json::json!({}),
        )
        .await
    }

    /// Deactivate the current adapter stack
    pub async fn deactivate_stack(&self) -> ApiResult<serde_json::Value> {
        self.post("/v1/adapter-stacks/deactivate", &serde_json::json!({}))
            .await
    }

    /// Delete an adapter stack
    pub async fn delete_stack(&self, id: &str) -> ApiResult<()> {
        self.delete(&format!("/v1/adapter-stacks/{}", id)).await
    }

    /// Update an adapter stack
    pub async fn update_stack(
        &self,
        id: &str,
        request: &UpdateStackRequest,
    ) -> ApiResult<StackResponse> {
        self.put(&format!("/v1/adapter-stacks/{}", id), request)
            .await
    }

    // --- Policies ---

    /// List policy packs
    pub async fn list_policies(&self) -> ApiResult<Vec<PolicyPackResponse>> {
        self.get("/v1/policies").await
    }

    /// Get policy pack by CPID
    pub async fn get_policy(&self, cpid: &str) -> ApiResult<PolicyPackResponse> {
        self.get(&format!("/v1/policies/{}", cpid)).await
    }

    /// Validate a policy pack content
    pub async fn validate_policy(
        &self,
        content: &str,
    ) -> ApiResult<PolicyValidationResponse> {
        self.post(
            "/v1/policies/validate",
            &ValidatePolicyRequest {
                content: content.to_string(),
            },
        )
        .await
    }

    /// Apply a policy pack (create or update)
    pub async fn apply_policy(
        &self,
        cpid: &str,
        content: &str,
        description: Option<String>,
    ) -> ApiResult<PolicyPackResponse> {
        self.post(
            "/v1/policies/apply",
            &ApplyPolicyRequest {
                cpid: cpid.to_string(),
                content: content.to_string(),
                description,
                activate: Some(true),
            },
        )
        .await
    }

    // --- Settings ---

    /// Get system settings
    pub async fn get_settings(&self) -> ApiResult<adapteros_api_types::SystemSettings> {
        self.get("/v1/settings").await
    }

    /// Update system settings
    pub async fn update_settings(
        &self,
        request: &adapteros_api_types::UpdateSettingsRequest,
    ) -> ApiResult<adapteros_api_types::SettingsUpdateResponse> {
        self.put("/v1/settings", request).await
    }

    // --- Dashboard ---

    /// Get dashboard widget configuration
    pub async fn dashboard_config(
        &self,
    ) -> ApiResult<adapteros_api_types::GetDashboardConfigResponse> {
        self.get("/v1/dashboard/config").await
    }

    // --- Inference ---

    /// Send an inference request (non-streaming)
    pub async fn infer(
        &self,
        request: &InferenceRequest,
    ) -> ApiResult<adapteros_api_types::InferResponse> {
        self.post("/v1/infer", request).await
    }

    /// Get inference stream URL for SSE
    pub fn infer_stream_url(&self) -> String {
        format!("{}/v1/infer/stream", self.base_url)
    }

    // --- Topology ---

    /// Get topology graph with optional router preview
    ///
    /// When `preview_text` is provided, runs a dry-run of the router to predict
    /// which adapters would be selected, returned in `predicted_path`.
    pub async fn get_topology_preview(
        &self,
        preview_text: Option<&str>,
    ) -> ApiResult<adapteros_api_types::TopologyGraph> {
        let path = match preview_text {
            Some(text) if !text.trim().is_empty() => {
                format!("/v1/topology?preview_text={}", encode(text))
            }
            _ => "/v1/topology".to_string(),
        };
        self.get(&path).await
    }

    // --- Collections ---

    /// List collections with pagination
    pub async fn list_collections(
        &self,
        page: u32,
        limit: u32,
    ) -> ApiResult<CollectionListResponse> {
        self.get(&format!("/v1/collections?page={}&limit={}", page, limit))
            .await
    }

    /// Get collection details with documents
    pub async fn get_collection(&self, id: &str) -> ApiResult<CollectionDetailResponse> {
        self.get(&format!("/v1/collections/{}", id)).await
    }

    /// Create a new collection
    pub async fn create_collection(
        &self,
        request: &CreateCollectionRequest,
    ) -> ApiResult<CollectionResponse> {
        self.post("/v1/collections", request).await
    }

    /// Delete a collection
    pub async fn delete_collection(&self, id: &str) -> ApiResult<()> {
        self.delete(&format!("/v1/collections/{}", id)).await
    }

    /// Add a document to a collection
    pub async fn add_document_to_collection(
        &self,
        collection_id: &str,
        document_id: &str,
    ) -> ApiResult<()> {
        let request = AddDocumentRequest {
            document_id: document_id.to_string(),
        };
        self.post_no_response(
            &format!("/v1/collections/{}/documents", collection_id),
            &request,
        )
        .await
    }

    /// Remove a document from a collection
    pub async fn remove_document_from_collection(
        &self,
        collection_id: &str,
        document_id: &str,
    ) -> ApiResult<()> {
        self.delete(&format!(
            "/v1/collections/{}/documents/{}",
            collection_id, document_id
        ))
        .await
    }

    // --- Repositories ---

    /// List all repositories with optional status filter
    pub async fn list_repositories(
        &self,
        status: Option<&str>,
    ) -> ApiResult<RepositoryListResponse> {
        let mut params = Vec::new();
        if let Some(s) = status {
            params.push(format!("status={}", encode(s)));
        }
        params.push("page=1".to_string());
        params.push("limit=100".to_string());
        let path = if params.is_empty() {
            "/v1/code/repositories".to_string()
        } else {
            format!("/v1/code/repositories?{}", params.join("&"))
        };
        self.get(&path).await
    }

    /// Get repository details by ID
    pub async fn get_repository(&self, repo_id: &str) -> ApiResult<RepositoryDetailResponse> {
        self.get(&format!("/v1/code/repositories/{}", repo_id)).await
    }

    /// Register a new repository
    pub async fn register_repository(
        &self,
        request: &RegisterRepositoryRequest,
    ) -> ApiResult<RegisterRepositoryResponse> {
        self.post("/v1/code/register-repo", request).await
    }

    /// Trigger a scan for a repository
    pub async fn scan_repository(
        &self,
        request: &ScanRepositoryRequest,
    ) -> ApiResult<ScanJobResponse> {
        self.post("/v1/code/scan", request).await
    }

    /// Fetch activity feed for the current user's workspaces
    pub async fn activity_feed(&self, limit: Option<i64>) -> ApiResult<Vec<ActivityEventResponse>> {
        let path = match limit {
            Some(l) => format!("/v1/activity/feed?limit={}", l),
            None => "/v1/activity/feed".to_string(),
        };
        self.get(&path).await
    }


    // --- Audit ---

    /// Query audit logs with filtering
    pub async fn query_audit_logs(&self, query: &AuditLogsQuery) -> ApiResult<AuditLogsResponse> {
        let mut path = "/v1/audit/logs?".to_string();
        let mut params = Vec::new();

        if let Some(ref user_id) = query.user_id {
            params.push(format!("user_id={}", encode(user_id)));
        }
        if let Some(ref action) = query.action {
            params.push(format!("action={}", encode(action)));
        }
        if let Some(ref resource_type) = query.resource_type {
            params.push(format!("resource_type={}", encode(resource_type)));
        }
        if let Some(ref status) = query.status {
            params.push(format!("status={}", encode(status)));
        }
        if let Some(ref from_time) = query.from_time {
            params.push(format!("from_time={}", encode(from_time)));
        }
        if let Some(ref to_time) = query.to_time {
            params.push(format!("to_time={}", encode(to_time)));
        }
        if let Some(limit) = query.limit {
            params.push(format!("limit={}", limit));
        }
        if let Some(offset) = query.offset {
            params.push(format!("offset={}", offset));
        }

        path.push_str(&params.join("&"));
        self.get(&path).await
    }

    /// Get audit chain with verification status
    pub async fn get_audit_chain(&self, limit: Option<usize>) -> ApiResult<AuditChainResponse> {
        let path = match limit {
            Some(l) => format!("/v1/audit/chain?limit={}", l),
            None => "/v1/audit/chain".to_string(),
        };
        self.get(&path).await
    }

    /// Verify audit chain integrity
    pub async fn verify_audit_chain(&self) -> ApiResult<ChainVerificationResponse> {
        self.get("/v1/audit/chain/verify").await
    }

    /// Get federation audit report
    pub async fn get_federation_audit(&self) -> ApiResult<FederationAuditResponse> {
        self.get("/v1/audit/federation").await
    }

    /// Get compliance audit report
    pub async fn get_compliance_audit(&self) -> ApiResult<ComplianceAuditResponse> {
        self.get("/v1/audit/compliance").await
    }

    // --- Diagnostics ---

    /// List diagnostic runs with filtering
    pub async fn list_diag_runs(
        &self,
        query: &adapteros_api_types::diagnostics::ListDiagRunsQuery,
    ) -> ApiResult<adapteros_api_types::diagnostics::ListDiagRunsResponse> {
        let mut path = "/v1/diag/runs?".to_string();
        let mut params = Vec::new();

        if let Some(since) = query.since {
            params.push(format!("since={}", since));
        }
        if let Some(limit) = query.limit {
            params.push(format!("limit={}", limit));
        }
        if let Some(ref after) = query.after {
            params.push(format!("after={}", encode(after)));
        }
        if let Some(ref status) = query.status {
            params.push(format!("status={}", encode(status)));
        }

        path.push_str(&params.join("&"));
        self.get(&path).await
    }

    /// Compare two diagnostic runs to find deterministic divergence
    pub async fn diff_diag_runs(
        &self,
        request: &adapteros_api_types::diagnostics::DiagDiffRequest,
    ) -> ApiResult<adapteros_api_types::diagnostics::DiagDiffResponse> {
        self.post("/v1/diag/diff", request).await
    }

    /// Export a diagnostic run with all events and timing
    pub async fn export_diag_run(
        &self,
        run_id: &str,
    ) -> ApiResult<adapteros_api_types::diagnostics::DiagExportResponse> {
        self.get(&format!("/v1/diag/runs/{}/export", run_id)).await
    }

    // --- Search ---

    /// Global search across entities
    pub async fn search(
        &self,
        query: &str,
        scope: Option<&str>,
        limit: Option<u32>,
    ) -> ApiResult<SearchResponse> {
        // Simple URL encoding for search query
        let encoded_query = query
            .replace('%', "%25")
            .replace(' ', "%20")
            .replace('&', "%26")
            .replace('=', "%3D")
            .replace('+', "%2B")
            .replace('#', "%23");
        let mut path = format!("/v1/search?q={}", encoded_query);
        if let Some(s) = scope {
            path.push_str(&format!("&scope={}", s));
        }
        if let Some(l) = limit {
            path.push_str(&format!("&limit={}", l));
        }
        self.get(&path).await
    }

    // --- Documents ---

    /// List documents with optional filtering
    pub async fn list_documents(
        &self,
        params: Option<&DocumentListParams>,
    ) -> ApiResult<DocumentListResponse> {
        let path = match params {
            Some(p) => {
                let mut query_parts = Vec::new();
                if let Some(status) = &p.status {
                    query_parts.push(format!("status={}", status));
                }
                if let Some(page) = p.page {
                    query_parts.push(format!("page={}", page));
                }
                if let Some(limit) = p.limit {
                    query_parts.push(format!("limit={}", limit));
                }
                if query_parts.is_empty() {
                    "/v1/documents".to_string()
                } else {
                    format!("/v1/documents?{}", query_parts.join("&"))
                }
            }
            None => "/v1/documents".to_string(),
        };
        self.get(&path).await
    }

    /// Get document by ID
    pub async fn get_document(&self, id: &str) -> ApiResult<DocumentResponse> {
        self.get(&format!("/v1/documents/{}", id)).await
    }

    /// Delete document by ID
    pub async fn delete_document(&self, id: &str) -> ApiResult<()> {
        self.delete(&format!("/v1/documents/{}", id)).await
    }

    /// Get document chunks
    pub async fn get_document_chunks(&self, id: &str) -> ApiResult<ChunkListResponse> {
        self.get(&format!("/v1/documents/{}/chunks", id)).await
    }

    /// Trigger document processing/reprocessing
    pub async fn process_document(&self, id: &str) -> ApiResult<ProcessDocumentResponse> {
        self.post(
            &format!("/v1/documents/{}/process", id),
            &serde_json::json!({}),
        )
        .await
    }

    /// Retry failed document processing
    pub async fn retry_document(&self, id: &str) -> ApiResult<DocumentResponse> {
        self.post(
            &format!("/v1/documents/{}/retry", id),
            &serde_json::json!({}),
        )
        .await
    }

    /// List failed documents
    pub async fn list_failed_documents(&self) -> ApiResult<DocumentListResponse> {
        self.get("/v1/documents/failed").await
    }

    // --- Document Upload (multipart) ---

    /// Upload a document via multipart form data
    ///
    /// This method uses the raw fetch API to support multipart/form-data uploads
    /// which are required for file uploads to the backend.
    #[cfg(target_arch = "wasm32")]
    pub async fn upload_document(&self, file: &web_sys::File) -> ApiResult<DocumentResponse> {
        use wasm_bindgen::JsCast;
        use wasm_bindgen_futures::JsFuture;

        let url = format!("{}/v1/documents/upload", self.base_url);

        // Create FormData and append file
        let form_data = web_sys::FormData::new()
            .map_err(|_| ApiError::Network("Failed to create FormData".into()))?;
        form_data
            .append_with_blob("file", file)
            .map_err(|_| ApiError::Network("Failed to append file to FormData".into()))?;

        // Build request options
        let opts = web_sys::RequestInit::new();
        opts.set_method("POST");
        opts.set_body(&form_data);

        // Add auth header if available
        let headers = web_sys::Headers::new()
            .map_err(|_| ApiError::Network("Failed to create Headers".into()))?;
        if let Some(token) = self.auth_token.read().ok().and_then(|t| t.clone()) {
            headers
                .set("Authorization", &format!("Bearer {}", token))
                .map_err(|_| ApiError::Network("Failed to set Authorization header".into()))?;
        }
        opts.set_headers(&headers);

        // Perform fetch
        let window = web_sys::window().ok_or_else(|| ApiError::Network("No window".into()))?;
        let resp_value = JsFuture::from(window.fetch_with_str_and_init(&url, &opts))
            .await
            .map_err(|_| ApiError::Network("Fetch failed".into()))?;
        let resp: web_sys::Response = resp_value
            .dyn_into()
            .map_err(|_| ApiError::Network("Invalid response".into()))?;

        if !resp.ok() {
            let status = resp.status();
            let text = match resp.text() {
                Ok(promise) => JsFuture::from(promise)
                    .await
                    .ok()
                    .and_then(|v| v.as_string())
                    .unwrap_or_default(),
                Err(_) => String::new(),
            };
            return Err(ApiError::from_response(status, &text));
        }

        // Parse JSON response
        let json_promise = resp
            .json()
            .map_err(|_| ApiError::Serialization("Failed to get JSON promise".into()))?;
        let json = JsFuture::from(json_promise)
            .await
            .map_err(|_| ApiError::Serialization("Failed to parse JSON".into()))?;
        let result: DocumentResponse = serde_wasm_bindgen::from_value(json)
            .map_err(|e| ApiError::Serialization(e.to_string()))?;
        Ok(result)
    }

    // --- Datasets ---

    /// List all datasets with optional type filter
    pub async fn list_datasets(
        &self,
        dataset_type: Option<&str>,
    ) -> ApiResult<DatasetListResponse> {
        let path = match dataset_type {
            Some(t) => format!("/v1/datasets?type={}", encode(t)),
            None => "/v1/datasets".to_string(),
        };
        self.get(&path).await
    }

    /// Get a single dataset by ID
    pub async fn get_dataset(&self, id: &str) -> ApiResult<DatasetResponse> {
        self.get(&format!("/v1/datasets/{}", id)).await
    }

    /// List dataset versions
    pub async fn list_dataset_versions(
        &self,
        dataset_id: &str,
    ) -> ApiResult<DatasetVersionsResponse> {
        self.get(&format!("/v1/datasets/{}/versions", dataset_id))
            .await
    }

    /// List dataset files
    pub async fn list_dataset_files(
        &self,
        dataset_id: &str,
    ) -> ApiResult<Vec<DatasetFileResponse>> {
        self.get(&format!("/v1/datasets/{}/files", dataset_id))
            .await
    }

    /// Fetch dataset file content as text
    pub async fn get_dataset_file_content(
        &self,
        dataset_id: &str,
        file_id: &str,
    ) -> ApiResult<String> {
        self.get_text(&format!(
            "/v1/datasets/{}/files/{}/content",
            dataset_id, file_id
        ))
        .await
    }

    /// Validate a single dataset file
    pub async fn validate_dataset_file(
        &self,
        dataset_id: &str,
        file_id: &str,
        request: &ValidateFileRequest,
    ) -> ApiResult<ValidateFileResponse> {
        self.post(
            &format!("/v1/datasets/{}/files/{}/validate", dataset_id, file_id),
            request,
        )
        .await
    }

    /// Validate all dataset files
    pub async fn validate_all_dataset_files(
        &self,
        dataset_id: &str,
        request: &ValidateFileRequest,
    ) -> ApiResult<ValidateAllFilesResponse> {
        self.post(
            &format!("/v1/datasets/{}/files/validate", dataset_id),
            request,
        )
        .await
    }

    /// Delete a dataset
    pub async fn delete_dataset(&self, id: &str) -> ApiResult<()> {
        self.delete(&format!("/v1/datasets/{}", id)).await
    }

    /// Get dataset statistics
    pub async fn get_dataset_statistics(&self, id: &str) -> ApiResult<DatasetStatisticsResponse> {
        self.get(&format!("/v1/datasets/{}/statistics", id)).await
    }

    /// Stream normalized dataset rows for a version
    pub async fn list_dataset_rows(
        &self,
        dataset_version_id: &str,
        split: Option<&str>,
        shuffle_seed: Option<&str>,
    ) -> ApiResult<Vec<CanonicalRow>> {
        let mut query_parts = Vec::new();
        if let Some(split) = split {
            let encoded = js_sys::encode_uri_component(split);
            query_parts.push(format!("split={}", encoded));
        }
        if let Some(seed) = shuffle_seed {
            let encoded = js_sys::encode_uri_component(seed);
            query_parts.push(format!("shuffle_seed={}", encoded));
        }
        let path = if query_parts.is_empty() {
            format!("/v1/training/dataset_versions/{}/rows", dataset_version_id)
        } else {
            format!(
                "/v1/training/dataset_versions/{}/rows?{}",
                dataset_version_id,
                query_parts.join("&")
            )
        };
        self.get(&path).await
    }

    /// Create a training dataset from existing document(s)
    pub async fn create_dataset_from_documents(
        &self,
        document_ids: Vec<String>,
        name: Option<String>,
    ) -> ApiResult<DatasetResponse> {
        #[derive(serde::Serialize)]
        struct CreateDatasetFromDocsRequest {
            document_ids: Vec<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            name: Option<String>,
        }
        self.post(
            "/v1/datasets/from-documents",
            &CreateDatasetFromDocsRequest { document_ids, name },
        )
        .await
    }

    /// Upload a training dataset via multipart form data.
    ///
    /// Accepts multiple files and forwards them to `/v1/datasets`.
    #[cfg(target_arch = "wasm32")]
    pub async fn upload_dataset(
        &self,
        form_data: &web_sys::FormData,
    ) -> ApiResult<adapteros_api_types::UploadDatasetResponse> {
        use wasm_bindgen::JsCast;
        use wasm_bindgen_futures::JsFuture;

        let url = format!("{}/v1/datasets", self.base_url);

        let opts = web_sys::RequestInit::new();
        opts.set_method("POST");
        opts.set_body(form_data);

        let headers = web_sys::Headers::new()
            .map_err(|_| ApiError::Network("Failed to create Headers".into()))?;
        if let Some(token) = self.auth_token.read().ok().and_then(|t| t.clone()) {
            headers
                .set("Authorization", &format!("Bearer {}", token))
                .map_err(|_| ApiError::Network("Failed to set Authorization header".into()))?;
        }
        opts.set_headers(&headers);

        let window = web_sys::window().ok_or_else(|| ApiError::Network("No window".into()))?;
        let request = web_sys::Request::new_with_str_and_init(&url, &opts)
            .map_err(|_| ApiError::Network("Failed to create Request".into()))?;

        let resp_value = JsFuture::from(window.fetch_with_request(&request))
            .await
            .map_err(|_| ApiError::Network("Upload request failed".into()))?;
        let resp: web_sys::Response = resp_value
            .dyn_into()
            .map_err(|_| ApiError::Network("Failed to cast Response".into()))?;

        if !resp.ok() {
            return Err(ApiError::Http {
                status: resp.status(),
                message: resp.status_text(),
            });
        }

        let json = JsFuture::from(
            resp.json()
                .map_err(|_| ApiError::Network("Failed to read response body".into()))?,
        )
        .await
        .map_err(|_| ApiError::Network("Failed to parse response JSON".into()))?;

        let result: adapteros_api_types::UploadDatasetResponse =
            serde_wasm_bindgen::from_value(json)
                .map_err(|e| ApiError::Serialization(e.to_string()))?;

        Ok(result)
    }

    /// Generate a training dataset from a file using local inference.
    ///
    /// Accepts multipart form data with:
    /// - `file`: The text file to generate from
    /// - `name`: Dataset name (optional)
    /// - `strategy`: "qa" or "summary" (default: qa)
    /// - `chunk_size`: Chunk size in characters (default: 2000)
    /// - `max_tokens`: Max tokens per inference (default: 512)
    #[cfg(target_arch = "wasm32")]
    pub async fn generate_dataset(
        &self,
        form_data: &web_sys::FormData,
    ) -> ApiResult<adapteros_api_types::training::GenerateDatasetResponse> {
        use wasm_bindgen::JsCast;
        use wasm_bindgen_futures::JsFuture;

        let url = format!("{}/v1/training/datasets/generate", self.base_url);

        let opts = web_sys::RequestInit::new();
        opts.set_method("POST");
        opts.set_body(form_data);

        let headers = web_sys::Headers::new()
            .map_err(|_| ApiError::Network("Failed to create Headers".into()))?;
        if let Some(token) = self.auth_token.read().ok().and_then(|t| t.clone()) {
            headers
                .set("Authorization", &format!("Bearer {}", token))
                .map_err(|_| ApiError::Network("Failed to set Authorization header".into()))?;
        }
        opts.set_headers(&headers);

        let window = web_sys::window().ok_or_else(|| ApiError::Network("No window".into()))?;
        let request = web_sys::Request::new_with_str_and_init(&url, &opts)
            .map_err(|_| ApiError::Network("Failed to create Request".into()))?;

        let resp_value = JsFuture::from(window.fetch_with_request(&request))
            .await
            .map_err(|_| ApiError::Network("Generate dataset request failed".into()))?;
        let resp: web_sys::Response = resp_value
            .dyn_into()
            .map_err(|_| ApiError::Network("Failed to cast Response".into()))?;

        if !resp.ok() {
            let status = resp.status();
            let text = match resp.text() {
                Ok(promise) => JsFuture::from(promise)
                    .await
                    .ok()
                    .and_then(|v| v.as_string())
                    .unwrap_or_default(),
                Err(_) => String::new(),
            };
            return Err(ApiError::from_response(status, &text));
        }

        let json = JsFuture::from(
            resp.json()
                .map_err(|_| ApiError::Network("Failed to read response body".into()))?,
        )
        .await
        .map_err(|_| ApiError::Network("Failed to parse response JSON".into()))?;

        let result: adapteros_api_types::training::GenerateDatasetResponse =
            serde_wasm_bindgen::from_value(json)
                .map_err(|e| ApiError::Serialization(e.to_string()))?;

        Ok(result)
    }

    /// Fetch a normalized dataset manifest for a version.
    pub async fn get_dataset_manifest(
        &self,
        dataset_version_id: &str,
    ) -> ApiResult<adapteros_api_types::DatasetManifest> {
        self.get(&format!(
            "/v1/training/dataset_versions/{}/manifest",
            dataset_version_id
        ))
        .await
    }

    // --- Code Policy ---

    /// Get code policy settings
    pub async fn get_code_policy(&self) -> ApiResult<GetCodePolicyResponse> {
        self.get("/v1/code-policy").await
    }

    /// Update code policy settings
    pub async fn update_code_policy(
        &self,
        request: &UpdateCodePolicyRequest,
    ) -> ApiResult<GetCodePolicyResponse> {
        self.put("/v1/code-policy", request).await
    }

    // --- Process Monitoring ---

    /// Get process logs for a worker
    pub async fn get_worker_logs(
        &self,
        worker_id: &str,
        level: Option<&str>,
        limit: Option<u32>,
    ) -> ApiResult<Vec<ProcessLogResponse>> {
        let mut path = format!("/v1/workers/{}/logs", worker_id);
        let mut params = Vec::new();
        if let Some(l) = level {
            params.push(format!("level={}", l));
        }
        if let Some(lim) = limit {
            params.push(format!("limit={}", lim));
        }
        if !params.is_empty() {
            path.push('?');
            path.push_str(&params.join("&"));
        }
        self.get(&path).await
    }

    /// Get process crash dumps for a worker
    pub async fn get_worker_crashes(
        &self,
        worker_id: &str,
    ) -> ApiResult<Vec<ProcessCrashDumpResponse>> {
        self.get(&format!("/v1/workers/{}/crashes", worker_id))
            .await
    }

    /// Get process health metrics
    pub async fn get_process_health_metrics(
        &self,
        worker_id: Option<&str>,
    ) -> ApiResult<Vec<ProcessHealthMetricResponse>> {
        let path = match worker_id {
            Some(id) => format!("/v1/monitoring/health?worker_id={}", id),
            None => "/v1/monitoring/health".to_string(),
        };
        self.get(&path).await
    }

    /// List process monitoring rules
    pub async fn list_monitoring_rules(&self) -> ApiResult<Vec<ProcessMonitoringRuleResponse>> {
        self.get("/v1/monitoring/rules").await
    }

    /// List process alerts
    pub async fn list_process_alerts(
        &self,
        status: Option<&str>,
    ) -> ApiResult<Vec<ProcessAlertResponse>> {
        let path = match status {
            Some(s) => format!("/v1/monitoring/alerts?status={}", s),
            None => "/v1/monitoring/alerts".to_string(),
        };
        self.get(&path).await
    }

    /// Acknowledge a process alert
    pub async fn acknowledge_alert(&self, alert_id: &str) -> ApiResult<ProcessAlertResponse> {
        self.post(
            &format!("/v1/monitoring/alerts/{}/acknowledge", alert_id),
            &serde_json::json!({}),
        )
        .await
    }

    /// List process anomalies
    pub async fn list_process_anomalies(
        &self,
        status: Option<&str>,
    ) -> ApiResult<Vec<ProcessAnomalyResponse>> {
        let path = match status {
            Some(s) => format!("/v1/monitoring/anomalies?status={}", s),
            None => "/v1/monitoring/anomalies".to_string(),
        };
        self.get(&path).await
    }

    /// Create a monitoring rule
    pub async fn create_monitoring_rule(
        &self,
        request: &CreateMonitoringRuleRequest,
    ) -> ApiResult<ProcessMonitoringRuleResponse> {
        self.post("/v1/monitoring/rules", request).await
    }

    /// Delete a monitoring rule
    pub async fn delete_monitoring_rule(&self, rule_id: &str) -> ApiResult<()> {
        self.delete(&format!("/v1/monitoring/rules/{}", rule_id))
            .await
    }

    /// Toggle a monitoring rule enabled/disabled
    pub async fn toggle_monitoring_rule(
        &self,
        rule_id: &str,
        enabled: bool,
    ) -> ApiResult<ProcessMonitoringRuleResponse> {
        self.put(
            &format!("/v1/monitoring/rules/{}", rule_id),
            &serde_json::json!({ "enabled": enabled }),
        )
        .await
    }

    /// Suppress a process alert
    pub async fn suppress_alert(
        &self,
        alert_id: &str,
        reason: &str,
    ) -> ApiResult<ProcessAlertResponse> {
        self.post(
            &format!("/v1/monitoring/alerts/{}/suppress", alert_id),
            &serde_json::json!({ "reason": reason }),
        )
        .await
    }

    /// Resolve a process alert
    pub async fn resolve_alert(&self, alert_id: &str) -> ApiResult<ProcessAlertResponse> {
        self.post(
            &format!("/v1/monitoring/alerts/{}/resolve", alert_id),
            &serde_json::json!({}),
        )
        .await
    }

    // --- Routing Decisions ---

    /// Get routing decisions with optional filters
    pub async fn get_routing_decisions(
        &self,
        query: &RoutingDecisionsQuery,
    ) -> ApiResult<RoutingDecisionsResponse> {
        let mut params = Vec::new();
        if let Some(ref tenant) = query.tenant {
            params.push(format!("tenant={}", encode(tenant)));
        }
        if let Some(ref stack_id) = query.stack_id {
            params.push(format!("stack_id={}", encode(stack_id)));
        }
        if let Some(ref adapter_id) = query.adapter_id {
            params.push(format!("adapter_id={}", encode(adapter_id)));
        }
        if let Some(anomalies_only) = query.anomalies_only {
            params.push(format!("anomalies_only={}", anomalies_only));
        }
        if let Some(min_entropy) = query.min_entropy {
            params.push(format!("min_entropy={}", min_entropy));
        }
        if let Some(limit) = query.limit {
            params.push(format!("limit={}", limit));
        }
        if let Some(offset) = query.offset {
            params.push(format!("offset={}", offset));
        }

        let path = if params.is_empty() {
            "/v1/routing/decisions".to_string()
        } else {
            format!("/v1/routing/decisions?{}", params.join("&"))
        };
        self.get(&path).await
    }

    /// Get a specific routing decision by ID
    pub async fn get_routing_decision(&self, id: &str) -> ApiResult<RoutingDecisionResponse> {
        self.get(&format!("/v1/routing/decisions/{}", id)).await
    }

    /// Debug routing for a prompt
    pub async fn debug_routing(
        &self,
        request: &RoutingDebugRequest,
    ) -> ApiResult<RoutingDebugResponse> {
        self.post("/v1/routing/debug", request).await
    }

    /// Get routing history
    pub async fn get_routing_history(
        &self,
        limit: Option<usize>,
    ) -> ApiResult<RoutingDecisionsResponse> {
        let path = match limit {
            Some(l) => format!("/v1/routing/history?limit={}", l),
            None => "/v1/routing/history".to_string(),
        };
        self.get(&path).await
    }

    /// Get routing decision chain for an inference request
    pub async fn get_routing_chain(
        &self,
        tenant: &str,
        inference_id: &str,
        verify: bool,
    ) -> ApiResult<RoutingDecisionChainResponse> {
        self.get(&format!(
            "/v1/routing/chain?tenant={}&inference_id={}&verify={}",
            encode(tenant),
            encode(inference_id),
            verify
        ))
        .await
    }

    // --- Client Errors ---

    /// List client errors with optional filtering
    #[allow(clippy::too_many_arguments)]
    pub async fn list_client_errors(
        &self,
        error_type: Option<&str>,
        http_status: Option<i32>,
        page_pattern: Option<&str>,
        since: Option<&str>,
        until: Option<&str>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> ApiResult<adapteros_api_types::telemetry::ClientErrorsListResponse> {
        let mut params = Vec::new();
        if let Some(t) = error_type {
            params.push(format!("error_type={}", encode(t)));
        }
        if let Some(s) = http_status {
            params.push(format!("http_status={}", s));
        }
        if let Some(p) = page_pattern {
            params.push(format!("page_pattern={}", encode(p)));
        }
        if let Some(s) = since {
            params.push(format!("since={}", encode(s)));
        }
        if let Some(u) = until {
            params.push(format!("until={}", encode(u)));
        }
        if let Some(l) = limit {
            params.push(format!("limit={}", l));
        }
        if let Some(o) = offset {
            params.push(format!("offset={}", o));
        }
        let path = if params.is_empty() {
            "/v1/telemetry/client-errors".to_string()
        } else {
            format!("/v1/telemetry/client-errors?{}", params.join("&"))
        };
        self.get(&path).await
    }

    /// Get client error statistics
    pub async fn get_client_error_stats(
        &self,
        since: Option<&str>,
    ) -> ApiResult<adapteros_api_types::telemetry::ClientErrorStatsResponse> {
        let path = match since {
            Some(s) => format!("/v1/telemetry/client-errors/stats?since={}", s),
            None => "/v1/telemetry/client-errors/stats".to_string(),
        };
        self.get(&path).await
    }

    /// Get a specific client error by ID
    pub async fn get_client_error(
        &self,
        id: &str,
    ) -> ApiResult<adapteros_api_types::telemetry::ClientErrorItem> {
        self.get(&format!("/v1/telemetry/client-errors/{}", id))
            .await
    }

    // ========================================================================
    // Routing Rules
    // ========================================================================

    /// List all routing rules for an identity dataset
    pub async fn list_routing_rules(
        &self,
        identity_dataset_id: &str,
    ) -> ApiResult<RoutingRulesResponse> {
        self.get(&format!(
            "/v1/routing-rules/identity/{}",
            identity_dataset_id
        ))
        .await
    }

    /// Create a new routing rule
    pub async fn create_routing_rule(
        &self,
        request: &CreateRoutingRuleRequest,
    ) -> ApiResult<RoutingRuleResponse> {
        self.post("/v1/routing-rules", request).await
    }

    /// Delete a routing rule
    pub async fn delete_routing_rule(&self, rule_id: &str) -> ApiResult<()> {
        self.delete(&format!("/v1/routing-rules/{}", rule_id)).await
    }

    // --- Error Alerts ---

    /// List error alert rules
    pub async fn list_error_alert_rules(&self) -> ApiResult<ErrorAlertRulesListResponse> {
        self.get("/v1/error-alerts/rules").await
    }

    /// Get a specific error alert rule
    pub async fn get_error_alert_rule(&self, id: &str) -> ApiResult<ErrorAlertRuleResponse> {
        self.get(&format!("/v1/error-alerts/rules/{}", id)).await
    }

    /// Create a new error alert rule
    pub async fn create_error_alert_rule(
        &self,
        request: &CreateErrorAlertRuleRequest,
    ) -> ApiResult<ErrorAlertRuleResponse> {
        self.post("/v1/error-alerts/rules", request).await
    }

    /// Update an error alert rule
    pub async fn update_error_alert_rule(
        &self,
        id: &str,
        request: &UpdateErrorAlertRuleRequest,
    ) -> ApiResult<ErrorAlertRuleResponse> {
        self.put(&format!("/v1/error-alerts/rules/{}", id), request)
            .await
    }

    /// Delete an error alert rule
    pub async fn delete_error_alert_rule(&self, id: &str) -> ApiResult<()> {
        self.delete(&format!("/v1/error-alerts/rules/{}", id))
            .await
    }

    // --- Embedding Benchmarks ---

    /// List embedding benchmark reports
    pub async fn list_embedding_benchmarks(
        &self,
        query: Option<&EmbeddingBenchmarksQuery>,
    ) -> ApiResult<EmbeddingBenchmarksResponse> {
        let path = match query {
            Some(q) => {
                let mut params = Vec::new();
                if let Some(ref model_name) = q.model_name {
                    params.push(format!("model_name={}", encode(model_name)));
                }
                if let Some(limit) = q.limit {
                    params.push(format!("limit={}", limit));
                }
                if let Some(offset) = q.offset {
                    params.push(format!("offset={}", offset));
                }
                if params.is_empty() {
                    "/v1/embeddings/benchmarks".to_string()
                } else {
                    format!("/v1/embeddings/benchmarks?{}", params.join("&"))
                }
            }
            None => "/v1/embeddings/benchmarks".to_string(),
        };
        self.get(&path).await
    }
}

/// Simple inference request for chat
#[derive(Debug, Clone, serde::Serialize)]
pub struct InferenceRequest {
    pub prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
}

// ============================================================================
// Local types for API responses not in adapteros-api-types (wasm feature)
// ============================================================================

/// System overview response with complete system state
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SystemOverviewResponse {
    #[serde(default)]
    pub schema_version: String,
    pub uptime_seconds: u64,
    pub process_count: usize,
    pub load_average: LoadAverageInfo,
    pub resource_usage: ResourceUsageInfo,
    pub services: Vec<ServiceStatus>,
    pub active_sessions: i32,
    pub active_workers: i32,
    pub adapter_count: i32,
    pub timestamp: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin_node_id: Option<String>,
}

/// Load average information
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LoadAverageInfo {
    pub load_1min: f64,
    pub load_5min: f64,
    pub load_15min: f64,
}

/// Resource usage information
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ResourceUsageInfo {
    pub cpu_usage_percent: f32,
    pub memory_usage_percent: f32,
    pub disk_usage_percent: f32,
    pub network_rx_mbps: f32,
    pub network_tx_mbps: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_utilization_percent: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_memory_used_gb: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_memory_total_gb: Option<f32>,
}

/// Service status in system overview
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ServiceStatus {
    pub name: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_check: Option<u64>,
}

/// Error alert rule response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ErrorAlertRuleResponse {
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_type_pattern: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_status_pattern: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_pattern: Option<String>,
    pub threshold_count: i32,
    pub threshold_window_minutes: i32,
    pub cooldown_minutes: i32,
    pub severity: String,
    pub is_active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notification_channels: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

/// Request to create an error alert rule
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CreateErrorAlertRuleRequest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_type_pattern: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_status_pattern: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_pattern: Option<String>,
    pub threshold_count: i32,
    pub threshold_window_minutes: i32,
    #[serde(default = "default_cooldown_minutes")]
    pub cooldown_minutes: i32,
    #[serde(default = "default_severity")]
    pub severity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notification_channels: Option<serde_json::Value>,
}

fn default_cooldown_minutes() -> i32 {
    15
}

fn default_severity() -> String {
    "warning".to_string()
}

/// Request to update an error alert rule
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct UpdateErrorAlertRuleRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_type_pattern: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_status_pattern: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_pattern: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold_count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold_window_minutes: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cooldown_minutes: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_active: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notification_channels: Option<serde_json::Value>,
}

/// List error alert rules response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ErrorAlertRulesListResponse {
    pub rules: Vec<ErrorAlertRuleResponse>,
    pub total: usize,
}

/// Model architecture summary
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ModelArchitectureSummary {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub architecture: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_layers: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hidden_size: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vocab_size: Option<usize>,
}

/// Model with stats response (from /v1/models endpoint)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ModelWithStatsResponse {
    pub id: String,
    pub name: String,
    pub hash_b3: String,
    pub config_hash_b3: String,
    pub tokenizer_hash_b3: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub import_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantization: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
    #[serde(default)]
    pub adapter_count: i64,
    #[serde(default)]
    pub training_job_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imported_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    #[serde(alias = "architecture_summary")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub architecture: Option<ModelArchitectureSummary>,
}

/// Model list response (uses UI-specific ModelWithStatsResponse)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ModelListResponse {
    pub models: Vec<ModelWithStatsResponse>,
    pub total: usize,
}

// Model types (AneMemoryStatus, ModelStatusResponse, BaseModelStatusResponse,
// AllModelsStatusResponse, SeedModelRequest, SeedModelResponse) are now
// imported from adapteros_api_types::models

/// Workflow type for adapter stacks
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowType {
    Parallel,
    UpstreamDownstream,
    Sequential,
}

/// Create stack request
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CreateStackRequest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub adapter_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_type: Option<WorkflowType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<std::collections::HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub determinism_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub routing_determinism_mode: Option<String>,
}

/// Update stack request
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UpdateStackRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapter_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_type: Option<WorkflowType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<std::collections::HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub determinism_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub routing_determinism_mode: Option<String>,
}

/// Stack response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StackResponse {
    #[serde(default)]
    pub schema_version: String,
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub adapter_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_type: Option<WorkflowType>,
    pub created_at: String,
    pub updated_at: String,
    pub is_active: bool,
    #[serde(default)]
    pub is_default: bool,
    pub version: i64,
    pub lifecycle_state: String,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub determinism_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub routing_determinism_mode: Option<String>,
}

/// Policy pack response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PolicyPackResponse {
    pub cpid: String,
    pub content: String,
    pub hash_b3: String,
    pub created_at: String,
}

/// Validate policy request
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ValidatePolicyRequest {
    pub content: String,
}

/// Policy validation response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PolicyValidationResponse {
    pub valid: bool,
    pub errors: Vec<String>,
    pub hash_b3: Option<String>,
}

/// Apply policy request
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ApplyPolicyRequest {
    pub cpid: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activate: Option<bool>,
}

/// Create training job request
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CreateTrainingJobRequest {
    pub adapter_name: String,
    pub config: TrainingConfigRequest,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dataset_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
}

/// Training config for job creation
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TrainingConfigRequest {
    pub rank: u32,
    pub alpha: u32,
    pub targets: Vec<String>,
    pub training_contract_version: String,
    pub pad_token_id: u32,
    pub ignore_index: i32,
    pub epochs: u32,
    pub learning_rate: f32,
    pub batch_size: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warmup_steps: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_seq_length: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gradient_accumulation_steps: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation_split: Option<f32>,
}

// WorkerMetricsResponse is now imported from adapteros_api_types::workers

// ============================================================================
// Collection types
// ============================================================================

/// Collection response (from /v1/collections endpoint)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CollectionResponse {
    pub schema_version: String,
    pub collection_id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub document_count: i32,
    pub tenant_id: String,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

/// Collection detail response (includes documents)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CollectionDetailResponse {
    pub schema_version: String,
    pub collection_id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub document_count: i32,
    pub tenant_id: String,
    pub documents: Vec<CollectionDocumentInfo>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

/// Document info within a collection
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CollectionDocumentInfo {
    pub document_id: String,
    pub name: String,
    pub size_bytes: i64,
    pub status: String,
    pub added_at: String,
}

/// Create collection request
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CreateCollectionRequest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Add document to collection request
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AddDocumentRequest {
    pub document_id: String,
}

/// Paginated collection list response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CollectionListResponse {
    pub schema_version: String,
    pub data: Vec<CollectionResponse>,
    pub total: u64,
    pub page: u32,
    pub limit: u32,
    pub pages: u32,
}

// ============================================================================
// Audit types
// ============================================================================

/// Audit log entry response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AuditLogEntry {
    pub id: String,
    pub timestamp: String,
    pub user_id: String,
    pub user_role: String,
    pub tenant_id: String,
    pub action: String,
    pub resource_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_id: Option<String>,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata_json: Option<String>,
}

/// Audit logs response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AuditLogsResponse {
    pub logs: Vec<AuditLogEntry>,
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
}

/// Audit chain entry with hash linkage
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AuditChainEntry {
    pub id: String,
    pub timestamp: String,
    pub action: String,
    pub resource_type: String,
    pub status: String,
    pub entry_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_hash: Option<String>,
    pub chain_sequence: i64,
    pub verified: bool,
}

/// Audit chain response with verification status
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AuditChainResponse {
    pub entries: Vec<AuditChainEntry>,
    pub chain_valid: bool,
    pub total_entries: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merkle_root: Option<String>,
}

/// Chain verification response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChainVerificationResponse {
    pub chain_valid: bool,
    pub total_entries: usize,
    pub verified_entries: usize,
    pub first_invalid_sequence: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merkle_root: Option<String>,
    pub verification_timestamp: String,
}

/// Federation audit response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FederationAuditResponse {
    pub total_hosts: usize,
    pub total_signatures: usize,
    pub verified_signatures: usize,
    pub quarantined: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quarantine_reason: Option<String>,
    pub host_chains: Vec<HostChainSummary>,
    pub timestamp: String,
}

/// Host chain summary
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HostChainSummary {
    pub host_id: String,
    pub bundle_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_bundle: Option<String>,
}

/// Compliance audit response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ComplianceAuditResponse {
    pub compliance_rate: f64,
    pub total_controls: usize,
    pub compliant_controls: usize,
    pub active_violations: usize,
    pub controls: Vec<ComplianceControl>,
    pub timestamp: String,
}

/// Compliance control
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ComplianceControl {
    pub control_id: String,
    pub control_name: String,
    pub status: String,
    pub last_checked: String,
    #[serde(default)]
    pub evidence: Vec<String>,
    #[serde(default)]
    pub findings: Vec<String>,
}

/// Audit query parameters
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct AuditLogsQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<usize>,
}

// ============================================================================
// Trace/Telemetry API methods
// ============================================================================

impl ApiClient {
    /// Search traces with optional filters
    pub async fn search_traces(&self, query: &TraceSearchQuery) -> ApiResult<Vec<String>> {
        let mut params = Vec::new();
        if let Some(ref span_name) = query.span_name {
            params.push(format!(
                "span_name={}",
                js_sys::encode_uri_component(span_name)
            ));
        }
        if let Some(ref status) = query.status {
            params.push(format!("status={}", status));
        }
        if let Some(start) = query.start_time_ns {
            params.push(format!("start_time_ns={}", start));
        }
        if let Some(end) = query.end_time_ns {
            params.push(format!("end_time_ns={}", end));
        }

        let path = if params.is_empty() {
            "/v1/traces/search".to_string()
        } else {
            format!("/v1/traces/search?{}", params.join("&"))
        };

        self.get(&path).await
    }

    /// Get trace details by ID
    pub async fn get_trace(&self, trace_id: &str) -> ApiResult<Option<TraceEvent>> {
        self.get(&format!("/v1/traces/{}", trace_id)).await
    }

    /// List inference traces for a request or session
    pub async fn list_inference_traces(
        &self,
        request_id: Option<&str>,
        limit: Option<usize>,
    ) -> ApiResult<Vec<InferenceTraceResponse>> {
        let mut params = Vec::new();
        if let Some(id) = request_id {
            params.push(format!("request_id={}", encode(id)));
        }
        if let Some(l) = limit {
            params.push(format!("limit={}", l));
        }

        let path = if params.is_empty() {
            "/v1/traces/inference".to_string()
        } else {
            format!("/v1/traces/inference?{}", params.join("&"))
        };

        self.get(&path).await
    }

    /// Get detailed inference trace with token-level breakdown
    pub async fn get_inference_trace_detail(
        &self,
        trace_id: &str,
    ) -> ApiResult<InferenceTraceDetailResponse> {
        self.get(&format!("/v1/traces/inference/{}", trace_id))
            .await
    }
}

// ============================================================================
// Trace/Telemetry types
// ============================================================================

/// Trace search query parameters
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct TraceSearchQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_time_ns: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time_ns: Option<u64>,
}

/// Trace event (from trace buffer)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TraceEvent {
    pub trace_id: String,
    pub span_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_span_id: Option<String>,
    pub timestamp: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    pub operation: String,
    pub status: String,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Inference trace summary response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InferenceTraceResponse {
    pub trace_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    pub created_at: String,
    pub latency_ms: u64,
    pub token_count: u32,
    pub adapters_used: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
}

/// Detailed inference trace with token-level breakdown
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InferenceTraceDetailResponse {
    pub trace_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    pub created_at: String,
    pub latency_ms: u64,
    pub adapters_used: Vec<String>,
    #[serde(default)]
    pub token_decisions: Vec<TokenDecision>,
    pub timing_breakdown: TimingBreakdown,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receipt: Option<TraceReceiptSummary>,
    /// Backend used (e.g., coreml, metal, mlx)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend_id: Option<String>,
}

/// Per-token routing decision
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TokenDecision {
    pub token_index: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_id: Option<u32>,
    pub adapter_ids: Vec<String>,
    pub gates_q15: Vec<i16>,
    pub entropy: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision_hash: Option<String>,
    /// Backend ID for this specific token (if different from trace)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend_id: Option<String>,
    /// Kernel version ID used for this token
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kernel_version_id: Option<String>,
}

/// Timing breakdown for latency analysis
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct TimingBreakdown {
    pub total_ms: u64,
    pub routing_ms: u64,
    pub inference_ms: u64,
    pub policy_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefill_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decode_ms: Option<u64>,
}

/// Receipt summary for trace verification
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TraceReceiptSummary {
    pub receipt_digest: String,
    pub run_head_hash: String,
    pub output_digest: String,
    pub logical_prompt_tokens: u32,
    pub logical_output_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason_token_index: Option<u32>,
    pub verified: bool,
    /// Hardware/Equipment attestation fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub processor_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engine_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ane_version: Option<String>,
    /// Cache metrics
    #[serde(default)]
    pub prefix_cache_hit: bool,
    #[serde(default)]
    pub prefix_kv_bytes: u64,
}

// ============================================================================
// Document types
// ============================================================================

/// Document response from the API
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DocumentResponse {
    #[serde(default)]
    pub schema_version: String,
    pub document_id: String,
    pub name: String,
    pub hash_b3: String,
    pub size_bytes: i64,
    pub mime_type: String,
    pub storage_path: String,
    /// Status: "processing", "indexed", "failed"
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk_count: Option<i32>,
    pub tenant_id: String,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    /// True if this response points to a pre-existing document with identical content
    #[serde(default)]
    pub deduplicated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(default)]
    pub retry_count: i32,
    #[serde(default)]
    pub max_retries: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub processing_started_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub processing_completed_at: Option<String>,
}

/// Document list response with pagination
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DocumentListResponse {
    #[serde(default)]
    pub schema_version: String,
    pub data: Vec<DocumentResponse>,
    pub total: u64,
    pub page: u32,
    pub limit: u32,
    pub pages: u32,
}

/// Document chunk response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChunkResponse {
    #[serde(default)]
    pub schema_version: String,
    pub chunk_id: String,
    pub document_id: String,
    pub chunk_index: i32,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    pub created_at: String,
}

/// Chunk list response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChunkListResponse {
    #[serde(default)]
    pub schema_version: String,
    pub chunks: Vec<ChunkResponse>,
    pub document_id: String,
    pub total_chunks: i32,
}

/// Document list query parameters
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct DocumentListParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

/// Process document response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProcessDocumentResponse {
    #[serde(default)]
    pub schema_version: String,
    pub document_id: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

// ============================================================================
// Search types
// ============================================================================

/// Search result item
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchResultItem {
    /// Result type: "adapter", "page", etc.
    pub result_type: String,
    /// Unique ID
    pub id: String,
    /// Display title
    pub title: String,
    /// Subtitle/description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    /// Link/path to navigate to
    pub path: String,
    /// Relevance score (0.0 - 1.0)
    pub score: f32,
}

/// Search response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchResponse {
    /// Search results
    pub results: Vec<SearchResultItem>,
    /// Total count (may be approximate)
    pub total: u32,
    /// Query execution time in milliseconds
    #[serde(default)]
    pub took_ms: u64,
}

// ============================================================================
// Dataset types
// ============================================================================

/// Dataset response from the API (from /v1/datasets endpoints)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DatasetResponse {
    #[serde(default)]
    pub schema_version: String,
    #[serde(alias = "dataset_id")]
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dataset_version_id: Option<String>,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub format: String,
    #[serde(alias = "hash")]
    #[serde(alias = "hash_b3")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hash_b3: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dataset_hash_b3: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage_path: Option<String>,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation_errors: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation_diagnostics: Option<Vec<JsonlValidationDiagnostic>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trust_state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_size_bytes: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dataset_type: Option<String>,
    #[serde(default)]
    pub created_by: Option<String>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

/// Response for listing datasets
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DatasetListResponse {
    #[serde(default)]
    pub schema_version: String,
    pub datasets: Vec<DatasetResponse>,
    #[serde(default)]
    pub total: i64,
}

/// Dataset statistics response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DatasetStatisticsResponse {
    #[serde(default)]
    pub schema_version: String,
    pub dataset_id: String,
    #[serde(default)]
    pub num_examples: i64,
    #[serde(default)]
    pub avg_input_length: f64,
    #[serde(default)]
    pub avg_target_length: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language_distribution: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_type_distribution: Option<serde_json::Value>,
    #[serde(default)]
    pub total_tokens: i64,
    #[serde(default)]
    pub computed_at: String,
}

fn default_validation_mode() -> String {
    "quick".to_string()
}

/// Request parameters for file validation
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ValidateFileRequest {
    /// Validation mode: "quick" or "deep"
    #[serde(default = "default_validation_mode")]
    pub mode: String,
    /// Whether to check required fields for JSONL training format
    #[serde(default)]
    pub check_training_format: bool,
    /// Custom required fields to validate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required_fields: Option<Vec<String>>,
}

/// Detailed file validation error
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FileValidationError {
    pub severity: String,
    pub category: String,
    pub message: String,
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_number: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column_number: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

/// Response from file validation
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ValidateFileResponse {
    pub schema_version: String,
    pub file_id: String,
    pub file_name: String,
    pub is_valid: bool,
    pub validation_mode: String,
    pub error_count: usize,
    pub warning_count: usize,
    pub info_count: usize,
    pub entries_validated: usize,
    pub duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<Vec<FileValidationError>>,
    pub validated_at: String,
}

/// Response from validating all files in a dataset
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ValidateAllFilesResponse {
    pub schema_version: String,
    pub dataset_id: String,
    pub is_valid: bool,
    pub validation_mode: String,
    pub files_validated: usize,
    pub total_error_count: usize,
    pub total_warning_count: usize,
    pub total_entries_validated: usize,
    pub duration_ms: u64,
    pub file_results: Vec<ValidateFileResponse>,
    pub validated_at: String,
}

// ============================================================================
// Code Policy types
// ============================================================================

/// Code policy settings for code generation safety constraints
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CodePolicy {
    /// Minimum number of evidence spans required
    #[serde(default = "default_min_evidence_spans")]
    pub min_evidence_spans: usize,
    /// Whether auto-apply is allowed
    #[serde(default)]
    pub allow_auto_apply: bool,
    /// Minimum test coverage threshold (0.0 - 1.0)
    #[serde(default = "default_test_coverage_min")]
    pub test_coverage_min: f32,
    /// Allowed file paths (glob patterns)
    #[serde(default)]
    pub path_allowlist: Vec<String>,
    /// Denied file paths (glob patterns)
    #[serde(default)]
    pub path_denylist: Vec<String>,
    /// Secret detection patterns (regex)
    #[serde(default)]
    pub secret_patterns: Vec<String>,
    /// Maximum patch size in bytes
    #[serde(default = "default_max_patch_size")]
    pub max_patch_size: usize,
}

fn default_min_evidence_spans() -> usize {
    1
}
fn default_test_coverage_min() -> f32 {
    0.8
}
fn default_max_patch_size() -> usize {
    100_000
}

impl Default for CodePolicy {
    fn default() -> Self {
        Self {
            min_evidence_spans: 1,
            allow_auto_apply: false,
            test_coverage_min: 0.8,
            path_allowlist: vec![],
            path_denylist: vec![],
            secret_patterns: vec![],
            max_patch_size: 100_000,
        }
    }
}

/// Response containing code policy
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GetCodePolicyResponse {
    pub policy: CodePolicy,
}

/// Request to update code policy
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UpdateCodePolicyRequest {
    pub policy: CodePolicy,
}

// ============================================================================
// Process Monitoring types
// ============================================================================

/// Process log entry
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProcessLogResponse {
    pub id: String,
    pub worker_id: String,
    pub level: String,
    pub message: String,
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata_json: Option<String>,
}

/// Process crash dump
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProcessCrashDumpResponse {
    pub id: String,
    pub worker_id: String,
    pub crash_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_trace: Option<String>,
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub core_dump_path: Option<String>,
}

/// Process health metric
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProcessHealthMetricResponse {
    pub id: String,
    pub worker_id: String,
    pub tenant_id: String,
    pub metric_name: String,
    pub metric_value: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metric_unit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<serde_json::Value>,
    pub collected_at: String,
}

/// Process monitoring rule
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProcessMonitoringRuleResponse {
    pub id: String,
    pub name: String,
    pub rule_type: String,
    pub condition_json: String,
    pub action_json: String,
    pub enabled: bool,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

/// Process alert
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProcessAlertResponse {
    pub id: String,
    pub rule_id: String,
    pub worker_id: String,
    pub severity: String,
    pub message: String,
    pub status: String,
    pub triggered_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub acknowledged_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub acknowledged_by: Option<String>,
}

/// Process anomaly detection result
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProcessAnomalyResponse {
    pub id: String,
    pub worker_id: String,
    pub anomaly_type: String,
    pub severity: String,
    pub description: String,
    pub status: String,
    pub detected_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<String>,
}

/// Request to create a monitoring rule
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CreateMonitoringRuleRequest {
    pub name: String,
    pub rule_type: String,
    pub condition: serde_json::Value,
    pub action: serde_json::Value,
    #[serde(default)]
    pub enabled: bool,
}

// ============================================================================
// Routing Decision Types
// ============================================================================

/// Query parameters for routing decisions
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct RoutingDecisionsQuery {
    pub tenant: Option<String>,
    pub stack_id: Option<String>,
    pub adapter_id: Option<String>,
    pub anomalies_only: Option<bool>,
    pub min_entropy: Option<f64>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// Paginated routing decisions response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RoutingDecisionsResponse {
    pub decisions: Vec<RoutingDecisionResponse>,
    pub total: usize,
    #[serde(default)]
    pub offset: usize,
    #[serde(default)]
    pub limit: usize,
}

/// A single routing decision
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RoutingDecisionResponse {
    pub id: String,
    pub tenant_id: String,
    pub request_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_id: Option<String>,
    pub step: i32,
    pub entropy: f64,
    pub k_value: i32,
    pub tau: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overhead_pct: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_inference_latency_us: Option<i64>,
    pub timestamp: String,
    #[serde(default)]
    pub candidates: Vec<RoutingCandidateResponse>,
    #[serde(default)]
    pub selected_adapter_ids: Vec<String>,
}

/// A routing candidate adapter
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RoutingCandidateResponse {
    pub adapter_id: String,
    pub gate_value: f64,
    pub rank: i32,
    pub selected: bool,
}

/// Request for routing debug
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RoutingDebugRequest {
    pub prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_id: Option<String>,
}

/// Response from routing debug
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RoutingDebugResponse {
    pub detected_features: DetectedFeaturesResponse,
    pub adapter_scores: Vec<AdapterScoreResponse>,
    pub selected_adapters: Vec<String>,
    pub entropy: f64,
    pub k_value: i32,
    pub explanation: String,
}

/// Detected features from routing debug
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DetectedFeaturesResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frameworks: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verb: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
}

/// Adapter score from routing debug
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AdapterScoreResponse {
    pub adapter_id: String,
    pub score: f64,
    pub gate_value: f64,
    pub selected: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Routing decision chain response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RoutingDecisionChainResponse {
    pub inference_id: String,
    pub tenant_id: String,
    pub decisions: Vec<RoutingDecisionResponse>,
    pub chain_verified: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chain_hash: Option<String>,
}

// ============================================================================
// Admin Types
// ============================================================================

// Admin types (UserResponse, ListUsersResponse) are now imported from
// adapteros_api_types::admin

// ============================================================================
// API Key Types
// ============================================================================

// API key types (CreateApiKeyRequest, CreateApiKeyResponse, ApiKeyInfo,
// ApiKeyListResponse, RevokeApiKeyResponse) are now imported from
// adapteros_api_types::api_keys
