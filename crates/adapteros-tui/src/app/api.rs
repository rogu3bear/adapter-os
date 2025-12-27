use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde_json::Value;
use tracing::{debug, info};

use super::types::{LogEntry, LogLevel, SystemMetrics};

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
            level: LogLevel::from_str(level_str),
            component,
            message,
            trace_id,
            tenant_id,
            latency_ms,
        })
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
    pub memory_mb: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub status: String,
    pub version: Option<String>,
    pub uptime_seconds: u64,
}
