//! Micro-LoRA training loop with forward/backward pass
//!
//! Implements LoRA training with low rank adaptation matrices.
//! This is a Rust-native implementation that avoids Python dependencies
//! and integrates with GPU backends (CoreML, MLX, Metal) for deterministic training.

use super::checkpoint::{CheckpointManager, TrainingCheckpoint};
pub use super::dataset::TrainingExample;
use adapteros_core::{derive_seed, AosError, Result};
use adapteros_db::{Db, TrainingMetricRow};
use adapteros_lora_kernel_api::FusedKernels;
use adapteros_telemetry::TelemetryWriter;
use chrono::Utc;
use parking_lot::RwLock;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

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
    kernels: Option<crate::backend_factory::KernelBox>,
    /// Selected backend for this training session
    selected_backend: Option<TrainingBackend>,
    /// Telemetry writer for training events
    telemetry: TelemetryWriter,
    /// Training seed for deterministic RNG
    training_seed: u64,
    /// Performance metrics for GPU utilization tracking
    performance_metrics: Arc<RwLock<TrainingPerformanceMetrics>>,
    /// Optional checkpoint manager for saving/resuming training
    checkpoint_manager: Option<CheckpointManager>,
    /// Cancellation token - set to true to request training stop
    cancel_token: Option<Arc<AtomicBool>>,
    /// Job ID for this training run (used for metrics persistence and cancellation)
    job_id: Option<String>,
    /// Optional database connection for metrics persistence
    db: Option<Db>,
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
    /// Checkpoint interval in epochs (None = no checkpoints, default = 5)
    #[serde(default)]
    pub checkpoint_interval: Option<u32>,
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
            checkpoint_interval: None, // Disabled by default
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

    /// Enable checkpoint saving every N epochs
    pub fn with_checkpoint_interval(mut self, interval: u32) -> Self {
        self.checkpoint_interval = Some(interval);
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
    /// True if training was cancelled before completion
    #[serde(default)]
    pub cancelled: bool,
    /// Epoch at which training stopped (whether completed or cancelled)
    #[serde(default)]
    pub stopped_at_epoch: Option<u32>,
    /// Total examples processed before stopping
    #[serde(default)]
    pub examples_processed: Option<u64>,
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
            checkpoint_manager: None,
            cancel_token: None,
            job_id: None,
            db: None,
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

    /// Set the cancellation token for this training run
    ///
    /// The token should be an `Arc<AtomicBool>` shared with the worker that can
    /// be set to `true` to request cancellation. The training loop checks this
    /// token at epoch boundaries and stops gracefully when set.
    pub fn set_cancel_token(&mut self, token: Arc<AtomicBool>) {
        self.cancel_token = Some(token);
    }

    /// Set the job ID for this training run
    ///
    /// The job ID is used for metrics persistence and logging.
    pub fn set_job_id(&mut self, job_id: String) {
        self.job_id = Some(job_id);
    }

    /// Set the database connection for metrics persistence
    ///
    /// When set, the trainer will persist metrics (loss, tokens/sec, etc.)
    /// to the `repository_training_metrics` table after each epoch.
    pub fn set_db(&mut self, db: Db) {
        self.db = Some(db);
    }

    /// Check if cancellation has been requested
    ///
    /// Returns `true` if the cancellation token is set and has been triggered.
    fn is_cancelled(&self) -> bool {
        self.cancel_token
            .as_ref()
            .map(|t| t.load(Ordering::SeqCst))
            .unwrap_or(false)
    }

    /// Persist training metrics to database
    ///
    /// Writes key metrics (loss, tokens_per_sec, etc.) to the repository_training_metrics table.
    /// If no DB is configured or job_id is not set, this is a no-op.
    async fn persist_epoch_metrics(
        &self,
        epoch: u32,
        step: u64,
        loss: f32,
        examples_count: u64,
        epoch_duration_us: u64,
    ) {
        let (job_id, db) = match (&self.job_id, &self.db) {
            (Some(jid), Some(db)) => (jid.clone(), db.clone()),
            _ => return, // No DB or job_id, skip persistence
        };

        let timestamp = Utc::now().to_rfc3339();
        let tokens_per_sec = if epoch_duration_us > 0 {
            // Rough estimate: examples * ~100 tokens per example / time
            (examples_count as f64 * 100.0) / (epoch_duration_us as f64 / 1_000_000.0)
        } else {
            0.0
        };

        let metrics = vec![
            TrainingMetricRow {
                id: Uuid::now_v7().to_string(),
                training_job_id: job_id.clone(),
                step: step as i64,
                epoch: Some(epoch as i64),
                metric_name: "loss".to_string(),
                metric_value: loss as f64,
                metric_timestamp: Some(timestamp.clone()),
            },
            TrainingMetricRow {
                id: Uuid::now_v7().to_string(),
                training_job_id: job_id.clone(),
                step: step as i64,
                epoch: Some(epoch as i64),
                metric_name: "tokens_per_sec".to_string(),
                metric_value: tokens_per_sec,
                metric_timestamp: Some(timestamp),
            },
        ];

        if let Err(e) = db.insert_training_metrics_batch(&metrics).await {
            warn!(
                job_id = %job_id,
                epoch = epoch,
                error = %e,
                "Failed to persist training metrics (non-fatal)"
            );
        } else {
            debug!(
                job_id = %job_id,
                epoch = epoch,
                loss = loss,
                tokens_per_sec = tokens_per_sec,
                "Training metrics persisted"
            );
        }
    }

    /// Enable checkpoint saving for resumable training
    ///
    /// This configures the trainer to save checkpoints periodically during training.
    /// Checkpoints allow training to be resumed from interruptions.
    ///
    /// # Arguments
    /// * `checkpoint_dir` - Directory to store checkpoint files
    /// * `adapter_id` - Adapter ID for naming checkpoint files
    /// * `max_checkpoints` - Maximum number of checkpoints to retain (older ones are deleted)
    pub fn enable_checkpointing<P: AsRef<std::path::Path>>(
        &mut self,
        checkpoint_dir: P,
        adapter_id: &str,
        max_checkpoints: usize,
    ) {
        let interval = self.config.checkpoint_interval.unwrap_or(5);
        self.checkpoint_manager = Some(CheckpointManager::new(
            checkpoint_dir,
            interval,
            max_checkpoints,
            adapter_id.to_string(),
        ));
        info!(
            adapter_id = %adapter_id,
            interval = interval,
            max_checkpoints = max_checkpoints,
            "Checkpoint saving enabled"
        );
    }

    /// Resume training from a checkpoint
    ///
    /// Loads the latest checkpoint and returns the starting epoch and weights.
    /// Returns None if no checkpoint exists.
    pub async fn try_resume_from_checkpoint(&self) -> Option<(u32, LoRAWeights, f32)> {
        let manager = self.checkpoint_manager.as_ref()?;

        if !manager.has_checkpoint().await {
            info!("No checkpoint found, starting fresh training");
            return None;
        }

        match manager.load_latest().await {
            Ok(checkpoint) => {
                info!(
                    epoch = checkpoint.epoch,
                    loss = checkpoint.loss,
                    "Resuming training from checkpoint"
                );
                Some((checkpoint.epoch, checkpoint.weights, checkpoint.best_loss))
            }
            Err(e) => {
                warn!(error = %e, "Failed to load checkpoint, starting fresh training");
                None
            }
        }
    }

    /// Train LoRA adapter on examples with GPU acceleration (if available)
    ///
    /// This method provides backward compatibility with automatic progress callback.
    /// For more control, use `train_with_callback` instead.
    pub async fn train(&mut self, examples: &[TrainingExample]) -> Result<TrainingResult> {
        // Backward-compatible behavior: no external progress callback
        self.train_with_callback(examples, |_epoch, _loss| {}).await
    }

    /// Train with automatic checkpoint resume
    ///
    /// If a checkpoint exists, resumes from the saved state. Otherwise starts fresh.
    /// This method automatically enables checkpointing if configured.
    pub async fn train_with_resume<C>(
        &mut self,
        examples: &[TrainingExample],
        on_epoch: C,
    ) -> Result<TrainingResult>
    where
        C: FnMut(usize, f32),
    {
        // Try to resume from checkpoint
        let resume_state = self.try_resume_from_checkpoint().await;

        if let Some((start_epoch, weights, _best_loss)) = resume_state {
            info!(
                start_epoch = start_epoch,
                "Resuming training from checkpoint"
            );
            self.train_from_epoch(examples, start_epoch as usize, Some(weights), on_epoch)
                .await
        } else {
            self.train_with_callback(examples, on_epoch).await
        }
    }

    /// Train starting from a specific epoch with optional initial weights
    async fn train_from_epoch<C>(
        &mut self,
        examples: &[TrainingExample],
        start_epoch: usize,
        initial_weights: Option<LoRAWeights>,
        mut on_epoch: C,
    ) -> Result<TrainingResult>
    where
        C: FnMut(usize, f32),
    {
        let backend_name = self.backend_info().unwrap_or("CPU");

        info!(
            "Resuming LoRA training from epoch {}: rank={}, epochs={}, examples={}, backend={}, seed={}",
            start_epoch,
            self.config.rank,
            self.config.epochs,
            examples.len(),
            backend_name,
            self.training_seed
        );

        self.telemetry.log(
            "training.resumed",
            serde_json::json!({
                "start_epoch": start_epoch,
                "total_epochs": self.config.epochs,
                "examples": examples.len(),
                "backend": backend_name,
            }),
        )?;

        let start = Instant::now();
        let adapter_id = Self::generate_adapter_id();

        // Use provided weights or initialize fresh
        let mut weights =
            initial_weights.unwrap_or_else(|| self.initialize_weights_deterministic().unwrap());

        // Training loop starting from resume point with cancellation support
        let mut final_loss = 0.0;
        let mut completed_epochs: u32 = start_epoch as u32;
        let mut examples_processed: u64 = 0;
        let mut was_cancelled = false;

        for epoch in start_epoch..self.config.epochs {
            // Check for cancellation at start of each epoch
            if self.is_cancelled() {
                let job_id_str = self.job_id.as_deref().unwrap_or("unknown");
                info!(
                    job_id = %job_id_str,
                    epoch = epoch,
                    "Cancellation requested, stopping resumed training"
                );
                self.telemetry
                    .log(
                        "training.cancelled",
                        serde_json::json!({
                            "job_id": job_id_str,
                            "adapter_id": adapter_id,
                            "stopped_at_epoch": epoch,
                            "final_loss": final_loss,
                            "examples_processed": examples_processed
                        }),
                    )
                    .ok();
                was_cancelled = true;
                break;
            }

            debug!("Epoch {}/{}", epoch + 1, self.config.epochs);

            let epoch_start = Instant::now();
            let epoch_loss = self.train_epoch_deterministic(&mut weights, examples, epoch)?;
            let epoch_duration_us = epoch_start.elapsed().as_micros() as u64;
            final_loss = epoch_loss;
            completed_epochs = (epoch + 1) as u32;
            examples_processed += examples.len() as u64;

            info!("Epoch {} loss: {:.4}", epoch + 1, epoch_loss);

            // Persist metrics to database
            self.persist_epoch_metrics(
                completed_epochs,
                examples_processed,
                epoch_loss,
                examples.len() as u64,
                epoch_duration_us,
            )
            .await;

            self.telemetry.log(
                "training.epoch_completed",
                serde_json::json!({
                    "epoch": epoch + 1,
                    "loss": epoch_loss,
                    "adapter_id": adapter_id
                }),
            )?;

            on_epoch(epoch + 1, epoch_loss);

            // Save checkpoint if configured
            if let Some(ref manager) = self.checkpoint_manager {
                let epoch_u32 = (epoch + 1) as u32;
                if manager.should_save(epoch_u32) {
                    let checkpoint = TrainingCheckpoint::new(
                        epoch_u32,
                        0,
                        epoch_loss,
                        self.config.learning_rate,
                        self.config.clone(),
                        weights.clone(),
                    );
                    if let Err(e) = manager.save_checkpoint(&checkpoint).await {
                        warn!(
                            epoch = epoch + 1,
                            error = %e,
                            "Failed to save checkpoint (non-fatal)"
                        );
                    } else {
                        info!(epoch = epoch + 1, loss = epoch_loss, "Checkpoint saved");
                    }
                }
            }

            // Check for cancellation after epoch completion
            if self.is_cancelled() {
                let job_id_str = self.job_id.as_deref().unwrap_or("unknown");
                info!(
                    job_id = %job_id_str,
                    epoch = epoch + 1,
                    "Cancellation confirmed after epoch completion"
                );
                was_cancelled = true;
                break;
            }

            if epoch_loss < 0.01 {
                info!("Early stopping: loss below threshold");
                break;
            }
        }

        let training_time_us = start.elapsed().as_micros() as u64;

        Ok(TrainingResult {
            adapter_id,
            final_loss,
            training_time_us,
            weights,
            cancelled: was_cancelled,
            stopped_at_epoch: Some(completed_epochs),
            examples_processed: Some(examples_processed),
        })
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

        // Training loop with telemetry and cancellation support
        let mut final_loss = 0.0;
        let mut completed_epochs: u32 = 0;
        let mut examples_processed: u64 = 0;
        let mut was_cancelled = false;

        for epoch in 0..self.config.epochs {
            // Check for cancellation at start of each epoch
            if self.is_cancelled() {
                let job_id_str = self.job_id.as_deref().unwrap_or("unknown");
                info!(
                    job_id = %job_id_str,
                    epoch = epoch,
                    "Cancellation requested, stopping training"
                );
                self.telemetry
                    .log(
                        "training.cancelled",
                        serde_json::json!({
                            "job_id": job_id_str,
                            "adapter_id": adapter_id,
                            "stopped_at_epoch": epoch,
                            "final_loss": final_loss,
                            "examples_processed": examples_processed
                        }),
                    )
                    .ok();
                was_cancelled = true;
                break;
            }

            debug!("Epoch {}/{}", epoch + 1, self.config.epochs);

            let epoch_start = Instant::now();
            let epoch_loss = self.train_epoch_deterministic(&mut weights, examples, epoch)?;
            let epoch_duration_us = epoch_start.elapsed().as_micros() as u64;
            final_loss = epoch_loss;
            completed_epochs = (epoch + 1) as u32;
            examples_processed += examples.len() as u64;

            info!("Epoch {} loss: {:.4}", epoch + 1, epoch_loss);

            // Persist metrics to database
            self.persist_epoch_metrics(
                completed_epochs,
                examples_processed,
                epoch_loss,
                examples.len() as u64,
                epoch_duration_us,
            )
            .await;

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

            // Save checkpoint if configured
            if let Some(ref manager) = self.checkpoint_manager {
                let epoch_u32 = (epoch + 1) as u32;
                if manager.should_save(epoch_u32) {
                    let checkpoint = TrainingCheckpoint::new(
                        epoch_u32,
                        0, // step within epoch (not tracked at epoch granularity)
                        epoch_loss,
                        self.config.learning_rate,
                        self.config.clone(),
                        weights.clone(),
                    );
                    if let Err(e) = manager.save_checkpoint(&checkpoint).await {
                        warn!(
                            epoch = epoch + 1,
                            error = %e,
                            "Failed to save checkpoint (non-fatal, training continues)"
                        );
                    } else {
                        info!(epoch = epoch + 1, loss = epoch_loss, "Checkpoint saved");
                        self.telemetry
                            .log(
                                "training.checkpoint_saved",
                                serde_json::json!({
                                    "epoch": epoch + 1,
                                    "loss": epoch_loss,
                                    "adapter_id": adapter_id
                                }),
                            )
                            .ok();
                    }
                }
            }

            // Check for cancellation after epoch completion (in case cancelled during training)
            if self.is_cancelled() {
                let job_id_str = self.job_id.as_deref().unwrap_or("unknown");
                info!(
                    job_id = %job_id_str,
                    epoch = epoch + 1,
                    "Cancellation confirmed after epoch completion"
                );
                self.telemetry
                    .log(
                        "training.cancelled",
                        serde_json::json!({
                            "job_id": job_id_str,
                            "adapter_id": adapter_id,
                            "stopped_at_epoch": epoch + 1,
                            "final_loss": final_loss,
                            "examples_processed": examples_processed
                        }),
                    )
                    .ok();
                was_cancelled = true;
                break;
            }

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

        if was_cancelled {
            info!(
                "Training cancelled: loss={:.4}, time={}us ({}ms), stopped_at_epoch={}, examples_processed={}",
                final_loss, training_time_us, training_time_ms, completed_epochs, examples_processed
            );
        } else {
            info!(
                "Training complete: loss={:.4}, time={}us ({}ms), backend={}, throughput={:.0} ex/s, seed={}",
                final_loss, training_time_us, training_time_ms, backend_name, examples_per_second, self.training_seed
            );
        }

        // Log training completion with performance metrics
        self.telemetry.log(
            if was_cancelled {
                "training.cancelled_final"
            } else {
                "training.completed"
            },
            serde_json::json!({
                "adapter_id": adapter_id,
                "final_loss": final_loss,
                "training_time_us": training_time_us,
                "training_time_ms": training_time_ms,
                "seed": self.training_seed,
                "backend": backend_name,
                "using_gpu": using_gpu,
                "cancelled": was_cancelled,
                "stopped_at_epoch": completed_epochs,
                "examples_processed": examples_processed,
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
            cancelled: was_cancelled,
            stopped_at_epoch: Some(completed_epochs),
            examples_processed: Some(examples_processed),
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
    ///
    /// Checks for cancellation every 10 batches to ensure bounded cancellation time.
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

        // Check cancel every N batches for bounded cancellation time
        const CANCEL_CHECK_INTERVAL: usize = 10;

        // Process examples in batches with deterministic ordering
        for batch_start in (0..examples.len()).step_by(self.config.batch_size) {
            // Check for cancellation every N batches
            if num_batches > 0 && num_batches % CANCEL_CHECK_INTERVAL == 0 {
                if self.is_cancelled() {
                    debug!(
                        epoch = epoch,
                        batch = num_batches,
                        "Cancellation detected mid-epoch, stopping batch loop"
                    );
                    // Return partial loss (average of completed batches)
                    return Ok(if num_batches > 0 {
                        total_loss / num_batches as f32
                    } else {
                        0.0
                    });
                }
            }

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
        // Scale to [-1, 1] range to prevent numerical instability
        // vocab_size for Qwen2.5 is ~152064, so we normalize appropriately
        let vocab_scale = 152064.0_f32;
        let hidden: Vec<f32> = input
            .iter()
            .take(self.config.hidden_dim)
            .map(|&token_id| ((token_id as f32) / vocab_scale) * 2.0 - 1.0)
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
        let vocab_scale = 152064.0_f32;

        for i in 0..n {
            // Use same scaling as forward pass
            let target_val = ((target[i] as f32) / vocab_scale) * 2.0 - 1.0;
            let diff = output[i] - target_val;
            loss += diff * diff; // MSE for simplicity
        }

        // Avoid returning 0.0 which could cause issues
        let avg_loss = loss / n as f32;
        if avg_loss.is_nan() || avg_loss.is_infinite() {
            0.1 // Fallback to small non-zero value
        } else {
            avg_loss
        }
    }

    /// Backward pass and weight update with deterministic RNG
    fn backward_and_update_deterministic(
        &self,
        weights: &mut LoRAWeights,
        hidden: &[f32],
        output: &[f32],
        target: &[u32],
        _loss: f32,
        rng: &mut impl Rng,
    ) -> Result<()> {
        // Simplified gradient descent with deterministic noise
        // In production, use proper backpropagation

        let n = output.len().min(target.len());
        let learning_rate = self.config.learning_rate;
        let vocab_scale = 152064.0_f32;

        // Compute gradient (simplified)
        let mut grad_output = vec![0.0; output.len()];
        for i in 0..n {
            // Use same scaling as forward pass
            let target_val = ((target[i] as f32) / vocab_scale) * 2.0 - 1.0;
            grad_output[i] = 2.0 * (output[i] - target_val) / n as f32;
        }

        // Add deterministic noise for regularization
        let noise_scale = 0.001;
        for grad in &mut grad_output {
            *grad += rng.gen_range(-noise_scale..noise_scale);
        }

        // Gradient clipping to prevent explosion
        const MAX_GRAD_NORM: f32 = 1.0;
        let grad_norm: f32 = grad_output.iter().map(|g| g * g).sum::<f32>().sqrt();
        if grad_norm > MAX_GRAD_NORM {
            let scale = MAX_GRAD_NORM / grad_norm;
            for grad in &mut grad_output {
                *grad *= scale;
            }
            debug!(
                "Clipped gradient norm from {:.4} to {:.4}",
                grad_norm, MAX_GRAD_NORM
            );
        }

        // NaN prevention: zero out any non-finite gradients
        for grad in &mut grad_output {
            if !grad.is_finite() {
                *grad = 0.0;
            }
        }

        // Update LoRA_A: gradient is dL/dA = hidden^T * grad_output (simplified)
        const MAX_UPDATE: f32 = 0.1;
        for r in 0..self.config.rank {
            for h_idx in 0..self.config.hidden_dim.min(hidden.len()) {
                if h_idx < weights.lora_a[r].len() {
                    let grad = grad_output[h_idx] * hidden[h_idx];
                    let update = (learning_rate * grad).clamp(-MAX_UPDATE, MAX_UPDATE);
                    weights.lora_a[r][h_idx] -= update;
                }
            }
        }

        // Update LoRA_B: gradient is dL/dB = intermediate^T * grad_output (simplified)
        for h_idx in 0..self.config.hidden_dim {
            if h_idx < weights.lora_b.len() {
                for r in 0..self.config.rank {
                    if r < weights.lora_b[h_idx].len() {
                        let grad = grad_output[h_idx] * hidden[h_idx];
                        let update = (learning_rate * grad).clamp(-MAX_UPDATE, MAX_UPDATE);
                        weights.lora_b[h_idx][r] -= update;
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

    // ========================================================================
    // Checkpoint Integration Tests
    // ========================================================================

    #[test]
    fn test_checkpoint_interval_config() {
        let config = TrainingConfig::default().with_checkpoint_interval(5);
        assert_eq!(config.checkpoint_interval, Some(5));
    }

    #[test]
    fn test_checkpoint_interval_default_none() {
        let config = TrainingConfig::default();
        assert_eq!(config.checkpoint_interval, None);
    }

    #[tokio::test]
    async fn test_enable_checkpointing() {
        let config = TrainingConfig {
            rank: 2,
            hidden_dim: 32,
            epochs: 10,
            checkpoint_interval: Some(2),
            ..Default::default()
        };
        let mut trainer = MicroLoRATrainer::new(config).unwrap();

        // Create temp dir for checkpoints
        let temp_dir = tempfile::TempDir::new().unwrap();

        // Enable checkpointing
        trainer.enable_checkpointing(temp_dir.path(), "test-adapter", 3);

        // Verify checkpoint manager is configured
        assert!(trainer.checkpoint_manager.is_some());
    }

    #[tokio::test]
    async fn test_train_with_checkpointing() {
        let config = TrainingConfig {
            rank: 2,
            hidden_dim: 32,
            batch_size: 1,
            epochs: 6,
            learning_rate: 0.01,
            checkpoint_interval: Some(2), // Save every 2 epochs
            ..Default::default()
        };
        let mut trainer = MicroLoRATrainer::new(config).unwrap();

        // Create temp dir for checkpoints
        let temp_dir = tempfile::TempDir::new().unwrap();
        trainer.enable_checkpointing(temp_dir.path(), "test-adapter", 3);

        let examples = vec![TrainingExample {
            input: vec![1, 2],
            target: vec![3, 4],
            metadata: HashMap::new(),
            weight: 1.0,
        }];

        // Train - checkpoints should be saved at epochs 2, 4, 6
        let result = trainer.train(&examples).await;
        assert!(result.is_ok());

        // Verify checkpoint files were created
        let checkpoint_files: Vec<_> = std::fs::read_dir(temp_dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map_or(false, |ext| ext == "ckpt"))
            .collect();

        // Should have at least the latest checkpoint
        assert!(
            !checkpoint_files.is_empty(),
            "Expected checkpoint files to be created"
        );
    }

    #[tokio::test]
    async fn test_try_resume_from_checkpoint_no_checkpoint() {
        let config = TrainingConfig {
            checkpoint_interval: Some(5),
            ..Default::default()
        };
        let trainer = MicroLoRATrainer::new(config).unwrap();

        // No checkpoint manager configured, should return None
        let resume_state = trainer.try_resume_from_checkpoint().await;
        assert!(resume_state.is_none());
    }

    #[tokio::test]
    async fn test_try_resume_from_checkpoint_with_checkpoint() {
        use crate::training::checkpoint::TrainingCheckpoint;

        let config = TrainingConfig {
            rank: 2,
            hidden_dim: 32,
            checkpoint_interval: Some(2),
            ..Default::default()
        };
        let mut trainer = MicroLoRATrainer::new(config.clone()).unwrap();

        // Create temp dir and save a checkpoint
        let temp_dir = tempfile::TempDir::new().unwrap();
        trainer.enable_checkpointing(temp_dir.path(), "test-adapter", 3);

        // Manually create a checkpoint
        let weights = LoRAWeights {
            lora_a: vec![vec![1.0, 2.0]],
            lora_b: vec![vec![3.0, 4.0]],
        };
        let checkpoint = TrainingCheckpoint::new(
            5, // epoch 5
            0, 0.5, // loss
            0.001, config, weights,
        );

        // Save checkpoint using the manager
        let manager = trainer.checkpoint_manager.as_ref().unwrap();
        manager.save_checkpoint(&checkpoint).await.unwrap();

        // Now try to resume
        let resume_state = trainer.try_resume_from_checkpoint().await;
        assert!(resume_state.is_some());

        let (epoch, _weights, _best_loss) = resume_state.unwrap();
        assert_eq!(epoch, 5);
    }
}
