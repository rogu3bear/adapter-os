//! HTTP API client
//!
//! Typed HTTP client for the adapterOS REST API.

pub use super::types::*;
use super::{api_base_url, ApiError, ApiResult};
use gloo_net::http::{Request, RequestBuilder};
use serde::{de::DeserializeOwned, Serialize};
use std::sync::{Arc, RwLock};
use urlencoding::encode;
use web_sys::RequestCredentials;

pub use adapteros_api_types::activity::ActivityEventResponse;
pub use adapteros_api_types::code_repositories::{
    RegisterRepositoryRequest, RegisterRepositoryResponse, RepositoryDetailResponse,
    RepositoryInfo, RepositoryListResponse, ScanJobResponse, ScanRepositoryRequest,
};
pub use adapteros_api_types::dataset_domain::CanonicalRow;
pub use adapteros_api_types::training::{
    DatasetFileResponse, DatasetVersionsResponse, JsonlValidationDiagnostic,
};
pub use adapteros_api_types::{DatasetManifest, UploadDatasetResponse};
// Consolidated types from shared crate
pub use adapteros_api_types::admin::{ListUsersResponse, UserResponse};
pub use adapteros_api_types::api_keys::{
    ApiKeyInfo, ApiKeyListResponse, CreateApiKeyRequest, CreateApiKeyResponse, RevokeApiKeyResponse,
};
pub use adapteros_api_types::auth::{TenantListResponse, TenantSummary};
pub use adapteros_api_types::embeddings::{
    EmbeddingBenchmarkReport, EmbeddingBenchmarksQuery, EmbeddingBenchmarksResponse,
};
pub use adapteros_api_types::model_status::ModelLoadStatus;
pub use adapteros_api_types::models::{
    AllModelsStatusResponse, AneMemoryStatus, BaseModelStatusResponse, ModelStatusResponse,
    SeedModelRequest, SeedModelResponse,
};
pub use adapteros_api_types::routing::{
    CreateRoutingRuleRequest, RoutingRuleResponse, RoutingRulesResponse,
};
pub use adapteros_api_types::workers::WorkerMetricsResponse;

/// Response for in-flight adapters endpoint
#[derive(Debug, Clone, serde::Deserialize)]
pub struct InFlightAdaptersResponse {
    /// Adapter IDs currently in use for inference
    pub adapter_ids: Vec<String>,
    /// Total count of in-flight inferences
    pub inference_count: usize,
}

/// Extract CSRF token from browser cookies.
///
/// The server sets a `csrf_token` cookie that must be sent back as an
/// `X-CSRF-Token` header on all mutating requests (POST, PUT, PATCH, DELETE).
#[cfg(target_arch = "wasm32")]
pub fn csrf_token_from_cookie() -> Option<String> {
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

/// Extract CSRF token from browser cookies (no-op for non-WASM targets).
#[cfg(not(target_arch = "wasm32"))]
pub fn csrf_token_from_cookie() -> Option<String> {
    None
}

/// HTTP API client for adapterOS backend
#[derive(Clone)]
pub struct ApiClient {
    base_url: String,
    auth_token: Arc<RwLock<Option<String>>>,
    auth_via_cookie: Arc<RwLock<bool>>,
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
            auth_via_cookie: Arc::new(RwLock::new(false)),
        }
    }

    /// Create client with custom base URL
    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            auth_token: Arc::new(RwLock::new(Self::load_token())),
            auth_via_cookie: Arc::new(RwLock::new(false)),
        }
    }

    /// Initialize in-memory auth state.
    ///
    /// Note: Auth tokens are managed via httpOnly cookies, not localStorage.
    /// This method returns None as the initial state.
    fn load_token() -> Option<String> {
        None
    }

    /// Set in-memory authentication token state.
    ///
    /// Note: This only updates in-memory state for tracking auth status.
    /// Actual authentication is handled via httpOnly cookies set by the server.
    /// No localStorage persistence is performed.
    pub fn set_token(&self, token: Option<String>) {
        let has_token = token.is_some();
        if let Ok(mut guard) = self.auth_token.write() {
            *guard = token;
        }
        if has_token {
            if let Ok(mut guard) = self.auth_via_cookie.write() {
                *guard = false;
            }
        }
    }

    /// Check if client is authenticated
    pub fn is_authenticated(&self) -> bool {
        if self
            .auth_via_cookie
            .read()
            .ok()
            .map(|g| *g)
            .unwrap_or(false)
        {
            return true;
        }
        self.auth_token
            .read()
            .ok()
            .map(|t| t.is_some())
            .unwrap_or(false)
    }

    /// Mark client as authenticated (for httpOnly cookie auth)
    ///
    /// With httpOnly cookies, the browser handles auth automatically.
    /// This sets in-memory state to track authenticated status.
    pub fn set_auth_status(&self, authenticated: bool) {
        if let Ok(mut guard) = self.auth_via_cookie.write() {
            *guard = authenticated;
        }
        if let Ok(mut guard) = self.auth_token.write() {
            *guard = None;
        }
    }

    /// Clear authentication status
    ///
    /// Clears local auth state. Server-side logout should also be called
    /// to clear httpOnly cookies.
    pub fn clear_auth_status(&self) {
        if let Ok(mut guard) = self.auth_via_cookie.write() {
            *guard = false;
        }
        self.set_token(None);
    }

    fn bearer_token(&self) -> Option<String> {
        if self
            .auth_via_cookie
            .read()
            .ok()
            .map(|g| *g)
            .unwrap_or(false)
        {
            return None;
        }
        self.auth_token.read().ok().and_then(|t| t.clone())
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

        // Only add Authorization header for bearer tokens (not cookie auth).
        if let Some(token) = self.bearer_token() {
            return req.header("Authorization", &format!("Bearer {}", token));
        }
        req
    }

    /// Perform a GET request
    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> ApiResult<T> {
        let response = self.request("GET", path).send().await?;
        self.handle_response(response).await
    }

    /// Perform a GET request and return status + JSON body (even on non-2xx)
    pub async fn get_with_status<T: DeserializeOwned>(&self, path: &str) -> ApiResult<(u16, T)> {
        let response = self.request("GET", path).send().await?;
        let status = response.status();
        let json = response
            .json()
            .await
            .map_err(|e| ApiError::Serialization(e.to_string()))?;
        Ok((status, json))
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

    /// List tenants accessible to the current user
    pub async fn list_user_tenants(&self) -> ApiResult<TenantListResponse> {
        self.get("/v1/auth/tenants").await
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

    /// Get adapter IDs currently in use for inference
    pub async fn get_in_flight_adapters(&self) -> ApiResult<InFlightAdaptersResponse> {
        self.get("/v1/adapters/in-flight").await
    }

    /// Transition adapter lifecycle state
    ///
    /// Changes an adapter's lifecycle state (e.g., draft -> active, active -> deprecated).
    /// Requires a reason for audit trail purposes.
    pub async fn transition_adapter_lifecycle(
        &self,
        adapter_id: &str,
        new_state: &str,
        reason: &str,
    ) -> ApiResult<adapteros_api_types::AdapterResponse> {
        #[derive(serde::Serialize)]
        struct TransitionRequest<'a> {
            new_state: &'a str,
            reason: &'a str,
        }
        self.post(
            &format!("/v1/adapters/{}/lifecycle", adapter_id),
            &TransitionRequest { new_state, reason },
        )
        .await
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

    /// Get preprocessed cache count
    pub async fn get_preprocessed_cache_count(
        &self,
    ) -> ApiResult<adapteros_api_types::training::PreprocessedCacheCountResponse> {
        self.get("/v1/training/preprocessed-cache/count").await
    }

    /// List preprocessed cache entries
    pub async fn list_preprocessed_cache(
        &self,
    ) -> ApiResult<adapteros_api_types::training::PreprocessedCacheListResponse> {
        self.get("/v1/training/preprocessed-cache").await
    }

    // --- Models ---

    /// List all models with stats
    pub async fn list_models(&self) -> ApiResult<ModelListResponse> {
        self.get("/internal/models").await
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
    pub async fn validate_policy(&self, content: &str) -> ApiResult<PolicyValidationResponse> {
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
        self.get(&format!("/v1/code/repositories/{}", repo_id))
            .await
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
        let request = adapteros_api_types::diagnostics::DiagExportRequest {
            trace_id: run_id.to_string(),
            format: "json".to_string(),
            include_events: true,
            include_timing: true,
            include_metadata: true,
            max_events: None,
        };
        self.post("/v1/diag/export", &request).await
    }

    /// Create a bundle export for a diagnostic run
    pub async fn create_bundle_export(
        &self,
        trace_id: &str,
    ) -> ApiResult<adapteros_api_types::diagnostics::DiagBundleExportResponse> {
        let request = adapteros_api_types::diagnostics::DiagBundleExportRequest {
            trace_id: trace_id.to_string(),
            format: "tar.zst".to_string(),
            include_evidence: false,
            evidence_auth_token: None,
        };
        self.post("/v1/diag/bundle", &request).await
    }

    /// Get an existing bundle export by ID
    pub async fn get_bundle_export(
        &self,
        export_id: &str,
    ) -> ApiResult<adapteros_api_types::diagnostics::DiagBundleExportResponse> {
        self.get(&format!("/v1/diag/bundle/{}", export_id)).await
    }

    /// Get the signature download URL for a bundle export
    pub fn signature_download_url(&self, export_id: &str) -> String {
        format!(
            "{}/v1/diag/bundle/{}/signature",
            super::api_base_url(),
            export_id
        )
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
    ///
    /// Security: Includes CSRF token header and credentials for cookie-based auth.
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
        // Include credentials (cookies) for httpOnly cookie auth
        opts.set_credentials(web_sys::RequestCredentials::Include);

        // Add headers: auth token (if bearer) and CSRF token (required for mutations)
        let headers = web_sys::Headers::new()
            .map_err(|_| ApiError::Network("Failed to create Headers".into()))?;
        if let Some(token) = self.bearer_token() {
            headers
                .set("Authorization", &format!("Bearer {}", token))
                .map_err(|_| ApiError::Network("Failed to set Authorization header".into()))?;
        }
        // CSRF token is required for all mutating requests
        if let Some(csrf_token) = csrf_token_from_cookie() {
            headers
                .set("X-CSRF-Token", &csrf_token)
                .map_err(|_| ApiError::Network("Failed to set X-CSRF-Token header".into()))?;
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

    /// Get a preview of dataset contents (first N examples, capped server-side).
    pub async fn preview_dataset(
        &self,
        dataset_id: &str,
        limit: Option<usize>,
    ) -> ApiResult<crate::api::types::DatasetPreviewResponse> {
        let path = match limit {
            Some(n) => format!("/v1/datasets/{}/preview?limit={}", dataset_id, n),
            None => format!("/v1/datasets/{}/preview", dataset_id),
        };
        self.get(&path).await
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

    /// Create a training dataset from raw text content
    pub async fn create_dataset_from_text(
        &self,
        content: String,
        name: Option<String>,
        format: Option<String>,
    ) -> ApiResult<adapteros_api_types::training::CreateDatasetFromTextResponse> {
        #[derive(serde::Serialize)]
        struct Request {
            content: String,
            #[serde(skip_serializing_if = "Option::is_none")]
            name: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            format: Option<String>,
        }
        self.post(
            "/v1/datasets/from-text",
            &Request {
                content,
                name,
                format,
            },
        )
        .await
    }

    /// Create a training dataset from chat messages
    pub async fn create_dataset_from_chat(
        &self,
        messages: Vec<adapteros_api_types::training::ChatMessageInput>,
        name: Option<String>,
        session_id: Option<String>,
    ) -> ApiResult<adapteros_api_types::training::CreateDatasetFromChatResponse> {
        #[derive(serde::Serialize)]
        struct Request {
            messages: Vec<adapteros_api_types::training::ChatMessageInput>,
            #[serde(skip_serializing_if = "Option::is_none")]
            name: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            session_id: Option<String>,
        }
        self.post(
            "/v1/datasets/from-chat",
            &Request {
                messages,
                name,
                session_id,
            },
        )
        .await
    }

    /// Check if a dataset is safe for training
    pub async fn check_dataset_safety(
        &self,
        dataset_id: &str,
    ) -> ApiResult<crate::api::types::DatasetSafetyCheckResult> {
        self.get(&format!("/v1/datasets/{}/safety-check", dataset_id))
            .await
    }

    /// Start preprocessing on a dataset (PII scrub, deduplication)
    pub async fn start_dataset_preprocessing(
        &self,
        dataset_id: &str,
        pii_scrub: bool,
        dedupe: bool,
    ) -> ApiResult<crate::api::types::StartDatasetPreprocessResponse> {
        self.post(
            &format!("/v1/datasets/{}/preprocess", dataset_id),
            &crate::api::types::StartDatasetPreprocessRequest { pii_scrub, dedupe },
        )
        .await
    }

    /// Get preprocessing job status for a dataset
    pub async fn get_dataset_preprocess_status(
        &self,
        dataset_id: &str,
    ) -> ApiResult<crate::api::types::DatasetPreprocessStatusResponse> {
        self.get(&format!("/v1/datasets/{}/preprocess/status", dataset_id))
            .await
    }

    /// Get a receipt by its digest
    pub async fn get_receipt_by_digest(&self, digest: &str) -> ApiResult<serde_json::Value> {
        self.get(&format!("/v1/adapteros/receipts/{}", digest))
            .await
    }

    /// Upload a training dataset via multipart form data.
    ///
    /// Accepts multiple files and forwards them to `/v1/datasets`.
    ///
    /// Security: Includes CSRF token header and credentials for cookie-based auth.
    #[cfg(target_arch = "wasm32")]
    pub async fn upload_dataset(
        &self,
        form_data: &web_sys::FormData,
        idempotency_key: Option<&str>,
    ) -> ApiResult<adapteros_api_types::UploadDatasetResponse> {
        use wasm_bindgen::JsCast;
        use wasm_bindgen_futures::JsFuture;

        let url = format!("{}/v1/datasets", self.base_url);

        let opts = web_sys::RequestInit::new();
        opts.set_method("POST");
        opts.set_body(form_data);
        // Include credentials (cookies) for httpOnly cookie auth
        opts.set_credentials(web_sys::RequestCredentials::Include);

        let headers = web_sys::Headers::new()
            .map_err(|_| ApiError::Network("Failed to create Headers".into()))?;
        if let Some(token) = self.bearer_token() {
            headers
                .set("Authorization", &format!("Bearer {}", token))
                .map_err(|_| ApiError::Network("Failed to set Authorization header".into()))?;
        }
        // CSRF token is required for all mutating requests
        if let Some(csrf_token) = csrf_token_from_cookie() {
            headers
                .set("X-CSRF-Token", &csrf_token)
                .map_err(|_| ApiError::Network("Failed to set X-CSRF-Token header".into()))?;
        }
        if let Some(key) = idempotency_key.and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        }) {
            headers
                .set("Idempotency-Key", key)
                .map_err(|_| ApiError::Network("Failed to set Idempotency-Key header".into()))?;
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

    /// Create a training dataset from a single uploaded document (multipart).
    ///
    /// POSTs to `/v1/training/datasets/from-upload`.
    ///
    /// Fields:
    /// - `file`: required
    /// - `name`: optional
    /// - `description`: optional
    /// - `training_strategy`: optional (e.g. "synthesis")
    ///
    /// Security: Includes CSRF token header and credentials for cookie-based auth.
    #[cfg(target_arch = "wasm32")]
    pub async fn create_training_dataset_from_upload(
        &self,
        file: &web_sys::File,
        name: Option<&str>,
        description: Option<&str>,
        training_strategy: Option<&str>,
    ) -> ApiResult<DatasetResponse> {
        use wasm_bindgen::JsCast;
        use wasm_bindgen_futures::JsFuture;

        let url = format!("{}/v1/training/datasets/from-upload", self.base_url);

        let form_data = web_sys::FormData::new()
            .map_err(|_| ApiError::Network("Failed to create FormData".into()))?;

        // Prefer passing the actual file name so the backend can infer format.
        form_data
            .append_with_blob_and_filename("file", file, &file.name())
            .map_err(|_| ApiError::Network("Failed to append file to FormData".into()))?;

        if let Some(value) = name.and_then(|v| {
            let trimmed = v.trim();
            if trimmed.is_empty() { None } else { Some(trimmed) }
        }) {
            form_data
                .append_with_str("name", value)
                .map_err(|_| ApiError::Network("Failed to append name to FormData".into()))?;
        }
        if let Some(value) = description.and_then(|v| {
            let trimmed = v.trim();
            if trimmed.is_empty() { None } else { Some(trimmed) }
        }) {
            form_data
                .append_with_str("description", value)
                .map_err(|_| ApiError::Network("Failed to append description to FormData".into()))?;
        }
        if let Some(value) = training_strategy.and_then(|v| {
            let trimmed = v.trim();
            if trimmed.is_empty() { None } else { Some(trimmed) }
        }) {
            form_data
                .append_with_str("training_strategy", value)
                .map_err(|_| {
                    ApiError::Network("Failed to append training_strategy to FormData".into())
                })?;
        }

        let opts = web_sys::RequestInit::new();
        opts.set_method("POST");
        opts.set_body(&form_data);
        // Include credentials (cookies) for httpOnly cookie auth
        opts.set_credentials(web_sys::RequestCredentials::Include);

        let headers = web_sys::Headers::new()
            .map_err(|_| ApiError::Network("Failed to create Headers".into()))?;
        if let Some(token) = self.bearer_token() {
            headers
                .set("Authorization", &format!("Bearer {}", token))
                .map_err(|_| ApiError::Network("Failed to set Authorization header".into()))?;
        }
        if let Some(csrf_token) = csrf_token_from_cookie() {
            headers
                .set("X-CSRF-Token", &csrf_token)
                .map_err(|_| ApiError::Network("Failed to set X-CSRF-Token header".into()))?;
        }
        opts.set_headers(&headers);

        let window = web_sys::window().ok_or_else(|| ApiError::Network("No window".into()))?;
        let request = web_sys::Request::new_with_str_and_init(&url, &opts)
            .map_err(|_| ApiError::Network("Failed to create Request".into()))?;

        let resp_value = JsFuture::from(window.fetch_with_request(&request))
            .await
            .map_err(|_| ApiError::Network("Training dataset upload request failed".into()))?;
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

        let result: DatasetResponse = serde_wasm_bindgen::from_value(json)
            .map_err(|e| ApiError::Serialization(e.to_string()))?;

        Ok(result)
    }

    /// Create a training dataset from a single uploaded document (multipart, async).
    ///
    /// POSTs to `/v1/training/datasets/from-upload/async` and returns a job id for polling
    /// via `/v1/jobs/{job_id}`.
    #[cfg(target_arch = "wasm32")]
    pub async fn create_training_dataset_from_upload_async(
        &self,
        file: &web_sys::File,
        name: Option<&str>,
        description: Option<&str>,
        training_strategy: Option<&str>,
    ) -> ApiResult<crate::api::JobResponse> {
        use wasm_bindgen::JsCast;
        use wasm_bindgen_futures::JsFuture;

        let url = format!("{}/v1/training/datasets/from-upload/async", self.base_url);

        let form_data = web_sys::FormData::new()
            .map_err(|_| ApiError::Network("Failed to create FormData".into()))?;

        form_data
            .append_with_blob_and_filename("file", file, &file.name())
            .map_err(|_| ApiError::Network("Failed to append file to FormData".into()))?;

        if let Some(value) = name.and_then(|v| {
            let trimmed = v.trim();
            if trimmed.is_empty() { None } else { Some(trimmed) }
        }) {
            form_data
                .append_with_str("name", value)
                .map_err(|_| ApiError::Network("Failed to append name to FormData".into()))?;
        }
        if let Some(value) = description.and_then(|v| {
            let trimmed = v.trim();
            if trimmed.is_empty() { None } else { Some(trimmed) }
        }) {
            form_data
                .append_with_str("description", value)
                .map_err(|_| ApiError::Network("Failed to append description to FormData".into()))?;
        }
        if let Some(value) = training_strategy.and_then(|v| {
            let trimmed = v.trim();
            if trimmed.is_empty() { None } else { Some(trimmed) }
        }) {
            form_data
                .append_with_str("training_strategy", value)
                .map_err(|_| {
                    ApiError::Network("Failed to append training_strategy to FormData".into())
                })?;
        }

        let opts = web_sys::RequestInit::new();
        opts.set_method("POST");
        opts.set_body(&form_data);
        opts.set_credentials(web_sys::RequestCredentials::Include);

        let headers = web_sys::Headers::new()
            .map_err(|_| ApiError::Network("Failed to create Headers".into()))?;
        if let Some(token) = self.bearer_token() {
            headers
                .set("Authorization", &format!("Bearer {}", token))
                .map_err(|_| ApiError::Network("Failed to set Authorization header".into()))?;
        }
        if let Some(csrf_token) = csrf_token_from_cookie() {
            headers
                .set("X-CSRF-Token", &csrf_token)
                .map_err(|_| ApiError::Network("Failed to set X-CSRF-Token header".into()))?;
        }
        opts.set_headers(&headers);

        let window = web_sys::window().ok_or_else(|| ApiError::Network("No window".into()))?;
        let request = web_sys::Request::new_with_str_and_init(&url, &opts)
            .map_err(|_| ApiError::Network("Failed to create Request".into()))?;

        let resp_value = JsFuture::from(window.fetch_with_request(&request))
            .await
            .map_err(|_| ApiError::Network("Training dataset async upload request failed".into()))?;
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

        let result: crate::api::JobResponse = serde_wasm_bindgen::from_value(json)
            .map_err(|e| ApiError::Serialization(e.to_string()))?;
        Ok(result)
    }

    /// Get a job detail record from `/v1/jobs/{job_id}`.
    pub async fn get_job(&self, job_id: &str) -> ApiResult<crate::api::JobDetailResponse> {
        self.get(&format!("/v1/jobs/{}", job_id)).await
    }

    /// Generate a training dataset from a file using local inference.
    ///
    /// Accepts multipart form data with:
    /// - `file`: The text file to generate from
    /// - `name`: Dataset name (optional)
    /// - `strategy`: "qa" or "summary" (default: qa)
    /// - `chunk_size`: Chunk size in characters (default: 2000)
    /// - `max_tokens`: Max tokens per inference (default: 512)
    ///
    /// Security: Includes CSRF token header and credentials for cookie-based auth.
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
        // Include credentials (cookies) for httpOnly cookie auth
        opts.set_credentials(web_sys::RequestCredentials::Include);

        let headers = web_sys::Headers::new()
            .map_err(|_| ApiError::Network("Failed to create Headers".into()))?;
        if let Some(token) = self.bearer_token() {
            headers
                .set("Authorization", &format!("Bearer {}", token))
                .map_err(|_| ApiError::Network("Failed to set Authorization header".into()))?;
        }
        // CSRF token is required for all mutating requests
        if let Some(csrf_token) = csrf_token_from_cookie() {
            headers
                .set("X-CSRF-Token", &csrf_token)
                .map_err(|_| ApiError::Network("Failed to set X-CSRF-Token header".into()))?;
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
            Some(id) => format!("/v1/monitoring/health-metrics?worker_id={}", id),
            None => "/v1/monitoring/health-metrics".to_string(),
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
        self.delete(&format!("/v1/error-alerts/rules/{}", id)).await
    }

    /// List error alert history
    pub async fn list_error_alert_history(
        &self,
        unresolved_only: Option<bool>,
        limit: Option<i64>,
    ) -> ApiResult<ErrorAlertHistoryListResponse> {
        let mut params = Vec::new();
        if let Some(flag) = unresolved_only {
            params.push(format!("unresolved_only={}", flag));
        }
        if let Some(limit) = limit {
            params.push(format!("limit={}", limit));
        }
        let path = if params.is_empty() {
            "/v1/error-alerts/history".to_string()
        } else {
            format!("/v1/error-alerts/history?{}", params.join("&"))
        };
        self.get(&path).await
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

    // ========================================================================
    // Discrepancies
    // ========================================================================

    /// Create a new discrepancy case
    pub async fn create_discrepancy(
        &self,
        request: &CreateDiscrepancyRequest,
    ) -> ApiResult<DiscrepancyResponse> {
        self.post("/v1/discrepancies", request).await
    }

    /// List discrepancy cases with optional status filter
    pub async fn list_discrepancy_cases(
        &self,
        status: Option<&str>,
    ) -> ApiResult<Vec<DiscrepancyResponse>> {
        let path = match status {
            Some(s) => format!("/v1/discrepancies?status={}", encode(s)),
            None => "/v1/discrepancies".to_string(),
        };
        self.get(&path).await
    }

    /// Get a single discrepancy case by ID
    pub async fn get_discrepancy(&self, id: &str) -> ApiResult<DiscrepancyResponse> {
        self.get(&format!("/v1/discrepancies/{}", id)).await
    }

    /// Resolve a discrepancy case
    pub async fn resolve_discrepancy(
        &self,
        id: &str,
        resolution: &str,
        notes: Option<&str>,
    ) -> ApiResult<DiscrepancyResponse> {
        #[derive(serde::Serialize)]
        struct ResolveRequest<'a> {
            resolution_status: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            notes: Option<&'a str>,
        }
        self.post(
            &format!("/v1/discrepancies/{}/resolve", id),
            &ResolveRequest {
                resolution_status: resolution,
                notes,
            },
        )
        .await
    }

    // ========================================================================
    // Verdicts
    // ========================================================================

    /// Get verdict for an inference by inference ID
    pub async fn get_inference_verdict(&self, inference_id: &str) -> ApiResult<VerdictResponse> {
        self.get(&format!("/v1/verdicts/{}", inference_id)).await
    }

    /// Derive a rule-based verdict for an inference
    pub async fn derive_rule_verdict(
        &self,
        request: &DeriveVerdictRequest,
    ) -> ApiResult<DeriveVerdictResponse> {
        self.post("/v1/verdicts/derive", request).await
    }

    // ========================================================================
    // Replay Sessions
    // ========================================================================

    /// Create a new replay session
    pub async fn create_replay_session(
        &self,
        request: &CreateReplaySessionRequest,
    ) -> ApiResult<ReplaySessionResponse> {
        self.post("/v1/replay/sessions", request).await
    }

    /// Get a replay session by ID
    pub async fn get_replay_session(&self, session_id: &str) -> ApiResult<ReplaySessionResponse> {
        self.get(&format!("/v1/replay/sessions/{}", session_id))
            .await
    }

    /// Verify a replay session's cryptographic integrity
    pub async fn verify_replay_session(
        &self,
        session_id: &str,
    ) -> ApiResult<ReplayVerificationResponse> {
        self.post_empty(&format!("/v1/replay/sessions/{}/verify", session_id))
            .await
    }
}

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

    /// Get public UI configuration
    pub async fn get_ui_config(&self) -> ApiResult<adapteros_api_types::UiConfigResponse> {
        self.get("/v1/ui/config").await
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
        token_limit: Option<u32>,
        token_after: Option<u32>,
    ) -> ApiResult<UiInferenceTraceDetailResponse> {
        let mut params = Vec::new();
        if let Some(limit) = token_limit {
            params.push(format!("tokens_limit={}", limit));
        }
        if let Some(after) = token_after {
            params.push(format!("tokens_after={}", after));
        }

        let path = if params.is_empty() {
            format!("/v1/ui/traces/inference/{}", trace_id)
        } else {
            format!("/v1/ui/traces/inference/{}?{}", trace_id, params.join("&"))
        };

        self.get(&path).await
    }

    /// Get receipt JSON by digest as raw text (for download)
    ///
    /// Fetches the stored receipt from `/v1/adapteros/receipts/{digest}` and returns
    /// the raw JSON text for direct file download.
    pub async fn get_receipt_json(&self, digest: &str) -> ApiResult<String> {
        self.get_text(&format!("/v1/adapteros/receipts/{}", digest))
            .await
    }
}
