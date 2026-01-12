//! MLX Subprocess Bridge - Python mlx-lm subprocess integration
//!
//! This module provides a subprocess-based bridge to Python's mlx-lm library,
//! enabling support for MoE (Mixture of Experts) models that are not supported
//! by the MLX FFI backend.
//!
//! ## Architecture
//!
//! - Spawns a Python process running scripts/mlx_bridge_server.py
//! - Communicates via stdin/stdout JSON messages
//! - Implements FusedKernels trait for integration with worker backend system
//! - Handles process lifecycle (start, health check, restart on failure)
//!
//! ## Protocol (v2)
//!
//! Request format (sent to stdin):
//! ```json
//! {
//!     "type": "generate",
//!     "prompt": "def hello():",
//!     "max_tokens": 50,
//!     "temperature": 0.7,
//!     "top_p": 0.9,
//!     "stream": true,
//!     "protocol_version": 2
//! }
//! ```
//!
//! Response format (non-streaming):
//! ```json
//! {
//!     "type": "generate_response",
//!     "text": "generated text",
//!     "tokens": 42,
//!     "finish_reason": "stop",
//!     "usage": {"prompt_tokens": 10, "completion_tokens": 42, "total_tokens": 52},
//!     "timing": {"total_ms": 2500.0, "tokens_per_second": 16.8}
//! }
//! ```
//!
//! Streaming response format (multiple JSON lines):
//! ```json
//! {"type": "stream_token", "token": "def", "index": 0, "token_id": 1234}
//! {"type": "stream_token", "token": " foo", "index": 1, "token_id": 5678}
//! {"type": "stream_end", "tokens": 2, "finish_reason": "stop", "text": "def foo",
//!  "usage": {...}, "timing": {"ttft_ms": 150.0, "total_ms": 500.0, ...}}
//! ```
//!
//! ## Streaming Usage
//!
//! Callback-based (recommended):
//! ```ignore
//! bridge.generate_stream("prompt", 50, 0.7, 0.9, |token| {
//!     print!("{}", token.text);
//!     true // continue
//! })?;
//! ```
//!
//! Iterator-based:
//! ```ignore
//! for event in bridge.generate_stream_iter("prompt", 50, 0.7, 0.9)? {
//!     match event? {
//!         StreamingEvent::Token(t) => print!("{}", t.token),
//!         StreamingEvent::Done(r) => println!("\n{} tokens", r.tokens_generated),
//!     }
//! }
//! ```

use adapteros_core::{AosError, B3Hash, Result};
use adapteros_lora_kernel_api::{
    attestation::{
        BackendType, DeterminismLevel, DeterminismReport, FloatingPointMode, RngSeedingMethod,
    },
    FusedKernels, IoBuffers, MoEInfo, RouterRing, SequenceExpertRouting, TextGenerationKernel,
    TextGenerationResult, TextGenerationTiming, TextGenerationUsage, TextToken,
};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

use crate::moe_prefix_cache::ExpertHeatMap;
use crate::moe_types::{ExpertId, ExpertRouting, LayerIdx};

// =============================================================================
// Routing Hash Chain - Deterministic expert routing attestation
// =============================================================================

/// Domain separator for routing hash chain (prevents cross-domain attacks)
const ROUTING_HASH_DOMAIN: &[u8] = b"aos.routing.v1";

/// Tracks per-token expert routing with a deterministic BLAKE3 hash chain.
///
/// The hash chain provides cryptographic attestation that expert routing
/// decisions were made in a specific order, enabling:
/// - Reproducibility verification across runs
/// - Audit trails for compliance
/// - Drift detection between backends
///
/// ## Hash Chain Construction
///
/// For each token step:
/// 1. Canonicalize routing: sort by (layer_idx asc, expert_id asc)
/// 2. Compute token digest: `BLAKE3(domain || token_index || num_pairs || pairs...)`
/// 3. Update chain head: `BLAKE3(prev_head || token_digest)`
///
/// ## Wire Format (per-token record)
///
/// ```text
/// [domain_separator: 14 bytes "aos.routing.v1"] (only once at init)
/// [token_index: u32 LE]
/// [num_pairs: u16 LE]
/// [pairs: (layer_idx: u16 LE, expert_id: u8)*]
/// ```
#[derive(Debug, Clone)]
pub struct RoutingHashChain {
    /// Current hash chain head
    head: B3Hash,
    /// Number of tokens processed
    token_count: u32,
    /// Running heat map for expert activations
    heat_map: ExpertHeatMap,
    /// Whether the domain separator has been mixed in
    initialized: bool,
}

impl RoutingHashChain {
    /// Create a new routing hash chain for a model with the given number of layers
    pub fn new(num_layers: usize) -> Self {
        Self {
            head: B3Hash::new([0u8; 32]),
            token_count: 0,
            heat_map: ExpertHeatMap::new(num_layers),
            initialized: false,
        }
    }

    /// Record a token's expert routing and update the hash chain
    ///
    /// # Arguments
    /// * `token_index` - Zero-based token index in the generation
    /// * `expert_routing` - Per-layer expert selections: (layer_idx, expert_id)
    pub fn record_token(&mut self, token_index: u32, expert_routing: &[(LayerIdx, ExpertId)]) {
        // Initialize with domain separator on first token
        if !self.initialized {
            self.head = B3Hash::hash(ROUTING_HASH_DOMAIN);
            self.initialized = true;
        }

        // Canonicalize: sort by (layer_idx asc, expert_id asc)
        let mut sorted_routing: Vec<(LayerIdx, ExpertId)> = expert_routing.to_vec();
        sorted_routing.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));

        // Build the canonical token record
        // Format: token_index (u32 LE) || num_pairs (u16 LE) || pairs...
        let num_pairs = sorted_routing.len() as u16;
        let record_size = 4 + 2 + (sorted_routing.len() * 3); // u32 + u16 + (u16 + u8)*n
        let mut record = Vec::with_capacity(record_size);

        // Token index (u32 LE)
        record.extend_from_slice(&token_index.to_le_bytes());

        // Number of pairs (u16 LE)
        record.extend_from_slice(&num_pairs.to_le_bytes());

        // Each pair: layer_idx (u16 LE), expert_id (u8)
        for &(layer_idx, expert_id) in &sorted_routing {
            record.extend_from_slice(&(layer_idx as u16).to_le_bytes());
            record.push(expert_id);
        }

        // Compute token digest
        let token_digest = B3Hash::hash(&record);

        // Update chain head: new_head = BLAKE3(prev_head || token_digest)
        self.head = B3Hash::hash_multi(&[self.head.as_bytes(), token_digest.as_bytes()]);

        // Record in heat map for statistical analysis
        self.heat_map.record_token_routing(expert_routing);

        self.token_count += 1;
    }

    /// Get the current hash chain head
    pub fn head(&self) -> &B3Hash {
        &self.head
    }

    /// Get the number of tokens processed
    pub fn token_count(&self) -> u32 {
        self.token_count
    }

    /// Get the accumulated heat map
    pub fn heat_map(&self) -> &ExpertHeatMap {
        &self.heat_map
    }

    /// Consume the chain and return the finalized heat map
    pub fn into_heat_map(mut self, top_k_experts: usize) -> ExpertHeatMap {
        self.heat_map.finalize(top_k_experts);
        self.heat_map
    }

    /// Get a summary of the routing statistics
    pub fn summary(&self) -> RoutingChainSummary {
        RoutingChainSummary {
            head_hash: self.head.to_string(),
            token_count: self.token_count,
            routing_stability: self.heat_map.routing_stability,
            sample_count: self.heat_map.sample_count,
        }
    }
}

/// Summary of routing hash chain state
#[derive(Debug, Clone)]
pub struct RoutingChainSummary {
    /// Current hash chain head (hex string)
    pub head_hash: String,
    /// Number of tokens in chain
    pub token_count: u32,
    /// Routing stability score (0.0-1.0)
    pub routing_stability: f32,
    /// Total samples recorded in heat map
    pub sample_count: u32,
}

/// Current protocol version for bridge communication
/// v2: Streaming support, usage stats, timing metrics
/// v3: MoE expert routing collection, free token integration
const PROTOCOL_VERSION: u32 = 3;

/// Default max restarts before giving up
const DEFAULT_MAX_RESTARTS: usize = 3;

/// Configuration for the MLX subprocess bridge
#[derive(Debug, Clone)]
pub struct MlxBridgeConfig {
    /// Python executable path (default: "python3")
    pub python_path: String,
    /// Process startup timeout in seconds (default: 120)
    pub startup_timeout_secs: u64,
    /// Request timeout in seconds (default: 300)
    pub request_timeout_secs: u64,
    /// Maximum restart attempts before giving up (default: 3)
    pub max_restarts: usize,
    /// Health check interval in seconds (default: 30)
    pub health_check_interval_secs: u64,
    /// Default temperature for generation (default: 0.7)
    pub default_temperature: f32,
    /// Default top_p for generation (default: 0.9)
    pub default_top_p: f32,
}

impl Default for MlxBridgeConfig {
    fn default() -> Self {
        Self {
            python_path: std::env::var("MLX_BRIDGE_PYTHON_PATH")
                .unwrap_or_else(|_| "python3".to_string()),
            startup_timeout_secs: std::env::var("MLX_BRIDGE_STARTUP_TIMEOUT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(120),
            request_timeout_secs: std::env::var("MLX_BRIDGE_TIMEOUT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(300),
            max_restarts: std::env::var("MLX_BRIDGE_MAX_RESTARTS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(DEFAULT_MAX_RESTARTS),
            health_check_interval_secs: 30,
            default_temperature: 0.7,
            default_top_p: 0.9,
        }
    }
}

/// Usage statistics from the bridge responses
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UsageStats {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
}

/// Timing statistics from the bridge responses
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TimingStats {
    pub ttft_ms: f64,
    pub total_ms: f64,
    pub tokens_per_second: f64,
}

impl From<UsageStats> for TextGenerationUsage {
    fn from(usage: UsageStats) -> Self {
        TextGenerationUsage {
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
            total_tokens: usage.total_tokens,
        }
    }
}

impl From<TimingStats> for TextGenerationTiming {
    fn from(timing: TimingStats) -> Self {
        TextGenerationTiming {
            ttft_ms: timing.ttft_ms,
            total_ms: timing.total_ms,
            tokens_per_second: timing.tokens_per_second,
        }
    }
}

/// Result of a text generation request
#[derive(Debug, Clone, Default)]
pub struct GenerationResult {
    /// Generated text
    pub text: String,
    /// Number of tokens generated
    pub token_count: usize,
    /// Reason for stopping (e.g., "stop", "length")
    pub finish_reason: String,
    /// Usage statistics
    pub usage: Option<UsageStats>,
    /// Timing statistics
    pub timing: Option<TimingStats>,
    /// Protocol v3: MoE info
    pub moe_info: Option<MoEInfo>,
    /// Protocol v3: expert routing
    pub expert_routing: Option<SequenceExpertRouting>,
}

/// Request types sent to bridge server
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum BridgeRequest {
    Generate {
        prompt: String,
        max_tokens: usize,
        temperature: f32,
        top_p: f32,
        #[serde(default)]
        stop_sequences: Vec<String>,
        #[serde(default)]
        stream: bool,
        #[serde(default)]
        protocol_version: u32,
        /// Protocol v3: Request expert routing data collection (MoE models)
        #[serde(default)]
        collect_routing: bool,
    },
    HealthCheck,
    Prewarm {
        experts: Vec<(usize, u8)>,
    },
    Shutdown,
}

/// A streamed token from generation (internal alias)
pub type StreamingToken = TextToken;

/// Result of a completed streaming generation (internal alias)
pub type StreamingResult = TextGenerationResult;

/// Response types received from bridge server
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum BridgeResponse {
    Ready {
        model_path: String,
        model_type: String,
        #[serde(default)]
        protocol_version: u32,
        #[serde(default)]
        streaming_supported: bool,
        /// Protocol v3: MoE model detection
        #[serde(default)]
        is_moe: bool,
        #[serde(default)]
        num_experts: usize,
        #[serde(default)]
        experts_per_token: usize,
    },
    GenerateResponse {
        text: String,
        tokens: usize,
        finish_reason: String,
        #[serde(default, rename = "usage")]
        usage: Option<UsageStats>,
        #[serde(default, rename = "timing")]
        timing: Option<TimingStats>,
        /// Protocol v3: MoE info
        #[serde(default)]
        moe_info: Option<MoEInfo>,
        /// Protocol v3: Expert routing data for the generated sequence
        #[serde(default)]
        expert_routing: Option<SequenceExpertRouting>,
    },
    StreamToken {
        #[serde(rename = "token")]
        text: String,
        index: usize,
        #[serde(default)]
        token_id: Option<usize>,
        /// Protocol v3: Expert routing for this token (layer_idx, expert_idx)
        /// Wire format key "routing" preserved for Python bridge compatibility
        #[serde(default, rename = "routing")]
        expert_routing: Option<ExpertRouting>,
    },
    StreamEnd {
        #[serde(rename = "tokens")]
        tokens: usize,
        finish_reason: String,
        #[serde(default)]
        text: String,
        #[serde(default, rename = "usage")]
        usage: Option<UsageStats>,
        #[serde(default, rename = "timing")]
        timing: Option<TimingStats>,
        /// Protocol v3: MoE info
        #[serde(default)]
        moe_info: Option<MoEInfo>,
        /// Protocol v3: All expert routing data per token
        #[serde(default)]
        expert_routing: Option<SequenceExpertRouting>,
    },
    HealthResponse {
        status: String,
        model_loaded: bool,
    },
    PrewarmResponse {
        status: String,
        experts_loaded: usize,
    },
    ShutdownAck,
    Error {
        error: String,
        error_type: String,
    },
}

/// Events emitted by StreamingIterator
#[derive(Debug)]
pub enum StreamingEvent {
    /// A new token was generated
    Token(StreamingToken),
    /// Generation completed
    Done(StreamingResult),
}

/// Iterator-based streaming interface
///
/// Provides a pull-based streaming interface where each call to \`next()\`
/// blocks until the next token is available or generation completes.
pub struct StreamingIterator {
    process: Arc<Mutex<Option<BridgeProcess>>>,
    done: bool,
    accumulated_text: String,
    token_count: usize,
}

impl Iterator for StreamingIterator {
    type Item = Result<StreamingEvent>;

    fn next(&mut self) -> Option<Self::Item> {
        // Use a loop instead of recursion to avoid borrow checker issues
        loop {
            if self.done {
                return None;
            }

            let mut process_guard = match self.process.lock() {
                Ok(guard) => guard,
                Err(e) => {
                    self.done = true;
                    return Some(Err(AosError::Kernel(format!(
                        "Failed to lock process: {}",
                        e
                    ))));
                }
            };

            let process = match process_guard.as_mut() {
                Some(p) => p,
                None => {
                    self.done = true;
                    return Some(Err(AosError::Kernel(
                        "Bridge process not available".to_string(),
                    )));
                }
            };

            let response = match MLXSubprocessBridge::read_response(&mut process.stdout) {
                Ok(r) => r,
                Err(e) => {
                    self.done = true;
                    return Some(Err(e));
                }
            };

            // Drop the guard before processing the response to allow loop iteration
            drop(process_guard);

            match response {
                BridgeResponse::StreamToken {
                    text,
                    index,
                    token_id,
                    expert_routing,
                } => {
                    self.accumulated_text.push_str(&text);
                    self.token_count = index + 1;

                    return Some(Ok(StreamingEvent::Token(StreamingToken {
                        text,
                        index,
                        token_id,
                        expert_routing,
                        is_free: false,
                    })));
                }
                BridgeResponse::StreamEnd {
                    tokens,
                    finish_reason,
                    text,
                    usage,
                    timing,
                    moe_info,
                    expert_routing,
                } => {
                    self.done = true;

                    let final_text = if text.is_empty() {
                        std::mem::take(&mut self.accumulated_text)
                    } else {
                        text
                    };

                    // Note: Iterator interface doesn't track routing hash chain
                    // Use generate_stream() callback interface for full routing attestation
                    return Some(Ok(StreamingEvent::Done(StreamingResult {
                        text: final_text,
                        tokens_generated: tokens,
                        finish_reason,
                        usage_stats: usage.map(Into::into),
                        timing_stats: timing.map(Into::into),
                        moe_info,
                        expert_routing,
                        free_tokens_delivered: 0,
                        routing_hash: None,
                    })));
                }
                BridgeResponse::Error { error, error_type } => {
                    self.done = true;
                    return Some(Err(AosError::Kernel(format!(
                        "Streaming generation failed ({}): {}",
                        error_type, error
                    ))));
                }
                _ => {
                    // Skip unexpected responses by continuing the loop
                    continue;
                }
            }
        }
    }
}

/// MLX subprocess bridge backend
pub struct MLXSubprocessBridge {
    /// Model path
    model_path: PathBuf,
    /// Bridge configuration
    config: MlxBridgeConfig,
    /// Bridge script path
    script_path: PathBuf,
    /// Child process handle
    process: Arc<Mutex<Option<BridgeProcess>>>,
    /// Device name for FusedKernels trait
    device: String,
    /// Manifest hash (optional, for determinism attestation)
    manifest_hash: Option<B3Hash>,
    /// Vocabulary size for output logits buffer
    vocab_size: usize,
    /// Process restart count
    restart_count: Arc<Mutex<usize>>,
    /// Last health check time
    last_health_check: Arc<Mutex<Option<Instant>>>,
    /// Whether native streaming is supported by the bridge
    streaming_supported: Arc<Mutex<bool>>,
    /// Protocol version reported by the bridge
    bridge_protocol_version: Arc<Mutex<u32>>,
    /// Accumulated context for run_step (token IDs seen so far)
    context_buffer: Arc<Mutex<Vec<u32>>>,
    /// Whether model is MoE (Mixture of Experts)
    is_moe: Arc<Mutex<bool>>,
    /// Number of experts (MoE models)
    num_experts: Arc<Mutex<usize>>,
    /// Active experts per token (MoE models)
    experts_per_token: Arc<Mutex<usize>>,
    /// Whether to collect expert routing data
    collect_routing: Arc<Mutex<bool>>,
    /// Current adapter ID (for free token lookup)
    current_adapter_id: Arc<Mutex<Option<String>>>,
}

impl std::fmt::Debug for MLXSubprocessBridge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MLXSubprocessBridge")
            .field("model_path", &self.model_path)
            .field("device", &self.device)
            .field("vocab_size", &self.vocab_size)
            .finish_non_exhaustive()
    }
}

/// Bridge process state
struct BridgeProcess {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl MLXSubprocessBridge {
    /// Create a new MLX subprocess bridge with default configuration
    ///
    /// # Arguments
    /// * `model_path` - Path to MLX model directory
    /// * `vocab_size` - Vocabulary size for output logits buffer
    pub fn new(model_path: PathBuf, vocab_size: usize) -> Result<Self> {
        Self::with_full_config(model_path, vocab_size, MlxBridgeConfig::default(), None)
    }

    /// Create bridge with custom Python path (legacy compatibility)
    ///
    /// # Arguments
    /// * `model_path` - Path to MLX model directory
    /// * `vocab_size` - Vocabulary size for output logits buffer
    /// * `python_path` - Custom Python executable path
    /// * `manifest_hash` - Optional manifest hash for attestation
    pub fn with_config(
        model_path: PathBuf,
        vocab_size: usize,
        python_path: Option<String>,
        manifest_hash: Option<B3Hash>,
    ) -> Result<Self> {
        let mut config = MlxBridgeConfig::default();
        if let Some(path) = python_path {
            config.python_path = path;
        }
        Self::with_full_config(model_path, vocab_size, config, manifest_hash)
    }

    /// Create bridge with full configuration
    ///
    /// # Arguments
    /// * `model_path` - Path to MLX model directory
    /// * `vocab_size` - Vocabulary size for output logits buffer
    /// * `config` - Bridge configuration
    /// * `manifest_hash` - Optional manifest hash for attestation
    pub fn with_full_config(
        model_path: PathBuf,
        vocab_size: usize,
        config: MlxBridgeConfig,
        manifest_hash: Option<B3Hash>,
    ) -> Result<Self> {
        // Validate model path
        if !model_path.exists() {
            return Err(AosError::Config(format!(
                "Model path does not exist: {}",
                model_path.display()
            )));
        }

        // Locate bridge script
        let script_path = Self::find_bridge_script()?;

        info!(
            model_path = %model_path.display(),
            python = %config.python_path,
            max_restarts = config.max_restarts,
            "Creating MLX subprocess bridge"
        );

        let bridge = Self {
            model_path,
            config,
            script_path,
            process: Arc::new(Mutex::new(None)),
            device: "MLX Subprocess (Python mlx-lm)".to_string(),
            manifest_hash,
            vocab_size,
            restart_count: Arc::new(Mutex::new(0)),
            last_health_check: Arc::new(Mutex::new(None)),
            streaming_supported: Arc::new(Mutex::new(false)),
            bridge_protocol_version: Arc::new(Mutex::new(1)),
            context_buffer: Arc::new(Mutex::new(Vec::new())),
            is_moe: Arc::new(Mutex::new(false)),
            num_experts: Arc::new(Mutex::new(0)),
            experts_per_token: Arc::new(Mutex::new(0)),
            collect_routing: Arc::new(Mutex::new(false)),
            current_adapter_id: Arc::new(Mutex::new(None)),
        };

        Ok(bridge)
    }

    /// Get the current configuration
    pub fn config(&self) -> &MlxBridgeConfig {
        &self.config
    }

    /// Check if the bridge supports streaming
    pub fn supports_streaming(&self) -> bool {
        *self.streaming_supported.lock().unwrap()
    }

    /// Get the restart count
    pub fn restart_count(&self) -> usize {
        *self.restart_count.lock().unwrap()
    }

    /// Get the protocol version of the bridge
    pub fn protocol_version(&self) -> u32 {
        *self.bridge_protocol_version.lock().unwrap()
    }

    /// Check if the model is MoE (Mixture of Experts)
    pub fn is_moe(&self) -> bool {
        *self.is_moe.lock().unwrap()
    }

    /// Get the number of experts (MoE models)
    pub fn num_experts(&self) -> usize {
        *self.num_experts.lock().unwrap()
    }

    /// Get the number of active experts per token (MoE models)
    pub fn experts_per_token(&self) -> usize {
        *self.experts_per_token.lock().unwrap()
    }

    /// Get MoE info as a struct
    pub fn moe_info(&self) -> Option<MoEInfo> {
        if self.is_moe() {
            Some(MoEInfo {
                is_moe: true,
                num_experts: self.num_experts(),
                experts_per_token: self.experts_per_token(),
            })
        } else {
            None
        }
    }

    /// Set whether to collect expert routing data
    pub fn set_collect_routing(&self, collect: bool) {
        *self.collect_routing.lock().unwrap() = collect;
    }

    /// Check if routing collection is enabled
    pub fn is_collecting_routing(&self) -> bool {
        *self.collect_routing.lock().unwrap()
    }

    /// Set the current adapter ID (for free token lookup)
    pub fn set_adapter_id(&self, adapter_id: Option<String>) {
        *self.current_adapter_id.lock().unwrap() = adapter_id;
    }

    /// Get the current adapter ID
    pub fn adapter_id(&self) -> Option<String> {
        self.current_adapter_id.lock().unwrap().clone()
    }

    /// Find the bridge script in the project
    fn find_bridge_script() -> Result<PathBuf> {
        // First check environment variable
        if let Ok(script_path) = std::env::var("MLX_BRIDGE_SCRIPT_PATH") {
            let path = PathBuf::from(&script_path);
            if path.exists() {
                info!(script_path = %path.display(), "Using bridge script from MLX_BRIDGE_SCRIPT_PATH");
                return Ok(path);
            }
            warn!(
                script_path = %script_path,
                "MLX_BRIDGE_SCRIPT_PATH set but file not found, trying other locations"
            );
        }

        // Try relative to current executable
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                let script_path = exe_dir.join("scripts/mlx_bridge_server.py");
                if script_path.exists() {
                    info!(script_path = %script_path.display(), "Found bridge script relative to executable");
                    return Ok(script_path);
                }
            }
        }

        // Try relative paths from working directory
        let candidates = vec![
            PathBuf::from("scripts/mlx_bridge_server.py"),
            PathBuf::from("../scripts/mlx_bridge_server.py"),
            PathBuf::from("../../scripts/mlx_bridge_server.py"),
        ];

        for path in candidates {
            if path.exists() {
                info!(script_path = %path.display(), "Found bridge script");
                return Ok(path);
            }
        }

        Err(AosError::Config(
            "Could not find mlx_bridge_server.py script. \
             Set MLX_BRIDGE_SCRIPT_PATH environment variable or ensure script is in scripts/ directory."
                .to_string(),
        ))
    }

    /// Start the Python subprocess
    fn start_process(&self) -> Result<BridgeProcess> {
        info!(
            python = %self.config.python_path,
            model_path = %self.model_path.display(),
            "Starting MLX bridge subprocess"
        );

        let mut child = Command::new(&self.config.python_path)
            .arg(&self.script_path)
            .env("MLX_MODEL_PATH", &self.model_path)
            .env("PYTHONUNBUFFERED", "1") // Force unbuffered I/O
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit()) // Pass stderr through for logging
            .spawn()
            .map_err(|e| {
                AosError::Kernel(format!(
                    "Failed to spawn Python process '{}': {}",
                    self.config.python_path, e
                ))
            })?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| AosError::Kernel("Failed to capture subprocess stdin".to_string()))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| AosError::Kernel("Failed to capture subprocess stdout".to_string()))?;

        let mut process = BridgeProcess {
            child,
            stdin,
            stdout: BufReader::new(stdout),
        };

        // Wait for ready message
        let response = Self::read_response(&mut process.stdout)?;
        match response {
            BridgeResponse::Ready {
                model_path,
                model_type,
                protocol_version,
                streaming_supported,
                is_moe,
                num_experts,
                experts_per_token,
            } => {
                info!(
                    model_path = %model_path,
                    model_type = %model_type,
                    protocol_version = protocol_version,
                    streaming_supported = streaming_supported,
                    is_moe = is_moe,
                    num_experts = num_experts,
                    experts_per_token = experts_per_token,
                    "MLX bridge ready"
                );

                // Update bridge capabilities
                *self.streaming_supported.lock().unwrap() = streaming_supported;
                *self.bridge_protocol_version.lock().unwrap() = protocol_version;

                // Update MoE info
                *self.is_moe.lock().unwrap() = is_moe;
                *self.num_experts.lock().unwrap() = num_experts;
                *self.experts_per_token.lock().unwrap() = experts_per_token;

                // Enable routing collection for MoE models
                if is_moe && protocol_version >= 3 {
                    *self.collect_routing.lock().unwrap() = true;
                    info!("Expert routing collection enabled for MoE model");
                }
            }
            BridgeResponse::Error { error, error_type } => {
                return Err(AosError::Kernel(format!(
                    "Bridge initialization failed ({}): {}",
                    error_type, error
                )));
            }
            _ => {
                return Err(AosError::Kernel(format!(
                    "Unexpected response during initialization: {:?}",
                    response
                )));
            }
        }

        Ok(process)
    }

    /// Ensure process is running, restart if needed
    fn ensure_running(&self) -> Result<()> {
        let mut process_guard = self.process.lock().unwrap();

        // Check if process exists and is alive
        let needs_start = match process_guard.as_mut() {
            None => true,
            Some(proc) => {
                // Check if process has exited
                match proc.child.try_wait() {
                    Ok(Some(status)) => {
                        warn!(status = ?status, "Bridge process exited unexpectedly");
                        true
                    }
                    Ok(None) => false, // Still running
                    Err(e) => {
                        warn!(error = %e, "Failed to check process status");
                        true
                    }
                }
            }
        };

        if needs_start {
            // Check restart limit
            let current_count = *self.restart_count.lock().unwrap();
            if current_count >= self.config.max_restarts {
                error!(
                    restart_count = current_count,
                    max_restarts = self.config.max_restarts,
                    "Bridge process exceeded maximum restart attempts"
                );
                return Err(AosError::Kernel(format!(
                    "Bridge process failed after {} restart attempts",
                    current_count
                )));
            }

            info!(
                attempt = current_count + 1,
                max_attempts = self.config.max_restarts,
                "Starting/restarting bridge process"
            );

            let new_process = self.start_process()?;
            *process_guard = Some(new_process);

            let mut count = self.restart_count.lock().unwrap();
            *count += 1;
            if *count > 1 {
                warn!(restart_count = *count, "Bridge process restarted");
            }
        }

        Ok(())
    }

    /// Reset the restart counter (call after successful operations)
    fn reset_restart_count(&self) {
        let mut count = self.restart_count.lock().unwrap();
        if *count > 0 {
            debug!(
                previous_count = *count,
                "Resetting restart counter after success"
            );
            *count = 0;
        }
    }

    /// Send a request to the bridge
    fn send_request(&self, request: &BridgeRequest) -> Result<()> {
        let mut process_guard = self.process.lock().unwrap();
        let process = process_guard
            .as_mut()
            .ok_or_else(|| AosError::Kernel("Bridge process not started".to_string()))?;

        let json = serde_json::to_string(request)
            .map_err(|e| AosError::Kernel(format!("Failed to serialize request: {}", e)))?;

        debug!(request = %json, "Sending bridge request");

        writeln!(process.stdin, "{}", json)
            .map_err(|e| AosError::Kernel(format!("Failed to write to subprocess: {}", e)))?;

        process
            .stdin
            .flush()
            .map_err(|e| AosError::Kernel(format!("Failed to flush subprocess stdin: {}", e)))?;

        Ok(())
    }

    /// Read a response from the bridge
    fn read_response(stdout: &mut BufReader<ChildStdout>) -> Result<BridgeResponse> {
        let mut line = String::new();
        stdout
            .read_line(&mut line)
            .map_err(|e| AosError::Kernel(format!("Failed to read from subprocess: {}", e)))?;

        if line.is_empty() {
            return Err(AosError::Kernel(
                "Subprocess closed stdout (process died?)".to_string(),
            ));
        }

        debug!(response = %line.trim(), "Received bridge response");

        serde_json::from_str(&line)
            .map_err(|e| AosError::Kernel(format!("Failed to parse response JSON: {}", e)))
    }

    /// Perform a health check on the bridge subprocess
    pub fn check_bridge_health(&self) -> Result<bool> {
        self.ensure_running()?;

        self.send_request(&BridgeRequest::HealthCheck)?;

        let mut process_guard = self.process.lock().unwrap();
        let process = process_guard
            .as_mut()
            .ok_or_else(|| AosError::Kernel("Bridge process not started".to_string()))?;

        let response = Self::read_response(&mut process.stdout)?;

        match response {
            BridgeResponse::HealthResponse {
                status,
                model_loaded,
            } => {
                let healthy = status == "healthy" && model_loaded;
                *self.last_health_check.lock().unwrap() = Some(Instant::now());
                Ok(healthy)
            }
            BridgeResponse::Error { error, error_type } => Err(AosError::Kernel(format!(
                "Health check failed ({}): {}",
                error_type, error
            ))),
            _ => Err(AosError::Kernel(format!(
                "Unexpected response to health check: {:?}",
                response
            ))),
        }
    }

    /// Pre-warm specific experts in the model
    pub fn prewarm_experts(&self, experts: Vec<(usize, u8)>) -> Result<usize> {
        if experts.is_empty() {
            return Ok(0);
        }

        self.ensure_running()?;

        self.send_request(&BridgeRequest::Prewarm { experts })?;

        let mut process_guard = self.process.lock().unwrap();
        let process = process_guard
            .as_mut()
            .ok_or_else(|| AosError::Kernel("Bridge process not started".to_string()))?;

        let response = Self::read_response(&mut process.stdout)?;

        match response {
            BridgeResponse::PrewarmResponse {
                status,
                experts_loaded,
            } => {
                if status == "success" {
                    Ok(experts_loaded)
                } else {
                    Err(AosError::Kernel("Pre-warm failed".to_string()))
                }
            }
            BridgeResponse::Error { error, error_type } => Err(AosError::Kernel(format!(
                "Pre-warm failed ({}): {}",
                error_type, error
            ))),
            _ => Err(AosError::Kernel(format!(
                "Unexpected response to pre-warm: {:?}",
                response
            ))),
        }
    }

    /// Generate text using the bridge (simple interface)
    pub fn generate(
        &self,
        prompt: &str,
        max_tokens: usize,
        temperature: f32,
        top_p: f32,
    ) -> Result<String> {
        let result = self.generate_text(prompt, max_tokens, temperature, top_p, &[])?;
        Ok(result.text)
    }

    /// Generate text with full result including usage and timing stats
    pub fn generate_text(
        &self,
        prompt: &str,
        max_tokens: usize,
        temperature: f32,
        top_p: f32,
        stop_sequences: &[String],
    ) -> Result<GenerationResult> {
        self.ensure_running()?;

        let request = BridgeRequest::Generate {
            prompt: prompt.to_string(),
            max_tokens,
            temperature,
            top_p,
            stop_sequences: stop_sequences.to_vec(),
            stream: false,
            protocol_version: PROTOCOL_VERSION,
            collect_routing: false, // Non-streaming doesn't collect routing
        };

        self.send_request(&request)?;

        let mut process_guard = self.process.lock().unwrap();
        let process = process_guard
            .as_mut()
            .ok_or_else(|| AosError::Kernel("Bridge process not started".to_string()))?;

        let response = Self::read_response(&mut process.stdout)?;
        drop(process_guard);

        // Reset restart count on successful operation
        self.reset_restart_count();

        match response {
            BridgeResponse::GenerateResponse {
                text,
                tokens,
                finish_reason,
                usage,
                timing,
                moe_info,
                expert_routing,
            } => {
                debug!(tokens = tokens, finish_reason = %finish_reason, "Generation completed");
                Ok(GenerationResult {
                    text,
                    token_count: tokens,
                    finish_reason,
                    usage,
                    timing,
                    moe_info,
                    expert_routing,
                })
            }
            BridgeResponse::Error { error, error_type } => Err(AosError::Kernel(format!(
                "Generation failed ({}): {}",
                error_type, error
            ))),
            _ => Err(AosError::Kernel(format!(
                "Unexpected response to generate: {:?}",
                response
            ))),
        }
    }

    /// Generate text with streaming, calling the callback for each token
    ///
    /// # Arguments
    /// * `prompt` - The input prompt
    /// * `max_tokens` - Maximum tokens to generate
    /// * `temperature` - Sampling temperature
    /// * `top_p` - Top-p (nucleus) sampling parameter
    /// * `on_token` - Callback for each token; return false to stop generation
    ///
    /// # Returns
    /// The final streaming result with full text and statistics
    pub fn generate_stream<F>(
        &self,
        prompt: &str,
        max_tokens: usize,
        temperature: f32,
        top_p: f32,
        mut on_token: F,
    ) -> Result<StreamingResult>
    where
        F: FnMut(StreamingToken) -> bool,
    {
        self.ensure_running()?;

        // Check if we should collect routing data
        let collect_routing = self.is_collecting_routing();

        let request = BridgeRequest::Generate {
            prompt: prompt.to_string(),
            max_tokens,
            temperature,
            top_p,
            stop_sequences: vec![],
            stream: true,
            protocol_version: PROTOCOL_VERSION,
            collect_routing,
        };

        self.send_request(&request)?;

        let mut process_guard = self.process.lock().unwrap();
        let process = process_guard
            .as_mut()
            .ok_or_else(|| AosError::Kernel("Bridge process not started".to_string()))?;

        let mut accumulated_text = String::new();
        let mut token_count = 0;
        let mut collected_routing: SequenceExpertRouting = Vec::new();

        // Initialize routing hash chain for MoE models
        // Use num_experts as a proxy for layer count (typical MoE has routing per layer)
        let num_layers = *self.num_experts.lock().unwrap();
        let mut routing_chain = if collect_routing && num_layers > 0 {
            Some(RoutingHashChain::new(num_layers))
        } else {
            None
        };

        // Read streaming responses until we get StreamEnd
        loop {
            let response = Self::read_response(&mut process.stdout)?;

            match response {
                BridgeResponse::StreamToken {
                    text,
                    index,
                    token_id,
                    expert_routing,
                } => {
                    accumulated_text.push_str(&text);
                    token_count = index + 1;

                    // Collect routing data and update hash chain if present
                    if let Some(ref r) = expert_routing {
                        collected_routing.push(r.clone());

                        // Update hash chain with this token's routing
                        if let Some(ref mut chain) = routing_chain {
                            chain.record_token(index as u32, r);
                        }
                    }

                    let streaming_token = StreamingToken {
                        text,
                        index,
                        token_id,
                        expert_routing,
                        is_free: false,
                    };

                    // Call callback, stop if it returns false
                    if !on_token(streaming_token) {
                        debug!(index = index, "Streaming stopped by callback");
                        break;
                    }
                }
                BridgeResponse::StreamEnd {
                    tokens,
                    finish_reason,
                    text,
                    usage,
                    timing,
                    moe_info,
                    expert_routing,
                } => {
                    // Use the full text from StreamEnd if available
                    let final_text = if text.is_empty() {
                        accumulated_text
                    } else {
                        text
                    };

                    // Use expert_routing from response if available, otherwise use collected
                    let final_routing = expert_routing.or_else(|| {
                        if collected_routing.is_empty() {
                            None
                        } else {
                            Some(collected_routing)
                        }
                    });

                    // Extract routing hash and summary from chain
                    let routing_hash = match routing_chain {
                        Some(chain) if chain.token_count() > 0 => {
                            let summary = chain.summary();
                            debug!(
                                routing_hash = %summary.head_hash,
                                token_count = summary.token_count,
                                stability = summary.routing_stability,
                                "Routing hash chain finalized"
                            );
                            Some(chain.head().clone())
                        }
                        _ => None,
                    };

                    drop(process_guard);
                    self.reset_restart_count();

                    return Ok(StreamingResult {
                        text: final_text,
                        tokens_generated: tokens,
                        finish_reason,
                        usage_stats: usage.map(Into::into),
                        timing_stats: timing.map(Into::into),
                        moe_info,
                        expert_routing: final_routing,
                        free_tokens_delivered: 0,
                        routing_hash,
                    });
                }
                BridgeResponse::Error { error, error_type } => {
                    return Err(AosError::Kernel(format!(
                        "Streaming generation failed ({}): {}",
                        error_type, error
                    )));
                }
                _ => {
                    warn!(response = ?response, "Unexpected response during streaming");
                }
            }
        }

        // If we got here, streaming was stopped by callback
        let final_routing = if collected_routing.is_empty() {
            None
        } else {
            Some(collected_routing.clone())
        };

        // Extract routing hash and summary from chain (early termination case)
        let routing_hash = match routing_chain {
            Some(chain) if chain.token_count() > 0 => {
                let summary = chain.summary();
                debug!(
                    routing_hash = %summary.head_hash,
                    token_count = summary.token_count,
                    "Routing hash chain finalized (early termination)"
                );
                Some(chain.head().clone())
            }
            _ => None,
        };

        Ok(StreamingResult {
            text: accumulated_text,
            tokens_generated: token_count,
            finish_reason: "stopped".to_string(),
            usage_stats: None,
            timing_stats: None,
            moe_info: self.moe_info(),
            expert_routing: final_routing,
            free_tokens_delivered: 0,
            routing_hash,
        })
    }

    /// Generate text with streaming, returning an iterator over tokens
    ///
    /// This provides a pull-based streaming interface. Each call to `next()`
    /// on the returned iterator will block until the next token is available.
    ///
    /// # Example
    /// ```ignore
    /// let iter = bridge.generate_stream_iter("def hello():", 50, 0.7, 0.9)?;
    /// for result in iter {
    ///     match result {
    ///         Ok(StreamingEvent::Token(token)) => print!("{}", token.text),
    ///         Ok(StreamingEvent::Done(result)) => println!("\nDone: {} tokens", result.tokens_generated),
    ///         Err(e) => eprintln!("Error: {}", e),
    ///     }
    /// }
    /// ```
    pub fn generate_stream_iter(
        &self,
        prompt: &str,
        max_tokens: usize,
        temperature: f32,
        top_p: f32,
    ) -> Result<StreamingIterator> {
        self.ensure_running()?;

        let collect_routing = self.is_collecting_routing();

        let request = BridgeRequest::Generate {
            prompt: prompt.to_string(),
            max_tokens,
            temperature,
            top_p,
            stop_sequences: vec![],
            stream: true,
            protocol_version: PROTOCOL_VERSION,
            collect_routing,
        };

        self.send_request(&request)?;

        Ok(StreamingIterator {
            process: self.process.clone(),
            done: false,
            accumulated_text: String::new(),
            token_count: 0,
        })
    }

    /// Shutdown the bridge process
    pub fn shutdown(&self) -> Result<()> {
        let mut process_guard = self.process.lock().unwrap();
        if let Some(mut process) = process_guard.take() {
            info!("Shutting down bridge process");

            // Send shutdown request (ignore errors if process already dead)
            let request = BridgeRequest::Shutdown;
            let json = serde_json::to_string(&request).ok();
            if let Some(json) = json {
                let _ = writeln!(process.stdin, "{}", json);
                let _ = process.stdin.flush();
            }

            // Wait for process to exit (with timeout)
            let start = Instant::now();
            loop {
                match process.child.try_wait() {
                    Ok(Some(status)) => {
                        info!(status = ?status, "Bridge process exited");
                        break;
                    }
                    Ok(None) => {
                        if start.elapsed() > Duration::from_secs(5) {
                            warn!("Bridge process did not exit gracefully, killing");
                            let _ = process.child.kill();
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(100));
                    }
                    Err(e) => {
                        warn!(error = %e, "Error waiting for process");
                        let _ = process.child.kill();
                        break;
                    }
                }
            }
        }

        Ok(())
    }
}

impl Drop for MLXSubprocessBridge {
    fn drop(&mut self) {
        // Best-effort cleanup
        let _ = self.shutdown();
    }
}

impl FusedKernels for MLXSubprocessBridge {
    fn load(&mut self, _plan_bytes: &[u8]) -> Result<()> {
        // Start the subprocess and wait for ready
        info!("Initializing MLX subprocess bridge");
        self.ensure_running()?;

        // Clear context buffer for fresh start
        self.context_buffer.lock().unwrap().clear();

        info!("MLX subprocess bridge ready");
        Ok(())
    }

    fn run_step(&mut self, _ring: &RouterRing, _io: &mut IoBuffers) -> Result<()> {
        // The MLX Bridge is designed for bulk text generation, not single-token inference.
        //
        // The FusedKernels::run_step() interface expects logits output, but the Python
        // mlx-lm library only exposes text generation, not raw logits. To properly
        // implement this, we would need to:
        //
        // 1. Add a "generate_with_logprobs" endpoint to the Python bridge
        // 2. Return token IDs and log probabilities for each generated token
        // 3. Convert logprobs back to logits for the IoBuffers
        //
        // For now, use generate_text() or generate_stream() directly for text generation.
        // If you need FusedKernels compatibility, use the MLX FFI backend instead.
        Err(AosError::Kernel(
            "MLX Bridge does not support run_step() for single-token inference. \
             Use generate_text() or generate_stream() for text generation, \
             or use the MLX FFI backend (BackendKind::Mlx) for FusedKernels compatibility."
                .to_string(),
        ))
    }

    fn device_name(&self) -> &str {
        &self.device
    }

    fn attest_determinism(&self) -> Result<DeterminismReport> {
        // MLX subprocess bridge has limited determinism guarantees
        // The Python subprocess uses system entropy for RNG and
        // floating point operations may vary
        Ok(DeterminismReport {
            backend_type: BackendType::MLX,
            metallib_hash: None, // No metallib for Python subprocess
            metallib_verified: false,
            manifest: None, // No kernel manifest for Python subprocess
            rng_seed_method: RngSeedingMethod::SystemEntropy,
            floating_point_mode: FloatingPointMode::FastMath,
            determinism_level: DeterminismLevel::None,
            compiler_flags: vec![],
            deterministic: false, // Python subprocess has weaker determinism guarantees
            runtime_version: None,
            device_id: None,
        })
    }

    fn health_check(&self) -> Result<adapteros_lora_kernel_api::BackendHealth> {
        match self.check_bridge_health() {
            Ok(true) => Ok(adapteros_lora_kernel_api::BackendHealth::Healthy),
            Ok(false) => Ok(adapteros_lora_kernel_api::BackendHealth::Degraded {
                reason: "Model not fully loaded".to_string(),
            }),
            Err(e) => Ok(adapteros_lora_kernel_api::BackendHealth::Failed {
                reason: e.to_string(),
                recoverable: true, // Bridge can be restarted
            }),
        }
    }

    #[allow(deprecated)]
    fn supports_text_generation(&self) -> bool {
        true
    }

    #[allow(deprecated)]
    fn generate_text_full(
        &self,
        prompt: &str,
        max_tokens: usize,
        temperature: f32,
        top_p: f32,
    ) -> Result<TextGenerationResult> {
        self.generate_text_complete(prompt, max_tokens, temperature, top_p)
    }

    fn supports_streaming_text_generation(&self) -> bool {
        true
    }

    fn generate_text_complete(
        &self,
        prompt: &str,
        max_tokens: usize,
        temperature: f32,
        top_p: f32,
    ) -> Result<TextGenerationResult> {
        // Use the existing generate_text method
        let result = self.generate_text(prompt, max_tokens, temperature, top_p, &[])?;

        // Compute routing hash if routing data is available
        let routing_hash = result.expert_routing.as_ref().map(|routing| {
            let mut hasher = blake3::Hasher::new();
            hasher.update(b"aos.routing.v1");
            for token_routing in routing {
                for (layer, expert) in token_routing {
                    hasher.update(&layer.to_le_bytes());
                    hasher.update(&expert.to_le_bytes());
                }
            }
            B3Hash::from_bytes(*hasher.finalize().as_bytes())
        });

        // Convert to TextGenerationResult
        Ok(TextGenerationResult {
            text: result.text,
            tokens_generated: result.token_count,
            finish_reason: result.finish_reason,
            usage_stats: result.usage.map(Into::into),
            timing_stats: result.timing.map(Into::into),
            moe_info: result.moe_info,
            expert_routing: result.expert_routing,
            free_tokens_delivered: 0,
            routing_hash,
        })
    }

    fn generate_text_stream(
        &self,
        prompt: &str,
        max_tokens: usize,
        temperature: f32,
        top_p: f32,
        on_token: &mut dyn FnMut(TextToken) -> bool,
    ) -> Result<TextGenerationResult> {
        self.generate_stream(prompt, max_tokens, temperature, top_p, |token| {
            on_token(token)
        })
    }

    fn prewarm_experts(&self, experts: Vec<(usize, u8)>) -> Result<usize> {
        self.prewarm_experts(experts)
    }

    fn is_moe(&self) -> bool {
        *self.is_moe.lock().unwrap()
    }

    fn num_experts(&self) -> usize {
        *self.num_experts.lock().unwrap()
    }

    fn experts_per_token(&self) -> usize {
        *self.experts_per_token.lock().unwrap()
    }
}

// ============================================================================
// TextGenerationKernel implementation for MLX Bridge (legacy compatibility)
// ============================================================================

impl TextGenerationKernel for MLXSubprocessBridge {
    #[allow(deprecated)]
    fn supports_text_generation(&self) -> bool {
        // Delegate to FusedKernels implementation
        FusedKernels::supports_text_generation(self)
    }

    #[allow(deprecated)]
    fn generate_text_full(
        &self,
        prompt: &str,
        max_tokens: usize,
        temperature: f32,
        top_p: f32,
    ) -> Result<TextGenerationResult> {
        // Delegate to FusedKernels implementation
        FusedKernels::generate_text_full(self, prompt, max_tokens, temperature, top_p)
    }

    fn text_generation_backend_name(&self) -> &str {
        &self.device
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = MlxBridgeConfig::default();
        assert_eq!(config.python_path, "python3");
        assert_eq!(config.max_restarts, DEFAULT_MAX_RESTARTS);
        assert_eq!(config.startup_timeout_secs, 120);
        assert_eq!(config.request_timeout_secs, 300);
        assert!((config.default_temperature - 0.7).abs() < 0.01);
        assert!((config.default_top_p - 0.9).abs() < 0.01);
    }

    #[test]
    fn test_config_custom() {
        let config = MlxBridgeConfig {
            python_path: "/usr/local/bin/python3".to_string(),
            max_restarts: 5,
            startup_timeout_secs: 60,
            request_timeout_secs: 180,
            health_check_interval_secs: 15,
            default_temperature: 0.5,
            default_top_p: 0.95,
        };
        assert_eq!(config.python_path, "/usr/local/bin/python3");
        assert_eq!(config.max_restarts, 5);
    }

    #[test]
    fn test_bridge_creation_invalid_path() {
        let model_path = PathBuf::from("/nonexistent/path/to/model");
        let result = MLXSubprocessBridge::new(model_path, 32000);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("does not exist"));
    }

    #[test]
    fn test_bridge_creation_valid_path() {
        // This test requires a valid model path
        let model_path = std::env::temp_dir().join("test_mlx_model");
        std::fs::create_dir_all(&model_path).ok();

        let result = MLXSubprocessBridge::new(model_path.clone(), 32000);

        // Cleanup
        std::fs::remove_dir_all(&model_path).ok();

        // We expect this to succeed in finding the script (or fail gracefully if script not found)
        assert!(
            result.is_ok()
                || result
                    .unwrap_err()
                    .to_string()
                    .contains("mlx_bridge_server.py")
        );
    }

    #[test]
    fn test_bridge_with_custom_config() {
        let model_path = std::env::temp_dir().join("test_mlx_config");
        std::fs::create_dir_all(&model_path).ok();

        let config = MlxBridgeConfig {
            python_path: "python3".to_string(),
            max_restarts: 2,
            ..Default::default()
        };

        let result = MLXSubprocessBridge::with_full_config(model_path.clone(), 32000, config, None);

        // Cleanup
        std::fs::remove_dir_all(&model_path).ok();

        if let Ok(bridge) = result {
            assert_eq!(bridge.config().max_restarts, 2);
            assert_eq!(bridge.vocab_size, 32000);
        }
    }

    #[test]
    fn test_bridge_request_serialization() {
        let request = BridgeRequest::Generate {
            prompt: "Hello, world!".to_string(),
            max_tokens: 100,
            temperature: 0.7,
            top_p: 0.9,
            stop_sequences: vec!["STOP".to_string()],
            stream: false,
            protocol_version: PROTOCOL_VERSION,
            collect_routing: true,
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"type\":\"generate\""));
        assert!(json.contains("\"prompt\":\"Hello, world!\""));
        assert!(json.contains("\"max_tokens\":100"));
        assert!(json.contains("\"collect_routing\":true"));
    }

    #[test]
    fn test_bridge_response_deserialization() {
        let json =
            r#"{"type":"generate_response","text":"Hello!","tokens":5,"finish_reason":"stop"}"#;
        let response: BridgeResponse = serde_json::from_str(json).unwrap();

        match response {
            BridgeResponse::GenerateResponse {
                text,
                tokens,
                finish_reason,
                ..
            } => {
                assert_eq!(text, "Hello!");
                assert_eq!(tokens, 5);
                assert_eq!(finish_reason, "stop");
            }
            _ => panic!("Expected GenerateResponse"),
        }
    }

    #[test]
    fn test_stream_token_response() {
        let json = r#"{"type":"stream_token","token":"hello","index":0,"token_id":1234}"#;
        let response: BridgeResponse = serde_json::from_str(json).unwrap();

        match response {
            BridgeResponse::StreamToken {
                text,
                index,
                token_id,
                expert_routing,
            } => {
                assert_eq!(text, "hello");
                assert_eq!(index, 0);
                assert_eq!(token_id, Some(1234));
                assert!(expert_routing.is_none());
            }
            _ => panic!("Expected StreamToken"),
        }
    }

    #[test]
    fn test_error_response() {
        let json =
            r#"{"type":"error","error":"Model not found","error_type":"initialization_error"}"#;
        let response: BridgeResponse = serde_json::from_str(json).unwrap();

        match response {
            BridgeResponse::Error { error, error_type } => {
                assert_eq!(error, "Model not found");
                assert_eq!(error_type, "initialization_error");
            }
            _ => panic!("Expected Error"),
        }
    }

    #[test]
    fn test_generation_result() {
        let result = GenerationResult {
            text: "Generated text".to_string(),
            token_count: 10,
            finish_reason: "stop".to_string(),
            usage: Some(UsageStats {
                prompt_tokens: 5,
                completion_tokens: 10,
                total_tokens: 15,
            }),
            timing: Some(TimingStats {
                ttft_ms: 50.0,
                total_ms: 200.0,
                tokens_per_second: 50.0,
            }),
            moe_info: None,
            expert_routing: None,
        };

        assert_eq!(result.text, "Generated text");
        assert_eq!(result.token_count, 10);
        assert!(result.usage.is_some());
        assert!(result.timing.is_some());
    }

    #[test]
    fn test_streaming_token() {
        let token = StreamingToken {
            text: "world".to_string(),
            index: 1,
            token_id: Some(5678),
            expert_routing: Some(vec![(0, 5), (1, 10)]),
            is_free: false,
        };

        assert_eq!(token.text, "world");
        assert!(token.expert_routing.is_some());
        assert_eq!(token.index, 1);
        assert_eq!(token.token_id, Some(5678));
    }

    #[test]
    fn test_routing_hash_chain() {
        let mut chain = RoutingHashChain::new(4);

        // Record some token routing
        chain.record_token(0, &[(0, 5), (1, 10)]);
        chain.record_token(1, &[(0, 3), (1, 8)]);
        chain.record_token(2, &[(0, 5), (1, 10)]); // Same as token 0

        assert_eq!(chain.token_count(), 3);
        assert!(!chain.head().as_bytes().iter().all(|&b| b == 0));

        // Verify heat map recorded activations
        let heat_map = chain.heat_map();
        assert_eq!(heat_map.sample_count, 3);

        // Check summary
        let summary = chain.summary();
        assert_eq!(summary.token_count, 3);
        assert!(!summary.head_hash.is_empty());
    }

    #[test]
    fn test_routing_hash_chain_determinism() {
        // Same routing should produce same hash
        let mut chain1 = RoutingHashChain::new(4);
        let mut chain2 = RoutingHashChain::new(4);

        let routing = vec![(0, 5), (1, 10), (2, 3)];
        chain1.record_token(0, &routing);
        chain2.record_token(0, &routing);

        assert_eq!(chain1.head().as_bytes(), chain2.head().as_bytes());
    }

    #[test]
    fn test_routing_hash_chain_canonicalization() {
        // Different order should produce same hash after canonicalization
        let mut chain1 = RoutingHashChain::new(4);
        let mut chain2 = RoutingHashChain::new(4);

        // Same routing but different order
        chain1.record_token(0, &[(0, 5), (1, 10), (2, 3)]);
        chain2.record_token(0, &[(2, 3), (0, 5), (1, 10)]); // Reordered

        assert_eq!(chain1.head().as_bytes(), chain2.head().as_bytes());
    }

    #[test]
    fn test_routing_hash_chain_different_routing() {
        // Different routing should produce different hash
        let mut chain1 = RoutingHashChain::new(4);
        let mut chain2 = RoutingHashChain::new(4);

        chain1.record_token(0, &[(0, 5), (1, 10)]);
        chain2.record_token(0, &[(0, 6), (1, 10)]); // Different expert

        assert_ne!(chain1.head().as_bytes(), chain2.head().as_bytes());
    }

    #[test]
    fn test_stream_token_routing_wire_compat() {
        // Verify JSON wire format "routing" deserializes to expert_routing field
        let json = r#"{"type":"stream_token","token":"hi","index":0,"routing":[[0,5],[1,10]]}"#;
        let response: BridgeResponse = serde_json::from_str(json).unwrap();

        match response {
            BridgeResponse::StreamToken { expert_routing, .. } => {
                let routing = expert_routing.expect("routing should be present");
                assert_eq!(routing, vec![(0, 5), (1, 10)]);
            }
            _ => panic!("Expected StreamToken"),
        }
    }

    #[test]
    fn test_stream_end_expert_routing_wire_compat() {
        // Verify expert_routing field in StreamEnd response
        let json = r#"{"type":"stream_end","tokens":2,"finish_reason":"stop","expert_routing":[[[0,5],[1,10]],[[0,3],[1,8]]]}"#;
        let response: BridgeResponse = serde_json::from_str(json).unwrap();

        match response {
            BridgeResponse::StreamEnd { expert_routing, .. } => {
                let routing = expert_routing.expect("expert_routing should be present");
                assert_eq!(routing.len(), 2);
                assert_eq!(routing[0], vec![(0, 5), (1, 10)]);
                assert_eq!(routing[1], vec![(0, 3), (1, 8)]);
            }
            _ => panic!("Expected StreamEnd"),
        }
    }
}
