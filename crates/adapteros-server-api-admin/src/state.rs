//! Application state trait for admin handlers
//!
//! Defines the interface that the main AppState must implement
//! for admin handlers to access required services.

use crate::auth::AdminClaims;
use crate::handlers::lifecycle::RuntimeMode;
use adapteros_boot::BootPhase as BootState;
use adapteros_core::{PluginHealth, Result};
use adapteros_db::tenants::Tenant;
use adapteros_db::users::User;
use adapteros_db::workers::Worker;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

/// Configuration snapshot for settings handlers
pub struct ConfigSnapshot {
    pub general: Option<GeneralConfig>,
    pub server: ServerConfig,
    pub security: SecurityConfig,
    pub performance: PerformanceConfig,
}

/// General configuration
pub struct GeneralConfig {
    pub system_name: Option<String>,
    pub environment: Option<String>,
    pub api_base_url: Option<String>,
}

/// Server configuration
pub struct ServerConfig {
    pub http_port: Option<u16>,
    pub https_port: Option<u16>,
    pub uds_socket: Option<String>,
    pub production_mode: bool,
}

/// Security configuration
pub struct SecurityConfig {
    pub jwt_mode: Option<String>,
    pub token_ttl_seconds: Option<i64>,
    pub require_mfa: Option<bool>,
    pub require_pf_deny: bool,
}

/// Performance configuration
pub struct PerformanceConfig {
    pub max_adapters: Option<i64>,
    pub max_workers: Option<i64>,
    pub memory_threshold_pct: Option<f64>,
    pub cache_size_mb: Option<i64>,
}

/// Maintenance signal response from worker
pub struct MaintenanceSignalResponse {
    /// Worker's current mode
    pub mode: String,
    /// Whether drain flag was set
    pub drain_flag_set: bool,
}

/// Supervisor client error
#[derive(Debug)]
pub struct SupervisorError {
    message: String,
    not_found: bool,
}

impl SupervisorError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            not_found: false,
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            not_found: true,
        }
    }

    pub fn is_not_found(&self) -> bool {
        self.not_found
    }
}

impl std::fmt::Display for SupervisorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for SupervisorError {}

/// UDS client interface for worker communication
pub trait UdsClient: Send + Sync {
    /// Signal a worker to enter maintenance mode
    fn signal_maintenance(
        &self,
        path: &Path,
        mode: &str,
        reason: Option<&str>,
    ) -> impl std::future::Future<Output = Result<MaintenanceSignalResponse>> + Send;
}

/// Supervisor client interface for service control
pub trait SupervisorClient: Send + Sync {
    /// Start a service
    fn start_service(
        &self,
        service_id: &str,
    ) -> impl std::future::Future<Output = std::result::Result<String, SupervisorError>> + Send;

    /// Stop a service
    fn stop_service(
        &self,
        service_id: &str,
    ) -> impl std::future::Future<Output = std::result::Result<String, SupervisorError>> + Send;

    /// Restart a service
    fn restart_service(
        &self,
        service_id: &str,
    ) -> impl std::future::Future<Output = std::result::Result<String, SupervisorError>> + Send;

    /// Start essential services
    fn start_essential_services(
        &self,
    ) -> impl std::future::Future<Output = std::result::Result<String, SupervisorError>> + Send;

    /// Stop essential services
    fn stop_essential_services(
        &self,
    ) -> impl std::future::Future<Output = std::result::Result<String, SupervisorError>> + Send;

    /// Get service logs
    fn get_service_logs(
        &self,
        service_id: &str,
        lines: Option<u32>,
    ) -> impl std::future::Future<Output = std::result::Result<Vec<String>, SupervisorError>> + Send;
}

/// Plugin registry interface
pub trait PluginRegistry: Send + Sync {
    /// Enable/disable plugin for tenant
    fn enable_for_tenant(
        &self,
        name: &str,
        tenant_id: &str,
        enabled: bool,
    ) -> impl std::future::Future<Output = Result<()>> + Send;

    /// Check if plugin is enabled for tenant
    fn is_enabled_for_tenant(
        &self,
        name: &str,
        tenant_id: &str,
    ) -> impl std::future::Future<Output = Result<bool>> + Send;

    /// Get health status for all plugins
    fn health_all(
        &self,
    ) -> impl std::future::Future<Output = HashMap<String, HashMap<String, PluginHealth>>> + Send;
}

/// Boot state manager interface
pub trait BootStateManager: Send + Sync {
    /// Get current boot state
    fn current_state(&self) -> BootState;

    /// Enter drain mode
    fn drain(&self) -> impl std::future::Future<Output = ()> + Send;

    /// Stop the server
    fn stop(&self) -> impl std::future::Future<Output = ()> + Send;

    /// Enter maintenance mode
    fn maintenance(&self, reason: &str) -> impl std::future::Future<Output = ()> + Send;
}

/// Database operations needed by admin handlers
pub trait AdminDb: Send + Sync {
    /// List users with pagination and filtering
    fn list_users(
        &self,
        page: Option<i64>,
        page_size: Option<i64>,
        role: Option<&str>,
        tenant_id: Option<&str>,
    ) -> impl std::future::Future<Output = Result<(Vec<User>, i64)>> + Send;

    /// List active workers
    fn list_active_workers(&self) -> impl std::future::Future<Output = Result<Vec<Worker>>> + Send;

    /// List all tenants
    fn list_tenants(&self) -> impl std::future::Future<Output = Result<Vec<Tenant>>> + Send;
}

/// Application state trait that must be implemented by the main server
///
/// This allows the admin handlers to be generic over the actual AppState type
/// while still accessing the services they need.
pub trait AdminAppState: Clone + Send + Sync + 'static {
    /// Database type
    type Db: AdminDb;
    /// UDS client type
    type UdsClient: UdsClient;
    /// Supervisor client type
    type SupervisorClient: SupervisorClient;
    /// Plugin registry type
    type PluginRegistry: PluginRegistry;
    /// Boot state manager type
    type BootStateManager: BootStateManager;

    /// Get database reference
    fn db(&self) -> &Self::Db;

    /// Get UDS client
    fn uds_client(&self) -> &Self::UdsClient;

    /// Get supervisor client (optional)
    fn supervisor_client(&self) -> Option<&Self::SupervisorClient>;

    /// Get plugin registry
    fn plugin_registry(&self) -> &Self::PluginRegistry;

    /// Get boot state manager (optional)
    fn boot_state(&self) -> Option<&Self::BootStateManager>;

    /// Get runtime mode
    fn runtime_mode(&self) -> Option<RuntimeMode>;

    /// Get configuration snapshot
    fn config(&self) -> std::result::Result<ConfigSnapshot, &'static str>;

    /// Log successful audit action
    fn log_audit_success(
        &self,
        claims: &AdminClaims,
        action: &str,
        resource_type: &str,
        resource_id: Option<&str>,
    ) -> impl std::future::Future<Output = ()> + Send;

    /// Log failed audit action
    fn log_audit_failure(
        &self,
        claims: &AdminClaims,
        action: &str,
        resource_type: &str,
        resource_id: Option<&str>,
        error: &str,
    ) -> impl std::future::Future<Output = ()> + Send;
}
