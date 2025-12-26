use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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
    pub cpu_percent: f32,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
    Debug,
}

impl LogLevel {
    pub fn as_str(&self) -> &str {
        match self {
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERROR",
            LogLevel::Debug => "DEBUG",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub level: LogLevel,
    pub component: String,
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub struct SystemConfig {
    pub server_port: u16,
    pub max_connections: u32,
    pub jwt_mode: JwtMode,
    pub require_pf_deny: bool,
    pub model_path: String,
    pub k_sparse_value: u8,
    pub batch_size: u32,
    pub cache_size_mb: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JwtMode {
    #[default]
    Hmac,
    EdDsa,
}

impl JwtMode {
    pub fn as_str(&self) -> &str {
        match self {
            JwtMode::Hmac => "HMAC",
            JwtMode::EdDsa => "EdDSA",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SetupState {
    pub missing_prereqs: Vec<String>,
    pub infrastructure_online: bool,
    pub last_action: Option<String>,
    pub last_output: Option<String>,
}

impl SetupState {
    pub fn new(missing_prereqs: Vec<String>) -> Self {
        Self {
            missing_prereqs,
            infrastructure_online: false,
            last_action: None,
            last_output: None,
        }
    }

    pub fn needs_setup(&self) -> bool {
        !self.missing_prereqs.is_empty() || !self.infrastructure_online
    }

    pub fn set_last_action<A, B>(&mut self, action: A, output: B)
    where
        A: Into<String>,
        B: Into<String>,
    {
        self.last_action = Some(action.into());
        self.last_output = Some(output.into());
    }
}
