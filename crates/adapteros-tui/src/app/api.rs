use anyhow::Result;
use reqwest::Client;
use serde_json::Value;
use tracing::{debug, error, info};

use super::types::SystemMetrics;

pub struct ApiClient {
    client: Client,
    base_url: String,
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

        match self.client.post(&url).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    info!("Successfully started all services");
                } else {
                    error!("Failed to start services: {}", response.status());
                }
            }
            Err(e) => {
                error!("Failed to call start services API: {}", e);
            }
        }

        Ok(())
    }

    /// Start a specific service
    pub async fn start_service(&self, name: &str) -> Result<()> {
        let url = format!("{}/api/services/{}/start", self.base_url, name);
        info!("Starting service: {}", name);

        match self.client.post(&url).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    info!("Successfully started service: {}", name);
                } else {
                    error!("Failed to start service {}: {}", name, response.status());
                }
            }
            Err(e) => {
                error!("Failed to call start service API for {}: {}", name, e);
            }
        }

        Ok(())
    }

    /// Stop a specific service
    pub async fn stop_service(&self, name: &str) -> Result<()> {
        let url = format!("{}/api/services/{}/stop", self.base_url, name);
        info!("Stopping service: {}", name);

        match self.client.post(&url).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    info!("Successfully stopped service: {}", name);
                } else {
                    error!("Failed to stop service {}: {}", name, response.status());
                }
            }
            Err(e) => {
                error!("Failed to call stop service API for {}: {}", name, e);
            }
        }

        Ok(())
    }

    /// Restart a specific service
    pub async fn restart_service(&self, name: &str) -> Result<()> {
        let url = format!("{}/api/services/{}/restart", self.base_url, name);
        info!("Restarting service: {}", name);

        match self.client.post(&url).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    info!("Successfully restarted service: {}", name);
                } else {
                    error!("Failed to restart service {}: {}", name, response.status());
                }
            }
            Err(e) => {
                error!("Failed to call restart service API for {}: {}", name, e);
            }
        }

        Ok(())
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

    /// Parse metrics from JSON response
    fn parse_metrics(&self, data: Value) -> SystemMetrics {
        SystemMetrics {
            inference_latency_p95_ms: data.get("inference_latency_p95_ms")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32,
            tokens_per_second: data.get("tokens_per_second")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32,
            queue_depth: data.get("queue_depth")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32,
            active_adapters: data.get("active_adapters")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32,
            total_adapters: data.get("total_adapters")
                .and_then(|v| v.as_u64())
                .unwrap_or(50) as u32,
            memory_headroom_percent: data.get("memory_headroom_percent")
                .and_then(|v| v.as_f64())
                .unwrap_or(15.0) as f32,
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
    pub memory_mb: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub status: String,
    pub version: Option<String>,
    pub uptime_seconds: u64,
}