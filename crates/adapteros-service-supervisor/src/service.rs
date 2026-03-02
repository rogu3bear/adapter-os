//! Service definitions and management

use crate::config::ServiceConfig;
use crate::error::{Result, SupervisorError};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::RwLock;
use tokio::time::{timeout, Duration};
use tracing::{debug, error, info, warn};

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
                // Prefer UDS health check if configured (production mode)
                if let Some(uds_path) = &self.config.health_check.uds_socket {
                    match self.check_uds_health(uds_path).await {
                        Ok(true) => Ok(HealthStatus::Healthy),
                        Ok(false) => Ok(HealthStatus::Unhealthy),
                        Err(e) => {
                            warn!("UDS health check failed for {}: {}", self.config.name, e);
                            Ok(HealthStatus::Unhealthy)
                        }
                    }
                } else if let Some(port) = self.config.port {
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
    /// Get log file path for this service
    fn log_file_path(&self) -> PathBuf {
        let log_dir =
            std::env::var("SUPERVISOR_LOG_DIR").unwrap_or_else(|_| "var/logs".to_string());
        PathBuf::from(log_dir).join(format!("{}.log", self.config.name))
    }

    /// Spawn a task to capture and log stdout/stderr
    async fn capture_output(
        service_name: String,
        log_path: PathBuf,
        stream: impl tokio::io::AsyncRead + Unpin + Send + 'static,
        stream_name: &'static str,
    ) {
        tokio::spawn(async move {
            // Ensure log directory exists
            if let Some(parent) = log_path.parent() {
                let _ = tokio::fs::create_dir_all(parent).await;
            }

            // Open log file in append mode
            let file = match OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path)
                .await
            {
                Ok(f) => f,
                Err(e) => {
                    error!(
                        "Failed to open log file for {}/{}: {}",
                        service_name, stream_name, e
                    );
                    return;
                }
            };

            let mut file = tokio::io::BufWriter::new(file);
            let mut reader = BufReader::new(stream);
            let mut line = String::new();

            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => break, // EOF
                    Ok(_) => {
                        let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S%.3f");
                        let log_line = format!("[{}] [{}] {}", timestamp, stream_name, line);

                        if let Err(e) = file.write_all(log_line.as_bytes()).await {
                            error!(
                                "Failed to write to log file for {}/{}: {}",
                                service_name, stream_name, e
                            );
                            break;
                        }

                        if let Err(e) = file.flush().await {
                            error!(
                                "Failed to flush log file for {}/{}: {}",
                                service_name, stream_name, e
                            );
                        }
                    }
                    Err(e) => {
                        error!("Error reading from {}/{}: {}", service_name, stream_name, e);
                        break;
                    }
                }
            }

            debug!("Log capture ended for {}/{}", service_name, stream_name);
        });
    }

    /// Rotate a log file to `.prev` if it exceeds `max_bytes`.
    async fn rotate_log_if_large(path: &std::path::Path, max_bytes: u64) {
        match tokio::fs::metadata(path).await {
            Ok(meta) if meta.len() > max_bytes => {
                let mut prev = path.to_path_buf().into_os_string();
                prev.push(".prev");
                if let Err(e) = tokio::fs::rename(path, &prev).await {
                    warn!("Failed to rotate log {}: {}", path.display(), e);
                } else {
                    info!(
                        "Rotated log {} -> {}",
                        path.display(),
                        prev.to_string_lossy()
                    );
                }
            }
            _ => {}
        }
    }

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

        let mut child = command.spawn().map_err(|e| {
            SupervisorError::Process(format!("Failed to spawn {}: {}", self.config.name, e))
        })?;

        // Rotate log file before opening if it exceeds 10MB
        let log_path = self.log_file_path();
        Self::rotate_log_if_large(&log_path, 10 * 1024 * 1024).await;

        if let Some(stdout) = child.stdout.take() {
            Self::capture_output(self.config.name.clone(), log_path.clone(), stdout, "stdout")
                .await;
        }

        if let Some(stderr) = child.stderr.take() {
            Self::capture_output(self.config.name.clone(), log_path.clone(), stderr, "stderr")
                .await;
        }

        Ok(child)
    }

    /// Read logs from log file
    pub async fn read_logs(&self, lines: usize) -> Result<Vec<String>> {
        let log_path = self.log_file_path();

        if !log_path.exists() {
            return Ok(vec![format!("No logs available for {}", self.config.name)]);
        }

        let file = File::open(&log_path)
            .await
            .map_err(|e| SupervisorError::Internal(format!("Failed to open log file: {}", e)))?;

        let reader = BufReader::new(file);
        let mut all_lines: Vec<String> = Vec::new();

        let mut lines_stream = reader.lines();
        while let Some(line) = lines_stream.next_line().await.transpose() {
            match line {
                Ok(l) => all_lines.push(l),
                Err(e) => {
                    warn!("Error reading log line: {}", e);
                    break;
                }
            }
        }

        // Return last N lines
        let start = all_lines.len().saturating_sub(lines);
        Ok(all_lines[start..].to_vec())
    }

    /// Check HTTP health endpoint
    async fn check_http_health(&self, endpoint: &str) -> Result<bool> {
        // Simple HTTP check using reqwest
        let timeout = Duration::from_secs(self.config.health_check.timeout_seconds);
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_millis(500))
            .timeout(timeout)
            .build()
            .map_err(|e| SupervisorError::Http(format!("Failed to create HTTP client: {}", e)))?;

        let max_attempts = 2u32;
        let mut attempt = 0u32;
        let mut backoff = Duration::from_millis(100);

        loop {
            attempt += 1;
            match client.get(endpoint).send().await {
                Ok(response) => return Ok(response.status().is_success()),
                Err(e) => {
                    if attempt >= max_attempts {
                        return Err(SupervisorError::Http(format!(
                            "HTTP request failed after {} attempts: {}",
                            attempt, e
                        )));
                    }
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(Duration::from_millis(500));
                }
            }
        }
    }

    /// Check TCP connectivity (development mode only)
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

    /// Check UDS connectivity (production mode - egress policy compliant)
    async fn check_uds_health(&self, uds_path: &std::path::Path) -> Result<bool> {
        use tokio::net::UnixStream;

        match tokio::time::timeout(
            Duration::from_secs(self.config.health_check.timeout_seconds),
            UnixStream::connect(uds_path),
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
