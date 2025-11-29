//! Workflow execution for adapter stacks
//!
//! This module implements different execution strategies for adapter stacks:
//! - Sequential: Adapters are executed one after another, output feeds into next
//! - Parallel: All adapters execute simultaneously, results are merged
//! - UpstreamDownstream: Two-phase execution with upstream adapters first, then downstream
//!
//! # Backend Integration
//!
//! The module supports both real and mock kernel backends:
//! - **RealBackendAdapterBackend**: Uses actual hardware backends (Metal/CoreML/MLX) for production
//! - **MockAdapterBackend**: Lightweight testing backend with minimal overhead
//! - **KernelAdapterBackend**: Legacy generic backend (deprecated, use RealBackendAdapterBackend instead)
//!
//! # Feature Flags
//!
//! To enable specific backends:
//! - `coreml-backend`: CoreML + Apple Neural Engine (macOS 13+)
//! - `multi-backend`: MLX FFI backend (research/training)
//! - `mlx-backend`: Alias for multi-backend

use adapteros_core::{AosError, Result};
use adapteros_lora_kernel_api::{AdapterLookup, FusedKernels, IoBuffers, RouterRing};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

/// Helper function to execute a single adapter with kernel backend
///
/// Shared implementation to avoid duplication between RealBackendAdapterBackend
/// and KernelAdapterBackend. Handles routing setup, kernel execution, and
/// output token extraction via argmax.
async fn execute_adapter_with_kernel<K: FusedKernels>(
    kernels: &Arc<Mutex<K>>,
    adapter_index: u16,
    adapter_id: &str,
    input_tokens: &[u32],
    vocab_size: usize,
) -> Result<AdapterExecutionResult> {
    // Create router ring with single adapter
    let mut ring = RouterRing::new(1);
    ring.set(&[adapter_index], &[i16::MAX]); // Full weight to single adapter

    // Create IO buffers
    let mut io = IoBuffers::new(vocab_size);
    io.input_ids = input_tokens.to_vec();

    // Execute kernel
    {
        let mut kernels_guard = kernels.lock().await;
        kernels_guard.run_step(&ring, &mut io).map_err(|e| {
            error!(
                adapter_id = %adapter_id,
                error = %e,
                "Kernel execution failed"
            );
            e
        })?;
    }

    // Convert logits to output tokens (simplified: argmax)
    let output_tokens = if io.output_logits.is_empty() {
        vec![]
    } else {
        // Find argmax
        let (max_idx, _) = io
            .output_logits
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .unwrap_or((0, &0.0));
        vec![max_idx as u32]
    };

    debug!(
        adapter_id = %adapter_id,
        output_tokens_len = output_tokens.len(),
        "Adapter execution completed"
    );

    Ok(AdapterExecutionResult {
        output_tokens,
        state_updates: HashMap::new(),
    })
}

/// Workflow type for adapter execution
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowType {
    /// All adapters run in parallel, results are merged
    Parallel,
    /// Two-phase execution: upstream adapters first, then downstream
    UpstreamDownstream,
    /// Adapters run one after another in sequence
    Sequential,
}

/// Execution context for workflows
#[derive(Debug, Clone)]
pub struct WorkflowContext {
    /// Input tokens to process
    pub input_tokens: Vec<u32>,
    /// Current model state
    pub model_state: HashMap<String, Vec<f32>>,
    /// Metadata about the request
    pub metadata: HashMap<String, String>,
}

/// Result from workflow execution
#[derive(Debug, Clone)]
pub struct WorkflowResult {
    /// Output tokens generated
    pub output_tokens: Vec<u32>,
    /// Final model state after execution
    pub final_state: HashMap<String, Vec<f32>>,
    /// Execution statistics
    pub stats: ExecutionStats,
}

/// Statistics about workflow execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionStats {
    /// Total execution time in milliseconds
    pub total_time_ms: u64,
    /// Number of adapters executed
    pub adapters_executed: usize,
    /// Execution phases (for multi-phase workflows)
    pub phases: Vec<PhaseStats>,
}

/// Statistics for a single execution phase
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseStats {
    /// Phase name
    pub name: String,
    /// Adapters executed in this phase
    pub adapter_ids: Vec<String>,
    /// Phase execution time in milliseconds
    pub time_ms: u64,
}

/// Trait for executing individual adapters
///
/// This allows WorkflowExecutor to work with different execution backends
/// (real kernels, mocks, etc.)
pub trait AdapterExecutionBackend: Send + Sync {
    /// Execute a single adapter on the given input tokens
    ///
    /// # Arguments
    /// * `adapter_id` - Adapter identifier
    /// * `input_tokens` - Input token IDs
    /// * `model_state` - Current model state
    ///
    /// # Returns
    /// Output tokens and state updates
    fn execute_adapter(
        &self,
        adapter_id: &str,
        input_tokens: &[u32],
        model_state: &HashMap<String, Vec<f32>>,
    ) -> impl std::future::Future<Output = Result<AdapterExecutionResult>> + Send;
}

/// Workflow executor that handles different execution strategies
pub struct WorkflowExecutor<B: AdapterExecutionBackend> {
    /// Workflow type to execute
    workflow_type: WorkflowType,
    /// Adapter IDs in execution order
    adapter_ids: Vec<String>,
    /// Execution backend
    backend: Arc<B>,
}

impl<B: AdapterExecutionBackend> WorkflowExecutor<B> {
    /// Create a new workflow executor with execution backend
    pub fn new(workflow_type: WorkflowType, adapter_ids: Vec<String>, backend: Arc<B>) -> Self {
        Self {
            workflow_type,
            adapter_ids,
            backend,
        }
    }

    /// Execute the workflow with the given context
    pub async fn execute(&self, context: WorkflowContext) -> Result<WorkflowResult> {
        let start = std::time::Instant::now();

        let result = match &self.workflow_type {
            WorkflowType::Sequential => self.execute_sequential(context).await?,
            WorkflowType::Parallel => self.execute_parallel(context).await?,
            WorkflowType::UpstreamDownstream => self.execute_upstream_downstream(context).await?,
        };

        let total_time_ms = start.elapsed().as_millis() as u64;

        Ok(WorkflowResult {
            output_tokens: result.output_tokens,
            final_state: result.final_state,
            stats: ExecutionStats {
                total_time_ms,
                adapters_executed: self.adapter_ids.len(),
                phases: result.stats.phases,
            },
        })
    }

    /// Execute adapters sequentially
    async fn execute_sequential(&self, mut context: WorkflowContext) -> Result<WorkflowResult> {
        info!(
            "Executing sequential workflow with {} adapters",
            self.adapter_ids.len()
        );

        let mut phases = Vec::new();
        let mut current_output = context.input_tokens.clone();

        for adapter_id in &self.adapter_ids {
            let phase_start = std::time::Instant::now();
            debug!("Executing adapter: {}", adapter_id);

            // Execute single adapter
            let adapter_result = self
                .execute_adapter(adapter_id, &current_output, &context.model_state)
                .await?;

            // Update for next iteration
            current_output = adapter_result.output_tokens;
            context.model_state.extend(adapter_result.state_updates);

            phases.push(PhaseStats {
                name: format!("sequential_{}", adapter_id),
                adapter_ids: vec![adapter_id.clone()],
                time_ms: phase_start.elapsed().as_millis() as u64,
            });
        }

        Ok(WorkflowResult {
            output_tokens: current_output,
            final_state: context.model_state,
            stats: ExecutionStats {
                total_time_ms: 0, // Will be set by caller
                adapters_executed: self.adapter_ids.len(),
                phases,
            },
        })
    }

    /// Execute adapters in parallel
    async fn execute_parallel(&self, context: WorkflowContext) -> Result<WorkflowResult> {
        use futures::future::join_all;

        info!(
            "Executing parallel workflow with {} adapters",
            self.adapter_ids.len()
        );

        let phase_start = std::time::Instant::now();

        // Launch all adapters in parallel
        let futures: Vec<_> = self
            .adapter_ids
            .iter()
            .map(|adapter_id| {
                let id = adapter_id.clone();
                let tokens = context.input_tokens.clone();
                let state = context.model_state.clone();
                async move { self.execute_adapter(&id, &tokens, &state).await }
            })
            .collect();

        // Wait for all to complete
        let results = join_all(futures).await;

        // Merge results
        let mut merged_output = Vec::new();
        let mut merged_state = context.model_state.clone();

        for result in results {
            let adapter_result = result?;
            // For parallel execution, we merge outputs (simplified: concatenate)
            // In practice, this would use a more sophisticated merging strategy
            merged_output.extend(adapter_result.output_tokens);
            merged_state.extend(adapter_result.state_updates);
        }

        let phase_stats = PhaseStats {
            name: "parallel_all".to_string(),
            adapter_ids: self.adapter_ids.clone(),
            time_ms: phase_start.elapsed().as_millis() as u64,
        };

        Ok(WorkflowResult {
            output_tokens: merged_output,
            final_state: merged_state,
            stats: ExecutionStats {
                total_time_ms: 0, // Will be set by caller
                adapters_executed: self.adapter_ids.len(),
                phases: vec![phase_stats],
            },
        })
    }

    /// Execute adapters in upstream/downstream pattern
    async fn execute_upstream_downstream(
        &self,
        context: WorkflowContext,
    ) -> Result<WorkflowResult> {
        info!(
            "Executing upstream/downstream workflow with {} adapters",
            self.adapter_ids.len()
        );

        // Split adapters into upstream and downstream
        // For simplicity, first half are upstream, second half are downstream
        let split_point = self.adapter_ids.len() / 2;
        let upstream_ids: Vec<_> = self.adapter_ids[..split_point].to_vec();
        let downstream_ids: Vec<_> = self.adapter_ids[split_point..].to_vec();

        let mut phases = Vec::new();

        // Phase 1: Execute upstream adapters in parallel
        let phase1_start = std::time::Instant::now();
        debug!(
            "Phase 1: Executing {} upstream adapters",
            upstream_ids.len()
        );

        let upstream_executor = WorkflowExecutor::new(
            WorkflowType::Parallel,
            upstream_ids.clone(),
            self.backend.clone(),
        );
        let upstream_result = upstream_executor.execute_parallel(context.clone()).await?;

        phases.push(PhaseStats {
            name: "upstream".to_string(),
            adapter_ids: upstream_ids,
            time_ms: phase1_start.elapsed().as_millis() as u64,
        });

        // Phase 2: Execute downstream adapters with upstream results
        let phase2_start = std::time::Instant::now();
        debug!(
            "Phase 2: Executing {} downstream adapters",
            downstream_ids.len()
        );

        let downstream_context = WorkflowContext {
            input_tokens: upstream_result.output_tokens,
            model_state: upstream_result.final_state,
            metadata: context.metadata,
        };

        let downstream_executor = WorkflowExecutor::new(
            WorkflowType::Parallel,
            downstream_ids.clone(),
            self.backend.clone(),
        );
        let downstream_result = downstream_executor
            .execute_parallel(downstream_context)
            .await?;

        phases.push(PhaseStats {
            name: "downstream".to_string(),
            adapter_ids: downstream_ids,
            time_ms: phase2_start.elapsed().as_millis() as u64,
        });

        Ok(WorkflowResult {
            output_tokens: downstream_result.output_tokens,
            final_state: downstream_result.final_state,
            stats: ExecutionStats {
                total_time_ms: 0, // Will be set by caller
                adapters_executed: self.adapter_ids.len(),
                phases,
            },
        })
    }

    /// Execute a single adapter using the configured backend
    async fn execute_adapter(
        &self,
        adapter_id: &str,
        input_tokens: &[u32],
        model_state: &HashMap<String, Vec<f32>>,
    ) -> Result<AdapterExecutionResult> {
        debug!(
            "Executing adapter {} with {} input tokens",
            adapter_id,
            input_tokens.len()
        );

        // Delegate to backend
        self.backend
            .execute_adapter(adapter_id, input_tokens, model_state)
            .await
    }
}

/// Result from a single adapter execution
pub struct AdapterExecutionResult {
    /// Output tokens from this adapter
    pub output_tokens: Vec<u32>,
    /// State updates from this adapter
    pub state_updates: HashMap<String, Vec<f32>>,
}

/// Production execution backend using real kernel implementations
///
/// This backend integrates directly with hardware kernels (Metal/CoreML/MLX)
/// to provide actual LoRA inference with proper determinism attestation.
///
/// # Example Usage
///
/// ```ignore
/// use adapteros_lora_lifecycle::RealBackendAdapterBackend;
/// use std::sync::Arc;
///
/// // Initialize with automatic backend selection (CoreML -> Metal -> MLX)
/// let backend = RealBackendAdapterBackend::new_auto(
///     vec!["adapter1".to_string(), "adapter2".to_string()],
///     152064  // Qwen2.5 vocab size
/// ).await?;
///
/// // Or use a specific backend
/// #[cfg(target_os = "macos")]
/// let backend = RealBackendAdapterBackend::new_metal(
///     vec!["adapter1".to_string()],
///     152064
/// ).await?;
///
/// // Use with WorkflowExecutor
/// let executor = WorkflowExecutor::new(
///     WorkflowType::Sequential,
///     vec!["adapter1".to_string()],
///     Arc::new(backend)
/// );
/// let result = executor.execute(context).await?;
/// ```
///
/// # Backend Selection
///
/// The `new_auto` method uses this fallback chain:
/// 1. CoreML with Apple Neural Engine (most power-efficient)
/// 2. Metal (production, guaranteed determinism)
/// 3. MLX (experimental, requires explicit feature flag)
pub struct RealBackendAdapterBackend {
    /// The actual fused kernel backend (Metal/CoreML/MLX)
    kernels: Arc<Mutex<Box<dyn FusedKernels>>>,
    /// Adapter name to routing index mapping
    adapter_name_to_index: HashMap<String, u16>,
    /// Vocabulary size for output buffers
    vocab_size: usize,
    /// Backend identifier for logging/diagnostics
    backend_name: String,
}

impl RealBackendAdapterBackend {
    /// Create a new real backend with automatic selection (CoreML -> Metal -> MLX)
    ///
    /// # Arguments
    /// * `adapter_names` - List of adapter names in routing order
    /// * `vocab_size` - Model vocabulary size
    ///
    /// # Errors
    /// * If no suitable backend is available on the system
    /// * If kernel initialization fails
    pub async fn new_auto(adapter_names: Vec<String>, vocab_size: usize) -> Result<Self> {
        // Try CoreML first (most power-efficient)
        #[cfg(all(target_os = "macos", feature = "coreml-backend"))]
        {
            match Self::new_coreml(adapter_names.clone(), vocab_size).await {
                Ok(backend) => return Ok(backend),
                Err(e) => {
                    warn!(error = %e, "CoreML initialization failed, trying Metal");
                }
            }
        }

        // Try Metal (production, deterministic)
        #[cfg(target_os = "macos")]
        {
            match Self::new_metal(adapter_names.clone(), vocab_size).await {
                Ok(backend) => return Ok(backend),
                Err(e) => {
                    warn!(error = %e, "Metal initialization failed, trying MLX");
                    // Continue to MLX fallback
                }
            }
        }

        // Try MLX (experimental)
        #[cfg(feature = "multi-backend")]
        {
            return Self::new_mlx("/dev/null".to_string(), adapter_names, vocab_size).await;
        }

        // If we reach here, no backend was available
        #[allow(unreachable_code)]
        Err(AosError::Config(
            "No suitable backend available. Ensure Metal GPU, CoreML with ANE, or MLX is present."
                .to_string(),
        ))
    }

    /// Create a new real backend with Metal backend
    ///
    /// # Errors
    /// * If Metal is not available (non-macOS system)
    /// * If kernel initialization fails
    #[cfg(target_os = "macos")]
    pub async fn new_metal(adapter_names: Vec<String>, vocab_size: usize) -> Result<Self> {
        use adapteros_lora_kernel_mtl::MetalKernels;

        info!(
            adapters_count = adapter_names.len(),
            vocab_size = vocab_size,
            "Initializing RealBackendAdapterBackend with Metal backend"
        );

        let kernels = MetalKernels::new().map_err(|e| {
            error!(error = %e, "Failed to initialize Metal kernels");
            AosError::Kernel(format!("Metal initialization failed: {}", e))
        })?;

        // Attest to determinism
        kernels.attest_determinism().map_err(|e| {
            warn!(error = %e, "Metal backend failed determinism attestation");
            e
        })?;

        let backend_name = kernels.device_name().to_string();
        let adapter_name_to_index = adapter_names
            .into_iter()
            .enumerate()
            .map(|(i, name)| (name, i as u16))
            .collect();

        Ok(Self {
            kernels: Arc::new(Mutex::new(Box::new(kernels))),
            adapter_name_to_index,
            vocab_size,
            backend_name,
        })
    }

    /// Create a new real backend with CoreML backend
    ///
    /// # Errors
    /// * If CoreML feature is not enabled or not available
    /// * If kernel initialization fails
    #[cfg(all(target_os = "macos", feature = "coreml-backend"))]
    pub async fn new_coreml(adapter_names: Vec<String>, vocab_size: usize) -> Result<Self> {
        use adapteros_lora_kernel_coreml::{init_coreml, ComputeUnits, CoreMLBackend};

        info!(
            adapters_count = adapter_names.len(),
            vocab_size = vocab_size,
            "Initializing RealBackendAdapterBackend with CoreML backend"
        );

        // Initialize CoreML runtime
        init_coreml().map_err(|e| {
            error!(error = %e, "Failed to initialize CoreML runtime");
            e
        })?;

        // Use CpuAndNeuralEngine for optimal ANE utilization
        let compute_units = ComputeUnits::CpuAndNeuralEngine;
        let backend = CoreMLBackend::new(compute_units, false).map_err(|e| {
            error!(error = %e, "Failed to create CoreML backend");
            e
        })?;

        let mut kernels = backend;

        // Attest to determinism
        kernels.attest_determinism().map_err(|e| {
            warn!(error = %e, "CoreML backend failed determinism attestation");
            e
        })?;

        let backend_name = kernels.device_name().to_string();
        let adapter_name_to_index = adapter_names
            .into_iter()
            .enumerate()
            .map(|(i, name)| (name, i as u16))
            .collect();

        Ok(Self {
            kernels: Arc::new(Mutex::new(Box::new(kernels))),
            adapter_name_to_index,
            vocab_size,
            backend_name,
        })
    }

    /// Create a new real backend with MLX backend
    ///
    /// # Arguments
    /// * `model_path` - Path to the MLX model file
    /// * `adapter_names` - List of adapter names in routing order
    /// * `vocab_size` - Model vocabulary size
    ///
    /// # Errors
    /// * If multi-backend feature is not enabled
    /// * If MLX model cannot be loaded
    /// * If kernel initialization fails
    #[cfg(feature = "multi-backend")]
    pub async fn new_mlx(
        model_path: String,
        adapter_names: Vec<String>,
        vocab_size: usize,
    ) -> Result<Self> {
        use adapteros_lora_mlx_ffi::{MLXFFIBackend, MLXFFIModel};

        info!(
            model_path = %model_path,
            adapters_count = adapter_names.len(),
            vocab_size = vocab_size,
            "Initializing RealBackendAdapterBackend with MLX backend"
        );

        // Load the model
        let model = MLXFFIModel::load(&model_path).map_err(|e| {
            error!(error = %e, model_path = %model_path, "Failed to load MLX model");
            AosError::Kernel(format!("MLX model load failed: {}", e))
        })?;

        let backend = MLXFFIBackend::new(model);
        let mut kernels = backend;

        // Attest to determinism (note: MLX may be non-deterministic)
        if let Err(e) = kernels.attest_determinism() {
            warn!(error = %e, "MLX backend may not provide determinism guarantees");
        }

        let backend_name = kernels.device_name().to_string();
        let adapter_name_to_index = adapter_names
            .into_iter()
            .enumerate()
            .map(|(i, name)| (name, i as u16))
            .collect();

        Ok(Self {
            kernels: Arc::new(Mutex::new(Box::new(kernels))),
            adapter_name_to_index,
            vocab_size,
            backend_name,
        })
    }

    /// Get the backend device name
    pub fn backend_name(&self) -> &str {
        &self.backend_name
    }
}

impl AdapterExecutionBackend for RealBackendAdapterBackend {
    async fn execute_adapter(
        &self,
        adapter_id: &str,
        input_tokens: &[u32],
        _model_state: &HashMap<String, Vec<f32>>,
    ) -> Result<AdapterExecutionResult> {
        debug!(
            adapter_id = %adapter_id,
            input_tokens_len = input_tokens.len(),
            "Executing adapter with real backend"
        );

        // Get adapter index for routing
        let adapter_index = self
            .adapter_name_to_index
            .get(adapter_id)
            .copied()
            .ok_or_else(|| {
                AosError::NotFound(format!(
                    "Adapter not found in routing table: {}",
                    adapter_id
                ))
            })?;

        execute_adapter_with_kernel(
            &self.kernels,
            adapter_index,
            adapter_id,
            input_tokens,
            self.vocab_size,
        )
        .await
    }
}

/// Mock execution backend for testing
///
/// Lightweight testing backend that simulates adapter execution without
/// requiring actual hardware kernels. Used for unit tests and integration
/// tests where real kernel access is not available or necessary.
///
/// # Features
///
/// - No hardware dependencies
/// - Deterministic output (input tokens echoed as output)
/// - Minimal memory footprint
/// - Fast execution (10ms per adapter)
///
/// # Example
///
/// ```ignore
/// use adapteros_lora_lifecycle::{WorkflowExecutor, WorkflowType, MockAdapterBackend};
/// use std::sync::Arc;
///
/// let backend = Arc::new(MockAdapterBackend);
/// let executor = WorkflowExecutor::new(
///     WorkflowType::Sequential,
///     vec!["adapter1".to_string()],
///     backend
/// );
/// let result = executor.execute(context).await?;
/// ```
#[derive(Default)]
pub struct MockAdapterBackend;

impl AdapterExecutionBackend for MockAdapterBackend {
    async fn execute_adapter(
        &self,
        adapter_id: &str,
        input_tokens: &[u32],
        _model_state: &HashMap<String, Vec<f32>>,
    ) -> Result<AdapterExecutionResult> {
        debug!("Mock execution of adapter {}", adapter_id);

        // Simulate processing
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        Ok(AdapterExecutionResult {
            output_tokens: input_tokens.to_vec(),
            state_updates: HashMap::new(),
        })
    }
}

/// Deprecated: Use `RealBackendAdapterBackend` instead
///
/// Generic kernel-based execution backend for advanced use cases.
/// This type remains for backwards compatibility with code using the `AdapterLookup` trait.
///
/// # Migration Guide
///
/// If you have code like:
/// ```ignore
/// let backend = KernelAdapterBackend::new(
///     kernels_arc.clone(),
///     lookup_arc,
///     adapter_names.clone(),
///     152064
/// );
/// ```
///
/// Replace with:
/// ```ignore
/// // For automatic backend selection:
/// let backend = RealBackendAdapterBackend::new_auto(
///     adapter_names,
///     152064
/// ).await?;
///
/// // Or for specific backend:
/// let backend = RealBackendAdapterBackend::new_metal(
///     adapter_names,
///     152064
/// ).await?;
/// ```
///
/// The new `RealBackendAdapterBackend` integrates with direct kernel creation
/// for automatic capability detection and backend selection.
pub struct KernelAdapterBackend<K: FusedKernels, L: AdapterLookup> {
    /// Adapter lookup (abstracts AdapterTable)
    lookup: Arc<L>,
    /// Fused kernels for execution
    kernels: Arc<Mutex<K>>,
    /// Adapter name to routing index mapping
    adapter_name_to_index: HashMap<String, u16>,
    /// Vocabulary size for output buffers
    vocab_size: usize,
}

impl<K: FusedKernels, L: AdapterLookup> KernelAdapterBackend<K, L> {
    /// Create a new kernel adapter backend
    ///
    /// # Deprecated
    /// Use `RealBackendAdapterBackend::new_auto()` or similar instead.
    ///
    /// # Arguments
    /// * `kernels` - Fused kernels for GPU execution
    /// * `lookup` - Adapter lookup implementation
    /// * `adapter_names` - List of adapter names in routing order
    /// * `vocab_size` - Model vocabulary size
    #[deprecated(
        since = "0.1.0",
        note = "Use RealBackendAdapterBackend::new_auto() instead"
    )]
    pub fn new(
        kernels: Arc<Mutex<K>>,
        lookup: Arc<L>,
        adapter_names: Vec<String>,
        vocab_size: usize,
    ) -> Self {
        let adapter_name_to_index: HashMap<String, u16> = adapter_names
            .into_iter()
            .enumerate()
            .map(|(i, name)| (name, i as u16))
            .collect();

        Self {
            lookup,
            kernels,
            adapter_name_to_index,
            vocab_size,
        }
    }
}

impl<K: FusedKernels + 'static, L: AdapterLookup + 'static> AdapterExecutionBackend
    for KernelAdapterBackend<K, L>
{
    async fn execute_adapter(
        &self,
        adapter_id: &str,
        input_tokens: &[u32],
        _model_state: &HashMap<String, Vec<f32>>,
    ) -> Result<AdapterExecutionResult> {
        // Get adapter index for routing
        let adapter_index = self
            .adapter_name_to_index
            .get(adapter_id)
            .copied()
            .or_else(|| self.lookup.get_adapter_index(adapter_id))
            .ok_or_else(|| {
                adapteros_core::AosError::NotFound(format!("Adapter not found: {}", adapter_id))
            })?;

        execute_adapter_with_kernel(
            &self.kernels,
            adapter_index,
            adapter_id,
            input_tokens,
            self.vocab_size,
        )
        .await
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sequential_workflow() {
        let backend = Arc::new(MockAdapterBackend);
        let executor = WorkflowExecutor::new(
            WorkflowType::Sequential,
            vec!["adapter1".to_string(), "adapter2".to_string()],
            backend,
        );

        let context = WorkflowContext {
            input_tokens: vec![1, 2, 3],
            model_state: HashMap::new(),
            metadata: HashMap::new(),
        };

        let result = executor.execute(context).await.unwrap();
        assert_eq!(result.stats.adapters_executed, 2);
        assert_eq!(result.stats.phases.len(), 2);
        assert_eq!(result.stats.phases[0].name, "sequential_adapter1");
    }

    #[tokio::test]
    async fn test_parallel_workflow() {
        let backend = Arc::new(MockAdapterBackend);
        let executor = WorkflowExecutor::new(
            WorkflowType::Parallel,
            vec![
                "adapter1".to_string(),
                "adapter2".to_string(),
                "adapter3".to_string(),
            ],
            backend,
        );

        let context = WorkflowContext {
            input_tokens: vec![1, 2, 3],
            model_state: HashMap::new(),
            metadata: HashMap::new(),
        };

        let result = executor.execute(context).await.unwrap();
        assert_eq!(result.stats.adapters_executed, 3);
        assert_eq!(result.stats.phases.len(), 1);
        assert_eq!(result.stats.phases[0].name, "parallel_all");
    }

    #[tokio::test]
    async fn test_upstream_downstream_workflow() {
        let backend = Arc::new(MockAdapterBackend);
        let executor = WorkflowExecutor::new(
            WorkflowType::UpstreamDownstream,
            vec![
                "upstream1".to_string(),
                "upstream2".to_string(),
                "downstream1".to_string(),
                "downstream2".to_string(),
            ],
            backend,
        );

        let context = WorkflowContext {
            input_tokens: vec![1, 2, 3],
            model_state: HashMap::new(),
            metadata: HashMap::new(),
        };

        let result = executor.execute(context).await.unwrap();
        assert_eq!(result.stats.adapters_executed, 4);
        assert_eq!(result.stats.phases.len(), 2);
        assert_eq!(result.stats.phases[0].name, "upstream");
        assert_eq!(result.stats.phases[1].name, "downstream");
    }
}
