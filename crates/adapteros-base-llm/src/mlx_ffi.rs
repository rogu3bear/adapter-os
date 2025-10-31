use crate::{BaseLLM, BaseLLMMetadata, ModelState};
use adapteros_core::{AosError, Result};
use adapteros_deterministic_exec::DeterministicExecutor;
use adapteros_lora_mlx_ffi::MLXFFIModel;
use adapteros_trace::Event;

/// Qwen backend implemented via MLX C++ FFI (no Python)
pub struct QwenMlxFfi {
    metadata: BaseLLMMetadata,
    model: Option<MLXFFIModel>,
    sequence: Vec<u32>,
    checkpoints: u64,
}

impl QwenMlxFfi {
    pub fn new(metadata: BaseLLMMetadata) -> Self {
        Self {
            metadata,
            model: None,
            sequence: Vec::new(),
            checkpoints: 0,
        }
    }
}

impl BaseLLM for QwenMlxFfi {
    fn load(&mut self, _executor: &mut DeterministicExecutor) -> Result<()> {
        // Expect model_path to be encoded in model_id or env var; prefer env var for clarity.
        // If not provided, error explicitly.
        let model_path = std::env::var("AOS_MLX_FFI_MODEL").ok().ok_or_else(|| {
            AosError::Config("AOS_MLX_FFI_MODEL not set (path to MLX model directory)".to_string())
        })?;

        let model = MLXFFIModel::load(&model_path)?;
        self.model = Some(model);
        Ok(())
    }

    fn forward(&mut self, input_ids: &[u32]) -> Result<Vec<f32>> {
        let model = self
            .model
            .as_ref()
            .ok_or_else(|| AosError::BaseLLM("Model not loaded".to_string()))?;
        self.sequence = input_ids.to_vec();
        self.checkpoints = self.checkpoints.wrapping_add(1);

        // Position is current sequence length - 1 (last token index)
        let pos = self.sequence.len().saturating_sub(1);
        let logits = model.forward(&self.sequence, pos)?;
        Ok(logits)
    }

    fn metadata(&self) -> &BaseLLMMetadata {
        &self.metadata
    }

    fn get_state(&self) -> Result<ModelState> {
        let state = serde_json::to_vec(&serde_json::json!({
            "sequence": self.sequence,
            "checkpoints": self.checkpoints,
        }))?;
        let checkpoint_hash = adapteros_core::B3Hash::hash(&state).to_string();
        Ok(ModelState {
            model_id: self.metadata.model_id.clone(),
            checkpoint_hash,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            state_data: state,
        })
    }

    fn restore_state(&mut self, state: &ModelState) -> Result<()> {
        if state.model_id != self.metadata.model_id {
            return Err(AosError::BaseLLM(
                "Model ID mismatch in checkpoint".to_string(),
            ));
        }
        let v: serde_json::Value = serde_json::from_slice(&state.state_data)?;
        self.sequence = v["sequence"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .map(|x| x.as_u64().unwrap_or(0) as u32)
            .collect();
        self.checkpoints = v["checkpoints"].as_u64().unwrap_or(0);
        Ok(())
    }

    fn reset(&mut self) -> Result<()> {
        self.sequence.clear();
        self.checkpoints = 0;
        Ok(())
    }

    fn create_trace_event(&self, operation: &str, input_hash: &str) -> Event {
        use std::collections::HashMap;
        // Build inputs/outputs maps
        let mut inputs: HashMap<String, serde_json::Value> = HashMap::new();
        inputs.insert(
            "input_hash".into(),
            serde_json::Value::String(input_hash.to_string()),
        );
        inputs.insert(
            "sequence_length".into(),
            serde_json::Value::Number(serde_json::Number::from(self.sequence.len())),
        );

        let mut outputs: HashMap<String, serde_json::Value> = HashMap::new();
        outputs.insert(
            "model_id".into(),
            serde_json::Value::String(self.metadata.model_id.clone()),
        );
        outputs.insert(
            "model_hash".into(),
            serde_json::Value::String(self.metadata.model_hash.clone()),
        );
        outputs.insert(
            "operation".into(),
            serde_json::Value::String(operation.to_string()),
        );
        outputs.insert(
            "checkpoint_counter".into(),
            serde_json::Value::Number(serde_json::Number::from(self.checkpoints)),
        );

        // Minimal metadata; callers can enrich at higher layers
        let metadata = adapteros_trace::EventMetadata {
            global_seed: adapteros_core::B3Hash::hash(b"mlx-ffi"),
            plan_id: "default".into(),
            cpid: "default".into(),
            tenant_id: "default".into(),
            session_id: uuid::Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)).to_string(),
            adapter_ids: vec![self.metadata.model_id.clone()],
            memory_usage_mb: 0,
            gpu_utilization_pct: 0.0,
            custom: HashMap::new(),
        };

        let ts = adapteros_trace::LogicalTimestamp::new(
            0,
            0,
            None,
            adapteros_core::B3Hash::hash(operation.as_bytes()),
        );
        Event::new(
            0,
            format!("mlxffi_{}", operation),
            format!("mlxffi_{}", operation),
            inputs,
            outputs,
            metadata,
            ts,
        )
    }
}
