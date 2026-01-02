//! HTTP API client
//!
//! Typed HTTP client for the AdapterOS REST API.

use super::{api_base_url, ApiError, ApiResult};
use gloo_net::http::{Request, RequestBuilder};
use serde::{de::DeserializeOwned, Serialize};
use std::sync::{Arc, RwLock};

/// HTTP API client for AdapterOS backend
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

    /// Load auth token from localStorage
    fn load_token() -> Option<String> {
        #[cfg(target_arch = "wasm32")]
        {
            web_sys::window()
                .and_then(|w| w.local_storage().ok().flatten())
                .and_then(|s| s.get_item("auth_token").ok().flatten())
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            None
        }
    }

    /// Set authentication token
    pub fn set_token(&self, token: Option<String>) {
        if let Ok(mut guard) = self.auth_token.write() {
            *guard = token.clone();
        }

        // Persist to localStorage
        #[cfg(target_arch = "wasm32")]
        if let Some(storage) = web_sys::window()
            .and_then(|w| w.local_storage().ok().flatten())
        {
            match &token {
                Some(t) => {
                    let _ = storage.set_item("auth_token", t);
                }
                None => {
                    let _ = storage.remove_item("auth_token");
                }
            }
        }
    }

    /// Check if client has an auth token
    pub fn is_authenticated(&self) -> bool {
        self.auth_token.read().ok().map(|t| t.is_some()).unwrap_or(false)
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

        let req = req.header("Content-Type", "application/json");

        if let Some(token) = self.auth_token.read().ok().and_then(|t| t.clone()) {
            req.header("Authorization", &format!("Bearer {}", token))
        } else {
            req
        }
    }

    /// Perform a GET request
    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> ApiResult<T> {
        let response = self.request("GET", path).send().await?;
        self.handle_response(response).await
    }

    /// Perform a POST request with JSON body
    pub async fn post<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> ApiResult<T> {
        let response = self
            .request("POST", path)
            .json(body)?
            .send()
            .await?;
        self.handle_response(response).await
    }

    /// Perform a POST request without response body
    pub async fn post_no_response<B: Serialize>(&self, path: &str, body: &B) -> ApiResult<()> {
        let response = self
            .request("POST", path)
            .json(body)?
            .send()
            .await?;

        if response.ok() {
            Ok(())
        } else {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            Err(ApiError::from_response(status, &text))
        }
    }

    /// Perform a PUT request with JSON body
    pub async fn put<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> ApiResult<T> {
        let response = self
            .request("PUT", path)
            .json(body)?
            .send()
            .await?;
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

        self.post(
            "/v1/auth/login",
            &LoginRequest { username, password },
        )
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

    /// List training jobs
    pub async fn list_training_jobs(&self) -> ApiResult<adapteros_api_types::TrainingJobListResponse> {
        self.get("/v1/training/jobs").await
    }

    /// Get training job details
    pub async fn get_training_job(&self, id: &str) -> ApiResult<adapteros_api_types::TrainingJobResponse> {
        self.get(&format!("/v1/training/jobs/{}", id)).await
    }

    /// Cancel a training job
    pub async fn cancel_training_job(&self, id: &str) -> ApiResult<()> {
        self.post_no_response(&format!("/v1/training/jobs/{}/cancel", id), &serde_json::json!({})).await
    }

    /// Create a new training job
    pub async fn create_training_job(
        &self,
        request: &CreateTrainingJobRequest,
    ) -> ApiResult<adapteros_api_types::TrainingJobResponse> {
        self.post("/v1/training/jobs", request).await
    }

    /// Get training logs for a job
    pub async fn get_training_logs(&self, job_id: &str) -> ApiResult<Vec<String>> {
        self.get(&format!("/v1/training/jobs/{}/logs", job_id)).await
    }

    // --- Models ---

    /// List all models with stats
    pub async fn list_models(&self) -> ApiResult<ModelListResponse> {
        self.get("/v1/models").await
    }

    /// Get model status by ID
    pub async fn get_model(&self, id: &str) -> ApiResult<ModelStatusResponse> {
        self.get(&format!("/v1/models/{}/status", id)).await
    }

    /// Import a new model
    pub async fn seed_model(&self, request: &SeedModelRequest) -> ApiResult<SeedModelResponse> {
        self.post("/v1/models/import", request).await
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
        self.post(&format!("/v1/adapter-stacks/{}/activate", id), &serde_json::json!({})).await
    }

    /// Deactivate the current adapter stack
    pub async fn deactivate_stack(&self) -> ApiResult<serde_json::Value> {
        self.post("/v1/adapter-stacks/deactivate", &serde_json::json!({})).await
    }

    /// Delete an adapter stack
    pub async fn delete_stack(&self, id: &str) -> ApiResult<()> {
        self.delete(&format!("/v1/adapter-stacks/{}", id)).await
    }

    /// Update an adapter stack
    pub async fn update_stack(&self, id: &str, request: &UpdateStackRequest) -> ApiResult<StackResponse> {
        self.put(&format!("/v1/adapter-stacks/{}", id), request).await
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
    pub async fn dashboard_config(&self) -> ApiResult<adapteros_api_types::GetDashboardConfigResponse> {
        self.get("/v1/dashboard/config").await
    }

    // --- Inference ---

    /// Send an inference request (non-streaming)
    pub async fn infer(&self, request: &InferenceRequest) -> ApiResult<adapteros_api_types::InferResponse> {
        self.post("/v1/infer", request).await
    }

    /// Get inference stream URL for SSE
    pub fn infer_stream_url(&self) -> String {
        format!("{}/v1/infer/stream", self.base_url)
    }

    // --- Collections ---

    /// List collections with pagination
    pub async fn list_collections(&self, page: u32, limit: u32) -> ApiResult<CollectionListResponse> {
        self.get(&format!("/v1/collections?page={}&limit={}", page, limit)).await
    }

    /// Get collection details with documents
    pub async fn get_collection(&self, id: &str) -> ApiResult<CollectionDetailResponse> {
        self.get(&format!("/v1/collections/{}", id)).await
    }

    /// Create a new collection
    pub async fn create_collection(&self, request: &CreateCollectionRequest) -> ApiResult<CollectionResponse> {
        self.post("/v1/collections", request).await
    }

    /// Delete a collection
    pub async fn delete_collection(&self, id: &str) -> ApiResult<()> {
        self.delete(&format!("/v1/collections/{}", id)).await
    }

    /// Add a document to a collection
    pub async fn add_document_to_collection(&self, collection_id: &str, document_id: &str) -> ApiResult<()> {
        let request = AddDocumentRequest {
            document_id: document_id.to_string(),
        };
        self.post_no_response(&format!("/v1/collections/{}/documents", collection_id), &request).await
    }

    /// Remove a document from a collection
    pub async fn remove_document_from_collection(&self, collection_id: &str, document_id: &str) -> ApiResult<()> {
        self.delete(&format!("/v1/collections/{}/documents/{}", collection_id, document_id)).await
    }

    // --- Repositories ---

    /// List all repositories
    pub async fn list_repositories(&self) -> ApiResult<RepositoryListResponse> {
        self.get("/v1/repositories").await
    }

    /// Get repository details by ID
    pub async fn get_repository(&self, id: &str) -> ApiResult<RepositoryDetailResponse> {
        self.get(&format!("/v1/repositories/{}", id)).await
    }

    /// Register a new repository
    pub async fn register_repository(&self, request: &RegisterRepositoryRequest) -> ApiResult<RepositoryResponse> {
        self.post("/v1/repositories", request).await
    }

    /// Delete a repository
    pub async fn delete_repository(&self, id: &str) -> ApiResult<()> {
        self.delete(&format!("/v1/repositories/{}", id)).await
    }

    /// Trigger a sync/scan for a repository
    pub async fn sync_repository(&self, id: &str) -> ApiResult<ScanStatusResponse> {
        self.post(&format!("/v1/repositories/{}/sync", id), &serde_json::json!({})).await
    }

    /// Get sync status for a repository
    pub async fn get_sync_status(&self, id: &str) -> ApiResult<ScanStatusResponse> {
        self.get(&format!("/v1/repositories/{}/sync/status", id)).await
    }

    /// Publish an adapter from a repository
    pub async fn publish_repository_adapter(&self, id: &str, request: &PublishAdapterRequest) -> ApiResult<PublishAdapterResponse> {
        self.post(&format!("/v1/repositories/{}/publish", id), request).await
    }

    // --- Audit ---

    /// Query audit logs with filtering
    pub async fn query_audit_logs(&self, query: &AuditLogsQuery) -> ApiResult<AuditLogsResponse> {
        let mut path = "/v1/audit/logs?".to_string();
        let mut params = Vec::new();

        if let Some(ref user_id) = query.user_id {
            params.push(format!("user_id={}", user_id));
        }
        if let Some(ref action) = query.action {
            params.push(format!("action={}", action));
        }
        if let Some(ref resource_type) = query.resource_type {
            params.push(format!("resource_type={}", resource_type));
        }
        if let Some(ref status) = query.status {
            params.push(format!("status={}", status));
        }
        if let Some(ref from_time) = query.from_time {
            params.push(format!("from_time={}", from_time));
        }
        if let Some(ref to_time) = query.to_time {
            params.push(format!("to_time={}", to_time));
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

    // --- Documents ---

    /// List documents with optional filtering
    pub async fn list_documents(&self, params: Option<&DocumentListParams>) -> ApiResult<DocumentListResponse> {
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
        self.post(&format!("/v1/documents/{}/process", id), &serde_json::json!({})).await
    }

    /// Retry failed document processing
    pub async fn retry_document(&self, id: &str) -> ApiResult<DocumentResponse> {
        self.post(&format!("/v1/documents/{}/retry", id), &serde_json::json!({})).await
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

        let url = format!("{}/v1/documents", self.base_url);

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
            let text = JsFuture::from(resp.text().unwrap())
                .await
                .ok()
                .and_then(|v| v.as_string())
                .unwrap_or_default();
            return Err(ApiError::from_response(status, &text));
        }

        // Parse JSON response
        let json = JsFuture::from(resp.json().unwrap())
            .await
            .map_err(|_| ApiError::Serialization("Failed to parse JSON".into()))?;
        let result: DocumentResponse = serde_wasm_bindgen::from_value(json)
            .map_err(|e| ApiError::Serialization(e.to_string()))?;
        Ok(result)
    }

    // --- Datasets from Documents ---

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
}

/// Simple inference request for chat
#[derive(Debug, Clone, serde::Serialize)]
pub struct InferenceRequest {
    pub prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
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
    pub created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub architecture_summary: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<Vec<String>>,
    #[serde(default)]
    pub adapter_count: i64,
    #[serde(default)]
    pub training_job_count: i64,
}

/// Model list response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ModelListResponse {
    pub models: Vec<ModelWithStatsResponse>,
    pub total: usize,
}

/// Model status response (from /v1/models/{id}/status endpoint)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ModelStatusResponse {
    pub model_id: String,
    pub model_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_path: Option<String>,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loaded_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_usage_mb: Option<i32>,
    pub is_loaded: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ane_memory: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uma_pressure_level: Option<String>,
}

/// Import model request (for POST /v1/models/import)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SeedModelRequest {
    pub model_name: String,
    pub model_path: String,
    /// Format: "mlx", "safetensors", "pytorch", "gguf"
    pub format: String,
    /// Backend: "mlx", "mlx-ffi", "metal"
    pub backend: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Import model response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SeedModelResponse {
    pub import_id: String,
    pub status: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<i32>,
}

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
    pub epochs: u32,
    pub learning_rate: f32,
    pub batch_size: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warmup_steps: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_seq_length: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gradient_accumulation_steps: Option<u32>,
}

/// Worker metrics response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WorkerMetricsResponse {
    pub worker_id: String,
    /// Memory usage in MB
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_used_mb: Option<u64>,
    /// Memory limit in MB
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_limit_mb: Option<u64>,
    /// GPU memory used in MB
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_memory_used_mb: Option<u64>,
    /// GPU memory total in MB
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_memory_total_mb: Option<u64>,
    /// GPU utilization percentage
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_utilization_pct: Option<f64>,
    /// CPU utilization percentage
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_utilization_pct: Option<f64>,
    /// Requests processed
    #[serde(default)]
    pub requests_processed: u64,
    /// Requests per second
    #[serde(default)]
    pub requests_per_second: f64,
    /// Average latency in ms
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avg_latency_ms: Option<f64>,
    /// P99 latency in ms
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p99_latency_ms: Option<f64>,
    /// Uptime in seconds
    #[serde(default)]
    pub uptime_seconds: u64,
    /// Cache entries count
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_entries: Option<u32>,
    /// Cache hit rate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_hit_rate: Option<f64>,
}

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
// Repository types
// ============================================================================

/// Repository list response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RepositoryListResponse {
    pub repositories: Vec<RepositoryResponse>,
    pub total: usize,
}

/// Repository response (mirrors adapteros_api_types::RepositoryResponse)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RepositoryResponse {
    #[serde(default)]
    pub schema_version: String,
    pub id: String,
    pub repo_id: String,
    pub path: String,
    pub languages: Vec<String>,
    pub default_branch: String,
    pub status: String,
    #[serde(default)]
    pub frameworks: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol_count: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
}

/// Register repository request
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RegisterRepositoryRequest {
    pub repo_id: String,
    pub path: String,
    pub languages: Vec<String>,
    pub default_branch: String,
}

/// Trigger scan request
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TriggerScanRequest {
    pub repo_id: String,
}

/// Scan status response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ScanStatusResponse {
    #[serde(default)]
    pub schema_version: String,
    pub repo_id: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Publish adapter request
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PublishAdapterRequest {
    pub repo_id: String,
    pub adapter_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// Publish adapter response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PublishAdapterResponse {
    pub adapter_id: String,
    pub status: String,
    pub message: String,
}

/// Repository adapter (adapter associated with a repository)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RepositoryAdapter {
    pub adapter_id: String,
    pub name: String,
    pub version: String,
    pub status: String,
    pub created_at: String,
}

/// Repository detail response with adapters
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RepositoryDetailResponse {
    #[serde(flatten)]
    pub repository: RepositoryResponse,
    #[serde(default)]
    pub adapters: Vec<RepositoryAdapter>,
    #[serde(default)]
    pub versions: Vec<RepositoryVersion>,
}

/// Repository version (git tag/branch snapshot)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RepositoryVersion {
    pub version: String,
    pub commit_hash: String,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapter_id: Option<String>,
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
            params.push(format!("span_name={}", js_sys::encode_uri_component(span_name)));
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
            params.push(format!("request_id={}", js_sys::encode_uri_component(id)));
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
        self.get(&format!("/v1/traces/inference/{}", trace_id)).await
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
    pub verified: bool,
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
// Dataset types
// ============================================================================

/// Dataset response from the API (from /v1/datasets endpoints)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DatasetResponse {
    #[serde(default)]
    pub schema_version: String,
    #[serde(alias = "dataset_id")]
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub format: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hash_b3: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage_path: Option<String>,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_size_bytes: Option<i64>,
    pub tenant_id: String,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}
