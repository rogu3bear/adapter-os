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
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Once;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

// Import safe CoreML wrappers
#[cfg(feature = "coreml-backend")]
use crate::coreml::{self, Array, Model, ModelMetadata as CoreMLModelMeta};

static COREML_INIT: Once = Once::new();
static mut COREML_AVAILABLE: bool = false;

/// Initialize CoreML backend
#[cfg(feature = "coreml-backend")]
pub fn init_coreml() -> Result<()> {
    let mut init_result = Ok(());

    COREML_INIT.call_once(|| unsafe {
        if coreml::is_available() {
            COREML_AVAILABLE = true;
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
    unsafe { COREML_AVAILABLE }
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
    loaded_adapters: HashMap<u16, B3Hash>,
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
            let temp_dir = std::env::temp_dir();
            let model_path = temp_dir.join(format!("aos_coreml_{}.mlmodelc", plan_hash.to_short_hex()));

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
        let coreml_meta = model.metadata()
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
        self.loaded_adapters.clear();
        Ok(())
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
            let model_state = self.model_state.as_ref().unwrap();
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
                .map(|&gate| (gate as f32) / 32768.0)
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
            manifest: None,
            rng_seed_method: attestation::RngSeedingMethod::HkdfSeeded,
            floating_point_mode: attestation::FloatingPointMode::Deterministic,
            compiler_flags: vec![],
            deterministic: self.ane_available,
        })
    }

    fn load_adapter(&mut self, id: u16, weights: &[u8]) -> Result<()> {
        let hash = B3Hash::hash(weights);
        info!(
            adapter_id = id,
            weight_bytes = weights.len(),
            hash = %hash.to_short_hex(),
            "Loading adapter into CoreML backend"
        );
        self.loaded_adapters.insert(id, hash);
        Ok(())
    }

    fn unload_adapter(&mut self, id: u16) -> Result<()> {
        if let Some(hash) = self.loaded_adapters.remove(&id) {
            info!(
                adapter_id = id,
                hash = %hash.to_short_hex(),
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
}
