//! Service definitions and management

use crate::config::ServiceConfig;
use crate::error::{Result, SupervisorError};
use serde::{Deserialize, Serialize};
use std::process::Stdio;
use std::sync::Arc;
use tokio::process::{Child, Command};
use tokio::sync::RwLock;
use tokio::time::{timeout, Duration};
use tracing::{error, info, warn};

/// Runtime state of a service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceStatus {
    pub id: String,
    pub name: String,
    pub state: ServiceState,
    pub pid: Option<u32>,
    pub port: Option<u16>,
    pub start_time: Option<chrono::DateTime<chrono::Utc>>,
    pub health_status: HealthStatus,
    pub restart_count: u32,
    pub last_error: Option<String>,
    pub uptime_seconds: Option<u64>,
}

impl ServiceStatus {
    /// Create a new service status from config
    pub fn from_config(config: &ServiceConfig) -> Self {
        Self {
            id: config.name.clone(),
            name: config.name.clone(),
            state: ServiceState::Stopped,
            pid: None,
            port: config.port,
            start_time: None,
            health_status: HealthStatus::Unknown,
            restart_count: 0,
            last_error: None,
            uptime_seconds: None,
        }
    }
}

/// Service states
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ServiceState {
    Stopped,
    Starting,
    Running,
    Stopping,
    Failed,
    Restarting,
}

/// Health status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    Unknown,
    Healthy,
    Unhealthy,
    Checking,
}

/// Managed service instance
pub struct ManagedService {
    config: ServiceConfig,
    status: Arc<RwLock<ServiceStatus>>,
    process: Arc<RwLock<Option<Child>>>,
}

impl ManagedService {
    /// Create a new managed service
    pub fn new(config: ServiceConfig) -> Self {
        let status = ServiceStatus::from_config(&config);

        Self {
            config,
            status: Arc::new(RwLock::new(status)),
            process: Arc::new(RwLock::new(None)),
        }
    }

    /// Get the current status
    pub async fn status(&self) -> ServiceStatus {
        self.status.read().await.clone()
    }

    /// Get the service ID
    pub fn id(&self) -> &str {
        &self.config.name
    }

    /// Start the service
    pub async fn start(&self) -> Result<()> {
        let mut status = self.status.write().await;

        if status.state == ServiceState::Running || status.state == ServiceState::Starting {
            return Ok(());
        }

        status.state = ServiceState::Starting;
        status.last_error = None;
        drop(status);

        info!("Starting service: {}", self.config.name);

        match self.spawn_process().await {
            Ok(child) => {
                let pid = child.id();
                let mut status = self.status.write().await;
                status.state = ServiceState::Running;
                status.pid = pid;
                status.start_time = Some(chrono::Utc::now());
                status.restart_count += 1;
                *self.process.write().await = Some(child);
                info!(
                    "Service {} started with PID {}",
                    self.config.name,
                    pid.unwrap_or(0)
                );
                Ok(())
            }
            Err(e) => {
                let mut status = self.status.write().await;
                status.state = ServiceState::Failed;
                status.last_error = Some(e.to_string());
                error!("Failed to start service {}: {}", self.config.name, e);
                Err(e)
            }
        }
    }

    /// Stop the service
    pub async fn stop(&self) -> Result<()> {
        let mut status = self.status.write().await;

        if status.state == ServiceState::Stopped || status.state == ServiceState::Stopping {
            return Ok(());
        }

        status.state = ServiceState::Stopping;
        drop(status);

        info!("Stopping service: {}", self.config.name);

        if let Some(mut child) = self.process.write().await.take() {
            // Try graceful shutdown first
            if let Some(pid) = child.id() {
                let _ = tokio::process::Command::new("kill")
                    .args(["-TERM", &pid.to_string()])
                    .status()
                    .await;
            }

            // Wait for graceful shutdown or force kill
            match timeout(Duration::from_secs(10), child.wait()).await {
                Ok(result) => match result {
                    Ok(status) => info!(
                        "Service {} stopped with exit code: {}",
                        self.config.name,
                        status.code().unwrap_or(-1)
                    ),
                    Err(e) => warn!("Error waiting for service {}: {}", self.config.name, e),
                },
                Err(_) => {
                    // Timeout - force kill
                    if let Some(pid) = child.id() {
                        let _ = tokio::process::Command::new("kill")
                            .args(["-KILL", &pid.to_string()])
                            .status()
                            .await;
                    }
                    warn!("Force killed service {} after timeout", self.config.name);
                }
            }
        }

        let mut status = self.status.write().await;
        status.state = ServiceState::Stopped;
        status.pid = None;
        status.uptime_seconds = None;

        Ok(())
    }

    /// Restart the service
    pub async fn restart(&self) -> Result<()> {
        info!("Restarting service: {}", self.config.name);
        self.stop().await?;
        tokio::time::sleep(Duration::from_millis(500)).await;
        self.start().await
    }

    /// Check if the service is healthy
    pub async fn check_health(&self) -> Result<HealthStatus> {
        let status = self.status.read().await;

        if status.state != ServiceState::Running {
            return Ok(HealthStatus::Unhealthy);
        }

        match &self.config.health_check.check_type {
            crate::config::HealthCheckType::Http => {
                if let Some(endpoint) = &self.config.health_check.endpoint {
                    match self.check_http_health(endpoint).await {
                        Ok(true) => Ok(HealthStatus::Healthy),
                        Ok(false) => Ok(HealthStatus::Unhealthy),
                        Err(e) => {
                            warn!("HTTP health check failed for {}: {}", self.config.name, e);
                            Ok(HealthStatus::Unhealthy)
                        }
                    }
                } else {
                    Ok(HealthStatus::Unhealthy)
                }
            }
            crate::config::HealthCheckType::Tcp => {
                if let Some(port) = self.config.port {
                    match self.check_tcp_health(port).await {
                        Ok(true) => Ok(HealthStatus::Healthy),
                        Ok(false) => Ok(HealthStatus::Unhealthy),
                        Err(e) => {
                            warn!("TCP health check failed for {}: {}", self.config.name, e);
                            Ok(HealthStatus::Unhealthy)
                        }
                    }
                } else {
                    Ok(HealthStatus::Unhealthy)
                }
            }
            crate::config::HealthCheckType::Process => {
                // Just check if the process is still running
                if let Some(child) = self.process.write().await.as_mut() {
                    match child.try_wait() {
                        Ok(Some(exit_status)) => {
                            warn!(
                                "Service {} process exited: {}",
                                self.config.name, exit_status
                            );
                            Ok(HealthStatus::Unhealthy)
                        }
                        Ok(None) => Ok(HealthStatus::Healthy),
                        Err(e) => {
                            warn!(
                                "Failed to check process status for {}: {}",
                                self.config.name, e
                            );
                            Ok(HealthStatus::Unhealthy)
                        }
                    }
                } else {
                    Ok(HealthStatus::Unhealthy)
                }
            }
            _ => Ok(HealthStatus::Unknown),
        }
    }

    /// Spawn the service process
    async fn spawn_process(&self) -> Result<Child> {
        let mut command = Command::new(&self.config.command);
        command.args(&self.config.args);

        if let Some(cwd) = &self.config.working_directory {
            command.current_dir(cwd);
        }

        // Set environment variables
        command.envs(&self.config.environment);

        // Configure stdio
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());

        let child = command.spawn().map_err(|e| {
            SupervisorError::Process(format!("Failed to spawn {}: {}", self.config.name, e))
        })?;

        Ok(child)
    }

    /// Check HTTP health endpoint
    async fn check_http_health(&self, endpoint: &str) -> Result<bool> {
        // Simple HTTP check using reqwest
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(
                self.config.health_check.timeout_seconds,
            ))
            .build()
            .map_err(|e| SupervisorError::Http(format!("Failed to create HTTP client: {}", e)))?;

        let response = client
            .get(endpoint)
            .send()
            .await
            .map_err(|e| SupervisorError::Http(format!("HTTP request failed: {}", e)))?;
        Ok(response.status().is_success())
    }

    /// Check TCP connectivity
    async fn check_tcp_health(&self, port: u16) -> Result<bool> {
        use tokio::net::TcpStream;

        match tokio::time::timeout(
            Duration::from_secs(self.config.health_check.timeout_seconds),
            TcpStream::connect(format!("127.0.0.1:{}", port)),
        )
        .await
        {
            Ok(Ok(_)) => Ok(true),
            _ => Ok(false),
        }
    }
}

/// API response for service operations
#[derive(Debug, Serialize, Deserialize)]
pub struct ServiceOperationResult {
    pub success: bool,
    pub message: String,
    pub service_id: String,
    pub operation: String,
}

/// API response for multiple service operations
#[derive(Debug, Serialize, Deserialize)]
pub struct BulkOperationResult {
    pub results: Vec<ServiceOperationResult>,
    pub successful: usize,
    pub failed: usize,
}

#[async_trait::async_trait]
impl crate::health::HealthCheck for ManagedService {
    async fn check(&self) -> crate::health::HealthResult {
        match self.check_health().await {
            Ok(HealthStatus::Healthy) => crate::health::HealthResult::Healthy,
            Ok(HealthStatus::Unhealthy) => {
                crate::health::HealthResult::Unhealthy("Service is unhealthy".to_string())
            }
            Ok(HealthStatus::Unknown) => crate::health::HealthResult::Unknown,
            Ok(HealthStatus::Checking) => crate::health::HealthResult::Unknown,
            Err(e) => crate::health::HealthResult::Unhealthy(e.to_string()),
        }
    }
}
