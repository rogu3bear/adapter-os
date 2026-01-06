//! Experimental mlx-rs backend implementation for FusedKernels.
//! This is a best-effort fallback when MLX FFI is unavailable.

use adapteros_core::{derive_seed, AosError, B3Hash, Result};
use adapteros_lora_kernel_api::{
    attestation::{
        BackendType, DeterminismLevel, DeterminismReport, FloatingPointMode, RngSeedingMethod,
    },
    BackendHealth, BackendMetrics, FusedKernels, IoBuffers, RouterRing,
};
use parking_lot::RwLock;
use std::sync::Arc;

use crate::backend::{BackendHealth as MlxBackendHealth, PerformanceMetrics};
use crate::{mlx_runtime_init_rs, mlx_runtime_is_initialized_rs, mlx_set_seed_from_bytes_rs};
use crate::{MLXResilienceConfig, MlxRsModel};

/// Experimental mlx-rs backend (no LoRA support yet).
pub struct MlxRsBackend {
    model: Arc<MlxRsModel>,
    device: String,
    resilience_config: MLXResilienceConfig,
    health_status: Arc<RwLock<MlxBackendHealth>>,
    performance_metrics: Arc<RwLock<PerformanceMetrics>>,
    manifest_hash: Option<B3Hash>,
}

impl MlxRsBackend {
    /// Create a new mlx-rs backend with a loaded model.
    pub fn new(model: MlxRsModel) -> Result<Self> {
        Self::new_internal(Arc::new(model), MLXResilienceConfig::default(), None)
    }

    /// Create a new mlx-rs backend with HKDF seeding from manifest hash.
    pub fn with_manifest_hash(model: MlxRsModel, manifest_hash: B3Hash) -> Result<Self> {
        Self::with_manifest_hash_arc(
            Arc::new(model),
            manifest_hash,
            MLXResilienceConfig::default(),
        )
    }

    /// Create a new mlx-rs backend with a shared model and custom resilience config.
    pub fn with_manifest_hash_arc(
        model: Arc<MlxRsModel>,
        manifest_hash: B3Hash,
        config: MLXResilienceConfig,
    ) -> Result<Self> {
        let seed = derive_seed(&manifest_hash, "mlx");
        mlx_set_seed_from_bytes_rs(&seed)?;
        Self::new_internal(model, config, Some(manifest_hash))
    }

    fn new_internal(
        model: Arc<MlxRsModel>,
        config: MLXResilienceConfig,
        manifest_hash: Option<B3Hash>,
    ) -> Result<Self> {
        if !mlx_runtime_is_initialized_rs() {
            mlx_runtime_init_rs().map_err(|e| {
                AosError::Config(format!("Failed to initialize mlx-rs runtime: {}", e))
            })?;
        }

        Ok(Self {
            model,
            device: "MLX (mlx-rs experimental)".to_string(),
            resilience_config: config,
            health_status: Arc::new(RwLock::new(MlxBackendHealth::default())),
            performance_metrics: Arc::new(RwLock::new(PerformanceMetrics::default())),
            manifest_hash,
        })
    }

    fn record_success(&self) {
        if let Some(mut health) = self.health_status.try_write() {
            health.successful_requests += 1;
            health.current_failure_streak = 0;
            health.last_failure = None;
        }
    }

    fn record_failure(&self) {
        if let Some(mut health) = self.health_status.try_write() {
            health.failed_requests += 1;
            health.current_failure_streak += 1;
            health.last_failure = Some(std::time::Instant::now());

            if health.current_failure_streak >= self.resilience_config.max_consecutive_failures {
                health.operational = false;
            }
        }
    }
}

impl FusedKernels for MlxRsBackend {
    fn load(&mut self, _plan_bytes: &[u8]) -> Result<()> {
        tracing::info!("MLX (mlx-rs) backend ready (no plan loading required)");
        Ok(())
    }

    fn run_step(&mut self, ring: &RouterRing, io: &mut IoBuffers) -> Result<()> {
        {
            let mut health = self.health_status.write();
            health.total_requests += 1;
        }

        if io.input_ids.is_empty() {
            self.record_failure();
            return Err(AosError::Validation(
                "Input token IDs cannot be empty".to_string(),
            ));
        }

        if ring.k > 0 {
            tracing::warn!(
                adapters_requested = ring.k,
                "mlx-rs backend does not support LoRA adapters; ignoring RouterRing"
            );
        }

        let inference_start = std::time::Instant::now();
        let logits = match self.model.forward(&io.input_ids, io.position as usize) {
            Ok(logits) => logits,
            Err(e) => {
                self.record_failure();
                return Err(e);
            }
        };

        if logits.is_empty() {
            self.record_failure();
            return Err(AosError::Mlx("Model returned empty logits".to_string()));
        }

        let output_len = logits.len().min(io.output_logits.len());
        if output_len == 0 {
            self.record_failure();
            return Err(AosError::Mlx(
                "Output buffer size mismatch - cannot copy logits".to_string(),
            ));
        }
        io.output_logits[..output_len].copy_from_slice(&logits[..output_len]);
        io.position += 1;

        let inference_time = inference_start.elapsed().as_millis() as u64;
        {
            let mut metrics = self.performance_metrics.write();
            metrics.total_requests += 1;
            metrics.total_inference_time_ms += inference_time;

            if metrics.total_requests > 0 {
                metrics.average_latency_ms =
                    metrics.total_inference_time_ms as f32 / metrics.total_requests as f32;
            }

            let logits_memory =
                (logits.len() * std::mem::size_of::<f32>()) as f32 / (1024.0 * 1024.0);
            if logits_memory > metrics.peak_memory_usage_mb {
                metrics.peak_memory_usage_mb = logits_memory;
            }
        }

        self.record_success();
        Ok(())
    }

    fn device_name(&self) -> &str {
        &self.device
    }

    fn attest_determinism(&self) -> Result<DeterminismReport> {
        let seeded = self.manifest_hash.is_some();
        let rng_method = if seeded {
            RngSeedingMethod::HkdfSeeded
        } else {
            RngSeedingMethod::SystemEntropy
        };

        let report = DeterminismReport {
            backend_type: BackendType::MLX,
            metallib_hash: self.manifest_hash,
            metallib_verified: false,
            manifest: None,
            rng_seed_method: rng_method,
            floating_point_mode: FloatingPointMode::Deterministic,
            determinism_level: if seeded {
                DeterminismLevel::BitExact
            } else {
                DeterminismLevel::None
            },
            compiler_flags: vec![],
            deterministic: seeded,
            runtime_version: Some("mlx-rs".to_string()),
            device_id: Some(self.device.clone()),
        };

        tracing::info!(
            deterministic = report.deterministic,
            rng_method = ?report.rng_seed_method,
            has_manifest_hash = self.manifest_hash.is_some(),
            "MLX (mlx-rs) determinism attestation"
        );

        Ok(report)
    }

    fn load_adapter(&mut self, _id: u16, _weights: &[u8]) -> Result<()> {
        Err(AosError::Kernel(
            "mlx-rs backend does not support LoRA adapters yet".to_string(),
        ))
    }

    fn unload_adapter(&mut self, _id: u16) -> Result<()> {
        Err(AosError::Kernel(
            "mlx-rs backend does not support LoRA adapters yet".to_string(),
        ))
    }

    fn get_metrics(&self) -> BackendMetrics {
        let metrics = self.performance_metrics.read();
        let health = self.health_status.read();

        BackendMetrics {
            total_operations: health.total_requests,
            successful_operations: health.successful_requests,
            failed_operations: health.failed_requests,
            avg_latency: std::time::Duration::from_millis(metrics.average_latency_ms as u64),
            memory_usage_bytes: (metrics.peak_memory_usage_mb * 1024.0 * 1024.0) as u64,
        }
    }

    fn health_check(&self) -> Result<BackendHealth> {
        let health = self.health_status.read();

        if !health.operational {
            return Ok(BackendHealth::Failed {
                reason: "Backend marked non-operational after consecutive failures".to_string(),
                recoverable: true,
            });
        }

        if health.current_failure_streak > 0 {
            return Ok(BackendHealth::Degraded {
                reason: format!(
                    "Recent failures detected: {} consecutive",
                    health.current_failure_streak
                ),
            });
        }

        Ok(BackendHealth::Healthy)
    }
}
