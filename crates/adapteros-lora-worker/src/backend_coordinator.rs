//! Backend coordination for hybrid execution and runtime switching
//!
//! This module implements:
//! - Multi-backend pipeline coordination
//! - Runtime backend switching on failure
//! - Tensor sharing between Metal and CoreML
//! - Health monitoring and telemetry

use crate::resource_monitor::ResourceMonitor;
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
    /// Resource monitor for GPU availability tracking (optional)
    resource_monitor: Option<Arc<ResourceMonitor>>,
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
        let primary = Arc::new(RwLock::new(create_backend(primary_choice)?));

        info!(
            target: "inference.backend",
            selected = ?primary_choice,
            reason = "strategy_primary",
            has_metal = capabilities.has_metal,
            has_coreml = capabilities.has_coreml,
            has_ane = capabilities.has_ane,
            "Backend selected for inference"
        );

        info!(
            primary_backend = ?primary_choice,
            "Created primary backend"
        );

        // Create fallback backend if enabled
        let (fallback, fallback_health) = if enable_fallback {
            match BackendCoordinator::select_fallback_backend(&primary_choice, &capabilities) {
                Ok(fallback_choice) => match create_backend(fallback_choice) {
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
            resource_monitor: None,
        })
    }

    /// Attach a resource monitor for GPU availability tracking
    pub fn with_resource_monitor(mut self, monitor: Arc<ResourceMonitor>) -> Self {
        self.resource_monitor = Some(monitor);
        self
    }

    /// Check if an error indicates GPU unavailability
    ///
    /// Returns true if the error is likely caused by GPU hardware issues
    /// that may be transient or require fallback.
    pub fn is_gpu_error(error: &AosError) -> bool {
        match error {
            AosError::GpuUnavailable { .. } => true,
            AosError::Mtl(msg) | AosError::Kernel(msg) | AosError::CoreML(msg) => {
                let lower = msg.to_lowercase();
                lower.contains("gpu")
                    || lower.contains("metal")
                    || lower.contains("device")
                    || lower.contains("timeout")
                    || lower.contains("not responding")
                    || lower.contains("command buffer")
                    || lower.contains("execution error")
            }
            _ => false,
        }
    }

    /// Handle GPU error by marking resource monitor and attempting fallback
    async fn handle_gpu_error(&self, error: &AosError) -> Option<AosError> {
        // Notify resource monitor of GPU unavailability
        if let Some(ref monitor) = self.resource_monitor {
            monitor.mark_gpu_unavailable();
        }

        // Mark primary as degraded
        {
            let mut health = self.primary_health.write().await;
            let previous_state = std::mem::replace(
                &mut *health,
                BackendHealth::Degraded {
                    reason: format!("GPU unavailable: {}", error),
                },
            );
            info!(
                target: "inference.backend",
                previous = ?previous_state,
                current = "degraded",
                reason = %error,
                "Health state transition: primary backend degraded"
            );
        }
        {
            let mut degraded = self.primary_degraded.write().await;
            *degraded = true;
        }

        // Check if fallback is available
        if self.fallback.is_some() {
            warn!(
                error = %error,
                "GPU error detected, attempting fallback to alternate backend"
            );

            // Switch to fallback
            {
                let mut active = self.active_backend.write().await;
                *active = ActiveBackend::Fallback;
            }
            {
                let mut metrics = self.metrics.write().await;
                metrics.backend_switches += 1;
            }

            info!(
                target: "inference.backend",
                selected = "fallback",
                reason = "gpu_error_recovery",
                trigger_error = %error,
                "Fallback backend activated"
            );

            None // Fallback available, caller should retry
        } else {
            // No fallback available, return structured GPU error
            Some(AosError::GpuUnavailable {
                reason: error.to_string(),
                device_id: None,
                cpu_fallback_available: false,
                is_transient: true,
            })
        }
    }

    /// Mark GPU as recovered after successful operation
    pub fn mark_gpu_recovered(&self) {
        if let Some(ref monitor) = self.resource_monitor {
            monitor.mark_gpu_available();
        }
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
                // MLX primary -> CoreML (ANE efficiency) or Metal fallback
                if capabilities.has_ane && capabilities.has_coreml {
                    Ok(BackendChoice::CoreML)
                } else if capabilities.has_metal {
                    Ok(BackendChoice::Metal)
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
            BackendChoice::CPU => Err(AosError::Config(
                "CPU backend is not supported for inference fallback".to_string(),
            )),
            BackendChoice::MlxBridge => {
                // MlxBridge primary -> MLX FFI or Metal fallback
                if capabilities.has_mlx {
                    Ok(BackendChoice::Mlx)
                } else if capabilities.has_metal {
                    Ok(BackendChoice::Metal)
                } else if capabilities.has_ane {
                    Ok(BackendChoice::CoreML)
                } else {
                    Err(AosError::Config(
                        "No suitable fallback for MlxBridge".to_string(),
                    ))
                }
            }
            BackendChoice::ModelServer => {
                // ModelServer primary -> MLX (same underlying backend) or Metal fallback
                if capabilities.has_mlx {
                    Ok(BackendChoice::Mlx)
                } else if capabilities.has_metal {
                    Ok(BackendChoice::Metal)
                } else if capabilities.has_ane {
                    Ok(BackendChoice::CoreML)
                } else {
                    Err(AosError::Config(
                        "No suitable fallback for ModelServer".to_string(),
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

        // Read and release lock before acquiring write locks to avoid deadlock.
        // The active_backend value is a simple enum that we copy out.
        let active_backend = {
            let guard = self.active_backend.read().await;
            *guard
            // guard dropped here
        };

        match active_backend {
            ActiveBackend::Primary => {
                // Execute the step while holding the primary lock
                let step_result = {
                    let mut primary = self.primary.write().await;
                    primary.run_step(ring, io)
                    // primary lock dropped here before updating other state
                };

                match step_result {
                    Ok(_) => {
                        // Mark GPU as recovered on successful operation
                        self.mark_gpu_recovered();

                        // Update state after releasing the primary lock.
                        // Each lock is acquired and released independently.
                        {
                            let mut degraded = self.primary_degraded.write().await;
                            *degraded = false;
                        }
                        {
                            let mut metrics = self.metrics.write().await;
                            metrics.total_operations += 1;
                            metrics.primary_operations += 1;
                            metrics.avg_latency_us = (metrics.avg_latency_us
                                * (metrics.total_operations - 1) as f32
                                + start.elapsed().as_micros() as f32)
                                / metrics.total_operations as f32;
                        }
                        Ok(())
                    }
                    Err(e) => {
                        warn!(error = %e, "Primary backend failed");

                        // Check if this is a GPU error
                        if Self::is_gpu_error(&e) {
                            if let Some(gpu_err) = self.handle_gpu_error(&e).await {
                                return Err(gpu_err);
                            }
                            // Fallback available - return error and let caller retry
                        }

                        // Update state after releasing the primary lock.
                        // Each lock is acquired and released independently.
                        {
                            let mut health = self.primary_health.write().await;
                            *health = BackendHealth::Degraded {
                                reason: format!("Execution failed: {}", e),
                            };
                        }
                        {
                            let mut degraded = self.primary_degraded.write().await;
                            *degraded = true;
                        }
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

                // Execute the step while holding the fallback lock
                let step_result = {
                    let mut fallback_backend = fallback.write().await;
                    fallback_backend.run_step(ring, io)
                    // fallback lock dropped here before updating metrics
                };

                match step_result {
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
        // Check if health check is needed; use block scope for lock
        let should_check = {
            let mut last_check = self.last_health_check.write().await;
            if last_check.elapsed() < self.health_check_interval {
                false
            } else {
                *last_check = Instant::now();
                true
            }
            // last_check lock dropped here
        };

        if !should_check {
            return Ok(());
        }

        // Check primary backend health.
        // Get the health result first, releasing the read lock before acquiring write locks.
        let primary_health_result = {
            let primary = self.primary.read().await;
            primary.health_check()
            // primary lock dropped here
        };

        match primary_health_result {
            Ok(health) => {
                let is_healthy = matches!(health, BackendHealth::Healthy);

                // Update primary_health (single lock acquisition)
                let previous_state = {
                    let mut guard = self.primary_health.write().await;
                    let prev = guard.clone();
                    *guard = health.clone();
                    prev
                };

                // Log health state transition if changed
                if std::mem::discriminant(&previous_state) != std::mem::discriminant(&health) {
                    info!(
                        target: "inference.backend",
                        previous = ?previous_state,
                        current = ?health,
                        "Health state transition: primary backend"
                    );
                }

                // Update primary_degraded (separate lock acquisition)
                {
                    let mut degraded = self.primary_degraded.write().await;
                    *degraded = !is_healthy;
                }

                // Update metrics if unhealthy (separate lock acquisition)
                if !is_healthy {
                    warn!(health = ?health, "Primary backend health check failed");
                    let mut metrics = self.metrics.write().await;
                    metrics.health_check_failures += 1;
                }
            }
            Err(e) => {
                error!(error = %e, "Primary backend health check error");

                // Update primary_health (single lock acquisition)
                {
                    let mut guard = self.primary_health.write().await;
                    *guard = BackendHealth::Failed {
                        reason: format!("Health check error: {}", e),
                        recoverable: true,
                    };
                }

                // Update primary_degraded (separate lock acquisition)
                {
                    let mut degraded = self.primary_degraded.write().await;
                    *degraded = true;
                }

                // Update metrics (separate lock acquisition)
                {
                    let mut metrics = self.metrics.write().await;
                    metrics.health_check_failures += 1;
                }
            }
        }

        // Check fallback backend health if present.
        // Same pattern: get health result first, then release lock before writing.
        if let Some(ref fallback) = self.fallback {
            if let Some(ref fallback_health_arc) = self.fallback_health {
                let fallback_health_result = {
                    let fallback_backend = fallback.read().await;
                    fallback_backend.health_check()
                    // fallback_backend lock dropped here
                };

                match fallback_health_result {
                    Ok(health) => {
                        let previous_state = {
                            let mut guard = fallback_health_arc.write().await;
                            let prev = guard.clone();
                            *guard = health.clone();
                            prev
                        };

                        // Log health state transition if changed
                        if std::mem::discriminant(&previous_state)
                            != std::mem::discriminant(&health)
                        {
                            info!(
                                target: "inference.backend",
                                previous = ?previous_state,
                                current = ?health,
                                "Health state transition: fallback backend"
                            );
                        }

                        if !matches!(health, BackendHealth::Healthy) {
                            warn!(health = ?health, "Fallback backend health check failed");
                        }
                    }
                    Err(e) => {
                        error!(error = %e, "Fallback backend health check error");
                        let mut guard = fallback_health_arc.write().await;
                        *guard = BackendHealth::Failed {
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
        if self.fallback.is_none() {
            return Err(AosError::Config(
                "No fallback backend available".to_string(),
            ));
        }

        // Update state with separate lock acquisitions to avoid holding multiple locks.
        {
            let mut health = self.primary_health.write().await;
            *health = BackendHealth::Degraded {
                reason: "Manual switch to fallback".to_string(),
            };
        }
        {
            let mut degraded = self.primary_degraded.write().await;
            *degraded = true;
        }

        // Check current state and update active backend
        let was_fallback = {
            let active = self.active_backend.read().await;
            *active == ActiveBackend::Fallback
        };

        if !was_fallback {
            let mut metrics = self.metrics.write().await;
            metrics.backend_switches += 1;
        }

        {
            let mut active = self.active_backend.write().await;
            *active = ActiveBackend::Fallback;
        }

        info!("Manually switched to fallback backend");
        Ok(())
    }

    /// Reset primary backend health (attempt recovery)
    pub async fn reset_primary_health(&self) {
        // Update state with separate lock acquisitions to avoid holding multiple locks.
        {
            let mut health = self.primary_health.write().await;
            *health = BackendHealth::Healthy;
        }
        {
            let mut degraded = self.primary_degraded.write().await;
            *degraded = false;
        }
        {
            let mut active = self.active_backend.write().await;
            *active = ActiveBackend::Primary;
        }
        info!("Reset primary backend health to Healthy");
    }

    /// Pin backend choice before starting a request
    pub async fn prepare_for_request(&self, strict_mode: bool) {
        // Read current state before acquiring write lock on active_backend.
        // This avoids holding multiple locks simultaneously.
        let (is_degraded, is_healthy) = {
            let degraded = *self.primary_degraded.read().await;
            let health = self.primary_health.read().await;
            let healthy = matches!(*health, BackendHealth::Healthy);
            (degraded, healthy)
            // Both read locks dropped here
        };

        let use_fallback = !strict_mode && self.fallback.is_some() && (is_degraded || !is_healthy);

        let next = if use_fallback {
            ActiveBackend::Fallback
        } else {
            ActiveBackend::Primary
        };

        // Read current active state to check if we need to increment metrics
        let current_active = {
            let active = self.active_backend.read().await;
            *active
        };

        if current_active != next {
            let mut metrics = self.metrics.write().await;
            metrics.backend_switches += 1;
        }

        // Now update active_backend
        {
            let mut active = self.active_backend.write().await;
            *active = next;
        }
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
        if metal::Device::system_default().is_none() {
            eprintln!("skipping: metal backend not available");
            return;
        }

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
        if metal::Device::system_default().is_none() {
            eprintln!("skipping: metal backend not available");
            return;
        }

        // Manifest verification uses deterministic test keys - no setup needed
        let coordinator = BackendCoordinator::new(BackendStrategy::MetalOnly, false, None)
            .await
            .expect("Failed to create coordinator");

        let metrics = coordinator.get_metrics().await;
        assert_eq!(metrics.total_operations, 0);
        assert_eq!(metrics.backend_switches, 0);
    }
}
