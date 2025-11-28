//! Main service supervisor implementation

use crate::auth::AuthService;
use crate::config::SupervisorConfig;
use crate::error::{Result, SupervisorError};
use crate::health::{HealthCheck, HealthMonitor, HealthResult};
use crate::service::{ManagedService, ServiceStatus};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

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
        // Load or generate keypair for JWT
        let keypair = if !keypair_pem.is_empty() {
            // Try to load from PEM if provided
            warn!("PEM loading not implemented yet, generating new keypair");
            adapteros_crypto::Keypair::generate()
        } else {
            // Generate a new keypair for development
            warn!("No keypair provided, generating new one for development");
            adapteros_crypto::Keypair::generate()
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
        supervisor.initialize_services().await?;

        // Start health monitoring
        supervisor.health_monitor.start_monitoring();

        Ok(supervisor)
    }

    /// Initialize services from configuration
    async fn initialize_services(&self) -> Result<()> {
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
                .map(|config| -(config.startup_order as i32)) // Negative for reverse order
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

    /// Get service logs
    pub async fn get_service_logs(&self, service_id: &str, lines: usize) -> Result<Vec<String>> {
        let services = self.services.read().await;
        if let Some(service) = services.get(service_id) {
            service.read_logs(lines).await
        } else {
            Err(SupervisorError::ServiceNotFound(service_id.to_string()))
        }
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
}
