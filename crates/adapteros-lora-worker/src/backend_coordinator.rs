//! Backend coordination for hybrid execution and runtime switching
//!
//! This module implements:
//! - Multi-backend pipeline coordination
//! - Runtime backend switching on failure
//! - Tensor sharing between Metal and CoreML
//! - Health monitoring and telemetry

use adapteros_core::{AosError, Result};
use adapteros_lora_kernel_api::{
    BackendHealth, BackendMetrics, FusedKernels, IoBuffers, RouterRing,
};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{error, info, warn};

use crate::backend_factory::{
    create_backend, detect_capabilities, BackendCapabilities, BackendChoice, BackendStrategy,
};

/// Backend coordinator for managing multiple backends and runtime switching
pub struct BackendCoordinator {
    /// Primary backend
    primary: Arc<RwLock<Box<dyn FusedKernels>>>,
    /// Fallback backend (optional)
    fallback: Option<Arc<RwLock<Box<dyn FusedKernels>>>>,
    /// Backend health status
    primary_health: Arc<RwLock<BackendHealth>>,
    fallback_health: Option<Arc<RwLock<BackendHealth>>>,
    /// Health check interval
    health_check_interval: Duration,
    /// Last health check time
    last_health_check: Arc<RwLock<Instant>>,
    /// Coordinator metrics
    metrics: Arc<RwLock<CoordinatorMetrics>>,
    /// Capabilities
    capabilities: BackendCapabilities,
}

/// Coordinator metrics for telemetry
#[derive(Debug, Clone, Default)]
pub struct CoordinatorMetrics {
    /// Total operations executed
    pub total_operations: u64,
    /// Operations on primary backend
    pub primary_operations: u64,
    /// Operations on fallback backend
    pub fallback_operations: u64,
    /// Number of backend switches
    pub backend_switches: u64,
    /// Number of health check failures
    pub health_check_failures: u64,
    /// Average operation latency (microseconds)
    pub avg_latency_us: f32,
}

impl BackendCoordinator {
    /// Create a new backend coordinator with automatic fallback
    ///
    /// # Arguments
    /// * `strategy` - Backend selection strategy
    /// * `enable_fallback` - Whether to create fallback backend
    /// * `model_size_bytes` - Optional model size for capacity checks
    ///
    /// # Examples
    /// ```ignore
    /// use adapteros_lora_worker::backend_coordinator::BackendCoordinator;
    /// use adapteros_lora_worker::backend_factory::BackendStrategy;
    ///
    /// // Async context required for await
    /// let coordinator = BackendCoordinator::new(
    ///     BackendStrategy::MetalWithCoreMLFallback,
    ///     true,
    ///     Some(8_000_000_000)
    /// ).await?;
    /// ```
    pub async fn new(
        strategy: BackendStrategy,
        enable_fallback: bool,
        model_size_bytes: Option<usize>,
    ) -> Result<Self> {
        let capabilities = detect_capabilities();

        // Select primary backend
        let primary_choice = strategy.select_backend(&capabilities, model_size_bytes)?;
        let primary = Arc::new(RwLock::new(create_backend(primary_choice.clone())?));

        info!(
            primary_backend = ?primary_choice,
            "Created primary backend"
        );

        // Create fallback backend if enabled
        let (fallback, fallback_health) = if enable_fallback {
            match BackendCoordinator::select_fallback_backend(&primary_choice, &capabilities) {
                Ok(fallback_choice) => match create_backend(fallback_choice.clone()) {
                    Ok(fallback_backend) => {
                        info!(
                            fallback_backend = ?fallback_choice,
                            "Created fallback backend"
                        );
                        (
                            Some(Arc::new(RwLock::new(fallback_backend))),
                            Some(Arc::new(RwLock::new(BackendHealth::Healthy))),
                        )
                    }
                    Err(e) => {
                        warn!(
                            error = %e,
                            "Failed to create fallback backend, continuing without fallback"
                        );
                        (None, None)
                    }
                },
                Err(e) => {
                    warn!(
                        error = %e,
                        "No suitable fallback backend available"
                    );
                    (None, None)
                }
            }
        } else {
            (None, None)
        };

        Ok(Self {
            primary,
            fallback,
            primary_health: Arc::new(RwLock::new(BackendHealth::Healthy)),
            fallback_health,
            health_check_interval: Duration::from_secs(30),
            last_health_check: Arc::new(RwLock::new(Instant::now())),
            metrics: Arc::new(RwLock::new(CoordinatorMetrics::default())),
            capabilities,
        })
    }

    /// Select appropriate fallback backend based on primary
    fn select_fallback_backend(
        primary: &BackendChoice,
        capabilities: &BackendCapabilities,
    ) -> Result<BackendChoice> {
        match primary {
            BackendChoice::Metal => {
                // Metal primary -> CoreML fallback
                if capabilities.has_ane {
                    Ok(BackendChoice::CoreML)
                } else {
                    Err(AosError::Config(
                        "No suitable fallback for Metal".to_string(),
                    ))
                }
            }
            BackendChoice::CoreML => {
                // CoreML primary -> Metal fallback
                if capabilities.has_metal {
                    Ok(BackendChoice::Metal)
                } else {
                    Err(AosError::Config(
                        "No suitable fallback for CoreML".to_string(),
                    ))
                }
            }
            BackendChoice::Mlx => {
                // MLX primary -> Metal or CoreML fallback
                if capabilities.has_metal {
                    Ok(BackendChoice::Metal)
                } else if capabilities.has_ane {
                    Ok(BackendChoice::CoreML)
                } else {
                    Err(AosError::Config("No suitable fallback for MLX".to_string()))
                }
            }
            BackendChoice::Auto => {
                // Auto should have been resolved already, fall back to Metal
                if capabilities.has_metal {
                    Ok(BackendChoice::Metal)
                } else if capabilities.has_ane {
                    Ok(BackendChoice::CoreML)
                } else {
                    Err(AosError::Config(
                        "No suitable fallback for Auto".to_string(),
                    ))
                }
            }
        }
    }

    /// Execute inference step with automatic fallback on failure
    pub async fn run_step(&self, ring: &RouterRing, io: &mut IoBuffers) -> Result<()> {
        let start = Instant::now();

        // Check if health check is needed
        self.periodic_health_check().await?;

        // Try primary backend first
        let primary_health = self.primary_health.read().await;
        let use_primary = matches!(*primary_health, BackendHealth::Healthy);
        drop(primary_health);

        if use_primary {
            let mut primary = self.primary.write().await;
            match primary.run_step(ring, io) {
                Ok(_) => {
                    // Success on primary
                    let mut metrics = self.metrics.write().await;
                    metrics.total_operations += 1;
                    metrics.primary_operations += 1;
                    metrics.avg_latency_us = (metrics.avg_latency_us
                        * (metrics.total_operations - 1) as f32
                        + start.elapsed().as_micros() as f32)
                        / metrics.total_operations as f32;
                    Ok(())
                }
                Err(e) => {
                    // Primary failed, mark as degraded and try fallback
                    warn!(error = %e, "Primary backend failed, attempting fallback");
                    *self.primary_health.write().await = BackendHealth::Degraded {
                        reason: format!("Execution failed: {}", e),
                    };

                    if let Some(ref fallback) = self.fallback {
                        let mut fallback_backend = fallback.write().await;
                        match fallback_backend.run_step(ring, io) {
                            Ok(_) => {
                                info!("Successfully failed over to fallback backend");
                                let mut metrics = self.metrics.write().await;
                                metrics.total_operations += 1;
                                metrics.fallback_operations += 1;
                                metrics.backend_switches += 1;
                                metrics.avg_latency_us = (metrics.avg_latency_us
                                    * (metrics.total_operations - 1) as f32
                                    + start.elapsed().as_micros() as f32)
                                    / metrics.total_operations as f32;
                                Ok(())
                            }
                            Err(fallback_err) => {
                                error!(error = %fallback_err, "Fallback backend also failed");
                                Err(AosError::Kernel(format!(
                                    "Both primary and fallback backends failed: primary={}, fallback={}",
                                    e, fallback_err
                                )))
                            }
                        }
                    } else {
                        Err(e)
                    }
                }
            }
        } else if let Some(ref fallback) = self.fallback {
            // Primary unhealthy, use fallback directly
            let mut fallback_backend = fallback.write().await;
            match fallback_backend.run_step(ring, io) {
                Ok(_) => {
                    let mut metrics = self.metrics.write().await;
                    metrics.total_operations += 1;
                    metrics.fallback_operations += 1;
                    metrics.avg_latency_us = (metrics.avg_latency_us
                        * (metrics.total_operations - 1) as f32
                        + start.elapsed().as_micros() as f32)
                        / metrics.total_operations as f32;
                    Ok(())
                }
                Err(e) => Err(e),
            }
        } else {
            Err(AosError::Kernel(
                "Primary backend unhealthy and no fallback available".to_string(),
            ))
        }
    }

    /// Perform periodic health checks on backends
    async fn periodic_health_check(&self) -> Result<()> {
        let mut last_check = self.last_health_check.write().await;
        if last_check.elapsed() < self.health_check_interval {
            return Ok(());
        }

        *last_check = Instant::now();
        drop(last_check);

        // Check primary backend health
        let primary = self.primary.read().await;
        match primary.health_check() {
            Ok(health) => {
                let mut guard: tokio::sync::RwLockWriteGuard<'_, BackendHealth> =
                    self.primary_health.write().await;
                *guard = health.clone();
                if !matches!(health, BackendHealth::Healthy) {
                    warn!(health = ?health, "Primary backend health check failed");
                    let mut metrics = self.metrics.write().await;
                    metrics.health_check_failures += 1;
                }
            }
            Err(e) => {
                error!(error = %e, "Primary backend health check error");
                *self.primary_health.write().await = BackendHealth::Failed {
                    reason: format!("Health check error: {}", e),
                    recoverable: true,
                };
                let mut metrics = self.metrics.write().await;
                metrics.health_check_failures += 1;
            }
        }
        drop(primary);

        // Check fallback backend health if present
        if let Some(ref fallback) = self.fallback {
            let fallback_backend = fallback.read().await;
            if let Some(ref fallback_health_arc) = self.fallback_health {
                match fallback_backend.health_check() {
                    Ok(health) => {
                        let mut guard: tokio::sync::RwLockWriteGuard<'_, BackendHealth> =
                            fallback_health_arc.write().await;
                        *guard = health.clone();
                        if !matches!(health, BackendHealth::Healthy) {
                            warn!(health = ?health, "Fallback backend health check failed");
                        }
                    }
                    Err(e) => {
                        error!(error = %e, "Fallback backend health check error");
                        *fallback_health_arc.write().await = BackendHealth::Failed {
                            reason: format!("Health check error: {}", e),
                            recoverable: true,
                        };
                    }
                }
            }
        }

        Ok(())
    }

    /// Get coordinator metrics
    pub async fn get_metrics(&self) -> CoordinatorMetrics {
        self.metrics.read().await.clone()
    }

    /// Get primary backend metrics
    pub async fn get_primary_metrics(&self) -> BackendMetrics {
        self.primary.read().await.get_metrics()
    }

    /// Get fallback backend metrics
    pub async fn get_fallback_metrics(&self) -> Option<BackendMetrics> {
        if let Some(ref fallback) = self.fallback {
            Some(fallback.read().await.get_metrics())
        } else {
            None
        }
    }

    /// Get backend capabilities
    pub fn capabilities(&self) -> &BackendCapabilities {
        &self.capabilities
    }

    /// Force switch to fallback backend
    pub async fn force_switch_to_fallback(&self) -> Result<()> {
        if self.fallback.is_some() {
            *self.primary_health.write().await = BackendHealth::Degraded {
                reason: "Manual switch to fallback".to_string(),
            };
            let mut metrics = self.metrics.write().await;
            metrics.backend_switches += 1;
            info!("Manually switched to fallback backend");
            Ok(())
        } else {
            Err(AosError::Config(
                "No fallback backend available".to_string(),
            ))
        }
    }

    /// Reset primary backend health (attempt recovery)
    pub async fn reset_primary_health(&self) {
        *self.primary_health.write().await = BackendHealth::Healthy;
        info!("Reset primary backend health to Healthy");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Set up test environment with manifest verification disabled
    fn setup_test_env() {
        // Skip manifest verification for tests (placeholder signing keys)
        std::env::set_var("AOS_SKIP_MANIFEST_VERIFY", "1");
    }

    #[tokio::test]
    #[cfg(target_os = "macos")]
    async fn test_coordinator_creation() {
        setup_test_env();
        let coordinator = BackendCoordinator::new(BackendStrategy::MetalOnly, false, None).await;

        assert!(
            coordinator.is_ok(),
            "Coordinator creation failed: {:?}",
            coordinator.err()
        );
    }

    #[tokio::test]
    #[cfg(target_os = "macos")]
    async fn test_coordinator_metrics() {
        setup_test_env();
        let coordinator = BackendCoordinator::new(BackendStrategy::MetalOnly, false, None)
            .await
            .expect("Failed to create coordinator");

        let metrics = coordinator.get_metrics().await;
        assert_eq!(metrics.total_operations, 0);
        assert_eq!(metrics.backend_switches, 0);
    }
}
