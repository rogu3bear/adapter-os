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
        let endpoints = [
            format!("{}/v1/metrics/snapshot", self.base_url),
            format!("{}/v1/metrics", self.base_url),
            format!("{}/metrics", self.base_url),
            format!("{}/api/metrics", self.base_url),
        ];

        for url in endpoints {
            match self.client.get(&url).send().await {
                Ok(response) => {
                    if !response.status().is_success() {
                        debug!(
                            "Metrics API returned status {} for {}",
                            response.status(),
                            url
                        );
                        continue;
                    }

                    let payload = response.text().await.unwrap_or_default();
                    if payload.trim().is_empty() {
                        debug!("Metrics endpoint {} returned empty payload", url);
                        continue;
                    }

                    let metrics = if let Ok(data) = serde_json::from_str::<Value>(&payload) {
                        debug!("Received JSON metrics from {}", url);
                        self.parse_metrics_json(&data)
                    } else {
                        debug!("Received Prometheus/OpenMetrics payload from {}", url);
                        self.parse_metrics_prometheus(&payload)
                    };

                    return Ok(metrics);
                }
                Err(e) => {
                    debug!("Failed to fetch metrics from {}: {}", url, e);
                }
            }
        }

        Ok(SystemMetrics::default())
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
        let endpoints = [
            format!("{}/v1/adapters", self.base_url),
            format!("{}/api/adapters", self.base_url),
        ];

        for url in endpoints {
            match self.client.get(&url).send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        let raw: Vec<Value> = response.json().await.unwrap_or_default();
                        let adapters: Vec<AdapterInfo> =
                            raw.into_iter().filter_map(Self::map_adapter_info).collect();
                        debug!("Received {} adapters from {}", adapters.len(), url);
                        return Ok(adapters);
                    }

                    debug!("Adapters API returned {} for {}", response.status(), url);
                }
                Err(e) => {
                    debug!("Failed to fetch adapters from {}: {}", url, e);
                }
            }
        }

        Ok(vec![])
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

    /// Parse metrics from JSON payloads (legacy and v1 snapshot forms).
    fn parse_metrics_json(&self, data: &Value) -> SystemMetrics {
        let active_adapters =
            Self::extract_metric_value(data, &["active_adapters", "aos_adapter_cache_entries"])
                .unwrap_or(0.0);
        let total_adapters = Self::extract_metric_value(data, &["total_adapters"])
            .unwrap_or(active_adapters.max(50.0));
        let memory_headroom_percent =
            Self::extract_metric_value(data, &["memory_headroom_percent"])
                .or_else(|| {
                    Self::extract_metric_value(data, &["aos_memory_pressure_ratio"])
                        .map(|ratio| (1.0 - ratio).clamp(0.0, 1.0) * 100.0)
                })
                .unwrap_or(15.0);

        SystemMetrics {
            inference_latency_p95_ms: Self::extract_metric_value(
                data,
                &[
                    "inference_latency_p95_ms",
                    "p95_latency_ms",
                    "avg_latency_ms",
                ],
            )
            .unwrap_or(0.0) as u32,
            tokens_per_second: Self::extract_metric_value(data, &["tokens_per_second", "tps"])
                .unwrap_or(0.0) as u32,
            queue_depth: Self::extract_metric_value(data, &["queue_depth"]).unwrap_or(0.0) as u32,
            active_adapters: active_adapters as u32,
            total_adapters: total_adapters as u32,
            memory_headroom_percent: memory_headroom_percent as f32,
        }
    }

    /// Parse metrics from Prometheus/OpenMetrics exposition payloads.
    fn parse_metrics_prometheus(&self, payload: &str) -> SystemMetrics {
        let active_adapters =
            Self::extract_prometheus_metric(payload, "aos_adapter_cache_entries", None)
                .unwrap_or(0.0);
        let memory_headroom_percent = Self::extract_prometheus_metric(
            payload,
            "aos_memory_pressure_ratio",
            Some(("pool_type", "system")),
        )
        .map(|ratio| (1.0 - ratio).clamp(0.0, 1.0) * 100.0)
        .unwrap_or(15.0);

        SystemMetrics {
            inference_latency_p95_ms: Self::extract_prometheus_metric(
                payload,
                "aos_inference_latency_p95_ms",
                None,
            )
            .or_else(|| Self::extract_prometheus_metric(payload, "inference_latency_p95_ms", None))
            .unwrap_or(0.0) as u32,
            tokens_per_second: Self::extract_prometheus_metric(
                payload,
                "aos_tokens_per_second",
                None,
            )
            .or_else(|| Self::extract_prometheus_metric(payload, "tokens_per_second", None))
            .unwrap_or(0.0) as u32,
            queue_depth: Self::extract_prometheus_metric(payload, "queue_depth", None)
                .or_else(|| Self::extract_prometheus_metric(payload, "adapteros_queue_depth", None))
                .unwrap_or(0.0) as u32,
            active_adapters: active_adapters as u32,
            total_adapters: active_adapters.max(50.0) as u32,
            memory_headroom_percent: memory_headroom_percent as f32,
        }
    }

    fn extract_metric_value(data: &Value, keys: &[&str]) -> Option<f64> {
        for key in keys {
            if let Some(value) = data.get(*key).and_then(Self::value_as_f64) {
                return Some(value);
            }

            for section in ["metrics", "gauges", "counters"] {
                if let Some(value) = data
                    .get(section)
                    .and_then(|v| v.get(*key))
                    .and_then(Self::value_as_f64)
                {
                    return Some(value);
                }
            }
        }

        None
    }

    fn extract_prometheus_metric(
        payload: &str,
        metric: &str,
        required_label: Option<(&str, &str)>,
    ) -> Option<f64> {
        for raw_line in payload.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') || !line.starts_with(metric) {
                continue;
            }

            let suffix = &line[metric.len()..];
            if !(suffix.starts_with('{') || suffix.starts_with(' ') || suffix.starts_with('\t')) {
                continue;
            }

            if let Some((label_key, label_value)) = required_label {
                if !line.contains(&format!(r#"{label_key}="{label_value}""#)) {
                    continue;
                }
            }

            if let Some(value) = line
                .split_whitespace()
                .last()
                .and_then(|v| v.parse::<f64>().ok())
            {
                return Some(value);
            }
        }

        None
    }

    fn value_as_f64(value: &Value) -> Option<f64> {
        value.as_f64().or_else(|| value.as_u64().map(|v| v as f64))
    }

    fn map_adapter_info(value: Value) -> Option<AdapterInfo> {
        let id = value
            .get("id")
            .or_else(|| value.get("adapter_id"))
            .and_then(|v| v.as_str())?
            .to_string();
        let name = value
            .get("name")
            .or_else(|| value.get("display_name"))
            .and_then(|v| v.as_str())
            .unwrap_or(&id)
            .to_string();
        let version = value
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let loaded = value
            .get("loaded")
            .and_then(|v| v.as_bool())
            .or_else(|| {
                let runtime_state = value
                    .get("runtime_state")
                    .and_then(|v| v.as_str())
                    .or_else(|| {
                        value
                            .get("runtime_state")
                            .and_then(|v| v.get("state"))
                            .and_then(|v| v.as_str())
                    })?;
                let lowered = runtime_state.to_ascii_lowercase();
                Some(matches!(
                    lowered.as_str(),
                    "loaded" | "warm" | "hot" | "active" | "running"
                ))
            })
            .unwrap_or(false);

        let pinned = value
            .get("pinned")
            .and_then(|v| v.as_bool().or_else(|| v.as_i64().map(|n| n != 0)))
            .unwrap_or(false);
        let memory_mb = value
            .get("memory_mb")
            .and_then(|v| v.as_u64())
            .and_then(|v| u32::try_from(v).ok())
            .or_else(|| {
                value
                    .get("memory_bytes")
                    .and_then(|v| v.as_u64())
                    .map(|bytes| bytes / (1024 * 1024))
                    .and_then(|mb| u32::try_from(mb).ok())
            });

        Some(AdapterInfo {
            id,
            name,
            version,
            loaded,
            pinned,
            memory_mb,
        })
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
