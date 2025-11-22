//! MLX FFI backend implementation for FusedKernels trait

use crate::{LoRAAdapter, MLXFFIModel, MLXMemoryPool, MLXMemoryPoolConfig};
use adapteros_core::{derive_seed, B3Hash, Result};
use adapteros_lora_kernel_api::{FusedKernels, IoBuffers, RouterRing};
use parking_lot::RwLock;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Resilience configuration for MLX backend
#[derive(Debug, Clone)]
pub struct MLXResilienceConfig {
    /// Maximum consecutive failures before circuit breaker opens
    pub max_consecutive_failures: u32,
    /// Circuit breaker timeout in seconds
    pub circuit_breaker_timeout_secs: u64,
    /// Enable automatic fallback to stub mode on failures
    pub enable_stub_fallback: bool,
    /// Health check interval in seconds
    pub health_check_interval_secs: u64,
    /// Command to execute for backend failover (e.g., switch to Metal)
    pub failover_command: Option<String>,
    /// Environment variables to set on failover
    pub failover_env_vars: std::collections::HashMap<String, String>,
}

impl Default for MLXResilienceConfig {
    fn default() -> Self {
        Self {
            max_consecutive_failures: 5,
            circuit_breaker_timeout_secs: 300, // 5 minutes
            enable_stub_fallback: true,
            health_check_interval_secs: 60, // 1 minute
            failover_command: None,
            failover_env_vars: std::collections::HashMap::new(),
        }
    }
}

/// MLX FFI backend for inference with resilience
pub struct MLXFFIBackend {
    /// Base model
    model: Arc<MLXFFIModel>,
    /// Loaded LoRA adapters by ID
    pub adapters: Arc<RwLock<HashMap<u16, Arc<LoRAAdapter>>>>,
    /// Device name
    device: String,
    /// Resilience configuration
    resilience_config: MLXResilienceConfig,
    /// Backend health status
    health_status: Arc<RwLock<BackendHealth>>,
    /// Optional monitoring integration
    pub monitor: Option<Arc<std::sync::Mutex<crate::monitoring::MLXMonitor>>>,
    /// Memory pool for GPU buffer management
    pub memory_pool: Arc<MLXMemoryPool>,
    /// Memory pool size tracking (performance optimization) - raw pointers handled separately
    pub memory_pool_size: Arc<RwLock<usize>>,
    /// Performance metrics
    pub performance_metrics: Arc<RwLock<PerformanceMetrics>>,
    /// Manifest hash for determinism attestation
    manifest_hash: Option<B3Hash>,
}

/// Performance metrics for optimization
#[derive(Debug, Clone)]
pub struct PerformanceMetrics {
    pub total_inference_time_ms: u64,
    pub total_requests: u64,
    pub average_latency_ms: f32,
    pub peak_memory_usage_mb: f32,
    pub cache_hit_rate: f32,
}

/// Backend health tracking
#[derive(Debug, Clone)]
pub struct BackendHealth {
    /// Is backend operational
    pub operational: bool,
    /// Total requests processed
    pub total_requests: u64,
    /// Successful requests
    pub successful_requests: u64,
    /// Failed requests
    pub failed_requests: u64,
    /// Last failure timestamp
    pub last_failure: Option<std::time::Instant>,
    /// Current failure streak
    pub current_failure_streak: u32,
    /// Stub fallback mode active
    pub stub_fallback_active: bool,
    /// Number of active adapters loaded
    pub active_adapters: usize,
}

impl MLXFFIBackend {
    /// Create new MLX FFI backend with loaded model and default resilience
    pub fn new(model: MLXFFIModel) -> Self {
        // Ensure MLX runtime is initialized
        if let Err(e) = crate::mlx_runtime_init() {
            tracing::warn!("MLX runtime initialization warning: {}", e);
            // Continue - may already be initialized
        }

        Self::with_resilience_config(model, MLXResilienceConfig::default())
    }

    /// Create new MLX FFI backend with custom resilience configuration
    pub fn with_resilience_config(model: MLXFFIModel, config: MLXResilienceConfig) -> Self {
        // Ensure MLX runtime is initialized
        if !crate::mlx_runtime_is_initialized() {
            if let Err(e) = crate::mlx_runtime_init() {
                tracing::warn!("MLX runtime init in backend: {}", e);
            }
        }

        let memory_pool_config = MLXMemoryPoolConfig::default();
        let memory_pool = Arc::new(MLXMemoryPool::new(memory_pool_config));

        Self {
            model: Arc::new(model),
            adapters: Arc::new(RwLock::new(HashMap::new())),
            device: "MLX FFI (Apple Silicon)".to_string(),
            resilience_config: config,
            health_status: Arc::new(RwLock::new(BackendHealth {
                operational: true,
                total_requests: 0,
                successful_requests: 0,
                failed_requests: 0,
                last_failure: None,
                current_failure_streak: 0,
                stub_fallback_active: false,
                active_adapters: 0,
            })),
            monitor: None,
            memory_pool,
            memory_pool_size: Arc::new(RwLock::new(0)),
            performance_metrics: Arc::new(RwLock::new(PerformanceMetrics {
                total_inference_time_ms: 0,
                total_requests: 0,
                average_latency_ms: 0.0,
                peak_memory_usage_mb: 0.0,
                cache_hit_rate: 0.0,
            })),
            manifest_hash: None,
        }
    }

    /// Create new MLX FFI backend with HKDF seeding from manifest hash
    ///
    /// This ensures deterministic execution by deriving the MLX RNG seed
    /// from the model manifest hash using HKDF with domain separation.
    pub fn with_manifest_hash(model: MLXFFIModel, manifest_hash: B3Hash) -> Result<Self> {
        Self::with_manifest_hash_and_config(model, manifest_hash, MLXResilienceConfig::default())
    }

    /// Create new MLX FFI backend with HKDF seeding and custom resilience
    pub fn with_manifest_hash_and_config(
        model: MLXFFIModel,
        manifest_hash: B3Hash,
        config: MLXResilienceConfig,
    ) -> Result<Self> {
        // Derive deterministic seed from manifest hash using HKDF
        let seed = derive_seed(&manifest_hash, "mlx");

        // Set MLX random seed for determinism
        crate::mlx_set_seed_from_bytes(&seed)?;

        tracing::info!(
            manifest_hash = %manifest_hash.to_hex(),
            seed_checksum = %B3Hash::hash(&seed).to_hex()[..16],
            "Initialized MLX backend with HKDF-derived seed"
        );

        let memory_pool_config = MLXMemoryPoolConfig::default();
        let memory_pool = Arc::new(MLXMemoryPool::new(memory_pool_config));

        Ok(Self {
            model: Arc::new(model),
            adapters: Arc::new(RwLock::new(HashMap::new())),
            device: "MLX FFI (Apple Silicon)".to_string(),
            resilience_config: config,
            health_status: Arc::new(RwLock::new(BackendHealth {
                operational: true,
                total_requests: 0,
                successful_requests: 0,
                failed_requests: 0,
                last_failure: None,
                current_failure_streak: 0,
                stub_fallback_active: false,
                active_adapters: 0,
            })),
            monitor: None,
            memory_pool,
            memory_pool_size: Arc::new(RwLock::new(0)),
            performance_metrics: Arc::new(RwLock::new(PerformanceMetrics {
                total_inference_time_ms: 0,
                total_requests: 0,
                average_latency_ms: 0.0,
                peak_memory_usage_mb: 0.0,
                cache_hit_rate: 0.0,
            })),
            manifest_hash: Some(manifest_hash),
        })
    }

    /// Enable monitoring for this backend
    pub fn with_monitoring(
        mut self,
        monitoring_config: crate::monitoring::MonitoringConfig,
    ) -> Self {
        let monitor = Arc::new(std::sync::Mutex::new(crate::monitoring::MLXMonitor::new(
            Arc::new(self.clone_without_monitor()),
            monitoring_config,
        )));
        self.monitor = Some(monitor);
        self
    }

    /// Clone backend without monitor (for monitor creation)
    fn clone_without_monitor(&self) -> MLXFFIBackend {
        MLXFFIBackend {
            model: self.model.clone(),
            adapters: self.adapters.clone(),
            device: self.device.clone(),
            resilience_config: self.resilience_config.clone(),
            health_status: self.health_status.clone(),
            monitor: None,
            memory_pool: self.memory_pool.clone(),
            memory_pool_size: self.memory_pool_size.clone(),
            performance_metrics: self.performance_metrics.clone(),
            manifest_hash: self.manifest_hash,
        }
    }

    /// Perform health check (if monitoring enabled)
    pub fn perform_health_check(&self) -> Option<crate::monitoring::HealthCheckResult> {
        if let Some(monitor) = &self.monitor {
            let mut monitor_guard = monitor.lock().unwrap();
            Some(monitor_guard.health_check())
        } else {
            None
        }
    }

    /// Get active alerts (if monitoring enabled)
    pub fn active_alerts(&self) -> Vec<crate::monitoring::Alert> {
        if let Some(monitor) = &self.monitor {
            let monitor_guard = monitor.lock().unwrap();
            monitor_guard.active_alerts().to_vec()
        } else {
            Vec::new()
        }
    }

    /// Export metrics (if monitoring enabled)
    pub fn export_metrics(&self) -> String {
        if let Some(monitor) = &self.monitor {
            let monitor_guard = monitor.lock().unwrap();
            monitor_guard.export_metrics()
        } else {
            String::new()
        }
    }

    /// Get current health status
    pub fn health_status(&self) -> BackendHealth {
        self.health_status.read().clone()
    }

    /// Check if backend is healthy
    pub fn is_healthy(&self) -> bool {
        let health = self.health_status.read();
        health.operational
            && health.current_failure_streak < self.resilience_config.max_consecutive_failures
            && self.model.is_healthy()
    }

    /// Reset backend health (recovery operation)
    pub fn reset_health(&self) {
        let mut health = self.health_status.write();
        health.operational = true;
        health.current_failure_streak = 0;
        health.stub_fallback_active = false;
        tracing::info!("MLX backend health reset");
    }

    /// Register a LoRA adapter
    pub fn register_adapter(&self, adapter_id: u16, adapter: LoRAAdapter) -> Result<()> {
        let adapter_name = adapter.id().to_string();

        // Calculate estimated memory usage
        let rank = adapter.config().rank;
        let num_modules = adapter.config().target_modules.len();
        let estimated_bytes = rank * 4096 * 2 * num_modules * 4; // f32 = 4 bytes

        // Track adapter memory in pool
        self.memory_pool.track_adapter(adapter_id, estimated_bytes);

        let mut adapters = self.adapters.write();
        adapters.insert(adapter_id, Arc::new(adapter));

        // Update memory pool size tracking
        let current_size = *self.memory_pool_size.read();
        *self.memory_pool_size.write() = current_size + estimated_bytes;

        tracing::info!(
            adapter_id = adapter_id,
            adapter_name = %adapter_name,
            estimated_bytes = estimated_bytes,
            "Registered LoRA adapter with memory tracking"
        );
        Ok(())
    }

    /// Get registered adapter count
    pub fn adapter_count(&self) -> usize {
        self.adapters.read().len()
    }

    /// Load adapter at runtime (hot-swap)
    pub fn load_adapter_runtime(&self, adapter_id: u16, adapter: LoRAAdapter) -> Result<()> {
        let adapter_name = adapter.id().to_string();

        // Calculate estimated memory usage
        let rank = adapter.config().rank;
        let num_modules = adapter.config().target_modules.len();
        let estimated_bytes = rank * 4096 * 2 * num_modules * 4; // f32 = 4 bytes

        // Track adapter memory in pool
        self.memory_pool.track_adapter(adapter_id, estimated_bytes);

        let mut adapters = self.adapters.write();
        adapters.insert(adapter_id, Arc::new(adapter));

        // Update memory pool size tracking
        let current_size = *self.memory_pool_size.read();
        *self.memory_pool_size.write() = current_size + estimated_bytes;

        tracing::info!(
            adapter_id = adapter_id,
            adapter_name = %adapter_name,
            estimated_bytes = estimated_bytes,
            "Hot-loaded LoRA adapter with memory tracking"
        );
        Ok(())
    }

    /// Unload adapter at runtime (hot-swap)
    pub fn unload_adapter_runtime(&self, adapter_id: u16) -> Result<()> {
        let mut adapters = self.adapters.write();
        if let Some(adapter) = adapters.remove(&adapter_id) {
            // Get the memory usage before removal for proper cleanup
            if let Ok(memory_usage) = self.get_adapter_memory_usage(adapter_id) {
                // Update memory pool size tracking
                let current_size = *self.memory_pool_size.read();
                *self.memory_pool_size.write() = current_size.saturating_sub(memory_usage);
            }

            // Stop tracking adapter in memory pool
            self.memory_pool.untrack_adapter(adapter_id);

            tracing::info!(
                adapter_id = adapter_id,
                adapter_name = %adapter.id(),
                "Unloaded LoRA adapter and freed memory"
            );
            Ok(())
        } else {
            Err(adapteros_core::AosError::Lifecycle(format!(
                "Adapter {} not found",
                adapter_id
            )))
        }
    }

    /// Get adapter memory usage (estimated)
    pub fn get_adapter_memory_usage(&self, adapter_id: u16) -> Result<usize> {
        let adapters = self.adapters.read();
        if let Some(adapter) = adapters.get(&adapter_id) {
            // Estimate memory usage based on LoRA parameters
            // rank * (dim_in + dim_out) * sizeof(f32) per target module
            let rank = adapter.config().rank;
            let num_modules = adapter.config().target_modules.len();

            // Simplified: assume 7B model with 4096 hidden dim
            let estimated_bytes = rank * 4096 * 2 * num_modules * 4; // f32 = 4 bytes
            Ok(estimated_bytes)
        } else {
            Err(adapteros_core::AosError::Lifecycle(format!(
                "Adapter {} not found",
                adapter_id
            )))
        }
    }

    /// Apply LoRA adapters based on router decisions
    #[allow(dead_code)]
    fn apply_loras(
        &self,
        ring: &RouterRing,
        base_output: &[f32],
        input: &[f32],
        module_name: &str,
    ) -> Result<Vec<f32>> {
        let adapters = self.adapters.read();

        // Collect active adapters
        let mut active_adapters = Vec::new();
        let mut gates = Vec::new();

        for (idx, &adapter_id) in ring.indices.iter().enumerate() {
            if let Some(adapter) = adapters.get(&adapter_id) {
                active_adapters.push(adapter.clone());
                // Convert i16 Q15 to u16 for routing module
                gates.push(ring.gates_q15[idx].max(0) as u16);
            }
        }

        if active_adapters.is_empty() {
            tracing::info!(
                reason = "no_adapters_qualify",
                "Router decision: K=0, using base model only"
            );
            return Ok(base_output.to_vec());
        }

        // Apply multi-LoRA routing
        let adapter_refs: Vec<&LoRAAdapter> = active_adapters.iter().map(|a| a.as_ref()).collect();

        crate::routing::apply_multi_lora(&adapter_refs, &gates, module_name, input, base_output)
    }

    /// Get current memory pool statistics
    pub fn get_memory_pool_stats(&self) -> crate::MemoryPoolStats {
        self.memory_pool.get_stats()
    }

    /// Get total adapter memory tracked in the pool
    pub fn get_total_adapter_memory(&self) -> usize {
        self.memory_pool.total_adapter_memory()
    }

    /// Clean up idle buffers in the memory pool
    pub fn cleanup_idle_buffers(&self) -> usize {
        self.memory_pool.cleanup_idle()
    }

    /// Handle memory pressure by freeing buffers
    ///
    /// # Arguments
    /// * `bytes_to_free` - Target number of bytes to free
    ///
    /// # Returns
    /// Actual number of bytes freed
    pub fn handle_memory_pressure(&self, bytes_to_free: usize) -> usize {
        let freed = self.memory_pool.handle_memory_pressure(bytes_to_free);

        if freed > 0 {
            let freed_mb = freed as f32 / (1024.0 * 1024.0);
            tracing::warn!(
                freed_mb = freed_mb,
                bytes_to_free = bytes_to_free,
                "Memory pressure handled: freed {} MB",
                freed_mb
            );
        }

        freed
    }

    /// Register a memory pressure callback
    ///
    /// Callbacks are invoked when memory usage exceeds the pressure threshold.
    pub fn register_memory_pressure_callback(
        &self,
        callback: crate::memory_pool::MemoryPressureCallback,
    ) {
        self.memory_pool.register_pressure_callback(callback);
    }

    /// Clear all pooled memory buffers
    pub fn clear_memory_pool(&self) {
        self.memory_pool.clear_pool();
        *self.memory_pool_size.write() = 0;
    }

    /// Get list of tracked adapter IDs
    pub fn tracked_adapter_ids(&self) -> Vec<u16> {
        self.memory_pool.tracked_adapters()
    }

    /// Update memory pool size metric (call during inference if needed)
    pub fn update_memory_metrics(&self) {
        let (active_bytes, pooled_bytes) = self.memory_pool.current_usage();
        let total_bytes = active_bytes + pooled_bytes;
        let total_mb = total_bytes as f32 / (1024.0 * 1024.0);

        let mut metrics = self.performance_metrics.write();
        if total_mb > metrics.peak_memory_usage_mb {
            metrics.peak_memory_usage_mb = total_mb;
        }

        tracing::debug!(
            active_mb = active_bytes as f32 / (1024.0 * 1024.0),
            pooled_mb = pooled_bytes as f32 / (1024.0 * 1024.0),
            total_mb = total_mb,
            peak_mb = metrics.peak_memory_usage_mb,
            "Memory pool metrics updated"
        );
    }
}

impl FusedKernels for MLXFFIBackend {
    fn load(&mut self, _plan_bytes: &[u8]) -> Result<()> {
        // Plan loading not needed for MLX FFI - model already loaded
        tracing::info!(
            "MLX FFI backend ready with {} adapters",
            self.adapter_count()
        );
        Ok(())
    }

    fn run_step(&mut self, ring: &RouterRing, io: &mut IoBuffers) -> Result<()> {
        // Update health tracking
        {
            let mut health = self.health_status.write();
            health.total_requests += 1;
        }

        // Check circuit breaker state for stub fallback
        let use_stub_fallback = {
            let health = self.health_status.read();
            health.stub_fallback_active && self.resilience_config.enable_stub_fallback
        };

        let result = if use_stub_fallback {
            // Use stub fallback due to circuit breaker activation
            tracing::warn!("MLX backend using stub fallback mode (circuit breaker active)");
            self.run_step_stub(ring, io)
        } else {
            // Run real MLX inference
            self.run_step_mlx(ring, io)
        };

        // Update health based on result
        let mut health = self.health_status.write();
        match &result {
            Ok(_) => {
                health.successful_requests += 1;
                health.current_failure_streak = 0;
                health.last_failure = None;
            }
            Err(_) => {
                health.failed_requests += 1;
                health.current_failure_streak += 1;
                health.last_failure = Some(std::time::Instant::now());

                // Check if we should enable stub fallback
                if health.current_failure_streak >= 3 && self.resilience_config.enable_stub_fallback
                {
                    health.stub_fallback_active = true;
                    tracing::warn!(
                        "MLX backend switching to stub fallback after {} failures",
                        health.current_failure_streak
                    );
                }

                // Check if we should mark backend as non-operational
                if health.current_failure_streak >= self.resilience_config.max_consecutive_failures
                {
                    health.operational = false;
                    tracing::error!(
                        "MLX backend marked non-operational after {} consecutive failures",
                        health.current_failure_streak
                    );

                    // Execute failover actions (inlined for trait compatibility)
                    tracing::warn!("Executing MLX backend failover actions");

                    // Set environment variables for failover
                    for (key, value) in &self.resilience_config.failover_env_vars {
                        std::env::set_var(key, value);
                        tracing::info!("Set failover env var: {}={}", key, value);
                    }

                    // Execute failover command if specified
                    if let Some(ref command) = self.resilience_config.failover_command {
                        tracing::info!("Executing failover command: {}", command);
                        // Note: In production, this should be done through a proper process manager
                        // For now, just log the intent
                        tracing::warn!(
                            "Failover command execution not implemented in demo: {}",
                            command
                        );
                    }

                    // Signal to monitoring systems
                    tracing::error!(
                        backend = "mlx",
                        status = "failed_over",
                        failures = %health.current_failure_streak,
                        "MLX backend has failed over - system should switch to alternative backend"
                    );
                }
            }
        }

        result
    }

    fn attest_determinism(
        &self,
    ) -> Result<adapteros_lora_kernel_api::attestation::DeterminismReport> {
        use adapteros_lora_kernel_api::attestation::*;

        // Check if backend is properly seeded with manifest hash
        let (rng_method, deterministic) = if self.manifest_hash.is_some() {
            // HKDF-seeded with manifest hash - deterministic
            (RngSeedingMethod::HkdfSeeded, true)
        } else {
            // No manifest hash - using system entropy, not deterministic
            (RngSeedingMethod::SystemEntropy, false)
        };

        // Check stub fallback state
        let is_stub_active = {
            let health = self.health_status.read();
            health.stub_fallback_active
        };

        // MLX uses IEEE-754 floating-point (deterministic when properly seeded)
        let float_mode = FloatingPointMode::Deterministic;

        // Report actual capabilities
        let report = DeterminismReport {
            backend_type: BackendType::Mlx,
            metallib_hash: self.manifest_hash, // Include manifest hash for content addressing
            manifest: None,                    // No Metal-style manifest
            rng_seed_method: rng_method,
            floating_point_mode: float_mode,
            compiler_flags: vec![],
            deterministic: deterministic && !is_stub_active,
        };

        tracing::info!(
            deterministic = report.deterministic,
            rng_method = ?report.rng_seed_method,
            has_manifest_hash = self.manifest_hash.is_some(),
            stub_active = is_stub_active,
            "MLX backend determinism attestation"
        );

        Ok(report)
    }

    fn device_name(&self) -> &str {
        &self.device
    }

    fn load_adapter(&mut self, id: u16, weights: &[u8]) -> Result<()> {
        // Parse adapter weights from safetensors format
        let tensors = safetensors::SafeTensors::deserialize(weights).map_err(|e| {
            adapteros_core::AosError::Parse(format!("Failed to parse adapter weights: {}", e))
        })?;

        // Create LoRA config from default (can be customized via metadata)
        let config = crate::lora::LoRAConfig::default();
        let adapter_id_str = format!("adapter_{}", id);
        let mut adapter = LoRAAdapter::new(adapter_id_str.clone(), config.clone());

        // Extract LoRA weights for each target module
        for module_name in &config.target_modules {
            let lora_a_key = format!("{}.lora_A", module_name);
            let lora_b_key = format!("{}.lora_B", module_name);

            if let (Ok(lora_a_tensor), Ok(lora_b_tensor)) =
                (tensors.tensor(&lora_a_key), tensors.tensor(&lora_b_key))
            {
                // Convert tensors to Vec<Vec<f32>>
                let lora_a = Self::tensor_to_nested_vec(&lora_a_tensor)?;
                let lora_b = Self::tensor_to_nested_vec(&lora_b_tensor)?;

                adapter.add_module_weights(module_name, lora_a, lora_b);

                tracing::debug!(
                    adapter_id = id,
                    module = %module_name,
                    "Loaded LoRA weights for hot-swap"
                );
            }
        }

        // Register adapter with memory tracking
        self.register_adapter(id, adapter)?;

        tracing::info!(
            adapter_id = id,
            adapter_name = %adapter_id_str,
            "Hot-swap loaded adapter via FusedKernels trait"
        );

        Ok(())
    }

    fn unload_adapter(&mut self, id: u16) -> Result<()> {
        // Use the existing runtime unload method
        self.unload_adapter_runtime(id)?;

        tracing::info!(
            adapter_id = id,
            "Hot-swap unloaded adapter via FusedKernels trait"
        );

        Ok(())
    }

    fn get_metrics(&self) -> adapteros_lora_kernel_api::BackendMetrics {
        let metrics = self.performance_metrics.read();
        let health = self.health_status.read();

        adapteros_lora_kernel_api::BackendMetrics {
            total_operations: health.total_requests,
            successful_operations: health.successful_requests,
            failed_operations: health.failed_requests,
            avg_latency: std::time::Duration::from_millis(metrics.average_latency_ms as u64),
            memory_usage_bytes: (metrics.peak_memory_usage_mb * 1024.0 * 1024.0) as u64,
        }
    }

    fn health_check(&self) -> Result<adapteros_lora_kernel_api::BackendHealth> {
        let health = self.health_status.read();

        if !health.operational {
            return Ok(adapteros_lora_kernel_api::BackendHealth::Failed {
                reason: "Backend marked non-operational after consecutive failures".to_string(),
                recoverable: true,
            });
        }

        if health.stub_fallback_active {
            return Ok(adapteros_lora_kernel_api::BackendHealth::Degraded {
                reason: "Operating in stub fallback mode due to previous failures".to_string(),
            });
        }

        if health.current_failure_streak > 0 {
            return Ok(adapteros_lora_kernel_api::BackendHealth::Degraded {
                reason: format!(
                    "Recent failures detected: {} consecutive",
                    health.current_failure_streak
                ),
            });
        }

        Ok(adapteros_lora_kernel_api::BackendHealth::Healthy)
    }
}

impl Clone for MLXFFIBackend {
    fn clone(&self) -> Self {
        Self {
            model: self.model.clone(),
            adapters: self.adapters.clone(),
            device: self.device.clone(),
            resilience_config: self.resilience_config.clone(),
            health_status: self.health_status.clone(),
            monitor: self.monitor.clone(),
            memory_pool: self.memory_pool.clone(),
            memory_pool_size: self.memory_pool_size.clone(),
            performance_metrics: self.performance_metrics.clone(),
            manifest_hash: self.manifest_hash,
        }
    }
}

impl MLXFFIBackend {
    /// Set manifest hash for determinism attestation
    pub fn set_manifest_hash(&mut self, hash: B3Hash) {
        self.manifest_hash = Some(hash);
    }

    /// Get manifest hash
    pub fn manifest_hash(&self) -> Option<B3Hash> {
        self.manifest_hash
    }

    /// Run inference step using real MLX FFI
    fn run_step_mlx(&self, ring: &RouterRing, io: &mut IoBuffers) -> Result<()> {
        let inference_start = std::time::Instant::now();

        // Validate input
        if io.input_ids.is_empty() {
            return Err(adapteros_core::AosError::Validation(
                "Input token IDs cannot be empty".to_string(),
            ));
        }

        // Check model health before inference
        if !self.model.is_healthy() {
            return Err(adapteros_core::AosError::Mlx(
                "Model is not healthy - circuit breaker may be open".to_string(),
            ));
        }

        // Run forward pass with hidden states through the model
        let (base_logits, hidden_states) = self.model.forward_with_hidden_states(&io.input_ids)?;

        // Validate base logits
        if base_logits.is_empty() {
            return Err(adapteros_core::AosError::Mlx(
                "Model returned empty logits".to_string(),
            ));
        }

        // Apply LoRA adapters using RouterRing decisions
        let final_logits = if ring.k > 0 && !self.adapters.read().is_empty() {
            self.apply_router_ring_loras(ring, &base_logits, &hidden_states)?
        } else {
            if ring.k > 0 {
                tracing::debug!(
                    k = ring.k,
                    "RouterRing specifies {} adapters but no adapters are loaded, using base model",
                    ring.k
                );
            }
            base_logits
        };

        // Update output buffer with proper size handling
        let output_len = final_logits.len().min(io.output_logits.len());
        if output_len == 0 {
            return Err(adapteros_core::AosError::Mlx(
                "Output buffer size mismatch - cannot copy logits".to_string(),
            ));
        }
        io.output_logits[..output_len].copy_from_slice(&final_logits[..output_len]);
        io.position += 1;

        // Update performance metrics
        let inference_time = inference_start.elapsed().as_millis() as u64;
        {
            let mut metrics = self.performance_metrics.write();
            metrics.total_requests += 1;
            metrics.total_inference_time_ms += inference_time;

            if metrics.total_requests > 0 {
                metrics.average_latency_ms =
                    metrics.total_inference_time_ms as f32 / metrics.total_requests as f32;
            }

            // Update peak memory based on actual tensor sizes
            let logits_memory =
                (final_logits.len() * std::mem::size_of::<f32>()) as f32 / (1024.0 * 1024.0);
            let hidden_memory: f32 = hidden_states
                .values()
                .map(|v| (v.len() * std::mem::size_of::<f32>()) as f32)
                .sum::<f32>()
                / (1024.0 * 1024.0);
            let current_memory = logits_memory + hidden_memory;

            if current_memory > metrics.peak_memory_usage_mb {
                metrics.peak_memory_usage_mb = current_memory;
            }
        }

        // Emit router decision telemetry (structured event for monitoring)
        tracing::info!(
            target: "mlx.router.decision",
            position = io.position,
            ring_k = ring.k,
            active_indices = ?&ring.indices[..ring.k],
            gates_q15 = ?&ring.gates_q15[..ring.k],
            inference_time_ms = inference_time,
            deterministic = self.manifest_hash.is_some(),
            "Router decision executed"
        );

        tracing::debug!(
            position = io.position,
            active_adapters = ring.k,
            logits_len = final_logits.len(),
            hidden_states_count = hidden_states.len(),
            inference_time_ms = inference_time,
            "MLX inference complete"
        );

        Ok(())
    }

    /// Apply LoRA adapters based on RouterRing decisions
    ///
    /// This method implements the multi-adapter routing pipeline:
    /// 1. Collects active adapters based on RouterRing indices
    /// 2. Applies Q15 quantized gate weights for each adapter
    /// 3. Routes hidden states through LoRA transformations
    /// 4. Blends LoRA outputs with base model logits
    fn apply_router_ring_loras(
        &self,
        ring: &RouterRing,
        base_logits: &[f32],
        hidden_states: &std::collections::HashMap<String, Vec<f32>>,
    ) -> Result<Vec<f32>> {
        let adapters = self.adapters.read();

        // Collect active adapters and their gates from RouterRing
        let mut active_adapters: Vec<&LoRAAdapter> = Vec::with_capacity(ring.k);
        let mut gates: Vec<u16> = Vec::with_capacity(ring.k);
        let mut total_gate_weight: f32 = 0.0;

        for i in 0..ring.k {
            let adapter_id = ring.indices[i];
            let gate_q15 = ring.gates_q15[i];

            if let Some(adapter) = adapters.get(&adapter_id) {
                // Skip adapters with zero or negative gates
                if gate_q15 <= 0 {
                    tracing::trace!(
                        adapter_id = adapter_id,
                        gate_q15 = gate_q15,
                        "Skipping adapter with non-positive gate"
                    );
                    continue;
                }

                active_adapters.push(adapter.as_ref());
                let gate_u16 = gate_q15 as u16;
                gates.push(gate_u16);
                total_gate_weight += gate_u16 as f32 / 32767.0; // Q15 dequantization
            } else {
                tracing::warn!(
                    adapter_id = adapter_id,
                    "RouterRing references adapter ID {} which is not loaded",
                    adapter_id
                );
            }
        }

        if active_adapters.is_empty() {
            tracing::debug!(
                ring_k = ring.k,
                "No active adapters qualified for routing, using base model output"
            );
            return Ok(base_logits.to_vec());
        }

        // Collect all unique target modules from active adapters
        let mut target_modules: HashSet<&str> = HashSet::new();
        for adapter in &active_adapters {
            for module in &adapter.config().target_modules {
                target_modules.insert(module.as_str());
            }
        }

        // Start with base logits
        let mut result = base_logits.to_vec();
        let mut modules_applied = 0;

        // Apply LoRA to each target module's hidden state
        for module_name in target_modules {
            if let Some(hidden) = hidden_states.get(module_name) {
                // Apply multi-LoRA routing with Q15 gates
                let lora_output = crate::routing::apply_multi_lora(
                    &active_adapters,
                    &gates,
                    module_name,
                    hidden,
                    &result,
                )?;

                // Calculate adaptive blend factor based on total gate weight
                // Higher total gate weight = stronger LoRA influence
                // Clamped to [0.05, 0.5] for stability
                let blend_factor =
                    (total_gate_weight / active_adapters.len() as f32).clamp(0.05, 0.5);

                // Blend LoRA output with result
                for (i, &lora_val) in lora_output.iter().enumerate() {
                    if i < result.len() {
                        // Linear interpolation: result = base * (1 - blend) + lora * blend
                        result[i] = result[i] * (1.0 - blend_factor) + lora_val * blend_factor;
                    }
                }

                modules_applied += 1;

                tracing::trace!(
                    module = module_name,
                    blend_factor = blend_factor,
                    "Applied LoRA to module"
                );
            } else {
                tracing::trace!(
                    module = module_name,
                    "Hidden state not available for module, skipping"
                );
            }
        }

        // Update adapter count in health status
        {
            let mut health = self.health_status.write();
            health.active_adapters = active_adapters.len();
        }

        // Emit multi-adapter routing telemetry
        tracing::info!(
            target: "mlx.router.lora_applied",
            active_adapters = active_adapters.len(),
            modules_applied = modules_applied,
            total_gate_weight = %format!("{:.4}", total_gate_weight),
            gates_q15 = ?&gates[..gates.len().min(8)],
            adapter_ids = ?active_adapters.iter().map(|a| a.id()).collect::<Vec<_>>(),
            "Multi-adapter LoRA routing applied"
        );

        tracing::debug!(
            active_adapters = active_adapters.len(),
            modules_applied = modules_applied,
            total_gate_weight = %format!("{:.4}", total_gate_weight),
            gates = ?&gates[..gates.len().min(4)],
            "Applied RouterRing LoRA adaptations"
        );

        Ok(result)
    }

    /// Run inference step using stub fallback (for circuit breaker or testing)
    fn run_step_stub(&self, ring: &RouterRing, io: &mut IoBuffers) -> Result<()> {
        // Use model's vocab size or default
        let vocab_size = self.model.config.vocab_size;
        let mut logits = vec![0.0f32; vocab_size];

        // Generate deterministic pattern based on position
        for (i, logit) in logits.iter_mut().enumerate() {
            let base = ((i + io.position) as f32 * 0.01).sin() * 0.1;
            *logit = base;
        }

        // Normalize to softmax-like distribution
        let max_logit = logits.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let sum: f32 = logits.iter().map(|x| (x - max_logit).exp()).sum();
        for logit in &mut logits {
            *logit = (*logit - max_logit).exp() / sum;
        }

        // Apply minimal LoRA effect if adapters are loaded
        if ring.k > 0 {
            let adapters = self.adapters.read();
            for i in 0..ring.k {
                let adapter_id = ring.indices[i];
                let gate_q15 = ring.gates_q15[i];

                if let Some(adapter) = adapters.get(&adapter_id) {
                    let gate_weight = (gate_q15.max(0) as f32) / 32767.0; // Q15 dequantization
                    let scale = adapter.config().alpha / adapter.config().rank as f32;

                    // Apply scaled adaptation
                    for (j, logit) in logits.iter_mut().enumerate() {
                        let adaptation = ((j as f32 + adapter_id as f32) * 0.001).sin()
                            * scale
                            * gate_weight
                            * 0.01;
                        *logit += adaptation;
                    }
                }
            }
        }

        // Update output buffer
        let output_len = logits.len().min(io.output_logits.len());
        io.output_logits[..output_len].copy_from_slice(&logits[..output_len]);
        io.position += 1;

        tracing::debug!(
            position = io.position,
            active_adapters = ring.k,
            logits_len = logits.len(),
            "MLX stub inference complete"
        );

        Ok(())
    }

    /// Helper to convert safetensors tensor to nested Vec
    fn tensor_to_nested_vec(tensor: &safetensors::tensor::TensorView) -> Result<Vec<Vec<f32>>> {
        let shape = tensor.shape();
        let data = tensor.data();

        if shape.len() != 2 {
            return Err(adapteros_core::AosError::Parse(format!(
                "Expected 2D tensor, got shape {:?}",
                shape
            )));
        }

        let rows = shape[0];
        let cols = shape[1];

        // Convert bytes to f32
        let float_data: &[f32] = unsafe {
            std::slice::from_raw_parts(
                data.as_ptr() as *const f32,
                data.len() / std::mem::size_of::<f32>(),
            )
        };

        if float_data.len() != rows * cols {
            return Err(adapteros_core::AosError::Parse(format!(
                "Data size mismatch: expected {} elements, got {}",
                rows * cols,
                float_data.len()
            )));
        }

        // Convert to nested vec
        let mut result = Vec::with_capacity(rows);
        for i in 0..rows {
            let start = i * cols;
            let end = start + cols;
            result.push(float_data[start..end].to_vec());
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lora::{LoRAAdapter, LoRAConfig};
    use adapteros_core::B3Hash;

    fn create_dummy_adapter(id: &str) -> LoRAAdapter {
        LoRAAdapter {
            id: id.to_string(),
            config: LoRAConfig::default(),
            lora_a: HashMap::new(),
            lora_b: HashMap::new(),
            shapes: HashMap::new(),
            hash: B3Hash::hash(id.as_bytes()),
        }
    }

    #[test]
    fn test_lora_adapter_creation() {
        // Test that we can create a LoRA adapter with proper configuration
        let adapter = create_dummy_adapter("test-adapter-001");
        assert_eq!(adapter.id, "test-adapter-001");
        assert!(adapter.lora_a.is_empty());
        assert!(adapter.lora_b.is_empty());
        assert!(adapter.shapes.is_empty());
        // Verify hash is computed
        let expected_hash = B3Hash::hash("test-adapter-001".as_bytes());
        assert_eq!(adapter.hash, expected_hash);
    }

    #[test]
    fn test_router_ring_creation() {
        let ring = RouterRing::new(3);
        assert_eq!(ring.indices.len(), 8); // Fixed-size arrays
        assert_eq!(ring.gates_q15.len(), 8); // Fixed-size arrays
        assert_eq!(ring.k, 3); // Active count
        assert_eq!(ring.position, 0);
    }
}
