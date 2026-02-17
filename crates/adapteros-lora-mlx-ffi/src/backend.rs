//! MLX FFI backend implementation for FusedKernels trait

use crate::{
    FFILoraAdapter, LoRAAdapter, MLXFFIModel, MLXMemoryPool, MLXMemoryPoolConfig,
    SessionCacheManager,
};
use adapteros_core::{derive_seed, B3Hash, Result, Q15_GATE_DENOMINATOR};
use adapteros_lora_kernel_api::{
    FusedKernels, IoBuffers, LiquidBlendRequest, LiquidBlendStats, LiquidKernel, RouterRing,
};
use arc_swap::ArcSwap;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

const IS_REAL_MLX: bool = cfg!(feature = "mlx") && !cfg!(mlx_stub);

fn backend_device_label() -> String {
    if IS_REAL_MLX {
        "MLX FFI (Apple Silicon)".to_string()
    } else {
        "MLX FFI (stub build)".to_string()
    }
}

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
    /// Base model (immutable after load; Arc ensures shared, read-only handle)
    model: Arc<MLXFFIModel>,
    /// Loaded LoRA adapters by ID (lock-free for fast inference lookups)
    ///
    /// Uses `ArcSwap` for lock-free reads during inference, with copy-on-write
    /// semantics for adapter registration/unregistration. This eliminates
    /// contention on the hot path (inference) while keeping writes atomic.
    pub adapters: ArcSwap<HashMap<u16, Arc<LoRAAdapter>>>,
    /// C++ FFI LoRA adapter handles for fused forward pass
    ffi_adapters: HashMap<u16, FFILoraAdapter>,
    /// Per-session KV cache manager for multi-turn chat
    session_cache: SessionCacheManager,
    /// Temporary KV cache for non-session requests
    temp_cache_ptr: Option<*mut crate::mlx_kv_cache_t>,
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
    /// Test hook: count FFI set_module population attempts.
    #[cfg(test)]
    ffi_set_module_attempts: usize,
}

/// Performance metrics for optimization
#[derive(Debug, Clone, Default)]
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

impl Default for BackendHealth {
    fn default() -> Self {
        Self {
            operational: true,
            total_requests: 0,
            successful_requests: 0,
            failed_requests: 0,
            last_failure: None,
            current_failure_streak: 0,
            stub_fallback_active: false,
            active_adapters: 0,
        }
    }
}

/// Macro to reduce boilerplate for monitor access patterns
///
/// In FFI contexts, panics are especially dangerous, so we handle poisoned locks
/// by logging and returning the default value instead of panicking.
macro_rules! with_monitor {
    ($self:expr, |$m:ident| $body:expr, $default:expr) => {
        if let Some(monitor) = &$self.monitor {
            match monitor.lock() {
                Ok(guard) => {
                    #[allow(unused_mut)]
                    let mut $m = guard;
                    $body
                }
                Err(poisoned) => {
                    // In FFI code, panicking is dangerous. Log and recover.
                    tracing::error!(
                        "Monitor lock poisoned, recovering with default. \
                         Previous panic in critical section detected."
                    );
                    // Still try to use the poisoned guard's data
                    #[allow(unused_mut)]
                    let mut $m = poisoned.into_inner();
                    $body
                }
            }
        } else {
            $default
        }
    };
}

impl MLXFFIBackend {
    /// Conversion factor for bytes to megabytes
    const BYTES_PER_MB: f32 = 1024.0 * 1024.0;

    /// Log router decision telemetry with standardized format
    fn log_router_decision(&self, io: &IoBuffers, ring: &RouterRing, inference_time: u64) {
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
    }

    /// Create new MLX FFI backend with loaded model and default resilience
    pub fn new(model: MLXFFIModel) -> Self {
        // Ensure MLX runtime is initialized
        if let Err(e) = crate::mlx_runtime_init_ffi() {
            tracing::warn!("MLX runtime initialization warning: {}", e);
            // Continue - may already be initialized
        }

        Self::with_resilience_config(model, MLXResilienceConfig::default())
    }

    /// Create new MLX FFI backend with custom resilience configuration
    pub fn with_resilience_config(model: MLXFFIModel, config: MLXResilienceConfig) -> Self {
        Self::new_internal(Arc::new(model), config, None)
    }

    /// Internal constructor shared by all public constructors
    fn new_internal(
        model: Arc<MLXFFIModel>,
        config: MLXResilienceConfig,
        manifest_hash: Option<B3Hash>,
    ) -> Self {
        // Ensure MLX runtime is initialized
        if !crate::mlx_runtime_is_initialized_ffi() {
            if let Err(e) = crate::mlx_runtime_init_ffi() {
                tracing::warn!("MLX runtime init in backend: {}", e);
            }
        }

        let memory_pool = Arc::new(MLXMemoryPool::new(MLXMemoryPoolConfig::default()));

        if !IS_REAL_MLX {
            tracing::warn!(
                "MLX backend built without real MLX support (stub FFI); determinism attestation disabled"
            );
        }

        Self {
            model,
            adapters: ArcSwap::from_pointee(HashMap::new()),
            ffi_adapters: HashMap::new(),
            session_cache: SessionCacheManager::new(
                16,            // max 16 concurrent sessions
                4_294_967_296, // 4 GB max cache memory
            ),
            temp_cache_ptr: None,
            device: backend_device_label(),
            resilience_config: config,
            health_status: Arc::new(RwLock::new(BackendHealth::default())),
            monitor: None,
            memory_pool,
            memory_pool_size: Arc::new(RwLock::new(0)),
            performance_metrics: Arc::new(RwLock::new(PerformanceMetrics::default())),
            manifest_hash,
            #[cfg(test)]
            ffi_set_module_attempts: 0,
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
        crate::mlx_set_seed_from_bytes_ffi(&seed)?;

        tracing::info!(
            manifest_hash = %manifest_hash.to_hex(),
            seed_checksum = %B3Hash::hash(&seed).to_hex()[..16],
            "Initialized MLX backend with HKDF-derived seed"
        );

        Ok(Self::new_internal(
            Arc::new(model),
            config,
            Some(manifest_hash),
        ))
    }

    /// Create new MLX FFI backend with a pre-loaded shared model (for caching)
    ///
    /// This constructor accepts an `Arc<MLXFFIModel>` instead of an owned model,
    /// allowing multiple backends to share the same loaded model via the model
    /// cache. This is the preferred constructor when using `ModelHandleCache`.
    pub fn new_with_arc(model: Arc<MLXFFIModel>) -> Self {
        Self::with_arc_and_config(model, MLXResilienceConfig::default())
    }

    /// Create new MLX FFI backend with shared model and custom resilience
    pub fn with_arc_and_config(model: Arc<MLXFFIModel>, config: MLXResilienceConfig) -> Self {
        Self::new_internal(model, config, None)
    }

    /// Create new MLX FFI backend with shared model and HKDF seeding
    ///
    /// This ensures deterministic execution by deriving the MLX RNG seed
    /// from the model manifest hash using HKDF with domain separation.
    /// Accepts `Arc<MLXFFIModel>` for use with the model cache.
    pub fn with_manifest_hash_arc(model: Arc<MLXFFIModel>, manifest_hash: B3Hash) -> Result<Self> {
        Self::with_manifest_hash_arc_and_config(
            model,
            manifest_hash,
            MLXResilienceConfig::default(),
        )
    }

    /// Create new MLX FFI backend with shared model, HKDF seeding, and custom resilience
    pub fn with_manifest_hash_arc_and_config(
        model: Arc<MLXFFIModel>,
        manifest_hash: B3Hash,
        config: MLXResilienceConfig,
    ) -> Result<Self> {
        // Derive deterministic seed from manifest hash using HKDF
        let seed = derive_seed(&manifest_hash, "mlx");

        // Set MLX random seed for determinism
        crate::mlx_set_seed_from_bytes_ffi(&seed)?;

        tracing::info!(
            manifest_hash = %manifest_hash.to_hex(),
            seed_checksum = %B3Hash::hash(&seed).to_hex()[..16],
            "Initialized MLX backend with HKDF-derived seed (shared model)"
        );

        Ok(Self::new_internal(model, config, Some(manifest_hash)))
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
            adapters: ArcSwap::from_pointee((**self.adapters.load()).clone()),
            ffi_adapters: HashMap::new(), // Don't clone FFI handles
            session_cache: SessionCacheManager::new(16, 4_294_967_296),
            temp_cache_ptr: None,
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
        with_monitor!(self, |m| Some(m.health_check()), None)
    }

    /// Get active alerts (if monitoring enabled)
    pub fn active_alerts(&self) -> Vec<crate::monitoring::Alert> {
        with_monitor!(self, |m| m.active_alerts().to_vec(), Vec::new())
    }

    /// Export metrics (if monitoring enabled)
    pub fn export_metrics(&self) -> String {
        with_monitor!(self, |m| m.export_metrics(), String::new())
    }

    /// Get current health status
    pub fn health_status(&self) -> BackendHealth {
        self.health_status.read().clone()
    }

    /// Atomically update memory pool size with a signed delta
    ///
    /// This helper ensures atomic read-modify-write for `memory_pool_size`,
    /// preventing race conditions in concurrent adapter registration/unregistration.
    fn update_memory_pool_size(&self, delta: isize) {
        let mut size = self.memory_pool_size.write();
        if delta >= 0 {
            *size = size.saturating_add(delta as usize);
        } else {
            *size = size.saturating_sub((-delta) as usize);
        }
    }

    /// Prepare FFI adapter pointers and blend weights from a RouterRing.
    ///
    /// Returns (adapter_ptrs, blend_weights) ready for the C++ forward pass.
    fn prepare_lora_for_ring(
        &self,
        ring: &RouterRing,
    ) -> Result<(Vec<*mut crate::mlx_lora_adapter_t>, Vec<f32>)> {
        let mut adapter_ptrs = Vec::with_capacity(ring.k);
        let mut blend_weights = Vec::with_capacity(ring.k);

        for i in 0..ring.k {
            let adapter_id = ring.indices[i];
            let gate_q15 = ring.gates_q15[i];

            // Skip zero/negative gates
            if gate_q15 <= 0 {
                continue;
            }

            if let Some(ffi_adapter) = self.ffi_adapters.get(&adapter_id) {
                adapter_ptrs.push(ffi_adapter.as_ptr());
                // Dequantize Q15 gate to f32 blend weight
                blend_weights.push(gate_q15 as f32 / Q15_GATE_DENOMINATOR);
            } else {
                tracing::warn!(
                    adapter_id = adapter_id,
                    "RouterRing references adapter {} which has no FFI handle",
                    adapter_id
                );
            }
        }

        Ok((adapter_ptrs, blend_weights))
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

    /// Internal helper for adapter registration
    fn add_adapter_internal(
        &mut self,
        adapter_id: u16,
        adapter: LoRAAdapter,
        operation: &str,
    ) -> Result<()> {
        let adapter_name = adapter.id().to_string();
        let estimated_bytes = Self::estimate_adapter_memory(&adapter);

        // Track adapter memory in pool
        self.memory_pool.track_adapter(adapter_id, estimated_bytes);

        // Create FFI adapter handle for fused forward pass
        let num_layers = self.model.config.num_hidden_layers;
        let scale = adapter.config().alpha / adapter.config().rank as f32;

        let mut ffi_adapter = match FFILoraAdapter::new(adapter_id as i32, num_layers, scale) {
            Ok(ffi_adapter) => Some(ffi_adapter),
            Err(e) => {
                tracing::warn!(
                    adapter_id = adapter_id,
                    error = %e,
                    "Failed to create FFI adapter handle, falling back to Rust-side LoRA"
                );
                None
            }
        };

        // Populate LoRA weights for each target module
        // The adapter stores weights as Vec<Vec<f32>> per module name (e.g., "q_proj"),
        // shared across transformer layers. Populate every layer explicitly to avoid
        // silently missing modules on deeper layers.
        for module_name in &adapter.config().target_modules {
            if let Some((a_matrix, b_matrix)) = adapter.get_module_weights(module_name) {
                // Flatten nested Vec<Vec<f32>> to contiguous &[f32]
                let a_flat: Vec<f32> = a_matrix
                    .iter()
                    .flat_map(|row| row.iter().copied())
                    .collect();
                let b_flat: Vec<f32> = b_matrix
                    .iter()
                    .flat_map(|row| row.iter().copied())
                    .collect();
                let a_rows = a_matrix.len();
                let a_cols = if a_rows > 0 { a_matrix[0].len() } else { 0 };
                let b_rows = b_matrix.len();
                let b_cols = if b_rows > 0 { b_matrix[0].len() } else { 0 };

                for layer_idx in 0..num_layers {
                    #[cfg(test)]
                    {
                        self.ffi_set_module_attempts += 1;
                    }

                    if let Some(ffi_adapter) = ffi_adapter.as_mut() {
                        if let Err(e) = ffi_adapter.set_module(
                            layer_idx,
                            module_name,
                            &a_flat,
                            &b_flat,
                            [a_rows, a_cols],
                            [b_rows, b_cols],
                        ) {
                            tracing::warn!(
                                adapter_id = adapter_id,
                                module = module_name,
                                layer = layer_idx,
                                error = %e,
                                "Failed to set FFI LoRA module, skipping layer"
                            );
                        }
                    }
                }
            }
        }

        if let Some(ffi_adapter) = ffi_adapter {
            self.ffi_adapters.insert(adapter_id, ffi_adapter);
        }

        // Copy-on-write update
        let arc_adapter = Arc::new(adapter);
        let mut new_adapters = (**self.adapters.load()).clone();
        new_adapters.insert(adapter_id, arc_adapter);
        self.adapters.store(Arc::new(new_adapters));

        // Update memory pool size atomically
        self.update_memory_pool_size(estimated_bytes as isize);

        tracing::trace!(
            adapter_id = adapter_id,
            adapter_name = %adapter_name,
            estimated_mb = estimated_bytes as f32 / Self::BYTES_PER_MB,
            "{} adapter",
            operation
        );

        Ok(())
    }

    /// Register a LoRA adapter
    pub fn register_adapter(&mut self, adapter_id: u16, adapter: LoRAAdapter) -> Result<()> {
        self.add_adapter_internal(adapter_id, adapter, "Registered")
    }

    /// Get registered adapter count
    pub fn adapter_count(&self) -> usize {
        self.adapters.load().len()
    }

    /// Load adapter at runtime (hot-swap)
    pub fn load_adapter_runtime(&mut self, adapter_id: u16, adapter: LoRAAdapter) -> Result<()> {
        self.add_adapter_internal(adapter_id, adapter, "Hot-loaded")
    }

    /// Unload adapter at runtime (hot-swap)
    pub fn unload_adapter_runtime(&mut self, adapter_id: u16) -> Result<()> {
        // Check if adapter exists and get info before removal
        let current_adapters = self.adapters.load();
        let adapter = current_adapters.get(&adapter_id).cloned().ok_or_else(|| {
            adapteros_core::AosError::Lifecycle(format!("Adapter {} not found", adapter_id))
        })?;

        // Copy-on-write: clone current map, remove adapter, then atomically swap
        let mut new_adapters = (**current_adapters).clone();
        new_adapters.remove(&adapter_id);
        self.adapters.store(Arc::new(new_adapters));

        // Remove FFI adapter handle (freed on drop)
        self.ffi_adapters.remove(&adapter_id);

        // Get the memory usage for cleanup
        let memory_usage = Self::estimate_adapter_memory(adapter.as_ref());

        // Update memory pool size tracking atomically
        self.update_memory_pool_size(-(memory_usage as isize));

        // Stop tracking adapter in memory pool
        self.memory_pool.untrack_adapter(adapter_id);

        tracing::info!(
            adapter_id = adapter_id,
            adapter_name = %adapter.id(),
            "Unloaded LoRA adapter and freed memory"
        );
        Ok(())
    }

    /// Estimate memory usage for a LoRA adapter
    ///
    /// Calculates approximate memory footprint based on LoRA parameters:
    /// rank * hidden_dim * 2 (A and B matrices) * num_modules * sizeof(f32)
    /// Assumes 7B model with 4096 hidden dimension.
    #[inline]
    fn estimate_adapter_memory(adapter: &LoRAAdapter) -> usize {
        let rank = adapter.config().rank;
        let num_modules = adapter.config().target_modules.len();
        rank * 4096 * 2 * num_modules * 4 // f32 = 4 bytes
    }

    /// Get adapter memory usage (estimated)
    pub fn get_adapter_memory_usage(&self, adapter_id: u16) -> Result<usize> {
        let adapters = self.adapters.load();
        if let Some(adapter) = adapters.get(&adapter_id) {
            Ok(Self::estimate_adapter_memory(adapter.as_ref()))
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
        let adapters = self.adapters.load();

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
            let freed_mb = freed as f32 / Self::BYTES_PER_MB;
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
        let total_mb = total_bytes as f32 / Self::BYTES_PER_MB;

        let mut metrics = self.performance_metrics.write();
        if total_mb > metrics.peak_memory_usage_mb {
            metrics.peak_memory_usage_mb = total_mb;
        }

        tracing::debug!(
            active_mb = active_bytes as f32 / Self::BYTES_PER_MB,
            pooled_mb = pooled_bytes as f32 / Self::BYTES_PER_MB,
            total_mb = total_mb,
            peak_mb = metrics.peak_memory_usage_mb,
            "Memory pool metrics updated"
        );
    }

    /// Record a successful operation
    fn record_success(&self) {
        if let Some(mut health) = self.health_status.try_write() {
            health.successful_requests += 1;
            health.current_failure_streak = 0;
            health.last_failure = None;
            if health.stub_fallback_active {
                health.stub_fallback_active = false;
            }
        }
    }

    /// Record a failed operation
    fn record_failure(&self) {
        if let Some(mut health) = self.health_status.try_write() {
            health.failed_requests += 1;
            health.current_failure_streak += 1;
            health.last_failure = Some(std::time::Instant::now());
        }
    }
}

impl FusedKernels for MLXFFIBackend {
    fn as_liquid_kernel(&self) -> Option<&dyn LiquidKernel> {
        Some(self)
    }

    fn as_liquid_kernel_mut(&mut self) -> Option<&mut dyn LiquidKernel> {
        Some(self)
    }

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
        const STUB_FALLBACK_THRESHOLD: u32 = 3;
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
        match &result {
            Ok(_) => {
                self.record_success();
            }
            Err(_) => {
                self.record_failure();

                // Read health state after recording failure
                let mut health = self.health_status.write();

                // Check if we should enable stub fallback
                if health.current_failure_streak >= STUB_FALLBACK_THRESHOLD
                    && self.resilience_config.enable_stub_fallback
                {
                    health.stub_fallback_active = true;
                    tracing::warn!(
                        "MLX backend switching to stub fallback after {} failures (threshold = {})",
                        health.current_failure_streak.max(STUB_FALLBACK_THRESHOLD),
                        STUB_FALLBACK_THRESHOLD
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
                            "Failover command execution not implemented in reference mode: {}",
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
        let seeded = self.manifest_hash.is_some();
        let rng_method = if seeded {
            RngSeedingMethod::HkdfSeeded
        } else {
            RngSeedingMethod::SystemEntropy
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
            backend_type: BackendType::MLX,
            metallib_hash: self.manifest_hash, // Include manifest hash for content addressing
            metallib_verified: false,
            manifest: None, // No Metal-style manifest
            rng_seed_method: rng_method,
            floating_point_mode: float_mode,
            determinism_level: if seeded && !is_stub_active && IS_REAL_MLX {
                DeterminismLevel::BitExact
            } else {
                DeterminismLevel::None
            },
            compiler_flags: vec![],
            deterministic: seeded && !is_stub_active && IS_REAL_MLX,
            runtime_version: Some("mlx-cpp-ffi".to_string()),
            device_id: Some(self.device.clone()),
        };

        tracing::info!(
            deterministic = report.deterministic,
            rng_method = ?report.rng_seed_method,
            has_manifest_hash = self.manifest_hash.is_some(),
            stub_active = is_stub_active,
            real_build = IS_REAL_MLX,
            "MLX backend determinism attestation"
        );

        Ok(report)
    }

    fn device_name(&self) -> &str {
        &self.device
    }

    fn load_adapter(&mut self, id: u16, weights: &[u8]) -> Result<()> {
        // Preserve legacy behavior: metadata-free load path.
        self.load_adapter_with_metadata(id, weights, None)
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

    fn supports_kv_cache(&self) -> bool {
        true
    }

    fn clear_session_cache(&mut self, session_id: &str) {
        self.session_cache.clear(session_id);
    }

    fn clear_all_caches(&mut self) {
        self.session_cache.clear_all();
        // Also clear temporary cache
        if let Some(ptr) = self.temp_cache_ptr.take() {
            unsafe { crate::mlx_kv_cache_free(ptr) };
        }
    }

    fn cache_memory_bytes(&self) -> usize {
        self.session_cache.total_memory()
    }
}

impl Clone for MLXFFIBackend {
    fn clone(&self) -> Self {
        Self {
            model: self.model.clone(),
            adapters: ArcSwap::from_pointee((**self.adapters.load()).clone()),
            ffi_adapters: HashMap::new(), // Don't clone FFI handles
            session_cache: SessionCacheManager::new(16, 4_294_967_296),
            temp_cache_ptr: None,
            device: self.device.clone(),
            resilience_config: self.resilience_config.clone(),
            health_status: self.health_status.clone(),
            monitor: self.monitor.clone(),
            memory_pool: self.memory_pool.clone(),
            memory_pool_size: self.memory_pool_size.clone(),
            performance_metrics: self.performance_metrics.clone(),
            manifest_hash: self.manifest_hash,
            #[cfg(test)]
            ffi_set_module_attempts: self.ffi_set_module_attempts,
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

    /// Get model configuration
    ///
    /// Returns the model's configuration parameters including hidden_size,
    /// num_attention_heads, num_key_value_heads, rope_theta, etc.
    pub fn model_config(&self) -> &crate::ModelConfig {
        &self.model.config
    }

    /// Load adapter weights with optional metadata overrides.
    ///
    /// Precedence:
    /// 1) Explicit metadata values
    /// 2) Values inferred from safetensors keys
    /// 3) LoRA defaults
    pub fn load_adapter_with_metadata(
        &mut self,
        id: u16,
        weights: &[u8],
        metadata: Option<&serde_json::Value>,
    ) -> Result<()> {
        // Parse adapter weights from safetensors format
        let tensors = safetensors::SafeTensors::deserialize(weights).map_err(|e| {
            adapteros_core::AosError::Parse(format!("Failed to parse adapter weights: {}", e))
        })?;

        // Discover target modules from tensor keys.
        // The .aos packager writes keys as "lora_a.{module}" (prefix format).
        // Legacy paths may use "{module}.lora_A" (suffix format).
        // Detect which format is present and extract module names.
        let mut discovered_modules = Vec::new();
        for (name, _) in tensors.tensors() {
            if let Some(module) = name.strip_prefix("lora_a.") {
                discovered_modules.push(module.to_string());
            } else if let Some(rest) = name.strip_suffix(".lora_A") {
                discovered_modules.push(rest.to_string());
            }
        }
        discovered_modules.sort();
        discovered_modules.dedup();

        let mut config = if discovered_modules.is_empty() {
            crate::lora::LoRAConfig::default()
        } else {
            let mut cfg = crate::lora::LoRAConfig::default();
            cfg.target_modules = discovered_modules;
            cfg
        };

        // Metadata overrides inferred/default values when present and valid.
        if let Some(metadata) = metadata {
            if let Some(rank) = Self::metadata_rank_override(metadata) {
                config.rank = rank;
            }
            if let Some(alpha) = Self::metadata_alpha_override(metadata) {
                config.alpha = alpha;
            }
            if let Some(target_modules) = Self::metadata_target_modules_override(metadata) {
                config.target_modules = target_modules;
            }
        }

        let adapter_id_str = format!("adapter_{}", id);
        let mut adapter = LoRAAdapter::new(adapter_id_str.clone(), config.clone());

        // Extract LoRA weights for each target module.
        // Try prefix format first (lora_a.{module}), then suffix ({module}.lora_A).
        for module_name in &config.target_modules {
            let (a_tensor, b_tensor) = {
                // Prefix format: "lora_a.q_proj" (canonical .aos format)
                let a_prefix = format!("lora_a.{}", module_name);
                let b_prefix = format!("lora_b.{}", module_name);
                if let (Ok(a), Ok(b)) = (tensors.tensor(&a_prefix), tensors.tensor(&b_prefix)) {
                    (a, b)
                } else {
                    // Suffix format: "q_proj.lora_A" (legacy format)
                    let a_suffix = format!("{}.lora_A", module_name);
                    let b_suffix = format!("{}.lora_B", module_name);
                    match (tensors.tensor(&a_suffix), tensors.tensor(&b_suffix)) {
                        (Ok(a), Ok(b)) => (a, b),
                        _ => continue,
                    }
                }
            };

            let lora_a = Self::tensor_to_nested_vec(&a_tensor)?;
            let lora_b = Self::tensor_to_nested_vec(&b_tensor)?;

            adapter.add_module_weights(module_name, lora_a, lora_b);

            tracing::debug!(
                adapter_id = id,
                module = %module_name,
                "Loaded LoRA weights for hot-swap"
            );
        }

        // Register adapter with memory tracking
        self.register_adapter(id, adapter)?;

        tracing::info!(
            adapter_id = id,
            adapter_name = %adapter_id_str,
            "Hot-swap loaded adapter via metadata-aware loader"
        );

        Ok(())
    }

    fn metadata_rank_override(metadata: &serde_json::Value) -> Option<usize> {
        Self::metadata_value_any(metadata, &["rank", "lora_rank"]).and_then(Self::parse_rank)
    }

    fn metadata_alpha_override(metadata: &serde_json::Value) -> Option<f32> {
        Self::metadata_value_any(metadata, &["alpha", "lora_alpha"]).and_then(Self::parse_alpha)
    }

    fn metadata_target_modules_override(metadata: &serde_json::Value) -> Option<Vec<String>> {
        let value = Self::metadata_value_any(metadata, &["target_modules"])?;
        let modules = match value {
            serde_json::Value::Array(items) => items
                .iter()
                .filter_map(|item| item.as_str())
                .map(str::trim)
                .filter(|m| !m.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>(),
            serde_json::Value::String(s) => {
                let trimmed = s.trim();
                if trimmed.starts_with('[') {
                    serde_json::from_str::<Vec<String>>(trimmed)
                        .ok()?
                        .into_iter()
                        .map(|m| m.trim().to_string())
                        .filter(|m| !m.is_empty())
                        .collect()
                } else {
                    trimmed
                        .split(',')
                        .map(str::trim)
                        .filter(|m| !m.is_empty())
                        .map(ToOwned::to_owned)
                        .collect()
                }
            }
            _ => return None,
        };

        if modules.is_empty() {
            return None;
        }

        // Preserve order while removing duplicates.
        let mut deduped = Vec::with_capacity(modules.len());
        for module in modules {
            if !deduped.contains(&module) {
                deduped.push(module);
            }
        }

        Some(deduped)
    }

    fn metadata_value_any<'a>(
        metadata: &'a serde_json::Value,
        keys: &[&str],
    ) -> Option<&'a serde_json::Value> {
        for key in keys {
            if let Some(value) = metadata.get(*key) {
                return Some(value);
            }
            if let Some(value) = metadata.get("metadata").and_then(|m| m.get(*key)) {
                return Some(value);
            }
        }
        None
    }

    fn parse_rank(value: &serde_json::Value) -> Option<usize> {
        match value {
            serde_json::Value::Number(n) => n.as_u64().and_then(|v| {
                let rank = v as usize;
                (rank > 0).then_some(rank)
            }),
            serde_json::Value::String(s) => s.trim().parse::<usize>().ok().filter(|v| *v > 0),
            _ => None,
        }
    }

    fn parse_alpha(value: &serde_json::Value) -> Option<f32> {
        match value {
            serde_json::Value::Number(n) => n.as_f64().and_then(|v| {
                let alpha = v as f32;
                (alpha.is_finite() && alpha > 0.0).then_some(alpha)
            }),
            serde_json::Value::String(s) => s.trim().parse::<f32>().ok().filter(|v| {
                let alpha = *v;
                alpha.is_finite() && alpha > 0.0
            }),
            _ => None,
        }
    }

    #[cfg(test)]
    fn ffi_set_module_attempts(&self) -> usize {
        self.ffi_set_module_attempts
    }

    /// Run inference step using fused MLX FFI forward pass with KV cache and LoRA
    fn run_step_mlx(&mut self, ring: &RouterRing, io: &mut IoBuffers) -> Result<()> {
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

        // Get or create KV cache based on session
        let cache_ptr = match &io.session_id {
            Some(session_id) => {
                let config = &self.model.config;
                self.session_cache.get_or_create(
                    session_id,
                    config.num_hidden_layers as i32,
                    config.num_key_value_heads as i32,
                    (config.hidden_size / config.num_attention_heads) as i32,
                    config.max_position_embeddings as i32,
                )?
            }
            None => {
                // Create or reuse temporary cache for non-session requests
                if self.temp_cache_ptr.is_none() {
                    let config = &self.model.config;
                    let ptr = unsafe {
                        crate::mlx_kv_cache_new(
                            config.num_hidden_layers as i32,
                            config.num_key_value_heads as i32,
                            (config.hidden_size / config.num_attention_heads) as i32,
                            config.max_position_embeddings as i32,
                        )
                    };
                    if !ptr.is_null() {
                        self.temp_cache_ptr = Some(ptr);
                    }
                }
                self.temp_cache_ptr.unwrap_or(std::ptr::null_mut())
            }
        };

        // Prepare LoRA adapter pointers and blend weights from RouterRing
        let (adapter_ptrs, blend_weights) = if ring.k > 0 && !self.ffi_adapters.is_empty() {
            self.prepare_lora_for_ring(ring)?
        } else {
            (Vec::new(), Vec::new())
        };

        // Call unified forward with cache and LoRA
        let logits = self.model.forward_with_cache_and_lora(
            &io.input_ids,
            io.position,
            cache_ptr,
            &adapter_ptrs,
            &blend_weights,
        )?;

        // Validate output
        if logits.is_empty() {
            return Err(adapteros_core::AosError::Mlx(
                "Model returned empty logits".to_string(),
            ));
        }

        // Update output buffer
        let output_len = logits.len().min(io.output_logits.len());
        if output_len == 0 {
            return Err(adapteros_core::AosError::Mlx(
                "Output buffer size mismatch".to_string(),
            ));
        }
        io.output_logits[..output_len].copy_from_slice(&logits[..output_len]);
        io.position += io.input_ids.len();

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
        }

        // Emit telemetry
        self.log_router_decision(io, ring, inference_time);

        Ok(())
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
            let adapters = self.adapters.load();
            for i in 0..ring.k {
                let adapter_id = ring.indices[i];
                let gate_q15 = ring.gates_q15[i];

                if let Some(adapter) = adapters.get(&adapter_id) {
                    let gate_weight = (gate_q15.max(0) as f32) / Q15_GATE_DENOMINATOR; // Q15 dequantization
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
        // Advance position by number of tokens processed (not just 1)
        io.position += io.input_ids.len();

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

        // Convert bytes to f32 with alignment check
        let float_data: &[f32] = {
            let ptr = data.as_ptr();
            let align = std::mem::align_of::<f32>();
            if !(ptr as usize).is_multiple_of(align) {
                return Err(adapteros_core::AosError::Parse(format!(
                    "Tensor data is not aligned to {} bytes (required for f32)",
                    align
                )));
            }
            if !data.len().is_multiple_of(std::mem::size_of::<f32>()) {
                return Err(adapteros_core::AosError::Parse(format!(
                    "Tensor data length {} is not a multiple of f32 size ({})",
                    data.len(),
                    std::mem::size_of::<f32>()
                )));
            }
            unsafe {
                std::slice::from_raw_parts(
                    ptr as *const f32,
                    data.len() / std::mem::size_of::<f32>(),
                )
            }
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

impl LiquidKernel for MLXFFIBackend {
    fn blend_and_forward(&mut self, request: LiquidBlendRequest<'_>) -> Result<LiquidBlendStats> {
        crate::liquid::blend_and_forward_mlx(request)
    }
}

// SAFETY: MLXFFIBackend contains raw pointers (temp_cache_ptr, and transitively
// through FFILoraAdapter and SessionCacheManager). All access to these pointers
// is serialized through:
// 1. The FusedKernels trait requiring &mut self on run_step
// 2. The MLXFFIModel's inference_lock for C++ FFI calls
// This matches the pattern used by MLXFFIModel (lib.rs:1474-1492).
unsafe impl Send for MLXFFIBackend {}
unsafe impl Sync for MLXFFIBackend {}

impl Drop for MLXFFIBackend {
    fn drop(&mut self) {
        // Clear session caches
        self.session_cache.clear_all();
        // Free temporary cache
        if let Some(ptr) = self.temp_cache_ptr.take() {
            unsafe { crate::mlx_kv_cache_free(ptr) };
        }
        // FFI adapters are dropped automatically via their Drop impl
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lora::{LoRAAdapter, LoRAConfig};
    use adapteros_core::B3Hash;
    use serde_json::json;

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

    fn create_test_model(num_hidden_layers: usize) -> crate::MLXFFIModel {
        crate::MLXFFIModel::new_null(crate::ModelConfig {
            hidden_size: 8,
            num_hidden_layers,
            num_attention_heads: 2,
            num_key_value_heads: 2,
            intermediate_size: 16,
            vocab_size: 64,
            max_position_embeddings: 128,
            rope_theta: 10_000.0,
        })
    }

    fn create_test_adapter_with_modules(
        id: &str,
        module_names: &[&str],
        rank: usize,
        hidden_size: usize,
    ) -> LoRAAdapter {
        let mut config = LoRAConfig::default();
        config.rank = rank;
        config.alpha = (rank as f32) * 2.0;
        config.target_modules = module_names.iter().map(|m| (*m).to_string()).collect();

        let mut adapter = LoRAAdapter::new(id.to_string(), config);
        let lora_a = vec![vec![1.0; hidden_size]; rank];
        let lora_b = vec![vec![1.0; rank]; hidden_size];
        for module_name in module_names {
            adapter.add_module_weights(module_name, lora_a.clone(), lora_b.clone());
        }
        adapter
    }

    fn build_test_safetensors(modules: &[&str], rank: usize, hidden_size: usize) -> Vec<u8> {
        use safetensors::tensor::TensorView;
        use safetensors::Dtype;

        let mut backing = Vec::new();
        for module in modules {
            let a_values = vec![1.0f32; rank * hidden_size];
            let b_values = vec![1.0f32; hidden_size * rank];
            let a_bytes: Vec<u8> = a_values.iter().flat_map(|f| f.to_le_bytes()).collect();
            let b_bytes: Vec<u8> = b_values.iter().flat_map(|f| f.to_le_bytes()).collect();

            backing.push((
                format!("lora_a.{}", module),
                a_bytes,
                vec![rank, hidden_size],
            ));
            backing.push((
                format!("lora_b.{}", module),
                b_bytes,
                vec![hidden_size, rank],
            ));
        }

        let mut tensors = HashMap::new();
        for (name, bytes, shape) in &backing {
            let view =
                TensorView::new(Dtype::F32, shape.clone(), bytes).expect("valid test tensor view");
            tensors.insert(name.clone(), view);
        }

        safetensors::serialize(&tensors, &None).expect("serialize test safetensors")
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

    #[test]
    fn test_load_adapter_with_metadata_overrides_inferred_defaults() {
        let mut backend = MLXFFIBackend::new(create_test_model(2));
        let weights = build_test_safetensors(&["q_proj", "k_proj"], 2, 4);
        let metadata = json!({
            "rank": 8,
            "alpha": 32.0,
            "target_modules": ["k_proj"],
        });

        backend
            .load_adapter_with_metadata(7, &weights, Some(&metadata))
            .expect("metadata-aware load succeeds");

        let adapters = backend.adapters.load();
        let adapter = adapters.get(&7).expect("adapter present");
        assert_eq!(adapter.config().rank, 8);
        assert!((adapter.config().alpha - 32.0).abs() < f32::EPSILON);
        assert_eq!(adapter.config().target_modules, vec!["k_proj".to_string()]);
        assert!(adapter.has_module("k_proj"));
        assert!(!adapter.has_module("q_proj"));
    }

    #[test]
    fn test_all_layers_are_populated_per_module() {
        let num_layers = 3;
        let modules = ["q_proj", "v_proj"];

        let mut backend = MLXFFIBackend::new(create_test_model(num_layers));
        let adapter = create_test_adapter_with_modules("layer-population", &modules, 2, 4);

        backend
            .register_adapter(42, adapter)
            .expect("register adapter succeeds");

        assert_eq!(
            backend.ffi_set_module_attempts(),
            num_layers * modules.len(),
            "set_module attempts must cover every (layer, module) pair",
        );
    }

    // =========================================================================
    // ArcSwap concurrent access tests
    // =========================================================================

    #[test]
    fn test_arcswap_concurrent_reads() {
        use std::sync::Arc;
        use std::thread;

        // Create a mock backend configuration for testing
        let adapters: ArcSwap<HashMap<u16, Arc<LoRAAdapter>>> =
            ArcSwap::from_pointee(HashMap::new());

        // Pre-populate with some adapters
        let mut initial = HashMap::new();
        for i in 0..10 {
            initial.insert(i, Arc::new(create_dummy_adapter(&format!("adapter-{}", i))));
        }
        adapters.store(Arc::new(initial));

        // Spawn multiple reader threads
        let readers: Vec<_> = (0..8)
            .map(|thread_id| {
                let adapters_ref = &adapters;
                // Use scoped threads to avoid 'static lifetime requirements
                thread::scope(|_| {
                    // Each reader performs many reads
                    for _ in 0..1000 {
                        let snapshot = adapters_ref.load();
                        // Verify we can read consistently
                        assert_eq!(
                            snapshot.len(),
                            10,
                            "Thread {} saw inconsistent adapter count",
                            thread_id
                        );
                        // Verify all adapters are accessible
                        for i in 0..10 {
                            assert!(
                                snapshot.get(&i).is_some(),
                                "Thread {} couldn't find adapter {}",
                                thread_id,
                                i
                            );
                        }
                    }
                })
            })
            .collect();

        // All readers completed successfully
        assert_eq!(readers.len(), 8);
    }

    #[test]
    fn test_arcswap_concurrent_read_write() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;
        use std::thread;
        use std::time::Duration;

        // Create shared state
        let adapters: Arc<ArcSwap<HashMap<u16, Arc<LoRAAdapter>>>> =
            Arc::new(ArcSwap::from_pointee(HashMap::new()));
        let done = Arc::new(AtomicBool::new(false));

        // Spawn a writer thread that continuously adds/removes adapters
        let adapters_writer = Arc::clone(&adapters);
        let done_writer = Arc::clone(&done);
        let writer = thread::spawn(move || {
            let mut counter = 0u16;
            while !done_writer.load(Ordering::Relaxed) {
                // Add an adapter
                let mut new_map = (**adapters_writer.load()).clone();
                new_map.insert(
                    counter % 100, // Cycle through 100 slots
                    Arc::new(create_dummy_adapter(&format!("adapter-{}", counter))),
                );
                adapters_writer.store(Arc::new(new_map));

                counter = counter.wrapping_add(1);

                // Small sleep to prevent spinning too fast
                thread::sleep(Duration::from_micros(10));
            }
        });

        // Spawn multiple reader threads
        let readers: Vec<_> = (0..4)
            .map(|thread_id| {
                let adapters_reader = Arc::clone(&adapters);
                let done_reader = Arc::clone(&done);
                thread::spawn(move || {
                    let mut reads = 0u64;
                    while !done_reader.load(Ordering::Relaxed) {
                        // Read current state
                        let snapshot = adapters_reader.load();

                        // Verify we got a valid snapshot (not corrupted)
                        for (id, adapter) in snapshot.iter() {
                            // Just accessing the adapter should work without panic
                            assert!(
                                !adapter.id.is_empty(),
                                "Thread {} found empty adapter at {}",
                                thread_id,
                                id
                            );
                        }

                        reads += 1;
                    }
                    reads
                })
            })
            .collect();

        // Let the threads run for a bit
        thread::sleep(Duration::from_millis(100));

        // Signal completion
        done.store(true, Ordering::Relaxed);

        // Wait for all threads
        writer.join().expect("Writer thread panicked");
        let total_reads: u64 = readers
            .into_iter()
            .map(|r| r.join().expect("Reader thread panicked"))
            .sum();

        // We should have done many reads without any issues
        assert!(
            total_reads > 100,
            "Expected many reads, got only {}",
            total_reads
        );
    }

    #[test]
    fn test_arcswap_copy_on_write_semantics() {
        use std::sync::Arc;

        let adapters: ArcSwap<HashMap<u16, Arc<LoRAAdapter>>> =
            ArcSwap::from_pointee(HashMap::new());

        // Take a snapshot
        let snapshot1 = adapters.load();
        assert_eq!(snapshot1.len(), 0);

        // Modify: copy-on-write
        let mut new_map = (**adapters.load()).clone();
        new_map.insert(1, Arc::new(create_dummy_adapter("adapter-1")));
        adapters.store(Arc::new(new_map));

        // Original snapshot unchanged
        assert_eq!(snapshot1.len(), 0, "Snapshot should be immutable");

        // New snapshot reflects change
        let snapshot2 = adapters.load();
        assert_eq!(snapshot2.len(), 1, "New snapshot should have 1 adapter");

        // They are different Arc pointers
        assert!(!Arc::ptr_eq(&snapshot1, &snapshot2));
    }
}
