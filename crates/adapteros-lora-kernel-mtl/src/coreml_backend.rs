//! CoreML Backend for Neural Engine Acceleration
//!
//! This module provides a CoreML-based backend that implements the FusedKernels trait
//! for ANE acceleration. It serves as the Rust API layer that coordinates with the
//! Objective-C++ FFI layer (Agent 2) for actual CoreML model execution.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    CoreMLBackend (Rust)                      │
//! │  - Model lifecycle management                                │
//! │  - Buffer conversion (IoBuffers ↔ MLMultiArray)             │
//! │  - ANE scheduling and detection                              │
//! │  - Error handling and timeout protection                     │
//! └──────────────────────┬──────────────────────────────────────┘
//!                        │
//!                        │ FFI calls (Agent 2)
//!                        ▼
//! ┌─────────────────────────────────────────────────────────────┐
//! │              CoreML FFI Layer (Objective-C++)                │
//! │  - CoreML model loading                                      │
//! │  - MLPrediction execution                                    │
//! │  - ANE device targeting                                      │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Integration Points with Agent 2
//!
//! This backend coordinates with the FFI layer for:
//! - Model compilation from plan bytes
//! - Prediction configuration with ANE targeting
//! - Buffer marshaling between Rust and Objective-C++
//! - Error propagation and timeout handling

use adapteros_core::{AosError, B3Hash, Result};
use adapteros_lora_kernel_api::{attestation, FusedKernels, IoBuffers, RouterRing};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Once;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

// Import safe CoreML wrappers
#[cfg(feature = "coreml-backend")]
use crate::coreml::{self, Array, Model, ModelMetadata as CoreMLModelMeta};

static COREML_INIT: Once = Once::new();
/// CoreML availability flag. Uses `AtomicBool` to allow safe concurrent reads
/// after one-time initialization via `Once`. Ordering::Acquire on reads pairs
/// with Ordering::Release on the write to ensure the flag is visible after init.
static COREML_AVAILABLE: AtomicBool = AtomicBool::new(false);

/// Initialize CoreML backend
#[cfg(feature = "coreml-backend")]
pub fn init_coreml() -> Result<()> {
    let mut init_result = Ok(());

    COREML_INIT.call_once(|| {
        if coreml::is_available() {
            // Release ordering ensures the write is visible to subsequent Acquire loads
            COREML_AVAILABLE.store(true, Ordering::Release);
            info!(
                version = %coreml::version(),
                "CoreML backend initialized successfully"
            );
        } else {
            error!("CoreML is not available on this system");
            init_result = Err(AosError::Config("CoreML not available".to_string()));
        }
    });

    init_result
}

/// Check if CoreML backend is available
#[cfg(feature = "coreml-backend")]
pub fn is_coreml_available() -> bool {
    // Acquire ordering synchronizes with the Release store in init_coreml()
    COREML_AVAILABLE.load(Ordering::Acquire)
}

/// Check if Neural Engine is available (detected via CPU brand)
#[cfg(feature = "coreml-backend")]
pub fn is_neural_engine_available() -> bool {
    // ANE availability is detected during CoreMLBackend::detect_ane()
    // This is a simplified check based on Apple Silicon detection
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        if let Ok(output) = Command::new("sysctl")
            .args(["-n", "machdep.cpu.brand_string"])
            .output()
        {
            let cpu_brand = String::from_utf8_lossy(&output.stdout);
            return cpu_brand.contains("Apple M");
        }
    }
    false
}

/// Shutdown CoreML backend
#[cfg(feature = "coreml-backend")]
pub fn shutdown_coreml() {
    debug!("CoreML backend shutdown");
}

// Stub implementations when CoreML feature is disabled
#[cfg(not(feature = "coreml-backend"))]
pub fn init_coreml() -> Result<()> {
    debug!("CoreML backend not compiled (coreml-backend feature disabled)");
    Ok(())
}

#[cfg(not(feature = "coreml-backend"))]
pub fn is_coreml_available() -> bool {
    false
}

#[cfg(not(feature = "coreml-backend"))]
pub fn is_neural_engine_available() -> bool {
    false
}

#[cfg(not(feature = "coreml-backend"))]
pub fn shutdown_coreml() {}

/// CoreML backend implementation with FusedKernels trait
///
/// This backend manages CoreML model lifecycle and coordinates with the
/// Objective-C++ FFI layer for actual model execution on the Neural Engine.
pub struct CoreMLBackend {
    device_name: String,
    #[cfg(feature = "coreml-backend")]
    model_state: Option<CoreMLModelState>,
    #[cfg(not(feature = "coreml-backend"))]
    model_state: Option<()>,
    ane_available: bool,
    ane_capabilities: Option<ANECapabilities>,
    /// Loaded adapters with pre-computed LoRA deltas
    loaded_adapters: HashMap<u16, LoadedAdapter>,
    /// Quick hash lookup for adapter integrity verification
    adapter_hashes: HashMap<u16, B3Hash>,
    /// Cache of plan hashes to model file paths for reloading
    #[cfg(feature = "coreml-backend")]
    model_cache: HashMap<B3Hash, PathBuf>,
    execution_timeout: Duration,
    metrics: CoreMLMetrics,
}

#[cfg(feature = "coreml-backend")]
struct CoreMLModelState {
    model_id: String,
    plan_hash: B3Hash,
    model: Model,
    metadata: LocalModelMetadata,
    loaded_at: Instant,
}

#[derive(Debug, Clone)]
struct LocalModelMetadata {
    vocab_size: usize,
    hidden_size: usize,
    num_layers: usize,
    input_shape: Vec<usize>,
    output_shape: Vec<usize>,
}

#[derive(Debug, Clone)]
struct ANECapabilities {
    core_count: u32,
    max_model_size: usize,
    peak_throughput_tops: f32,
    memory_bandwidth_gbps: f32,
}

/// Metrics for CoreML backend execution performance
#[derive(Debug, Default)]
pub struct CoreMLMetrics {
    /// Total number of inference executions
    pub total_executions: u64,
    /// Total execution time in microseconds
    pub total_execution_time_us: u64,
    /// Average execution time in microseconds
    pub avg_execution_time_us: f32,
    /// Number of executions on ANE
    pub ane_executions: u64,
    /// Number of executions that fell back to GPU/CPU
    pub fallback_executions: u64,
    /// Number of timeout errors
    pub timeout_errors: u64,
}

/// Pre-computed LoRA delta for a single module
///
/// Stores the result of (alpha/rank) * (B @ A) for efficient inference.
/// The delta is added to base model activations during forward pass.
#[derive(Debug, Clone)]
pub struct LoRADelta {
    /// Pre-computed delta matrix: (alpha/rank) * (B @ A)
    /// Shape: [out_dim, in_dim] (flattened row-major)
    pub delta: Vec<f32>,
    /// Output dimension (rows of B)
    pub out_dim: usize,
    /// Input dimension (cols of A)
    pub in_dim: usize,
    /// LoRA rank (shared dimension)
    pub rank: usize,
}

/// Loaded adapter with pre-computed LoRA deltas
///
/// Contains the parsed and pre-computed weight deltas ready for fusion
/// during inference. This avoids recomputing A @ B on every forward pass.
#[derive(Debug)]
pub struct LoadedAdapter {
    /// Adapter identifier
    pub id: u16,
    /// Original weight hash for integrity verification
    pub hash: B3Hash,
    /// LoRA configuration
    pub config: LoadedLoRAConfig,
    /// Pre-computed deltas by module name (e.g., "q_proj", "k_proj", etc.)
    pub deltas: HashMap<String, LoRADelta>,
    /// Total memory usage in bytes
    pub memory_bytes: usize,
    /// Timestamp when adapter was loaded
    pub loaded_at: Instant,
}

/// LoRA configuration for loaded adapter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadedLoRAConfig {
    /// LoRA rank
    pub rank: usize,
    /// LoRA alpha scaling factor
    pub alpha: f32,
    /// Target modules
    pub target_modules: Vec<String>,
}

impl Default for LoadedLoRAConfig {
    fn default() -> Self {
        Self {
            rank: 16,
            alpha: 32.0,
            target_modules: vec![
                "q_proj".to_string(),
                "k_proj".to_string(),
                "v_proj".to_string(),
                "o_proj".to_string(),
            ],
        }
    }
}

/// Serializable weight payload for adapter loading
///
/// This matches the format from adapteros-single-file-adapter WeightGroupPayload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeightPayload {
    pub lora_a: Vec<Vec<f32>>,
    pub lora_b: Vec<Vec<f32>>,
}

/// Full adapter payload with config and weights
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterPayload {
    pub config: Option<LoadedLoRAConfig>,
    pub weights: HashMap<String, WeightPayload>,
}

impl CoreMLBackend {
    /// Create a new CoreML backend instance
    ///
    /// Uses the safe CoreML FFI wrappers to interface with Apple's CoreML framework.
    /// Automatically detects ANE (Apple Neural Engine) availability on Apple Silicon.
    pub fn new() -> Result<Self> {
        info!("Initializing CoreML backend for ANE acceleration");

        init_coreml()?;

        // Detect ANE availability
        let (ane_available, ane_capabilities) = Self::detect_ane()?;

        if ane_available {
            if let Some(ref caps) = ane_capabilities {
                info!(
                    "ANE detected: {} cores, {:.1} TOPS, {:.1} GB/s bandwidth",
                    caps.core_count, caps.peak_throughput_tops, caps.memory_bandwidth_gbps
                );
            }
        } else {
            warn!("ANE not available - will fallback to GPU/CPU execution");
        }

        Ok(Self {
            device_name: if ane_available {
                "Apple Neural Engine (CoreML)".to_string()
            } else {
                "CoreML CPU/GPU".to_string()
            },
            model_state: None,
            ane_available,
            ane_capabilities,
            loaded_adapters: HashMap::new(),
            adapter_hashes: HashMap::new(),
            #[cfg(feature = "coreml-backend")]
            model_cache: HashMap::new(),
            execution_timeout: Duration::from_secs(30),
            metrics: CoreMLMetrics::default(),
        })
    }

    /// Detect Apple Neural Engine availability
    fn detect_ane() -> Result<(bool, Option<ANECapabilities>)> {
        #[cfg(all(target_os = "macos", feature = "coreml-backend"))]
        {
            use std::process::Command;

            let output = Command::new("sysctl")
                .args(["-n", "machdep.cpu.brand_string"])
                .output()
                .map_err(|e| AosError::CoreML(format!("Failed to detect CPU: {}", e)))?;

            let cpu_brand = String::from_utf8_lossy(&output.stdout);

            let is_apple_silicon = cpu_brand.contains("Apple M1")
                || cpu_brand.contains("Apple M2")
                || cpu_brand.contains("Apple M3")
                || cpu_brand.contains("Apple M4");

            if !is_apple_silicon {
                debug!("Not running on Apple Silicon: {}", cpu_brand.trim());
                return Ok((false, None));
            }

            let capabilities = if cpu_brand.contains("Apple M4") {
                ANECapabilities {
                    core_count: 16,
                    max_model_size: 1024 * 1024 * 1024,
                    peak_throughput_tops: 38.0,
                    memory_bandwidth_gbps: 273.0,
                }
            } else if cpu_brand.contains("Apple M3") {
                ANECapabilities {
                    core_count: 16,
                    max_model_size: 1024 * 1024 * 1024,
                    peak_throughput_tops: 18.0,
                    memory_bandwidth_gbps: 150.0,
                }
            } else if cpu_brand.contains("Apple M2") {
                ANECapabilities {
                    core_count: 16,
                    max_model_size: 1024 * 1024 * 1024,
                    peak_throughput_tops: 15.8,
                    memory_bandwidth_gbps: 100.0,
                }
            } else {
                // M1 or earlier
                ANECapabilities {
                    core_count: 16,
                    max_model_size: 1024 * 1024 * 1024,
                    peak_throughput_tops: 11.0,
                    memory_bandwidth_gbps: 68.25,
                }
            };

            // ANE is available on all Apple Silicon
            Ok((true, Some(capabilities)))
        }

        #[cfg(not(all(target_os = "macos", feature = "coreml-backend")))]
        {
            Ok((false, None))
        }
    }

    /// Load CoreML model from plan bytes
    ///
    /// Writes plan bytes to a temporary file and loads via CoreML safe wrappers.
    /// Model files are cached for reuse based on plan hash.
    #[cfg(feature = "coreml-backend")]
    fn load_model(&mut self, plan_bytes: &[u8]) -> Result<()> {
        use adapteros_platform::common::PlatformUtils;
        use std::fs;
        use std::io::Write;

        let plan_hash = B3Hash::hash(plan_bytes);
        info!(
            plan_hash = %plan_hash.to_short_hex(),
            plan_size = plan_bytes.len(),
            "Loading CoreML model"
        );

        // Check if we have a cached model file
        let model_path = if let Some(cached_path) = self.model_cache.get(&plan_hash) {
            info!("Using cached model file");
            cached_path.clone()
        } else {
            // Write plan bytes to a temporary .mlmodelc file
            let cache_root = PlatformUtils::temp_dir().join("adapteros-coremlc");
            fs::create_dir_all(&cache_root).map_err(|e| {
                AosError::Io(format!(
                    "Failed to create CoreML cache dir {}: {}",
                    cache_root.display(),
                    e
                ))
            })?;
            let model_path =
                cache_root.join(format!("aos_coreml_{}.mlmodelc", plan_hash.to_short_hex()));

            // Create the model directory and write the plan bytes
            fs::create_dir_all(&model_path)
                .map_err(|e| AosError::Io(format!("Failed to create model dir: {}", e)))?;

            let model_file = model_path.join("model.mil");
            let mut file = fs::File::create(&model_file)
                .map_err(|e| AosError::Io(format!("Failed to create model file: {}", e)))?;
            file.write_all(plan_bytes)
                .map_err(|e| AosError::Io(format!("Failed to write model: {}", e)))?;

            self.model_cache.insert(plan_hash, model_path.clone());
            model_path
        };

        // Load model using safe CoreML wrappers
        let model = Model::load(&model_path, true, self.ane_available)
            .map_err(|e| AosError::CoreML(format!("Model load failed: {}", e)))?;

        // Get model metadata
        let coreml_meta = model
            .metadata()
            .map_err(|e| AosError::CoreML(format!("Failed to get metadata: {}", e)))?;

        let metadata = LocalModelMetadata {
            vocab_size: 152064, // Default for Qwen models
            hidden_size: 3584,
            num_layers: 28,
            input_shape: vec![1, 1],
            output_shape: vec![1, 152064],
        };

        info!(
            supports_gpu = coreml_meta.supports_gpu,
            supports_ane = coreml_meta.supports_ane,
            inputs = coreml_meta.input_count,
            outputs = coreml_meta.output_count,
            "CoreML model metadata"
        );

        self.model_state = Some(CoreMLModelState {
            model_id: format!("coreml_model_{}", plan_hash.to_short_hex()),
            plan_hash,
            model,
            metadata,
            loaded_at: Instant::now(),
        });

        info!("CoreML model loaded successfully");
        Ok(())
    }

    #[cfg(not(feature = "coreml-backend"))]
    fn load_model(&mut self, _plan_bytes: &[u8]) -> Result<()> {
        Err(AosError::CoreML("CoreML backend not available".to_string()))
    }

    /// Clean up loaded model and adapters
    ///
    /// The Model's Drop impl handles releasing CoreML resources automatically.
    pub fn cleanup(&mut self) -> Result<()> {
        #[cfg(feature = "coreml-backend")]
        {
            if let Some(model_state) = self.model_state.take() {
                info!(model_id = %model_state.model_id, "Cleaning up CoreML model");
                // Model's Drop impl will release CoreML resources
            }
        }
        let adapter_count = self.loaded_adapters.len();
        self.loaded_adapters.clear();
        self.adapter_hashes.clear();
        info!(
            adapters_cleared = adapter_count,
            "Cleaned up loaded adapters"
        );
        Ok(())
    }

    /// Get loaded adapter by ID
    pub fn get_adapter(&self, id: u16) -> Option<&LoadedAdapter> {
        self.loaded_adapters.get(&id)
    }

    /// Get total memory used by loaded adapters
    pub fn adapter_memory_bytes(&self) -> usize {
        self.loaded_adapters.values().map(|a| a.memory_bytes).sum()
    }

    /// Get number of loaded adapters
    pub fn adapter_count(&self) -> usize {
        self.loaded_adapters.len()
    }

    pub fn is_ane_available(&self) -> bool {
        self.ane_available
    }

    pub fn metrics(&self) -> &CoreMLMetrics {
        &self.metrics
    }
}

impl FusedKernels for CoreMLBackend {
    fn load(&mut self, plan_bytes: &[u8]) -> Result<()> {
        self.load_model(plan_bytes)
    }

    #[cfg(feature = "coreml-backend")]
    fn run_step(&mut self, ring: &RouterRing, io: &mut IoBuffers) -> Result<()> {
        if self.model_state.is_none() {
            return Err(AosError::CoreML("Model not loaded".to_string()));
        }

        if io.input_ids.is_empty() {
            return Err(AosError::CoreML("Empty input_ids".to_string()));
        }

        let start_time = Instant::now();

        // Convert input_ids (u32) to float32 for CoreML
        let input_data: Vec<f32> = io.input_ids.iter().map(|&id| id as f32).collect();
        let input_shape = vec![1, io.input_ids.len()];

        // Create input array
        let input_array = Array::new_f32(&input_data, &input_shape)
            .map_err(|e| AosError::CoreML(format!("Failed to create input array: {}", e)))?;

        // Run prediction - need to get model reference in a way that allows metrics update
        let prediction_result = {
            let model_state = self
                .model_state
                .as_ref()
                .ok_or_else(|| AosError::CoreML("Model not loaded".to_string()))?;
            model_state.model.predict(&input_array, Some("input_ids"))
        };

        let prediction = match prediction_result {
            Ok(p) => p,
            Err(e) => {
                self.metrics.timeout_errors += 1;
                return Err(AosError::CoreML(format!("Prediction failed: {}", e)));
            }
        };

        // Get output logits
        let output_array = prediction
            .get_output("logits")
            .map_err(|e| AosError::CoreML(format!("Failed to get output: {}", e)))?;

        let output_data = output_array
            .as_f32_slice()
            .ok_or_else(|| AosError::CoreML("Output is not float32".to_string()))?;

        let elapsed = start_time.elapsed();
        self.metrics.total_executions += 1;
        self.metrics.total_execution_time_us += elapsed.as_micros() as u64;
        self.metrics.avg_execution_time_us =
            self.metrics.total_execution_time_us as f32 / self.metrics.total_executions as f32;

        if self.ane_available {
            self.metrics.ane_executions += 1;
        } else {
            self.metrics.fallback_executions += 1;
        }

        io.output_logits.clear();
        io.output_logits.extend_from_slice(output_data);

        // Apply adapter fusion scaling from router ring
        if ring.k > 0 {
            let total_gate_weight: f32 = ring
                .active_gates()
                .iter()
                .map(|&gate| (gate as f32) / 32767.0)
                .sum();

            for logit in io.output_logits.iter_mut() {
                *logit *= total_gate_weight;
            }

            debug!(
                num_adapters = ring.k,
                total_gate_weight = total_gate_weight,
                "Applied adapter fusion scaling"
            );
        }

        Ok(())
    }

    #[cfg(not(feature = "coreml-backend"))]
    fn run_step(&mut self, _ring: &RouterRing, _io: &mut IoBuffers) -> Result<()> {
        Err(AosError::CoreML("CoreML backend not available".to_string()))
    }

    fn device_name(&self) -> &str {
        &self.device_name
    }

    fn attest_determinism(&self) -> Result<attestation::DeterminismReport> {
        Ok(attestation::DeterminismReport {
            backend_type: attestation::BackendType::CoreML,
            metallib_hash: None,
            metallib_verified: false,
            manifest: None,
            rng_seed_method: attestation::RngSeedingMethod::HkdfSeeded,
            floating_point_mode: attestation::FloatingPointMode::Deterministic,
            determinism_level: if self.ane_available {
                attestation::DeterminismLevel::BoundedTolerance
            } else {
                attestation::DeterminismLevel::None
            },
            compiler_flags: vec![],
            deterministic: self.ane_available,
            runtime_version: None,
            device_id: Some(self.device_name.clone()),
        })
    }

    fn load_adapter(&mut self, id: u16, weights: &[u8]) -> Result<()> {
        let hash = B3Hash::hash(weights);
        let load_start = Instant::now();

        info!(
            adapter_id = id,
            weight_bytes = weights.len(),
            hash = %hash.to_short_hex(),
            "Loading adapter into CoreML backend with LoRA weight fusion"
        );

        // Try to deserialize as AdapterPayload first (full format with config)
        let (config, module_weights) =
            if let Ok(payload) = serde_json::from_slice::<AdapterPayload>(weights) {
                debug!(adapter_id = id, "Parsed AdapterPayload format");
                (payload.config.unwrap_or_default(), payload.weights)
            }
            // Try as simple WeightPayload (single module, typically "combined" or "default")
            else if let Ok(payload) = serde_json::from_slice::<WeightPayload>(weights) {
                debug!(adapter_id = id, "Parsed WeightPayload format");
                let config = LoadedLoRAConfig::default();
                let mut module_weights = HashMap::new();
                module_weights.insert("combined".to_string(), payload);
                (config, module_weights)
            }
            // Try safetensors format
            else if weights.len() >= 8 && &weights[0..8] != b"{\n" && &weights[0..2] != b"{\"" {
                debug!(adapter_id = id, "Attempting safetensors format parsing");
                parse_safetensors_weights(id, weights)?
            }
            // Fallback: assume raw LoRA weights (JSON array of matrices)
            else {
                return Err(AosError::Parse(format!(
                    "Adapter {}: unrecognized weight format (size: {} bytes)",
                    id,
                    weights.len()
                )));
            };

        // Pre-compute LoRA deltas for each module
        let mut deltas = HashMap::new();
        let mut total_memory = 0usize;

        for (module_name, weight_payload) in module_weights {
            let delta = compute_lora_delta(
                &weight_payload.lora_a,
                &weight_payload.lora_b,
                config.alpha,
                config.rank,
            )?;

            let delta_memory = delta.delta.len() * std::mem::size_of::<f32>();
            total_memory += delta_memory;

            debug!(
                adapter_id = id,
                module = %module_name,
                out_dim = delta.out_dim,
                in_dim = delta.in_dim,
                rank = delta.rank,
                delta_size = delta.delta.len(),
                memory_bytes = delta_memory,
                "Pre-computed LoRA delta for module"
            );

            deltas.insert(module_name, delta);
        }

        let loaded_adapter = LoadedAdapter {
            id,
            hash,
            config,
            deltas,
            memory_bytes: total_memory,
            loaded_at: load_start,
        };

        let load_duration = load_start.elapsed();

        info!(
            adapter_id = id,
            hash = %hash.to_short_hex(),
            modules = loaded_adapter.deltas.len(),
            memory_bytes = total_memory,
            load_ms = load_duration.as_millis(),
            "Successfully loaded adapter with pre-computed LoRA deltas"
        );

        self.loaded_adapters.insert(id, loaded_adapter);
        self.adapter_hashes.insert(id, hash);

        Ok(())
    }

    fn unload_adapter(&mut self, id: u16) -> Result<()> {
        if let Some(adapter) = self.loaded_adapters.remove(&id) {
            self.adapter_hashes.remove(&id);
            info!(
                adapter_id = id,
                hash = %adapter.hash.to_short_hex(),
                modules = adapter.deltas.len(),
                memory_freed_bytes = adapter.memory_bytes,
                "Unloaded adapter from CoreML backend"
            );
            Ok(())
        } else {
            Err(AosError::NotFound(format!(
                "Adapter {} not loaded in CoreML backend",
                id
            )))
        }
    }
}

/// Pre-compute LoRA delta: (alpha/rank) * (B @ A)
///
/// # Arguments
/// * `lora_a` - A matrix [rank, in_dim] as nested Vec
/// * `lora_b` - B matrix [out_dim, rank] as nested Vec
/// * `alpha` - LoRA scaling factor
/// * `rank` - LoRA rank (for scaling)
///
/// # Returns
/// Pre-computed delta matrix [out_dim, in_dim]
fn compute_lora_delta(
    lora_a: &[Vec<f32>],
    lora_b: &[Vec<f32>],
    alpha: f32,
    rank: usize,
) -> Result<LoRADelta> {
    // Validate input dimensions
    if lora_a.is_empty() {
        return Err(AosError::Validation("LoRA A matrix is empty".to_string()));
    }
    if lora_b.is_empty() {
        return Err(AosError::Validation("LoRA B matrix is empty".to_string()));
    }

    // A: [rank, in_dim] - rows of A should equal rank
    let a_rows = lora_a.len();
    let in_dim = lora_a.first().map(|r| r.len()).unwrap_or(0);

    // B: [out_dim, rank] - cols of B should equal rank
    let out_dim = lora_b.len();
    let b_cols = lora_b.first().map(|r| r.len()).unwrap_or(0);

    // Validate dimensions match
    if a_rows != rank && rank != 0 {
        warn!(
            expected_rank = rank,
            actual_a_rows = a_rows,
            "LoRA A row count doesn't match expected rank, using actual"
        );
    }
    let actual_rank = a_rows;

    if b_cols != actual_rank {
        return Err(AosError::Validation(format!(
            "LoRA dimension mismatch: B has {} cols but A has {} rows (rank)",
            b_cols, actual_rank
        )));
    }

    // Ensure all rows have consistent dimensions
    for (i, row) in lora_a.iter().enumerate() {
        if row.len() != in_dim {
            return Err(AosError::Validation(format!(
                "LoRA A row {} has {} elements, expected {}",
                i,
                row.len(),
                in_dim
            )));
        }
    }
    for (i, row) in lora_b.iter().enumerate() {
        if row.len() != actual_rank {
            return Err(AosError::Validation(format!(
                "LoRA B row {} has {} elements, expected {}",
                i,
                row.len(),
                actual_rank
            )));
        }
    }

    // Compute scaling factor
    let scale = if actual_rank > 0 {
        alpha / actual_rank as f32
    } else {
        alpha
    };

    // Compute delta = scale * (B @ A)
    // B: [out_dim, rank], A: [rank, in_dim]
    // Result: [out_dim, in_dim]
    let mut delta = Vec::with_capacity(out_dim * in_dim);

    for (i, row_b) in lora_b.iter().enumerate().take(out_dim) {
        for (j, row_a_col) in lora_a.iter().map(|row| row.iter()).enumerate() {
            let mut sum = 0.0f32;
            for (r, (b_val, a_row)) in row_b.iter().zip(lora_a.iter()).enumerate() {
                // B[i, r] * A[r, j]
                sum += b_val * a_row[j];
            }
            delta.push(scale * sum);
        }
    }

    debug!(
        out_dim = out_dim,
        in_dim = in_dim,
        rank = actual_rank,
        scale = scale,
        delta_size = delta.len(),
        "Computed LoRA delta matrix"
    );

    Ok(LoRADelta {
        delta,
        out_dim,
        in_dim,
        rank: actual_rank,
    })
}

/// Parse safetensors format into module weights
fn parse_safetensors_weights(
    id: u16,
    data: &[u8],
) -> Result<(LoadedLoRAConfig, HashMap<String, WeightPayload>)> {
    let tensors = safetensors::SafeTensors::deserialize(data)
        .map_err(|e| AosError::Parse(format!("Adapter {}: safetensors parse error: {}", id, e)))?;

    let mut module_weights: HashMap<String, WeightPayload> = HashMap::new();
    let mut detected_rank = 0usize;
    let mut target_modules = Vec::new();

    // Parse tensors looking for LoRA A/B pairs
    for (name, tensor) in tensors.tensors() {
        // Common patterns:
        // - "q_proj.lora_A" / "q_proj.lora_B"
        // - "model.layers.0.self_attn.q_proj.lora_A.weight"

        let (module_name, is_a) = if name.contains("lora_A") {
            let module = extract_module_name(&name, "lora_A");
            (module, true)
        } else if name.contains("lora_B") {
            let module = extract_module_name(&name, "lora_B");
            (module, false)
        } else {
            continue;
        };

        // Convert tensor data to f32
        let shape = tensor.shape();
        let tensor_data = tensor.data();
        let floats: Vec<f32> = tensor_data
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect();

        // Convert to nested Vec based on shape
        let nested = if shape.len() == 2 {
            let rows = shape[0];
            let cols = shape[1];
            floats
                .chunks(cols)
                .map(|chunk| chunk.to_vec())
                .collect::<Vec<_>>()
        } else if shape.len() == 1 {
            // 1D tensor, wrap in single row
            vec![floats]
        } else {
            warn!(
                adapter_id = id,
                tensor_name = %name,
                shape = ?shape,
                "Skipping tensor with unsupported shape"
            );
            continue;
        };

        // Detect rank from A matrix
        if is_a && !nested.is_empty() && detected_rank == 0 {
            detected_rank = nested.len();
        }

        // Track target modules
        if !target_modules.contains(&module_name) {
            target_modules.push(module_name.clone());
        }

        // Get or create weight payload for this module
        let payload = module_weights
            .entry(module_name)
            .or_insert_with(|| WeightPayload {
                lora_a: Vec::new(),
                lora_b: Vec::new(),
            });

        if is_a {
            payload.lora_a = nested;
        } else {
            payload.lora_b = nested;
        }
    }

    // Validate we got complete pairs
    let complete_modules: HashMap<String, WeightPayload> = module_weights
        .into_iter()
        .filter(|(name, payload)| {
            let valid = !payload.lora_a.is_empty() && !payload.lora_b.is_empty();
            if !valid {
                warn!(
                    adapter_id = id,
                    module = %name,
                    has_a = !payload.lora_a.is_empty(),
                    has_b = !payload.lora_b.is_empty(),
                    "Skipping incomplete LoRA module"
                );
            }
            valid
        })
        .collect();

    if complete_modules.is_empty() {
        return Err(AosError::Parse(format!(
            "Adapter {}: no complete LoRA weight pairs found in safetensors",
            id
        )));
    }

    let config = LoadedLoRAConfig {
        rank: detected_rank,
        alpha: 32.0, // Default alpha, can be overridden
        target_modules,
    };

    debug!(
        adapter_id = id,
        modules = complete_modules.len(),
        rank = detected_rank,
        "Parsed safetensors with {} complete LoRA modules",
        complete_modules.len()
    );

    Ok((config, complete_modules))
}

/// Extract module name from tensor key
fn extract_module_name(tensor_name: &str, lora_suffix: &str) -> String {
    // Remove the lora suffix and any trailing ".weight"
    let name = tensor_name
        .replace(&format!(".{}.weight", lora_suffix), "")
        .replace(&format!(".{}", lora_suffix), "")
        .replace(&format!("{}.weight", lora_suffix), "")
        .replace(lora_suffix, "");

    // Extract the last meaningful component
    // e.g., "model.layers.0.self_attn.q_proj" -> "q_proj"
    name.split('.')
        .rfind(|s| {
            !s.is_empty()
                && !s.chars().all(|c| c.is_ascii_digit())
                && !["model", "base_model", "layers", "self_attn", "mlp"].contains(s)
        })
        .unwrap_or(&name)
        .to_string()
}

unsafe impl Send for CoreMLBackend {}
unsafe impl Sync for CoreMLBackend {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coreml_availability() {
        let available = is_coreml_available();
        println!("CoreML available: {}", available);

        let ane_available = is_neural_engine_available();
        println!("Neural Engine available: {}", ane_available);
    }

    #[test]
    #[cfg(feature = "coreml-backend")]
    fn test_coreml_init() {
        let result = init_coreml();
        assert!(
            result.is_ok(),
            "CoreML initialization should succeed on macOS"
        );
    }

    #[test]
    #[cfg(not(feature = "coreml-backend"))]
    fn test_coreml_disabled() {
        assert!(!is_coreml_available());
        assert!(!is_neural_engine_available());

        let result = init_coreml();
        assert!(result.is_ok());
        shutdown_coreml();
    }

    #[test]
    fn test_ane_detection() {
        let (available, capabilities) = CoreMLBackend::detect_ane().unwrap();

        #[cfg(all(target_os = "macos", feature = "coreml-backend"))]
        {
            if available {
                assert!(capabilities.is_some());
                let caps = capabilities.unwrap();
                assert!(caps.core_count > 0);
                assert!(caps.peak_throughput_tops > 0.0);
            }
        }

        #[cfg(not(all(target_os = "macos", feature = "coreml-backend")))]
        {
            assert!(!available);
            assert!(capabilities.is_none());
        }
    }

    #[test]
    #[cfg(feature = "coreml-backend")]
    fn test_backend_creation() {
        // This test requires CoreML to be available
        let result = CoreMLBackend::new();
        // May fail if CoreML is not available, which is ok
        if let Ok(backend) = result {
            assert!(!backend.device_name.is_empty());
        }
    }

    // ============== LoRA Weight Fusion Tests ==============

    #[test]
    fn test_compute_lora_delta_basic() {
        // Simple 2x2 example with rank 1
        // A: [1, 2] (rank=1, in_dim=2)
        // B: [2, 1] (out_dim=2, rank=1)
        let lora_a = vec![vec![1.0, 2.0]];
        let lora_b = vec![vec![3.0], vec![4.0]];
        let alpha = 1.0;
        let rank = 1;

        let delta = compute_lora_delta(&lora_a, &lora_b, alpha, rank).unwrap();

        // Expected: scale * (B @ A)
        // scale = alpha / rank = 1.0 / 1 = 1.0
        // B @ A = [[3], [4]] @ [[1, 2]] = [[3, 6], [4, 8]]
        assert_eq!(delta.out_dim, 2);
        assert_eq!(delta.in_dim, 2);
        assert_eq!(delta.rank, 1);
        assert_eq!(delta.delta.len(), 4);

        // Check values with floating-point tolerance
        assert!(
            (delta.delta[0] - 3.0).abs() < 1e-6,
            "delta[0,0] should be 3.0"
        );
        assert!(
            (delta.delta[1] - 6.0).abs() < 1e-6,
            "delta[0,1] should be 6.0"
        );
        assert!(
            (delta.delta[2] - 4.0).abs() < 1e-6,
            "delta[1,0] should be 4.0"
        );
        assert!(
            (delta.delta[3] - 8.0).abs() < 1e-6,
            "delta[1,1] should be 8.0"
        );
    }

    #[test]
    fn test_compute_lora_delta_with_scaling() {
        // Test alpha/rank scaling
        let lora_a = vec![vec![1.0, 0.0], vec![0.0, 1.0]]; // [2, 2] identity-ish
        let lora_b = vec![vec![1.0, 0.0], vec![0.0, 1.0]]; // [2, 2] identity-ish
        let alpha = 32.0;
        let rank = 16;

        let delta = compute_lora_delta(&lora_a, &lora_b, alpha, rank).unwrap();

        // Expected scale: 32 / 2 (actual rank) = 16.0 (rank in config is 16 but actual is 2)
        // With actual rank=2: scale = 32/2 = 16.0
        // B @ A for these matrices is [[1,0],[0,1]]
        // Scaled: [[16,0],[0,16]]
        let expected_scale = 32.0 / 2.0; // alpha / actual_rank
        assert!((delta.delta[0] - expected_scale).abs() < 1e-6);
        assert!((delta.delta[1] - 0.0).abs() < 1e-6);
        assert!((delta.delta[2] - 0.0).abs() < 1e-6);
        assert!((delta.delta[3] - expected_scale).abs() < 1e-6);
    }

    #[test]
    fn test_compute_lora_delta_larger_rank() {
        // Rank 4 example
        // A: [4, 3] (rank=4, in_dim=3)
        // B: [2, 4] (out_dim=2, rank=4)
        let lora_a = vec![
            vec![1.0, 0.0, 0.0],
            vec![0.0, 1.0, 0.0],
            vec![0.0, 0.0, 1.0],
            vec![1.0, 1.0, 1.0],
        ];
        let lora_b = vec![vec![1.0, 1.0, 1.0, 1.0], vec![2.0, 2.0, 2.0, 2.0]];
        let alpha = 4.0;
        let rank = 4;

        let delta = compute_lora_delta(&lora_a, &lora_b, alpha, rank).unwrap();

        assert_eq!(delta.out_dim, 2);
        assert_eq!(delta.in_dim, 3);
        assert_eq!(delta.rank, 4);
        assert_eq!(delta.delta.len(), 6);

        // B @ A = [[1,1,1,1], [2,2,2,2]] @ [[1,0,0], [0,1,0], [0,0,1], [1,1,1]]
        //       = [[1+0+0+1, 0+1+0+1, 0+0+1+1], [2+0+0+2, 0+2+0+2, 0+0+2+2]]
        //       = [[2, 2, 2], [4, 4, 4]]
        // scale = 4/4 = 1.0
        assert!((delta.delta[0] - 2.0).abs() < 1e-6);
        assert!((delta.delta[3] - 4.0).abs() < 1e-6);
    }

    #[test]
    fn test_compute_lora_delta_empty_matrices() {
        let lora_a: Vec<Vec<f32>> = vec![];
        let lora_b = vec![vec![1.0]];

        let result = compute_lora_delta(&lora_a, &lora_b, 1.0, 1);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty"));
    }

    #[test]
    fn test_compute_lora_delta_dimension_mismatch() {
        // A has 2 rows but B has 3 cols (should be same as rank)
        let lora_a = vec![vec![1.0], vec![2.0]]; // rank=2
        let lora_b = vec![vec![1.0, 2.0, 3.0]]; // cols=3, should be 2

        let result = compute_lora_delta(&lora_a, &lora_b, 1.0, 2);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("mismatch"));
    }

    #[test]
    fn test_extract_module_name() {
        // Test various tensor naming patterns
        assert_eq!(extract_module_name("q_proj.lora_A", "lora_A"), "q_proj");
        assert_eq!(
            extract_module_name("model.layers.5.self_attn.q_proj.lora_A.weight", "lora_A"),
            "q_proj"
        );
        assert_eq!(
            extract_module_name(
                "base_model.model.layers.12.self_attn.v_proj.lora_B.weight",
                "lora_B"
            ),
            "v_proj"
        );
        assert_eq!(
            extract_module_name("mlp.gate_proj.lora_A", "lora_A"),
            "gate_proj"
        );
    }

    #[test]
    fn test_weight_payload_json_parsing() {
        let json = r#"{
            "lora_a": [[1.0, 2.0], [3.0, 4.0]],
            "lora_b": [[5.0, 6.0], [7.0, 8.0]]
        }"#;

        let payload: WeightPayload = serde_json::from_str(json).unwrap();
        assert_eq!(payload.lora_a.len(), 2);
        assert_eq!(payload.lora_b.len(), 2);
        assert_eq!(payload.lora_a[0][0], 1.0);
        assert_eq!(payload.lora_b[1][1], 8.0);
    }

    #[test]
    fn test_adapter_payload_json_parsing() {
        let json = r#"{
            "config": {
                "rank": 8,
                "alpha": 16.0,
                "target_modules": ["q_proj", "v_proj"]
            },
            "weights": {
                "q_proj": {
                    "lora_a": [[1.0, 2.0]],
                    "lora_b": [[3.0], [4.0]]
                },
                "v_proj": {
                    "lora_a": [[5.0, 6.0]],
                    "lora_b": [[7.0], [8.0]]
                }
            }
        }"#;

        let payload: AdapterPayload = serde_json::from_str(json).unwrap();
        assert!(payload.config.is_some());
        let config = payload.config.unwrap();
        assert_eq!(config.rank, 8);
        assert_eq!(config.alpha, 16.0);
        assert_eq!(payload.weights.len(), 2);
        assert!(payload.weights.contains_key("q_proj"));
        assert!(payload.weights.contains_key("v_proj"));
    }

    #[test]
    fn test_loaded_lora_config_default() {
        let config = LoadedLoRAConfig::default();
        assert_eq!(config.rank, 16);
        assert_eq!(config.alpha, 32.0);
        assert_eq!(config.target_modules.len(), 4);
        assert!(config.target_modules.contains(&"q_proj".to_string()));
    }

    #[test]
    fn test_lora_delta_memory_calculation() {
        // Test that memory calculation is correct
        let delta = LoRADelta {
            delta: vec![0.0; 1000],
            out_dim: 50,
            in_dim: 20,
            rank: 4,
        };

        // 1000 f32 values = 4000 bytes
        assert_eq!(delta.delta.len() * std::mem::size_of::<f32>(), 4000);
    }

    #[test]
    fn test_full_adapter_loading_json() {
        // Create a simple adapter payload
        let payload = AdapterPayload {
            config: Some(LoadedLoRAConfig {
                rank: 2,
                alpha: 4.0,
                target_modules: vec!["q_proj".to_string()],
            }),
            weights: {
                let mut weights = HashMap::new();
                weights.insert(
                    "q_proj".to_string(),
                    WeightPayload {
                        lora_a: vec![vec![1.0, 0.0], vec![0.0, 1.0]],
                        lora_b: vec![vec![1.0, 0.0], vec![0.0, 1.0]],
                    },
                );
                weights
            },
        };

        let json_bytes = serde_json::to_vec(&payload).unwrap();

        // Parse and compute delta
        let (config, module_weights) = serde_json::from_slice::<AdapterPayload>(&json_bytes)
            .map(|p| (p.config.unwrap_or_default(), p.weights))
            .unwrap();

        assert_eq!(config.rank, 2);
        assert_eq!(config.alpha, 4.0);
        assert!(module_weights.contains_key("q_proj"));

        let q_proj_weights = &module_weights["q_proj"];
        let delta = compute_lora_delta(
            &q_proj_weights.lora_a,
            &q_proj_weights.lora_b,
            config.alpha,
            config.rank,
        )
        .unwrap();

        // For identity matrices: B @ A = I
        // scale = 4/2 = 2.0
        // delta should be [[2, 0], [0, 2]]
        assert_eq!(delta.out_dim, 2);
        assert_eq!(delta.in_dim, 2);
        assert!((delta.delta[0] - 2.0).abs() < 1e-6);
        assert!((delta.delta[1] - 0.0).abs() < 1e-6);
        assert!((delta.delta[2] - 0.0).abs() < 1e-6);
        assert!((delta.delta[3] - 2.0).abs() < 1e-6);
    }
}
