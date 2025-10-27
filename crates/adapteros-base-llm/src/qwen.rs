//! Qwen base LLM implementation
//!
//! Implements the BaseLLM trait for Qwen models with deterministic
//! execution guarantees following the patterns established in the codebase.

use super::{BaseLLM, BaseLLMMetadata, ModelState};
use adapteros_core::{AosError, B3Hash, Result};
use adapteros_deterministic_exec::DeterministicExecutor;
use adapteros_trace::{Event, LogicalTimestamp};
// use serde::{Deserialize, Serialize}; // Not used in current implementation
use parking_lot::RwLock;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Qwen base LLM implementation
pub struct QwenBaseLLM {
    metadata: BaseLLMMetadata,
    state: Arc<RwLock<QwenState>>,
    #[cfg(feature = "mlx")]
    py_model: Option<pyo3::PyObject>,
    #[cfg(feature = "mlx")]
    py_tokenizer: Option<pyo3::PyObject>,
    #[cfg(feature = "mlx")]
    py_generate: Option<pyo3::PyObject>,
    #[cfg(feature = "mlx")]
    mlx_active: bool,
}

/// Internal state for Qwen model
#[derive(Debug)]
struct QwenState {
    initialized: bool,
    executor: Option<DeterministicExecutor>,
    model_loaded: bool,
    current_sequence: Vec<u32>,
    checkpoint_counter: u64,
}

impl QwenBaseLLM {
    /// Create new Qwen base LLM
    pub fn new(metadata: BaseLLMMetadata) -> Result<Self> {
        let state = Arc::new(RwLock::new(QwenState {
            initialized: false,
            executor: None,
            model_loaded: false,
            current_sequence: Vec::new(),
            checkpoint_counter: 0,
        }));

        info!("Created Qwen base LLM: {}", metadata.model_id);

        Ok(Self {
            metadata,
            state,
            #[cfg(feature = "mlx")]
            py_model: None,
            #[cfg(feature = "mlx")]
            py_tokenizer: None,
            #[cfg(feature = "mlx")]
            py_generate: None,
            #[cfg(feature = "mlx")]
            mlx_active: false,
        })
    }

    /// Initialize the model with deterministic executor
    fn initialize_model(&mut self, executor: &mut DeterministicExecutor) -> Result<()> {
        let mut state = self.state.write();

        if state.initialized {
            warn!("Qwen model already initialized");
            return Ok(());
        }

        // Derive deterministic seed for this model
        let model_seed = executor.derive_seed(&format!("qwen_model:{}", self.metadata.model_id));

        info!(
            "Initializing Qwen model '{}' with seed: {:?}",
            self.metadata.model_id,
            &model_seed[..8]
        );

        // In a real implementation, this would:
        // 1. Load model weights from disk
        // 2. Initialize tokenizer
        // 3. Set up GPU/CPU compute resources
        // 4. Verify model integrity

        // Attempt MLX backend init if enabled
        #[cfg(feature = "mlx")]
        {
            use crate::mlx_backend::load_qwen_via_mlx;
            // Use env var for model reference; default to MLX community Qwen 4-bit build
            let model_ref = std::env::var("AOS_MLX_MODEL").unwrap_or_else(|_| "mlx-community/Qwen2.5-7B-Instruct-4bit".to_string());
            let seed64 = u64::from_le_bytes([
                model_seed[0], model_seed[1], model_seed[2], model_seed[3],
                model_seed[4], model_seed[5], model_seed[6], model_seed[7],
            ]);
            match load_qwen_via_mlx(&model_ref, seed64) {
                Ok((m, t, g)) => {
                    self.py_model = Some(m);
                    self.py_tokenizer = Some(t);
                    self.py_generate = Some(g);
                    self.mlx_active = true;
                    info!("MLX backend initialized for {} via {}", self.metadata.model_id, model_ref);
                }
                Err(err) => {
                    warn!("MLX backend initialization failed: {}. Falling back to mock forward.", err);
                    self.mlx_active = false;
                }
            }
        }

        state.initialized = true;
        state.model_loaded = true;
        // Note: DeterministicExecutor doesn't implement Clone
        // In a real implementation, we'd store a reference or handle this differently
        // For now, we'll just mark as initialized

        debug!(
            "Qwen model '{}' initialized successfully",
            self.metadata.model_id
        );
        Ok(())
    }

    /// Perform forward pass through the model
    fn perform_forward_pass(&self, input_ids: &[u32]) -> Result<Vec<f32>> {
        let state = self.state.read();

        if !state.initialized || !state.model_loaded {
            return Err(AosError::BaseLLM("Model not initialized".to_string()));
        }

        // If MLX backend is initialized, attempt to use generate() to produce next token
        #[cfg(feature = "mlx")]
        if self.mlx_active {
            if let (Some(ref model), Some(ref tokenizer), Some(ref generate)) =
                (&self.py_model, &self.py_tokenizer, &self.py_generate)
            {
                // Decode input_ids to a prompt string
                if let Ok(prompt) = crate::mlx_backend::decode_ids(tokenizer, input_ids) {
                    // Generate one token deterministically (temperature 0)
                    if let Ok(gen_text) = crate::mlx_backend::generate_text(
                        model,
                        tokenizer,
                        generate,
                        &prompt,
                        1,
                    ) {
                        // Encode combined text and extract next token id
                        if let Ok(encoded) = crate::mlx_backend::encode_ids(tokenizer, &gen_text) {
                            let output_size = self.metadata.vocab_size;
                            let mut output = vec![0.0; output_size];
                            let next_idx = input_ids.len();
                            if next_idx < encoded.len() {
                                let next_token = encoded[next_idx] as usize;
                                if next_token < output_size {
                                    output[next_token] = 1.0;
                                    debug!(
                                        "MLX forward pass (one-step) for '{}' (prompt_len={}, next_token={})",
                                        self.metadata.model_id,
                                        input_ids.len(),
                                        next_token
                                    );
                                    return Ok(output);
                                }
                            }
                        }
                    }
                }
            }
        }

        // Fallback deterministic vector
        let output_size = self.metadata.vocab_size;
        let mut output = vec![0.0; output_size];
        if !input_ids.is_empty() {
            let idx = (input_ids[0] as usize).min(output_size.saturating_sub(1));
            output[idx] = 1.0;
        }
        Ok(output)
    }

    /// Create checkpoint of current state
    fn create_checkpoint(&self) -> Result<ModelState> {
        let state = self.state.read();

        let checkpoint_data = serde_json::to_vec(&serde_json::json!({
            "sequence": state.current_sequence,
            "counter": state.checkpoint_counter,
            "initialized": state.initialized,
            "model_loaded": state.model_loaded,
        }))?;

        let checkpoint_hash = adapteros_core::B3Hash::hash(&checkpoint_data).to_string();

        Ok(ModelState {
            model_id: self.metadata.model_id.clone(),
            checkpoint_hash,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("System time before UNIX epoch")
                .as_nanos(),
            state_data: checkpoint_data,
        })
    }

    /// Restore state from checkpoint
    fn restore_from_checkpoint(&self, state: &ModelState) -> Result<()> {
        if state.model_id != self.metadata.model_id {
            return Err(AosError::BaseLLM(format!(
                "Checkpoint model ID mismatch: expected {}, got {}",
                self.metadata.model_id, state.model_id
            )));
        }

        let checkpoint_data: serde_json::Value = serde_json::from_slice(&state.state_data)?;

        let mut qwen_state = self.state.write();
        qwen_state.current_sequence = checkpoint_data["sequence"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .map(|v| v.as_u64().unwrap_or(0) as u32)
            .collect();
        qwen_state.checkpoint_counter = checkpoint_data["counter"].as_u64().unwrap_or(0);

        info!(
            "Restored Qwen model state from checkpoint: {}",
            state.checkpoint_hash
        );
        Ok(())
    }
}

impl BaseLLM for QwenBaseLLM {
    fn load(&mut self, executor: &mut DeterministicExecutor) -> Result<()> {
        self.initialize_model(executor)
    }

    fn forward(&mut self, input_ids: &[u32]) -> Result<Vec<f32>> {
        // Update current sequence
        {
            let mut state = self.state.write();
            state.current_sequence = input_ids.to_vec();
            state.checkpoint_counter += 1;
        }

        self.perform_forward_pass(input_ids)
    }

    fn metadata(&self) -> &BaseLLMMetadata {
        &self.metadata
    }

    fn get_state(&self) -> Result<ModelState> {
        self.create_checkpoint()
    }

    fn restore_state(&mut self, state: &ModelState) -> Result<()> {
        self.restore_from_checkpoint(state)
    }

    fn reset(&mut self) -> Result<()> {
        let mut state = self.state.write();
        state.current_sequence.clear();
        state.checkpoint_counter = 0;

        info!("Reset Qwen model '{}' state", self.metadata.model_id);
        Ok(())
    }

    fn create_trace_event(&self, operation: &str, input_hash: &str) -> Event {
        let state = self.state.read();

        use adapteros_core::B3Hash;
        use std::collections::HashMap;

        let mut inputs = HashMap::new();
        inputs.insert(
            "input_hash".to_string(),
            serde_json::Value::String(input_hash.to_string()),
        );
        inputs.insert(
            "sequence_length".to_string(),
            serde_json::Value::Number(state.current_sequence.len().into()),
        );

        let mut outputs = HashMap::new();
        outputs.insert(
            "model_id".to_string(),
            serde_json::Value::String(self.metadata.model_id.clone()),
        );
        outputs.insert(
            "model_hash".to_string(),
            serde_json::Value::String(self.metadata.model_hash.clone()),
        );
        outputs.insert(
            "operation".to_string(),
            serde_json::Value::String(operation.to_string()),
        );
        outputs.insert(
            "checkpoint_counter".to_string(),
            serde_json::Value::Number(state.checkpoint_counter.into()),
        );

        let metadata = adapteros_trace::EventMetadata {
            global_seed: B3Hash::hash(b"default"),
            plan_id: "default".to_string(),
            cpid: "default".to_string(),
            tenant_id: "default".to_string(),
            session_id: uuid::Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)).to_string(),
            adapter_ids: vec![self.metadata.model_id.clone()],
            memory_usage_mb: 0,
            gpu_utilization_pct: 0.0,
            custom: HashMap::new(),
        };

        let logical_timestamp = LogicalTimestamp::new(
            0,                                                      // global_tick
            0,                                                      // op_tick
            None,                                                   // token_position
            B3Hash::hash(format!("qwen_{}", operation).as_bytes()), // derivation_hash
        );

        Event::new(
            0,                             // tick_id
            format!("qwen_{}", operation), // op_id
            format!("qwen_{}", operation), // event_type
            inputs,
            outputs,
            metadata,
            logical_timestamp,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_deterministic_exec::ExecutorConfig;

    #[test]
    fn test_qwen_creation() {
        let metadata = BaseLLMMetadata::default();
        let qwen = QwenBaseLLM::new(metadata).unwrap();

        assert_eq!(qwen.metadata().model_id, "Qwen2.5-7B-Instruct");
        assert_eq!(qwen.metadata().vocab_size, 152064);
    }

    #[test]
    fn test_qwen_initialization() {
        let metadata = BaseLLMMetadata::default();
        let mut qwen = QwenBaseLLM::new(metadata).unwrap();

        let config = ExecutorConfig::default();
        let mut executor = DeterministicExecutor::new(config);

        qwen.load(&mut executor).unwrap();

        // Test forward pass
        let input_ids = vec![1, 2, 3, 4, 5];
        let output = qwen.forward(&input_ids).unwrap();

        assert_eq!(output.len(), 152064); // vocab_size
        assert!(output.iter().any(|&x| x > 0.0)); // Should have some non-zero values
    }

    #[test]
    fn test_qwen_state_management() {
        let metadata = BaseLLMMetadata::default();
        let mut qwen = QwenBaseLLM::new(metadata).unwrap();

        let config = ExecutorConfig::default();
        let mut executor = DeterministicExecutor::new(config);

        qwen.load(&mut executor).unwrap();

        // Perform some operations
        let input_ids = vec![1, 2, 3];
        qwen.forward(&input_ids).unwrap();

        // Create checkpoint
        let state = qwen.get_state().unwrap();
        assert_eq!(state.model_id, "Qwen2.5-7B-Instruct");
        assert!(!state.checkpoint_hash.is_empty());

        // Restore state
        qwen.restore_state(&state).unwrap();

        // Reset
        qwen.reset().unwrap();
    }

    #[test]
    fn test_qwen_trace_events() {
        let metadata = BaseLLMMetadata::default();
        let qwen = QwenBaseLLM::new(metadata).unwrap();

        let event = qwen.create_trace_event("forward", "test_hash");

        assert!(event.event_type.starts_with("qwen_"));
        assert_eq!(event.outputs["model_id"], "Qwen2.5-7B-Instruct");
        assert_eq!(event.outputs["operation"], "forward");
        assert_eq!(event.inputs["input_hash"], "test_hash");
    }
}
