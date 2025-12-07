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
    KernelBox,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActiveBackend {
    Primary,
    Fallback,
}

/// Backend coordinator for managing multiple backends and runtime switching
pub struct BackendCoordinator {
    /// Primary backend
    primary: Arc<RwLock<KernelBox>>,
    /// Fallback backend (optional)
    fallback: Option<Arc<RwLock<KernelBox>>>,
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
    /// Pinned backend for the current request
    active_backend: Arc<RwLock<ActiveBackend>>,
    /// Whether primary has been marked degraded
    primary_degraded: Arc<RwLock<bool>>,
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
    /// ```no_run
    /// use adapteros_lora_worker::backend_coordinator::BackendCoordinator;
    /// use adapteros_lora_worker::backend_factory::BackendStrategy;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let rt = tokio::runtime::Runtime::new()?;
    /// # rt.block_on(async {
    /// let coordinator = BackendCoordinator::new(
    ///     BackendStrategy::MetalWithCoreMLFallback,
    ///     true,
    ///     Some(8_000_000_000)
    /// ).await?;
    /// # Ok::<(), adapteros_core::AosError>(())
    /// # })?;
    /// # Ok(())
    /// # }
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
            active_backend: Arc::new(RwLock::new(ActiveBackend::Primary)),
            primary_degraded: Arc::new(RwLock::new(false)),
        })
    }

    /// Select appropriate fallback backend based on primary
    pub fn select_fallback_backend(
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

        let active_backend = *self.active_backend.read().await;

        match active_backend {
            ActiveBackend::Primary => {
                let mut primary = self.primary.write().await;
                match primary.run_step(ring, io) {
                    Ok(_) => {
                        *self.primary_degraded.write().await = false;
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
                        warn!(error = %e, "Primary backend failed");
                        *self.primary_health.write().await = BackendHealth::Degraded {
                            reason: format!("Execution failed: {}", e),
                        };
                        *self.primary_degraded.write().await = true;
                        Err(e)
                    }
                }
            }
            ActiveBackend::Fallback => {
                let Some(ref fallback) = self.fallback else {
                    return Err(AosError::Kernel(
                        "Fallback backend not available".to_string(),
                    ));
                };

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
            }
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
                *self.primary_degraded.write().await = !matches!(health, BackendHealth::Healthy);
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
                *self.primary_degraded.write().await = true;
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
            *self.primary_degraded.write().await = true;
            let mut active = self.active_backend.write().await;
            if *active != ActiveBackend::Fallback {
                let mut metrics = self.metrics.write().await;
                metrics.backend_switches += 1;
            }
            *active = ActiveBackend::Fallback;
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
        *self.primary_degraded.write().await = false;
        *self.active_backend.write().await = ActiveBackend::Primary;
        info!("Reset primary backend health to Healthy");
    }

    /// Pin backend choice before starting a request
    pub async fn prepare_for_request(&self, strict_mode: bool) {
        let mut active = self.active_backend.write().await;

        let use_fallback = !strict_mode
            && self.fallback.is_some()
            && (*self.primary_degraded.read().await
                || !matches!(*self.primary_health.read().await, BackendHealth::Healthy));

        let next = if use_fallback {
            ActiveBackend::Fallback
        } else {
            ActiveBackend::Primary
        };

        if *active != next {
            let mut metrics = self.metrics.write().await;
            metrics.backend_switches += 1;
        }

        *active = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // NOTE: No setup_test_env() function needed anymore!
    // The manifest verification now uses deterministic test keys that are:
    // 1. Generated at build time in adapteros-lora-kernel-mtl/build.rs
    // 2. Verified at runtime using the same deterministic seed in keys.rs
    // This eliminates the need for AOS_SKIP_MANIFEST_VERIFY environment variable hack.

    #[tokio::test]
    #[cfg(target_os = "macos")]
    async fn test_coordinator_creation() {
        // Manifest verification uses deterministic test keys - no setup needed
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
        // Manifest verification uses deterministic test keys - no setup needed
        let coordinator = BackendCoordinator::new(BackendStrategy::MetalOnly, false, None)
            .await
            .expect("Failed to create coordinator");

        let metrics = coordinator.get_metrics().await;
        assert_eq!(metrics.total_operations, 0);
        assert_eq!(metrics.backend_switches, 0);
    }
}
