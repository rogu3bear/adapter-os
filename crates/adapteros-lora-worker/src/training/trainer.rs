//! Micro-LoRA training loop with forward/backward pass
//!
//! Implements LoRA training with low rank adaptation matrices.
//! This is a Rust-native implementation that avoids Python dependencies
//! and integrates with GPU backends (CoreML, MLX, Metal) for deterministic training.

pub use super::dataset::TrainingExample;
use adapteros_core::{derive_seed, AosError, Result};
use adapteros_lora_kernel_api::FusedKernels;
use adapteros_telemetry::TelemetryWriter;
use parking_lot::RwLock;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, error, info, warn};

/// Performance metrics for GPU training
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingPerformanceMetrics {
    /// Total GPU time in milliseconds
    pub total_gpu_time_ms: u64,
    /// Total CPU time in milliseconds
    pub total_cpu_time_ms: u64,
    /// Number of GPU operations
    pub gpu_operations: u64,
    /// Number of CPU operations
    pub cpu_operations: u64,
    /// Average GPU utilization percentage (0-100)
    pub avg_gpu_utilization: f32,
    /// Peak GPU memory usage in MB
    pub peak_gpu_memory_mb: f32,
    /// Total training batches processed
    pub total_batches: u64,
    /// Throughput (examples per second)
    pub throughput_examples_per_sec: f32,
}

/// GPU backend choice for training acceleration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrainingBackend {
    /// CoreML backend with Neural Engine (ANE acceleration, production)
    CoreML,
    /// MLX backend for research and training
    Mlx,
    /// Metal GPU backend (deterministic, fallback)
    Metal,
    /// CPU-only training (no GPU acceleration)
    Cpu,
}

impl TrainingBackend {
    /// Check if this backend requires GPU availability
    pub fn requires_gpu(&self) -> bool {
        matches!(
            self,
            TrainingBackend::CoreML | TrainingBackend::Mlx | TrainingBackend::Metal
        )
    }

    /// Get human-readable name
    pub fn name(&self) -> &'static str {
        match self {
            TrainingBackend::CoreML => "CoreML (ANE)",
            TrainingBackend::Mlx => "MLX",
            TrainingBackend::Metal => "Metal",
            TrainingBackend::Cpu => "CPU",
        }
    }
}

/// Micro-LoRA trainer with multi-backend GPU support
pub struct MicroLoRATrainer {
    pub config: TrainingConfig,
    /// GPU kernels for accelerated training
    kernels: Option<Box<dyn FusedKernels>>,
    /// Selected backend for this training session
    selected_backend: Option<TrainingBackend>,
    /// Telemetry writer for training events
    telemetry: TelemetryWriter,
    /// Training seed for deterministic RNG
    training_seed: u64,
    /// Performance metrics for GPU utilization tracking
    performance_metrics: Arc<RwLock<TrainingPerformanceMetrics>>,
}

/// Training configuration with GPU support
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingConfig {
    /// LoRA rank
    pub rank: usize,
    /// LoRA alpha scaling factor
    pub alpha: f32,
    /// Learning rate
    pub learning_rate: f32,
    /// Batch size
    pub batch_size: usize,
    /// Number of epochs
    pub epochs: usize,
    /// Hidden dimension size
    pub hidden_dim: usize,
    /// Vocabulary size (model-specific)
    pub vocab_size: usize,
    /// Preferred GPU backend (None = auto-select, falls back to CPU if unavailable)
    #[serde(skip, default)]
    pub preferred_backend: Option<TrainingBackend>,
    /// Require GPU acceleration (error if GPU unavailable)
    #[serde(skip)]
    pub require_gpu: bool,
    /// Maximum GPU memory to use in MB (0 = unlimited)
    #[serde(skip)]
    pub max_gpu_memory_mb: u64,
}

impl Default for TrainingConfig {
    fn default() -> Self {
        Self {
            rank: 4,
            alpha: 16.0,
            learning_rate: 1e-4,
            batch_size: 8,
            epochs: 3,
            hidden_dim: 768,
            vocab_size: 32000, // Default LLaMA/Mistral vocab size
            preferred_backend: None,
            require_gpu: false,
            max_gpu_memory_mb: 0,
        }
    }
}

impl TrainingConfig {
    /// Create a new configuration with GPU acceleration required
    pub fn with_gpu_required(mut self) -> Self {
        self.require_gpu = true;
        self
    }

    /// Set preferred GPU backend
    pub fn with_backend(mut self, backend: TrainingBackend) -> Self {
        self.preferred_backend = Some(backend);
        self
    }

    /// Set maximum GPU memory usage
    pub fn with_max_gpu_memory(mut self, max_mb: u64) -> Self {
        self.max_gpu_memory_mb = max_mb;
        self
    }
}

/// Training result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingResult {
    pub adapter_id: String,
    pub final_loss: f32,
    /// Training time in microseconds for high precision measurement.
    /// Use `.training_time_ms()` method for millisecond conversion.
    pub training_time_us: u64,
    pub weights: LoRAWeights,
}

impl TrainingResult {
    /// Get training time in milliseconds (for backward compatibility and display)
    pub fn training_time_ms(&self) -> u64 {
        self.training_time_us / 1000
    }
}

/// LoRA weight matrices
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoRAWeights {
    /// Down-projection matrix (rank × hidden_dim)
    pub lora_a: Vec<Vec<f32>>,
    /// Up-projection matrix (hidden_dim × rank)
    pub lora_b: Vec<Vec<f32>>,
}

impl MicroLoRATrainer {
    /// Create a new trainer with configuration
    pub fn new(config: TrainingConfig) -> Result<Self> {
        // Derive deterministic training seed
        let global_seed = adapteros_core::B3Hash::hash(b"training");
        let training_seed_bytes = derive_seed(&global_seed, "lora_trainer");
        let training_seed = u64::from_le_bytes([
            training_seed_bytes[0],
            training_seed_bytes[1],
            training_seed_bytes[2],
            training_seed_bytes[3],
            training_seed_bytes[4],
            training_seed_bytes[5],
            training_seed_bytes[6],
            training_seed_bytes[7],
        ]);

        // Initialize telemetry
        let telemetry = TelemetryWriter::new("training", 1000, 1024 * 1024)?;

        info!(
            "Created MicroLoRA trainer with seed: {}, GPU optional: {}",
            training_seed, !config.require_gpu
        );

        Ok(Self {
            config,
            kernels: None,
            selected_backend: None,
            telemetry,
            training_seed,
            performance_metrics: Arc::new(RwLock::new(TrainingPerformanceMetrics {
                total_gpu_time_ms: 0,
                total_cpu_time_ms: 0,
                gpu_operations: 0,
                cpu_operations: 0,
                avg_gpu_utilization: 0.0,
                peak_gpu_memory_mb: 0.0,
                total_batches: 0,
                throughput_examples_per_sec: 0.0,
            })),
        })
    }

    /// Detect available GPU backends and select optimal one
    fn detect_available_backends() -> Vec<(TrainingBackend, &'static str)> {
        let mut backends = Vec::new();

        // Check backend availability in priority order
        // Priority: CoreML (ANE) -> Metal -> MLX -> CPU
        #[cfg(all(target_os = "macos", feature = "coreml-backend"))]
        {
            backends.push((TrainingBackend::CoreML, "CoreML with ANE available"));
        }

        #[cfg(target_os = "macos")]
        {
            backends.push((TrainingBackend::Metal, "Metal GPU available"));
        }

        #[cfg(feature = "multi-backend")]
        {
            backends.push((TrainingBackend::Mlx, "MLX backend available"));
        }

        backends.push((TrainingBackend::Cpu, "CPU-only training"));

        backends
    }

    /// Get a description of available backends
    pub fn describe_available_backends() -> String {
        let backends = Self::detect_available_backends();
        let mut desc = String::from("Available training backends:\n");
        for (backend, reason) in backends {
            desc.push_str(&format!("  - {}: {}\n", backend.name(), reason));
        }
        desc
    }

    /// Validate GPU requirements and provide actionable error messages
    fn validate_gpu_requirements(&self) -> Result<()> {
        if !self.config.require_gpu {
            return Ok(());
        }

        let available = Self::detect_available_backends();
        let has_gpu = available.iter().any(|(b, _)| b.requires_gpu());

        if !has_gpu {
            let available_desc = Self::describe_available_backends();
            error!(
                "GPU acceleration required but no GPU backends available\n{}",
                available_desc
            );
            return Err(AosError::Config(format!(
                "GPU acceleration required but no suitable GPU backend available. {}",
                available_desc
            )));
        }

        Ok(())
    }

    /// Select optimal GPU backend based on availability and preference
    fn select_optimal_backend(&self) -> (TrainingBackend, &'static str) {
        // If user specified preferred backend, try to use it
        if let Some(preferred) = self.config.preferred_backend {
            if preferred.requires_gpu() {
                return (preferred, "user-specified backend");
            }
        }

        // Auto-select best available GPU backend
        let available = Self::detect_available_backends();
        for (backend, reason) in available {
            if backend.requires_gpu() {
                return (backend, reason);
            }
        }

        // Fallback to CPU
        (TrainingBackend::Cpu, "no GPU available, using CPU")
    }

    /// Initialize GPU kernels for training with automatic backend selection
    ///
    /// This method attempts to initialize GPU acceleration for training using the
    /// best available backend. It follows this priority:
    /// 1. User-specified backend (if provided)
    /// 2. CoreML with ANE (best power efficiency)
    /// 3. Metal GPU (deterministic, production)
    /// 4. MLX (research/training)
    /// 5. CPU (fallback)
    ///
    /// # Arguments
    /// * `plan_bytes` - Compiled model plan in backend-specific format
    ///
    /// # Errors
    /// Returns an error if:
    /// - GPU is required (`config.require_gpu=true`) but no GPU backend is available
    /// - Plan loading fails on the selected backend
    /// - Memory constraints are violated
    pub fn init_kernels(&mut self, plan_bytes: &[u8]) -> Result<()> {
        // Validate GPU requirements first
        self.validate_gpu_requirements()?;

        // Select optimal backend
        let (backend, reason) = self.select_optimal_backend();
        self.selected_backend = Some(backend);

        info!(
            "Initializing {} kernels for training: {}",
            backend.name(),
            reason
        );

        // Log backend selection
        self.telemetry
            .log(
                "training.backend_selected",
                serde_json::json!({
                    "backend": backend.name(),
                    "reason": reason,
                    "plan_size": plan_bytes.len(),
                    "seed": self.training_seed,
                    "require_gpu": self.config.require_gpu
                }),
            )
            .ok();

        // Return early for CPU-only training (no kernel initialization needed)
        if backend == TrainingBackend::Cpu {
            info!("Training will run on CPU (GPU not available or not required)");
            return Ok(());
        }

        // Attempt GPU backend initialization
        match self.init_gpu_backend(backend, plan_bytes) {
            Ok(()) => {
                info!(
                    "Successfully initialized {} backend for training",
                    backend.name()
                );
                Ok(())
            }
            Err(e) => {
                if self.config.require_gpu {
                    // GPU was required but initialization failed
                    error!(
                        "Failed to initialize required GPU backend {}: {}",
                        backend.name(),
                        e
                    );
                    Err(e)
                } else {
                    // GPU was optional, fall back to CPU with warning
                    warn!(
                        "Failed to initialize {} backend: {}, falling back to CPU training",
                        backend.name(),
                        e
                    );
                    self.selected_backend = Some(TrainingBackend::Cpu);
                    self.kernels = None;

                    self.telemetry
                        .log(
                            "training.gpu_fallback",
                            serde_json::json!({
                                "original_backend": backend.name(),
                                "reason": e.to_string(),
                                "using_cpu": true
                            }),
                        )
                        .ok();

                    Ok(())
                }
            }
        }
    }

    /// Initialize a specific GPU backend
    fn init_gpu_backend(&mut self, backend: TrainingBackend, plan_bytes: &[u8]) -> Result<()> {
        use crate::backend_factory::{create_backend, BackendChoice};

        let backend_choice = match backend {
            #[cfg(all(target_os = "macos", feature = "coreml-backend"))]
            TrainingBackend::CoreML => {
                info!("Creating CoreML backend with Neural Engine acceleration");
                BackendChoice::CoreML
            }
            #[cfg(all(target_os = "macos", not(feature = "coreml-backend")))]
            TrainingBackend::CoreML => {
                return Err(AosError::Config(
                    "CoreML backend requested but 'coreml-backend' feature not enabled".to_string(),
                ));
            }
            #[cfg(not(target_os = "macos"))]
            TrainingBackend::CoreML => {
                return Err(AosError::Config(
                    "CoreML backend requires macOS".to_string(),
                ));
            }

            #[cfg(target_os = "macos")]
            TrainingBackend::Metal => {
                info!("Creating Metal GPU backend");
                BackendChoice::Metal
            }
            #[cfg(not(target_os = "macos"))]
            TrainingBackend::Metal => {
                return Err(AosError::Config("Metal backend requires macOS".to_string()));
            }

            #[cfg(feature = "multi-backend")]
            TrainingBackend::Mlx => {
                return Err(AosError::Config(
                    "MLX backend requires explicit model path for training".to_string(),
                ));
            }
            #[cfg(not(feature = "multi-backend"))]
            TrainingBackend::Mlx => {
                return Err(AosError::Config(
                    "MLX backend requires 'multi-backend' feature".to_string(),
                ));
            }

            TrainingBackend::Cpu => {
                return Err(AosError::Internal(
                    "CPU backend should not be initialized via GPU path".to_string(),
                ));
            }
        };

        // Create and initialize backend
        let mut kernel = create_backend(backend_choice).map_err(|e| {
            error!("Failed to create {} backend: {}", backend.name(), e);
            e
        })?;

        // Load plan
        kernel.load(plan_bytes).map_err(|e| {
            error!(
                "Failed to load plan on {} backend (size={}): {}",
                backend.name(),
                plan_bytes.len(),
                e
            );
            e
        })?;

        self.kernels = Some(kernel);

        // Log kernel initialization success
        self.telemetry
            .log(
                "training.kernels_initialized",
                serde_json::json!({
                    "backend": backend.name(),
                    "plan_size": plan_bytes.len(),
                    "seed": self.training_seed,
                    "timestamp": std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0)
                }),
            )
            .ok();

        Ok(())
    }

    /// Get information about the selected training backend
    pub fn backend_info(&self) -> Option<&'static str> {
        self.selected_backend.map(|b| b.name())
    }

    /// Check if training will use GPU acceleration
    pub fn using_gpu(&self) -> bool {
        matches!(
            self.selected_backend,
            Some(TrainingBackend::CoreML | TrainingBackend::Metal | TrainingBackend::Mlx)
        )
    }

    /// Get the training seed used for deterministic RNG
    ///
    /// Returns the 64-bit seed derived from HKDF during trainer construction.
    /// Two trainers with identical configuration will have the same seed,
    /// ensuring deterministic training results.
    pub fn training_seed(&self) -> u64 {
        self.training_seed
    }

    /// Train LoRA adapter on examples with GPU acceleration (if available)
    ///
    /// This method provides backward compatibility with automatic progress callback.
    /// For more control, use `train_with_callback` instead.
    pub async fn train(&mut self, examples: &[TrainingExample]) -> Result<TrainingResult> {
        // Backward-compatible behavior: no external progress callback
        self.train_with_callback(examples, |_epoch, _loss| {}).await
    }

    /// Train with a per-epoch progress callback and GPU acceleration
    ///
    /// The callback is invoked after each epoch with (epoch_index starting at 1, epoch_loss).
    /// This method automatically selects the best available GPU backend if kernels have been
    /// initialized, otherwise falls back to CPU training.
    ///
    /// # Arguments
    /// * `examples` - Training examples with input/target pairs
    /// * `on_epoch` - Callback invoked after each epoch with (epoch_number, epoch_loss)
    pub async fn train_with_callback<C>(
        &mut self,
        examples: &[TrainingExample],
        mut on_epoch: C,
    ) -> Result<TrainingResult>
    where
        C: FnMut(usize, f32),
    {
        let backend_name = self.backend_info().unwrap_or("CPU");
        let using_gpu = self.using_gpu();

        info!(
            "Starting LoRA training: rank={}, epochs={}, examples={}, backend={}, seed={}",
            self.config.rank,
            self.config.epochs,
            examples.len(),
            backend_name,
            self.training_seed
        );

        // Log training start with GPU information
        self.telemetry.log(
            "training.started",
            serde_json::json!({
                "rank": self.config.rank,
                "epochs": self.config.epochs,
                "examples": examples.len(),
                "seed": self.training_seed,
                "backend": backend_name,
                "using_gpu": using_gpu,
                "has_kernels": self.kernels.is_some(),
                "config": {
                    "batch_size": self.config.batch_size,
                    "learning_rate": self.config.learning_rate,
                    "alpha": self.config.alpha,
                    "hidden_dim": self.config.hidden_dim
                }
            }),
        )?;

        let start = Instant::now();
        let adapter_id = Self::generate_adapter_id();

        // Initialize LoRA weights with deterministic seeding
        let mut weights = self.initialize_weights_deterministic()?;

        // Training loop with telemetry
        let mut final_loss = 0.0;
        for epoch in 0..self.config.epochs {
            debug!("Epoch {}/{}", epoch + 1, self.config.epochs);

            let epoch_loss = self.train_epoch_deterministic(&mut weights, examples, epoch)?;
            final_loss = epoch_loss;

            info!("Epoch {} loss: {:.4}", epoch + 1, epoch_loss);

            // Log epoch completion
            self.telemetry.log(
                "training.epoch_completed",
                serde_json::json!({
                    "epoch": epoch + 1,
                    "loss": epoch_loss,
                    "adapter_id": adapter_id
                }),
            )?;

            // Notify orchestrator/UI via callback
            on_epoch(epoch + 1, epoch_loss);

            // Early stopping if loss is very low
            if epoch_loss < 0.01 {
                info!("Early stopping: loss below threshold");
                break;
            }
        }

        let training_time_us = start.elapsed().as_micros() as u64;
        let training_time_ms = training_time_us / 1000;

        // Calculate throughput metrics
        let examples_per_second = if training_time_us > 0 {
            (examples.len() as f32) / ((training_time_us as f32) / 1_000_000.0)
        } else {
            0.0
        };

        let backend_name = self.backend_info().unwrap_or("CPU");
        info!(
            "Training complete: loss={:.4}, time={}us ({}ms), backend={}, throughput={:.0} ex/s, seed={}",
            final_loss, training_time_us, training_time_ms, backend_name, examples_per_second, self.training_seed
        );

        // Log training completion with performance metrics
        self.telemetry.log(
            "training.completed",
            serde_json::json!({
                "adapter_id": adapter_id,
                "final_loss": final_loss,
                "training_time_us": training_time_us,
                "training_time_ms": training_time_ms,
                "seed": self.training_seed,
                "backend": backend_name,
                "using_gpu": using_gpu,
                "performance": {
                    "examples_per_second": examples_per_second,
                    "total_examples": examples.len(),
                    "total_epochs": self.config.epochs,
                    "rank": self.config.rank,
                    "hidden_dim": self.config.hidden_dim
                }
            }),
        )?;

        Ok(TrainingResult {
            adapter_id,
            final_loss,
            training_time_us,
            weights,
        })
    }

    /// Initialize LoRA weight matrices with deterministic seeding
    fn initialize_weights_deterministic(&self) -> Result<LoRAWeights> {
        use rand::{Rng, SeedableRng};
        use rand_chacha::ChaCha20Rng;

        // Create deterministic RNG from training seed
        let mut rng = ChaCha20Rng::seed_from_u64(self.training_seed);

        // Initialize lora_a with small random values
        let lora_a = (0..self.config.rank)
            .map(|_| {
                (0..self.config.hidden_dim)
                    .map(|_| rng.gen_range(-0.01..0.01))
                    .collect()
            })
            .collect();

        // Initialize lora_b with zeros (standard practice)
        let lora_b = (0..self.config.hidden_dim)
            .map(|_| vec![0.0; self.config.rank])
            .collect();

        debug!(
            "Initialized LoRA weights deterministically with seed: {}",
            self.training_seed
        );

        Ok(LoRAWeights { lora_a, lora_b })
    }

    /// Train one epoch with deterministic execution
    fn train_epoch_deterministic(
        &mut self,
        weights: &mut LoRAWeights,
        examples: &[TrainingExample],
        epoch: usize,
    ) -> Result<f32> {
        use rand::SeedableRng;
        use rand_chacha::ChaCha20Rng;

        // Create epoch-specific RNG seed
        let epoch_seed_bytes = derive_seed(
            &adapteros_core::B3Hash::hash(&self.training_seed.to_le_bytes()),
            &format!("epoch_{}", epoch),
        );
        let epoch_seed = u64::from_le_bytes([
            epoch_seed_bytes[0],
            epoch_seed_bytes[1],
            epoch_seed_bytes[2],
            epoch_seed_bytes[3],
            epoch_seed_bytes[4],
            epoch_seed_bytes[5],
            epoch_seed_bytes[6],
            epoch_seed_bytes[7],
        ]);
        let mut rng = ChaCha20Rng::seed_from_u64(epoch_seed);

        let mut total_loss = 0.0;
        let mut num_batches = 0;

        // Process examples in batches with deterministic ordering
        for batch_start in (0..examples.len()).step_by(self.config.batch_size) {
            let batch_end = (batch_start + self.config.batch_size).min(examples.len());
            let batch = &examples[batch_start..batch_end];

            let loss = self.train_batch_deterministic(weights, batch, &mut rng)?;
            total_loss += loss;
            num_batches += 1;
        }

        Ok(total_loss / num_batches as f32)
    }

    /// Train one batch with deterministic RNG (GPU-accelerated if kernels available)
    fn train_batch_deterministic(
        &mut self,
        weights: &mut LoRAWeights,
        batch: &[TrainingExample],
        rng: &mut impl Rng,
    ) -> Result<f32> {
        // Check if GPU kernels are available
        if self.kernels.is_some() {
            // GPU-accelerated training path
            self.train_batch_gpu(weights, batch, rng)
        } else {
            // CPU-only training path (fallback)
            self.train_batch_cpu(weights, batch, rng)
        }
    }

    /// Train one batch on GPU (using FusedKernels)
    fn train_batch_gpu(
        &mut self,
        weights: &mut LoRAWeights,
        batch: &[TrainingExample],
        rng: &mut impl Rng,
    ) -> Result<f32> {
        use adapteros_lora_kernel_api::{IoBuffers, RouterRing};

        let batch_start = Instant::now();
        let mut batch_loss = 0.0;
        let vocab_size = self.config.vocab_size;

        let mut gpu_time_us = 0u64;

        for example in batch {
            // Prepare router ring for GPU kernel (using all available adapters)
            let mut ring = RouterRing::new(1); // K=1 for training (single adapter)
            ring.set(&[0], &[32767]); // Max Q15 gate value for training

            // Prepare IO buffers for GPU inference
            let mut io = IoBuffers::new(vocab_size);
            io.input_ids = example.input.clone();
            io.position = 0;

            // Measure GPU forward pass time
            let gpu_start = Instant::now();

            // GPU forward pass through kernels
            if let Some(ref mut kernels) = self.kernels {
                kernels.run_step(&ring, &mut io)?;
            }

            gpu_time_us += gpu_start.elapsed().as_micros() as u64;

            // Extract hidden state from GPU output
            let hidden: Vec<f32> = io.output_logits[..self.config.hidden_dim].to_vec();
            let output = io.output_logits.clone();

            // Compute loss
            let loss = self.compute_loss(&output, &example.target);
            batch_loss += loss;

            // Backward pass and update weights (CPU-based gradient descent)
            // TODO: Move gradient computation to GPU kernels for full GPU training
            self.backward_and_update_deterministic(
                weights,
                &hidden,
                &output,
                &example.target,
                loss,
                rng,
            )?;
        }

        // Update performance metrics
        let batch_time_us = batch_start.elapsed().as_micros() as u64;
        let cpu_time_us = batch_time_us.saturating_sub(gpu_time_us);

        let gpu_utilization = if batch_time_us > 0 {
            (gpu_time_us as f32 / batch_time_us as f32) * 100.0
        } else {
            0.0
        };

        {
            let mut metrics = self.performance_metrics.write();
            metrics.total_gpu_time_ms += gpu_time_us / 1000;
            metrics.total_cpu_time_ms += cpu_time_us / 1000;
            metrics.gpu_operations += batch.len() as u64;
            metrics.total_batches += 1;

            // Running average of GPU utilization
            let total_time = metrics.total_gpu_time_ms + metrics.total_cpu_time_ms;
            if total_time > 0 {
                metrics.avg_gpu_utilization =
                    (metrics.total_gpu_time_ms as f32 / total_time as f32) * 100.0;
            }
        }

        debug!(
            "GPU batch: {}us GPU, {}us CPU, {:.1}% GPU utilization",
            gpu_time_us, cpu_time_us, gpu_utilization
        );

        Ok(batch_loss / batch.len() as f32)
    }

    /// Train one batch on CPU (fallback when GPU unavailable)
    fn train_batch_cpu(
        &self,
        weights: &mut LoRAWeights,
        batch: &[TrainingExample],
        rng: &mut impl Rng,
    ) -> Result<f32> {
        let batch_start = Instant::now();
        let mut batch_loss = 0.0;

        for example in batch {
            // CPU forward pass
            let (output, hidden) = self.forward(weights, &example.input)?;

            // Compute loss (simplified cross-entropy)
            let loss = self.compute_loss(&output, &example.target);
            batch_loss += loss;

            // Backward pass and update weights with deterministic RNG
            self.backward_and_update_deterministic(
                weights,
                &hidden,
                &output,
                &example.target,
                loss,
                rng,
            )?;
        }

        // Update CPU metrics
        let cpu_time_ms = batch_start.elapsed().as_millis() as u64;
        {
            let mut metrics = self.performance_metrics.write();
            metrics.total_cpu_time_ms += cpu_time_ms;
            metrics.cpu_operations += batch.len() as u64;
            metrics.total_batches += 1;
        }

        Ok(batch_loss / batch.len() as f32)
    }

    /// Get current GPU utilization percentage
    pub fn get_gpu_utilization(&self) -> f32 {
        self.performance_metrics.read().avg_gpu_utilization
    }

    /// Get performance metrics
    pub fn get_performance_metrics(&self) -> TrainingPerformanceMetrics {
        self.performance_metrics.read().clone()
    }

    /// Reset performance metrics
    pub fn reset_metrics(&self) {
        let mut metrics = self.performance_metrics.write();
        *metrics = TrainingPerformanceMetrics {
            total_gpu_time_ms: 0,
            total_cpu_time_ms: 0,
            gpu_operations: 0,
            cpu_operations: 0,
            avg_gpu_utilization: 0.0,
            peak_gpu_memory_mb: 0.0,
            total_batches: 0,
            throughput_examples_per_sec: 0.0,
        };
    }

    /// Forward pass with LoRA injection
    fn forward(&self, weights: &LoRAWeights, input: &[u32]) -> Result<(Vec<f32>, Vec<f32>)> {
        // Simplified forward pass
        // In production, this would integrate with the actual model

        // Create hidden state from input (simplified embedding)
        let hidden: Vec<f32> = input
            .iter()
            .take(self.config.hidden_dim)
            .map(|&token_id| (token_id as f32) / 1000.0)
            .collect();

        // Pad to hidden_dim if needed
        let mut hidden = hidden;
        while hidden.len() < self.config.hidden_dim {
            hidden.push(0.0);
        }

        // Apply LoRA: output = hidden + hidden * LoRA_B * LoRA_A
        let lora_output = self.apply_lora(&hidden, weights);

        // Combine base hidden with LoRA adjustment
        let output: Vec<f32> = hidden
            .iter()
            .zip(lora_output.iter())
            .map(|(h, l)| h + l * self.config.alpha / self.config.rank as f32)
            .collect();

        Ok((output, hidden))
    }

    /// Apply LoRA transformation
    #[allow(clippy::needless_range_loop)]
    fn apply_lora(&self, hidden: &[f32], weights: &LoRAWeights) -> Vec<f32> {
        // Compute: hidden * LoRA_A^T * LoRA_B^T

        // First: hidden * LoRA_A^T = intermediate (size: rank)
        let mut intermediate = vec![0.0; self.config.rank];
        for r in 0..self.config.rank {
            for (h_idx, &h_val) in hidden.iter().enumerate() {
                if h_idx < weights.lora_a[r].len() {
                    intermediate[r] += h_val * weights.lora_a[r][h_idx];
                }
            }
        }

        // Second: intermediate * LoRA_B^T = output (size: hidden_dim)
        let mut output = vec![0.0; self.config.hidden_dim];
        for h_idx in 0..self.config.hidden_dim {
            if h_idx < weights.lora_b.len() {
                for (r, &inter_val) in intermediate.iter().enumerate() {
                    if r < weights.lora_b[h_idx].len() {
                        output[h_idx] += inter_val * weights.lora_b[h_idx][r];
                    }
                }
            }
        }

        output
    }

    /// Compute loss (simplified cross-entropy)
    fn compute_loss(&self, output: &[f32], target: &[u32]) -> f32 {
        let mut loss = 0.0;
        let n = output.len().min(target.len());

        for i in 0..n {
            let target_val = (target[i] as f32) / 1000.0;
            let diff = output[i] - target_val;
            loss += diff * diff; // MSE for simplicity
        }

        loss / n as f32
    }

    /// Backward pass and weight update with deterministic RNG
    fn backward_and_update_deterministic(
        &self,
        weights: &mut LoRAWeights,
        hidden: &[f32],
        output: &[f32],
        target: &[u32],
        loss: f32,
        rng: &mut impl Rng,
    ) -> Result<()> {
        // Simplified gradient descent with deterministic noise
        // In production, use proper backpropagation

        let n = output.len().min(target.len());
        let learning_rate = self.config.learning_rate;

        // Compute gradient (simplified)
        let mut grad_output = vec![0.0; output.len()];
        for i in 0..n {
            let target_val = (target[i] as f32) / 1000.0;
            grad_output[i] = 2.0 * (output[i] - target_val) / n as f32;
        }

        // Add deterministic noise for regularization
        let noise_scale = 0.001;
        for grad in &mut grad_output {
            *grad += rng.gen_range(-noise_scale..noise_scale);
        }

        // Update LoRA_A
        for r in 0..self.config.rank {
            for h_idx in 0..self.config.hidden_dim.min(hidden.len()) {
                if h_idx < weights.lora_a[r].len() {
                    let grad = grad_output[h_idx] * hidden[h_idx] * loss;
                    weights.lora_a[r][h_idx] -= learning_rate * grad;
                }
            }
        }

        // Update LoRA_B
        for h_idx in 0..self.config.hidden_dim {
            if h_idx < weights.lora_b.len() {
                for r in 0..self.config.rank {
                    if r < weights.lora_b[h_idx].len() {
                        let grad = grad_output[h_idx] * hidden[h_idx] * loss;
                        weights.lora_b[h_idx][r] -= learning_rate * grad;
                    }
                }
            }
        }

        Ok(())
    }

    /// Generate unique adapter ID
    fn generate_adapter_id() -> String {
        use std::time::SystemTime;
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        format!("microlora_{}", timestamp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_training_backend_enum() {
        assert!(TrainingBackend::CoreML.requires_gpu());
        assert!(TrainingBackend::Metal.requires_gpu());
        assert!(TrainingBackend::Mlx.requires_gpu());
        assert!(!TrainingBackend::Cpu.requires_gpu());

        assert_eq!(TrainingBackend::CoreML.name(), "CoreML (ANE)");
        assert_eq!(TrainingBackend::Cpu.name(), "CPU");
    }

    #[test]
    fn test_training_config_with_gpu_required() {
        let config = TrainingConfig::default().with_gpu_required();
        assert!(config.require_gpu);
        assert_eq!(config.rank, 4); // Default values preserved
    }

    #[test]
    fn test_training_config_with_backend() {
        let config = TrainingConfig::default().with_backend(TrainingBackend::Metal);
        assert_eq!(config.preferred_backend, Some(TrainingBackend::Metal));
    }

    #[test]
    fn test_training_config_with_max_gpu_memory() {
        let config = TrainingConfig::default().with_max_gpu_memory(2048);
        assert_eq!(config.max_gpu_memory_mb, 2048);
    }

    #[test]
    fn test_available_backends_detection() {
        let backends = MicroLoRATrainer::detect_available_backends();
        // At minimum, CPU should always be available
        assert!(!backends.is_empty());
        let has_cpu = backends.iter().any(|(b, _)| *b == TrainingBackend::Cpu);
        assert!(has_cpu, "CPU backend should always be available");
    }

    #[test]
    fn test_describe_available_backends() {
        let desc = MicroLoRATrainer::describe_available_backends();
        assert!(desc.contains("Available training backends:"));
        assert!(desc.contains("CPU")); // At minimum, CPU should be listed
    }

    #[test]
    fn test_initialize_weights() {
        let config = TrainingConfig {
            rank: 4,
            hidden_dim: 768,
            ..Default::default()
        };
        let trainer = MicroLoRATrainer::new(config).unwrap();
        let weights = trainer.initialize_weights_deterministic().unwrap();

        assert_eq!(weights.lora_a.len(), 4);
        assert_eq!(weights.lora_a[0].len(), 768);
        assert_eq!(weights.lora_b.len(), 768);
        assert_eq!(weights.lora_b[0].len(), 4);
    }

    #[test]
    fn test_forward_pass() {
        let config = TrainingConfig {
            rank: 4,
            hidden_dim: 768,
            ..Default::default()
        };
        let trainer = MicroLoRATrainer::new(config).unwrap();
        let weights = trainer.initialize_weights_deterministic().unwrap();

        let input = vec![1, 2, 3, 4, 5];
        let (output, hidden) = trainer.forward(&weights, &input).unwrap();

        assert_eq!(output.len(), 768);
        assert_eq!(hidden.len(), 768);
    }

    #[test]
    fn test_trainer_gpu_status_initially_cpu() {
        let config = TrainingConfig::default();
        let trainer = MicroLoRATrainer::new(config).unwrap();

        // Before init_kernels, no backend is selected
        assert_eq!(trainer.backend_info(), None);
        assert!(!trainer.using_gpu());
    }

    #[tokio::test]
    async fn test_train_small() {
        let config = TrainingConfig {
            rank: 2,
            hidden_dim: 64,
            batch_size: 2,
            epochs: 1,
            learning_rate: 0.01,
            ..Default::default()
        };
        let mut trainer = MicroLoRATrainer::new(config).unwrap();

        let examples = vec![
            TrainingExample {
                input: vec![1, 2, 3],
                target: vec![4, 5, 6],
                metadata: HashMap::new(),
                weight: 1.0,
            },
            TrainingExample {
                input: vec![7, 8, 9],
                target: vec![10, 11, 12],
                metadata: HashMap::new(),
                weight: 1.0,
            },
        ];

        let result = trainer.train(&examples).await.unwrap();
        assert!(result.final_loss >= 0.0);
        assert!(
            result.training_time_us > 0,
            "Training time should be positive (actual work done), got: {}us",
            result.training_time_us
        );
        assert_eq!(result.weights.lora_a.len(), 2);
    }

    #[tokio::test]
    async fn test_train_with_cpu_backend_optional() {
        // Training should work without GPU when GPU is optional
        let config = TrainingConfig {
            rank: 2,
            hidden_dim: 32,
            batch_size: 1,
            epochs: 1,
            learning_rate: 0.01,
            require_gpu: false,
            ..Default::default()
        };
        let mut trainer = MicroLoRATrainer::new(config).unwrap();

        let examples = vec![TrainingExample {
            input: vec![1, 2],
            target: vec![3, 4],
            metadata: HashMap::new(),
            weight: 1.0,
        }];

        // init_kernels should complete successfully (CPU path)
        trainer
            .init_kernels(&[])
            .expect("CPU kernel init should succeed");

        // Training should complete without errors
        let result = trainer.train(&examples).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().weights.lora_a.len(), 2);
    }

    #[test]
    fn test_backend_selection_priority() {
        let config = TrainingConfig {
            preferred_backend: Some(TrainingBackend::Metal),
            ..Default::default()
        };
        let trainer = MicroLoRATrainer::new(config).unwrap();

        let (selected, _reason) = trainer.select_optimal_backend();
        // If user specifies Metal and it's available on macOS, it should be selected
        #[cfg(target_os = "macos")]
        {
            assert_eq!(selected, TrainingBackend::Metal);
        }
    }
}
