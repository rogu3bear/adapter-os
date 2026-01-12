use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde_json::Value;
use tracing::{debug, info};

use super::types::{LogEntry, SystemMetrics};

// Re-export response types for use by app.rs
pub use self::response_types::*;

mod response_types {
    use serde::Deserialize;

    #[derive(Debug, Clone, Deserialize, Default)]
    #[allow(dead_code)]
    pub struct AdapterDetailResponse {
        pub id: String,
        pub name: String,
        pub version: String,
        #[serde(default)]
        pub tier: String,
        #[serde(default)]
        pub rank: u32,
        #[serde(default)]
        pub loaded: bool,
        #[serde(default)]
        pub pinned: bool,
        pub memory_mb: Option<u32>,
        #[serde(default)]
        pub activation_count: u64,
        pub last_activated: Option<String>,
    }

    #[derive(Debug, Clone, Deserialize, Default)]
    pub struct TrainingJobResponse {
        pub id: String,
        #[serde(default)]
        pub status: String,
        #[serde(default)]
        pub progress_pct: f32,
        #[serde(default)]
        pub current_epoch: u32,
        #[serde(default)]
        pub current_loss: f32,
        #[serde(default)]
        pub tokens_per_second: f32,
        #[allow(dead_code)]
        pub estimated_time_remaining: Option<String>,
        pub dataset_name: Option<String>,
        pub backend: Option<String>,
    }

    #[derive(Debug, Clone, Deserialize, Default)]
    #[allow(dead_code)]
    pub struct InferResponse {
        pub text: String,
        #[serde(default)]
        pub tokens_generated: u32,
        #[serde(default)]
        pub latency_ms: u64,
    }

    #[derive(Debug, Clone, Deserialize, Default)]
    #[allow(dead_code)]
    pub struct MemoryBreakdown {
        #[serde(default)]
        pub total_mb: u64,
        #[serde(default)]
        pub used_mb: u64,
        #[serde(default)]
        pub model_mb: u64,
        #[serde(default)]
        pub adapters_mb: u64,
        #[serde(default)]
        pub cache_mb: u64,
        #[serde(default)]
        pub headroom_percent: f32,
    }

    #[derive(Debug, Clone, Deserialize, Default)]
    #[allow(dead_code)]
    pub struct SystemStatusResponse {
        #[serde(default)]
        pub inference_ready: String,
        #[serde(default)]
        pub inference_blockers: Vec<String>,
    }
}

pub struct ApiClient {
    client: Client,
    base_url: String,
}

#[derive(Debug, Clone, Default)]
pub struct LogQuery {
    pub tenant_id: Option<String>,
    pub trace_id: Option<String>,
}

impl ApiClient {
    pub fn new(base_url: String) -> Result<Self> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()?;

        Ok(Self { client, base_url })
    }

    /// Get system metrics from the server
    pub async fn get_metrics(&self) -> Result<SystemMetrics> {
        let url = format!("{}/api/metrics", self.base_url);

        match self.client.get(&url).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    let data: Value = response.json().await?;
                    debug!("Received metrics: {:?}", data);
                    Ok(self.parse_metrics(data))
                } else {
                    debug!("Metrics API returned status: {}", response.status());
                    Ok(SystemMetrics::default())
                }
            }
            Err(e) => {
                debug!("Failed to fetch metrics (server may not be running): {}", e);
                // Return default metrics if API is not available
                Ok(SystemMetrics::default())
            }
        }
    }

    /// Get service status from the server
    pub async fn get_service_status(&self) -> Result<Vec<ServiceStatusResponse>> {
        let url = format!("{}/api/services/status", self.base_url);

        match self.client.get(&url).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    let services: Vec<ServiceStatusResponse> = response.json().await?;
                    debug!("Received {} service statuses", services.len());
                    Ok(services)
                } else {
                    debug!("Service status API returned: {}", response.status());
                    Ok(vec![])
                }
            }
            Err(e) => {
                debug!("Failed to fetch service status: {}", e);
                Ok(vec![])
            }
        }
    }

    /// Start all services
    pub async fn start_all_services(&self) -> Result<()> {
        let url = format!("{}/api/services/start-all", self.base_url);
        info!("Starting all services via API");

        let response = self
            .client
            .post(&url)
            .send()
            .await
            .map_err(|e| anyhow!("Failed to call start services API: {}", e))?;

        if response.status().is_success() {
            info!("Successfully started all services");
            Ok(())
        } else {
            Err(anyhow!(
                "Failed to start services: HTTP {}",
                response.status()
            ))
        }
    }

    /// Start a specific service
    pub async fn start_service(&self, name: &str) -> Result<()> {
        let url = format!("{}/api/services/{}/start", self.base_url, name);
        info!("Starting service: {}", name);

        let response = self
            .client
            .post(&url)
            .send()
            .await
            .map_err(|e| anyhow!("Failed to call start service API for {}: {}", name, e))?;

        if response.status().is_success() {
            info!("Successfully started service: {}", name);
            Ok(())
        } else {
            Err(anyhow!(
                "Failed to start service {}: HTTP {}",
                name,
                response.status()
            ))
        }
    }

    /// Stop a specific service
    #[allow(dead_code)]
    pub async fn stop_service(&self, name: &str) -> Result<()> {
        let url = format!("{}/api/services/{}/stop", self.base_url, name);
        info!("Stopping service: {}", name);

        let response = self
            .client
            .post(&url)
            .send()
            .await
            .map_err(|e| anyhow!("Failed to call stop service API for {}: {}", name, e))?;

        if response.status().is_success() {
            info!("Successfully stopped service: {}", name);
            Ok(())
        } else {
            Err(anyhow!(
                "Failed to stop service {}: HTTP {}",
                name,
                response.status()
            ))
        }
    }

    /// Restart a specific service
    #[allow(dead_code)]
    pub async fn restart_service(&self, name: &str) -> Result<()> {
        let url = format!("{}/api/services/{}/restart", self.base_url, name);
        info!("Restarting service: {}", name);

        let response = self
            .client
            .post(&url)
            .send()
            .await
            .map_err(|e| anyhow!("Failed to call restart service API for {}: {}", name, e))?;

        if response.status().is_success() {
            info!("Successfully restarted service: {}", name);
            Ok(())
        } else {
            Err(anyhow!(
                "Failed to restart service {}: HTTP {}",
                name,
                response.status()
            ))
        }
    }

    /// Get adapter list
    pub async fn get_adapters(&self) -> Result<Vec<AdapterInfo>> {
        let url = format!("{}/api/adapters", self.base_url);

        match self.client.get(&url).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    let adapters: Vec<AdapterInfo> = response.json().await?;
                    debug!("Received {} adapters", adapters.len());
                    Ok(adapters)
                } else {
                    debug!("Adapters API returned: {}", response.status());
                    Ok(vec![])
                }
            }
            Err(e) => {
                debug!("Failed to fetch adapters: {}", e);
                Ok(vec![])
            }
        }
    }

    /// Get server health
    pub async fn get_health(&self) -> Result<HealthStatus> {
        let url = format!("{}/api/health", self.base_url);

        match self.client.get(&url).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    let health: HealthStatus = response.json().await?;
                    debug!("Health check: {:?}", health);
                    Ok(health)
                } else {
                    Ok(HealthStatus {
                        status: "unhealthy".to_string(),
                        version: None,
                        uptime_seconds: 0,
                    })
                }
            }
            Err(e) => {
                debug!("Health check failed: {}", e);
                Ok(HealthStatus {
                    status: "offline".to_string(),
                    version: None,
                    uptime_seconds: 0,
                })
            }
        }
    }

    /// Get recent logs from the server
    pub async fn get_logs(&self, filters: &LogQuery) -> Result<Vec<LogEntry>> {
        let url = format!("{}/api/logs/query", self.base_url);
        let mut params: Vec<(String, String)> = vec![("limit".to_string(), "100".to_string())];

        if let Some(trace) = &filters.trace_id {
            params.push(("trace_id".to_string(), trace.clone()));
        }
        if let Some(tenant) = &filters.tenant_id {
            params.push(("tenant_id".to_string(), tenant.clone()));
        }

        match self.client.get(&url).query(&params).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    let raw_logs: Vec<Value> = response.json().await.unwrap_or_default();
                    let logs: Vec<LogEntry> = raw_logs
                        .into_iter()
                        .filter_map(Self::map_log_entry)
                        .collect();
                    debug!("Received {} logs", logs.len());
                    Ok(logs)
                } else {
                    debug!("Logs API returned status: {}", response.status());
                    Ok(vec![])
                }
            }
            Err(e) => {
                debug!("Failed to fetch logs: {}", e);
                Ok(vec![])
            }
        }
    }

    /// Parse metrics from JSON response
    fn parse_metrics(&self, data: Value) -> SystemMetrics {
        SystemMetrics {
            inference_latency_p95_ms: data
                .get("inference_latency_p95_ms")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32,
            tokens_per_second: data
                .get("tokens_per_second")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32,
            queue_depth: data
                .get("queue_depth")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32,
            active_adapters: data
                .get("active_adapters")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32,
            total_adapters: data
                .get("total_adapters")
                .and_then(|v| v.as_u64())
                .unwrap_or(50) as u32,
            memory_headroom_percent: data
                .get("memory_headroom_percent")
                .and_then(|v| v.as_f64())
                .unwrap_or(15.0) as f32,
        }
    }

    fn map_log_entry(value: Value) -> Option<LogEntry> {
        let timestamp = value
            .get("timestamp")
            .and_then(|v| v.as_str())
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(Utc::now);

        let level_str = value
            .get("level")
            .and_then(|v| v.as_str())
            .unwrap_or("info");

        let component = value
            .get("component")
            .and_then(|v| v.as_str())
            .or_else(|| value.get("event_type").and_then(|v| v.as_str()))
            .unwrap_or("telemetry")
            .to_string();

        let message = value
            .get("message")
            .and_then(|v| v.as_str())
            .or_else(|| value.get("event_type").and_then(|v| v.as_str()))
            .unwrap_or("log")
            .to_string();

        let trace_id = value
            .get("trace_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| {
                value
                    .get("metadata")
                    .and_then(|m| m.get("trace_id"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            });

        let tenant_id = value
            .get("identity")
            .and_then(|id| id.get("tenant_id"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| {
                value
                    .get("metadata")
                    .and_then(|m| m.get("tenant_id"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            });

        let latency_ms = value
            .get("metadata")
            .and_then(|m| {
                m.get("latency_ms")
                    .or_else(|| m.get("duration_ms"))
                    .or_else(|| m.get("latency"))
            })
            .and_then(|v| v.as_u64())
            .or_else(|| value.get("duration_ms").and_then(|v| v.as_u64()));

        Some(LogEntry {
            timestamp,
            level: level_str.parse().unwrap(),
            component,
            message,
            trace_id,
            tenant_id,
            latency_ms,
        })
    }

    // === Adapter Lifecycle Methods ===

    /// Load an adapter into memory
    pub async fn load_adapter(&self, adapter_id: &str) -> Result<()> {
        let url = format!("{}/v1/adapters/{}/load", self.base_url, adapter_id);
        info!("Loading adapter: {}", adapter_id);

        let response = self
            .client
            .post(&url)
            .send()
            .await
            .map_err(|e| anyhow!("Failed to call load adapter API: {}", e))?;

        if response.status().is_success() {
            info!("Successfully loaded adapter: {}", adapter_id);
            Ok(())
        } else {
            Err(anyhow!(
                "Failed to load adapter {}: HTTP {}",
                adapter_id,
                response.status()
            ))
        }
    }

    /// Unload an adapter from memory
    pub async fn unload_adapter(&self, adapter_id: &str) -> Result<()> {
        let url = format!("{}/v1/adapters/{}/unload", self.base_url, adapter_id);
        info!("Unloading adapter: {}", adapter_id);

        let response = self
            .client
            .post(&url)
            .send()
            .await
            .map_err(|e| anyhow!("Failed to call unload adapter API: {}", e))?;

        if response.status().is_success() {
            info!("Successfully unloaded adapter: {}", adapter_id);
            Ok(())
        } else {
            Err(anyhow!(
                "Failed to unload adapter {}: HTTP {}",
                adapter_id,
                response.status()
            ))
        }
    }

    /// Pin an adapter to keep it loaded in memory
    pub async fn pin_adapter(&self, adapter_id: &str) -> Result<()> {
        let url = format!("{}/v1/adapters/{}/pin", self.base_url, adapter_id);
        info!("Pinning adapter: {}", adapter_id);

        let response = self
            .client
            .post(&url)
            .send()
            .await
            .map_err(|e| anyhow!("Failed to call pin adapter API: {}", e))?;

        if response.status().is_success() {
            info!("Successfully pinned adapter: {}", adapter_id);
            Ok(())
        } else {
            Err(anyhow!(
                "Failed to pin adapter {}: HTTP {}",
                adapter_id,
                response.status()
            ))
        }
    }

    /// Unpin an adapter to allow it to be evicted from memory
    pub async fn unpin_adapter(&self, adapter_id: &str) -> Result<()> {
        let url = format!("{}/v1/adapters/{}/pin", self.base_url, adapter_id);
        info!("Unpinning adapter: {}", adapter_id);

        let response = self
            .client
            .delete(&url)
            .send()
            .await
            .map_err(|e| anyhow!("Failed to call unpin adapter API: {}", e))?;

        if response.status().is_success() {
            info!("Successfully unpinned adapter: {}", adapter_id);
            Ok(())
        } else {
            Err(anyhow!(
                "Failed to unpin adapter {}: HTTP {}",
                adapter_id,
                response.status()
            ))
        }
    }

    /// Get detailed information about a specific adapter
    #[allow(dead_code)]
    pub async fn get_adapter_detail(&self, adapter_id: &str) -> Result<AdapterDetailResponse> {
        let url = format!("{}/v1/adapters/{}/detail", self.base_url, adapter_id);

        match self.client.get(&url).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    let detail: AdapterDetailResponse = response.json().await?;
                    debug!("Received adapter detail for: {}", adapter_id);
                    Ok(detail)
                } else {
                    Err(anyhow!(
                        "Failed to get adapter detail: HTTP {}",
                        response.status()
                    ))
                }
            }
            Err(e) => Err(anyhow!("Failed to fetch adapter detail: {}", e)),
        }
    }

    // === Training Job Methods ===

    /// List all training jobs
    pub async fn list_training_jobs(&self) -> Result<Vec<TrainingJobResponse>> {
        let url = format!("{}/v1/training/jobs", self.base_url);

        match self.client.get(&url).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    let jobs: Vec<TrainingJobResponse> = response.json().await.unwrap_or_default();
                    debug!("Received {} training jobs", jobs.len());
                    Ok(jobs)
                } else {
                    debug!("Training jobs API returned: {}", response.status());
                    Ok(Vec::new())
                }
            }
            Err(e) => {
                debug!("Failed to fetch training jobs: {}", e);
                Ok(Vec::new())
            }
        }
    }

    /// Get details of a specific training job
    #[allow(dead_code)]
    pub async fn get_training_job(&self, job_id: &str) -> Result<TrainingJobResponse> {
        let url = format!("{}/v1/training/jobs/{}", self.base_url, job_id);

        match self.client.get(&url).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    let job: TrainingJobResponse = response.json().await?;
                    debug!("Received training job detail for: {}", job_id);
                    Ok(job)
                } else {
                    Err(anyhow!(
                        "Failed to get training job: HTTP {}",
                        response.status()
                    ))
                }
            }
            Err(e) => Err(anyhow!("Failed to fetch training job: {}", e)),
        }
    }

    /// Cancel a running training job
    pub async fn cancel_training_job(&self, job_id: &str) -> Result<()> {
        let url = format!("{}/v1/training/jobs/{}/cancel", self.base_url, job_id);
        info!("Cancelling training job: {}", job_id);

        let response = self
            .client
            .post(&url)
            .send()
            .await
            .map_err(|e| anyhow!("Failed to call cancel training job API: {}", e))?;

        if response.status().is_success() {
            info!("Successfully cancelled training job: {}", job_id);
            Ok(())
        } else {
            Err(anyhow!(
                "Failed to cancel training job {}: HTTP {}",
                job_id,
                response.status()
            ))
        }
    }

    // === Inference Methods ===

    /// Run inference with optional adapter
    #[allow(dead_code)]
    pub async fn infer(&self, prompt: &str, adapter_id: Option<&str>) -> Result<InferResponse> {
        let url = format!("{}/v1/infer", self.base_url);
        info!("Running inference with adapter: {:?}", adapter_id);

        let mut body = serde_json::json!({
            "prompt": prompt,
            "max_tokens": 512,
            "temperature": 0.7,
        });
        if let Some(adapter) = adapter_id {
            body["adapter_id"] = serde_json::json!(adapter);
        }

        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| anyhow!("Failed to call inference API: {}", e))?;

        if response.status().is_success() {
            let infer_response: InferResponse = response.json().await?;
            info!(
                "Inference completed: {} tokens in {}ms",
                infer_response.tokens_generated, infer_response.latency_ms
            );
            Ok(infer_response)
        } else {
            Err(anyhow!("Inference failed: HTTP {}", response.status()))
        }
    }

    // === Memory/Capacity Methods ===

    /// Get unified memory breakdown
    #[allow(dead_code)]
    pub async fn get_memory_breakdown(&self) -> Result<MemoryBreakdown> {
        let url = format!("{}/v1/memory/uma", self.base_url);

        match self.client.get(&url).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    let breakdown: MemoryBreakdown = response.json().await?;
                    debug!("Received memory breakdown: {:?}", breakdown);
                    Ok(breakdown)
                } else {
                    debug!("Memory API returned: {}", response.status());
                    Ok(MemoryBreakdown::default())
                }
            }
            Err(e) => {
                debug!("Failed to fetch memory breakdown: {}", e);
                Ok(MemoryBreakdown::default())
            }
        }
    }

    // === System Status Methods ===

    /// Get comprehensive system status
    #[allow(dead_code)]
    pub async fn get_system_status(&self) -> Result<SystemStatusResponse> {
        let url = format!("{}/v1/system/status", self.base_url);

        match self.client.get(&url).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    let status: SystemStatusResponse = response.json().await?;
                    debug!("Received system status: {:?}", status);
                    Ok(status)
                } else {
                    Err(anyhow!(
                        "Failed to get system status: HTTP {}",
                        response.status()
                    ))
                }
            }
            Err(e) => Err(anyhow!("Failed to fetch system status: {}", e)),
        }
    }
}

// Response types for API calls
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceStatusResponse {
    pub name: String,
    pub status: String,
    pub message: Option<String>,
    pub pid: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub loaded: bool,
    #[serde(default)]
    pub pinned: bool,
    pub memory_mb: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub status: String,
    pub version: Option<String>,
    pub uptime_seconds: u64,
}
