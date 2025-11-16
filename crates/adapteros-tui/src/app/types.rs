use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Running,
    Starting,
    Stopped,
    Failed,
    Warning,
}

impl Status {
    pub fn as_str(&self) -> &str {
        match self {
            Status::Running => "Running",
            Status::Starting => "Starting",
            Status::Stopped => "Stopped",
            Status::Failed => "Failed",
            Status::Warning => "Warning",
        }
    }

    pub fn color_code(&self) -> &str {
        match self {
            Status::Running => "OK",
            Status::Starting => "..",
            Status::Stopped => "--",
            Status::Failed => "XX",
            Status::Warning => "!!",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ServiceStatus {
    pub name: String,
    pub status: Status,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct ModelStatus {
    pub name: String,
    pub loaded: bool,
    pub memory_usage_mb: u32,
    pub total_memory_mb: u32,
}

#[derive(Debug, Clone)]
pub struct SystemStatus {
    pub uptime: Duration,
    pub cpu_percent: f32,
    pub memory_percent: f32,
    pub disk_percent: f32,
    pub network_rx_mbps: f32,
    pub network_tx_mbps: f32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SystemMetrics {
    pub inference_latency_p95_ms: u32,
    pub tokens_per_second: u32,
    pub queue_depth: u32,
    pub active_adapters: u32,
    pub total_adapters: u32,
    pub memory_headroom_percent: f32,
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub level: LogLevel,
    pub component: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Error,
    Warning,
    Info,
    Debug,
}

impl LogLevel {
    pub fn as_str(&self) -> &str {
        match self {
            LogLevel::Error => "ERROR",
            LogLevel::Warning => "WARN",
            LogLevel::Info => "INFO",
            LogLevel::Debug => "DEBUG",
        }
    }

    pub fn color_code(&self) -> &str {
        match self {
            LogLevel::Error => "ERR",
            LogLevel::Warning => "WRN",
            LogLevel::Info => "INF",
            LogLevel::Debug => "DBG",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Alert {
    pub timestamp: DateTime<Utc>,
    pub severity: AlertSeverity,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertSeverity {
    Critical,
    Warning,
    Info,
}

#[derive(Debug, Clone, Default)]
pub struct SystemConfig {
    pub server_port: u16,
    pub uds_socket: Option<String>,
    pub max_connections: u32,
    pub jwt_mode: JwtMode,
    pub require_pf_deny: bool,
    pub skip_pf_check: bool,
    pub model_path: String,
    pub k_sparse_value: u8,
    pub batch_size: u32,
    pub cache_size_mb: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JwtMode {
    Hmac,
    EdDsa,
}

impl Default for JwtMode {
    fn default() -> Self {
        JwtMode::Hmac
    }
}

impl JwtMode {
    pub fn as_str(&self) -> &str {
        match self {
            JwtMode::Hmac => "HMAC",
            JwtMode::EdDsa => "EdDSA",
        }
    }
}