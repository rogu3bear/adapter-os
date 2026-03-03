//! Main service supervisor implementation

use crate::auth::AuthService;
use crate::config::{RestartPolicy, RestartPolicyType, SupervisorConfig};
use crate::error::{Result, SupervisorError};
use crate::health::{HealthCheck, HealthMonitor, HealthResult};
use crate::service::{ManagedService, ServiceState, ServiceStatus};
use adapteros_config::path_resolver::resolve_supervisor_signing_key_path;
use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Health check wrapper for ManagedService
struct ManagedServiceHealthCheck(Arc<ManagedService>);

#[async_trait::async_trait]
impl HealthCheck for ManagedServiceHealthCheck {
    async fn check(&self) -> HealthResult {
        match self.0.check_health().await {
            Ok(crate::service::HealthStatus::Healthy) => HealthResult::Healthy,
            Ok(crate::service::HealthStatus::Unhealthy) => {
                HealthResult::Unhealthy("Service is unhealthy".to_string())
            }
            Ok(crate::service::HealthStatus::Unknown) => HealthResult::Unknown,
            Ok(crate::service::HealthStatus::Checking) => HealthResult::Unknown,
            Err(e) => HealthResult::Unhealthy(e.to_string()),
        }
    }
}

#[derive(Default)]
struct ServiceRemediationState {
    restart_attempts: VecDeque<Instant>,
    cooldown_until: Option<Instant>,
}

impl ServiceRemediationState {
    fn prune_window(&mut self, now: Instant, window: std::time::Duration) {
        while let Some(oldest) = self.restart_attempts.front().copied() {
            if now.duration_since(oldest) > window {
                self.restart_attempts.pop_front();
            } else {
                break;
            }
        }
    }
}

fn should_attempt_auto_restart(policy: &RestartPolicy, state: &ServiceState) -> bool {
    if matches!(
        state,
        ServiceState::Starting | ServiceState::Stopping | ServiceState::Restarting
    ) {
        return false;
    }

    match policy.policy {
        RestartPolicyType::Never => false,
        RestartPolicyType::Always => true,
        RestartPolicyType::OnFailure => !matches!(state, ServiceState::Stopped),
        RestartPolicyType::UnlessStopped => !matches!(state, ServiceState::Stopped),
    }
}

/// Main service supervisor
pub struct ServiceSupervisor {
    config: SupervisorConfig,
    auth_service: Arc<AuthService>,
    health_monitor: Arc<HealthMonitor>,
    services: Arc<RwLock<HashMap<String, Arc<ManagedService>>>>,
}

impl ServiceSupervisor {
    /// Create a new service supervisor
    pub async fn new(config: SupervisorConfig, keypair_pem: &str) -> Result<Self> {
        // Resolve key path with /tmp rejection and env var support
        let default_key_path = resolve_supervisor_signing_key_path()
            .map_err(|e| SupervisorError::Configuration(format!("Invalid key path: {}", e)))?
            .path;

        // Check if running in production mode
        let is_production = std::env::var("AOS_PRODUCTION_MODE")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false);

        // Load or generate keypair for JWT authentication
        let keypair = if !keypair_pem.is_empty() {
            // Try to load from PEM string (base64-encoded 32-byte Ed25519 seed)
            match Self::load_keypair_from_pem(keypair_pem) {
                Ok(kp) => {
                    info!("Loaded signing keypair from PEM");
                    kp
                }
                Err(e) => {
                    error!(
                        "Failed to parse keypair PEM: {}. Falling back to file-based key.",
                        e
                    );
                    Self::load_or_generate_keypair(&default_key_path, is_production)?
                }
            }
        } else {
            // No PEM provided, use file-based key management
            Self::load_or_generate_keypair(&default_key_path, is_production)?
        };

        let auth_service = Arc::new(AuthService::new(keypair, config.auth.token_ttl_hours));
        let health_monitor = Arc::new(HealthMonitor::new(
            config.monitoring.health_check_interval_seconds,
        ));

        let supervisor = Self {
            config,
            auth_service,
            health_monitor,
            services: Arc::new(RwLock::new(HashMap::new())),
        };

        // Initialize services from config
        supervisor.init_services().await?;

        // Start health monitoring
        supervisor.health_monitor.start_monitoring();
        supervisor.start_autonomous_remediation_loop();

        Ok(supervisor)
    }

    /// Initialize services from configuration
    async fn init_services(&self) -> Result<()> {
        let mut services = self.services.write().await;

        for (service_id, service_config) in &self.config.services {
            let managed_service = Arc::new(ManagedService::new(service_config.clone()));
            services.insert(service_id.clone(), managed_service.clone());

            // Register health check
            if service_config.health_check.enabled {
                self.health_monitor
                    .register_check(
                        service_id.clone(),
                        Box::new(ManagedServiceHealthCheck(managed_service.clone())),
                    )
                    .await;
            }

            info!("Initialized service: {}", service_id);
        }

        Ok(())
    }

    fn start_autonomous_remediation_loop(&self) {
        let services = Arc::clone(&self.services);
        let health_monitor = Arc::clone(&self.health_monitor);
        let service_configs = self.config.services.clone();
        let interval_seconds = self.config.monitoring.health_check_interval_seconds.max(1);

        tokio::spawn(async move {
            let mut ticker =
                tokio::time::interval(std::time::Duration::from_secs(interval_seconds));
            let mut remediation_state: HashMap<String, ServiceRemediationState> = HashMap::new();

            loop {
                ticker.tick().await;

                let health_statuses = health_monitor.get_all_statuses().await;

                for (service_id, service_config) in &service_configs {
                    let health_result = health_statuses
                        .get(service_id)
                        .cloned()
                        .unwrap_or(HealthResult::Unknown);

                    match health_result {
                        HealthResult::Healthy => {
                            remediation_state.remove(service_id);
                            continue;
                        }
                        HealthResult::Unhealthy(_) => {}
                        HealthResult::Unknown => continue,
                    }

                    let service = {
                        let guard = services.read().await;
                        guard.get(service_id).cloned()
                    };

                    let Some(service) = service else {
                        continue;
                    };

                    let runtime_status = service.status().await;
                    let restart_policy = &service_config.restart_policy;
                    if !should_attempt_auto_restart(restart_policy, &runtime_status.state) {
                        continue;
                    }

                    let now = Instant::now();
                    let entry = remediation_state.entry(service_id.clone()).or_default();
                    entry.prune_window(now, restart_policy.window_duration());

                    if let Some(cooldown_until) = entry.cooldown_until {
                        if now < cooldown_until {
                            debug!(
                                service_id = %service_id,
                                cooldown_remaining_secs = cooldown_until.duration_since(now).as_secs(),
                                "Autonomous remediation cooldown active; skipping restart"
                            );
                            continue;
                        }
                        entry.cooldown_until = None;
                    }

                    let max_attempts = restart_policy.max_attempts_in_window();
                    if entry.restart_attempts.len() as u32 >= max_attempts {
                        let cooldown = restart_policy.escalation_cooldown();
                        entry.cooldown_until = Some(now + cooldown);
                        error!(
                            service_id = %service_id,
                            attempts_in_window = entry.restart_attempts.len(),
                            max_attempts = max_attempts,
                            window_seconds = restart_policy.window_duration().as_secs(),
                            cooldown_seconds = cooldown.as_secs(),
                            "Restart budget exhausted; escalating unhealthy service"
                        );
                        continue;
                    }

                    let attempt_number = entry.restart_attempts.len() as u32 + 1;
                    info!(
                        service_id = %service_id,
                        attempt = attempt_number,
                        max_attempts = max_attempts,
                        "Autonomous remediation restart requested"
                    );

                    let restart_result = service.restart().await;
                    entry.restart_attempts.push_back(now);
                    let cooldown = restart_policy.cooldown_for_attempt(attempt_number);
                    entry.cooldown_until = Some(now + cooldown);

                    match restart_result {
                        Ok(()) => {
                            info!(
                                service_id = %service_id,
                                attempt = attempt_number,
                                cooldown_seconds = cooldown.as_secs(),
                                "Autonomous remediation restart succeeded"
                            );
                        }
                        Err(e) => {
                            warn!(
                                service_id = %service_id,
                                attempt = attempt_number,
                                cooldown_seconds = cooldown.as_secs(),
                                error = %e,
                                "Autonomous remediation restart failed"
                            );
                        }
                    }
                }
            }
        });
    }

    /// Get authentication service
    pub fn auth_service(&self) -> Arc<AuthService> {
        Arc::clone(&self.auth_service)
    }

    /// Get health monitor
    pub fn health_monitor(&self) -> Arc<HealthMonitor> {
        Arc::clone(&self.health_monitor)
    }

    /// Get all services
    pub async fn get_services(&self) -> Vec<ServiceStatus> {
        let services = self.services.read().await;
        let mut statuses = Vec::new();

        for service in services.values() {
            statuses.push(service.status().await);
        }

        // Sort by startup order
        statuses.sort_by_key(|s| {
            self.config
                .services
                .get(&s.id)
                .map(|config| config.startup_order)
                .unwrap_or(999)
        });

        statuses
    }

    /// Get a specific service
    pub async fn get_service(&self, service_id: &str) -> Result<ServiceStatus> {
        let services = self.services.read().await;
        if let Some(service) = services.get(service_id) {
            Ok(service.status().await)
        } else {
            Err(SupervisorError::ServiceNotFound(service_id.to_string()))
        }
    }

    /// Start a service
    pub async fn start_service(&self, service_id: &str) -> Result<String> {
        let services = self.services.read().await;
        if let Some(service) = services.get(service_id) {
            service.start().await?;
            Ok(format!("Service {} started successfully", service_id))
        } else {
            Err(SupervisorError::ServiceNotFound(service_id.to_string()))
        }
    }

    /// Stop a service
    pub async fn stop_service(&self, service_id: &str) -> Result<String> {
        let services = self.services.read().await;
        if let Some(service) = services.get(service_id) {
            service.stop().await?;
            Ok(format!("Service {} stopped successfully", service_id))
        } else {
            Err(SupervisorError::ServiceNotFound(service_id.to_string()))
        }
    }

    /// Restart a service
    pub async fn restart_service(&self, service_id: &str) -> Result<String> {
        let services = self.services.read().await;
        if let Some(service) = services.get(service_id) {
            service.restart().await?;
            Ok(format!("Service {} restarted successfully", service_id))
        } else {
            Err(SupervisorError::ServiceNotFound(service_id.to_string()))
        }
    }

    /// Start all essential services
    pub async fn start_essential_services(&self) -> Result<Vec<String>> {
        let services = self.services.read().await;
        let mut results = Vec::new();
        let mut errors = Vec::new();

        // Sort by startup order
        let mut essential_services: Vec<_> = services
            .values()
            .filter(|service| {
                if let Some(config) = self.config.services.get(service.id()) {
                    config.essential
                } else {
                    false
                }
            })
            .collect();

        essential_services.sort_by_key(|service| {
            self.config
                .services
                .get(service.id())
                .map(|config| config.startup_order)
                .unwrap_or(999)
        });

        for service in essential_services {
            let service_id = service.id();
            match service.start().await {
                Ok(_) => results.push(format!("{}: started", service_id)),
                Err(e) => {
                    let error_msg = format!("{}: failed - {}", service_id, e);
                    errors.push(error_msg.clone());
                    results.push(error_msg);
                }
            }
        }

        if errors.is_empty() {
            Ok(results)
        } else {
            Err(SupervisorError::ServiceOperation(format!(
                "Some services failed to start: {}",
                errors.join(", ")
            )))
        }
    }

    /// Stop all essential services
    pub async fn stop_essential_services(&self) -> Result<Vec<String>> {
        let services = self.services.read().await;
        let mut results = Vec::new();

        // Sort by reverse startup order for shutdown
        let mut essential_services: Vec<_> = services
            .values()
            .filter(|service| {
                if let Some(config) = self.config.services.get(service.id()) {
                    config.essential
                } else {
                    false
                }
            })
            .collect();

        essential_services.sort_by_key(|service| {
            self.config
                .services
                .get(service.id())
                .map(|config| -config.startup_order) // Negative for reverse order
                .unwrap_or(-999)
        });

        for service in essential_services {
            let service_id = service.id();
            match service.stop().await {
                Ok(_) => results.push(format!("{}: stopped", service_id)),
                Err(e) => results.push(format!("{}: failed - {}", service_id, e)),
            }
        }

        Ok(results)
    }

    /// Get health status
    pub async fn get_health_status(&self) -> Result<crate::health::HealthResponse> {
        Ok(crate::health::HealthResponse::from_monitor(&self.health_monitor).await)
    }

    /// Shutdown the supervisor
    pub async fn shutdown(&self) -> Result<()> {
        info!("Shutting down service supervisor...");

        // Stop all services
        let services = self.services.read().await;
        for (service_id, service) in services.iter() {
            if let Err(e) = service.stop().await {
                error!("Failed to stop service {}: {}", service_id, e);
            }
        }

        info!("Service supervisor shutdown complete");
        Ok(())
    }

    /// Load keypair from PEM string.
    ///
    /// # Expected Format
    /// Base64-encoded 32-byte Ed25519 seed, optionally wrapped in PEM headers:
    /// ```text
    /// -----BEGIN ED25519 PRIVATE KEY-----
    /// <base64 encoded 32 bytes>
    /// -----END ED25519 PRIVATE KEY-----
    /// ```
    /// Or raw base64 without headers.
    ///
    /// **Note:** This is NOT standard PKCS#8 format. The expected input is either:
    /// - Raw base64 of exactly 32 bytes (Ed25519 seed)
    /// - The same wrapped in simple PEM headers
    ///
    /// To generate a compatible key, use `adapteros_crypto::generate_signing_key()`
    /// or base64-encode a 32-byte random seed directly.
    fn load_keypair_from_pem(pem: &str) -> Result<adapteros_crypto::Keypair> {
        use base64::Engine;

        // Parse PEM-like format: extract base64 content between headers
        let content = pem
            .replace("-----BEGIN PRIVATE KEY-----", "")
            .replace("-----END PRIVATE KEY-----", "")
            .replace("-----BEGIN ED25519 PRIVATE KEY-----", "")
            .replace("-----END ED25519 PRIVATE KEY-----", "")
            .replace(['\n', '\r', ' '], "");

        // If no headers found, assume raw base64
        let key_bytes = base64::engine::general_purpose::STANDARD
            .decode(&content)
            .map_err(|e| SupervisorError::Configuration(format!("Invalid PEM base64: {}", e)))?;

        if key_bytes.len() != 32 {
            return Err(SupervisorError::Configuration(format!(
                "Invalid key length: {} (expected 32 bytes for Ed25519 seed)",
                key_bytes.len()
            )));
        }

        let mut key_array = [0u8; 32];
        key_array.copy_from_slice(&key_bytes);

        Ok(adapteros_crypto::Keypair::from_bytes(&key_array))
    }

    /// Load keypair from file or generate a new one with self-healing
    fn load_or_generate_keypair(
        key_path: &Path,
        is_production: bool,
    ) -> Result<adapteros_crypto::Keypair> {
        if key_path.exists() {
            // Load existing key
            match adapteros_crypto::load_signing_key(key_path) {
                Ok(keypair) => {
                    info!(
                        key_path = %key_path.display(),
                        "Loaded supervisor signing keypair from file"
                    );
                    return Ok(keypair);
                }
                Err(e) => {
                    error!(
                        key_path = %key_path.display(),
                        error = %e,
                        "Failed to load existing keypair, will generate new one"
                    );
                }
            }
        }

        // Key doesn't exist or failed to load - generate new one
        if is_production {
            // CRITICAL: In production, generating a new keypair breaks existing tokens
            warn!(
                "⚠️ PRODUCTION WARNING: Generating new supervisor signing keypair. \
                 This will invalidate all existing supervisor JWT tokens! \
                 Consider using SUPERVISOR_KEYPAIR_PEM environment variable for persistent keys."
            );
        } else {
            info!(
                key_path = %key_path.display(),
                "Generating new supervisor signing keypair (development mode)"
            );
        }

        // Generate and save the keypair
        let keypair = adapteros_crypto::generate_signing_key(key_path).map_err(|e| {
            SupervisorError::Internal(format!(
                "Failed to generate signing keypair at {}: {}",
                key_path.display(),
                e
            ))
        })?;

        info!(
            key_path = %key_path.display(),
            public_key = %hex::encode(keypair.public_key().to_bytes()),
            "Generated and saved new supervisor signing keypair"
        );

        Ok(keypair)
    }
}
