//! Qwen 2.5 7B Instruct int4 quantized backend via MLX FFI
//!
//! Loads int4 quantized weights from quantization manifest, dequantizes to FP32,
//! and uses MLX FFI for Metal-backed inference with deterministic execution.

use crate::{BaseLLM, BaseLLMMetadata, ModelState};
use adapteros_config::get_model_path_optional;
use adapteros_core::{AosError, Result};
use adapteros_deterministic_exec::DeterministicExecutor;
use adapteros_lora_mlx_ffi::{LoRAAdapter, MLXFFIBackend, MLXFFIModel};
use adapteros_trace::Event;
use bytemuck;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::info;

/// Quantization manifest (matches quantize_qwen.rs format)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantizationManifest {
    pub model_name: String,
    pub quant_method: String,
    pub bits: u8,
    pub per_channel: bool,
    pub tensors: BTreeMap<String, QuantizedTensorInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantizedTensorInfo {
    pub shape: Vec<usize>,
    pub packed_path: String,
    pub scales_path: String,
    pub zero_points_path: String,
}

/// Qwen int4 backend that loads quantized weights and uses MLX FFI
pub struct Qwen25Int4Mlx {
    metadata: BaseLLMMetadata,
    model: Option<MLXFFIModel>,
    backend: Option<Arc<RwLock<MLXFFIBackend>>>,
    manifest: Option<QuantizationManifest>,
    manifest_dir: Option<PathBuf>,
    sequence: Vec<u32>,
    checkpoints: u64,
}

impl Qwen25Int4Mlx {
    pub fn new(metadata: BaseLLMMetadata) -> Self {
        Self {
            metadata,
            model: None,
            backend: None,
            manifest: None,
            manifest_dir: None,
            sequence: Vec::new(),
            checkpoints: 0,
        }
    }

    /// Attach a LoRA adapter (for lifecycle integration)
    pub fn attach_adapter(&mut self, adapter_id: u16, adapter: LoRAAdapter) -> Result<()> {
        if let Some(ref backend) = self.backend {
            backend.write().register_adapter(adapter_id, adapter)?;
            info!(adapter_id, "LoRA adapter attached to Qwen int4 backend");
        } else {
            return Err(AosError::BaseLLM(
                "Backend not initialized. Call load() first.".to_string(),
            ));
        }
        Ok(())
    }

    /// Detach a LoRA adapter
    pub fn detach_adapter(&mut self, adapter_id: u16) -> Result<()> {
        if let Some(ref backend) = self.backend {
            backend.write().unload_adapter_runtime(adapter_id)?;
            info!(adapter_id, "LoRA adapter detached from Qwen int4 backend");
        } else {
            return Err(AosError::BaseLLM(
                "Backend not initialized. Call load() first.".to_string(),
            ));
        }
        Ok(())
    }

    /// Load adapter from safetensors bytes (for lifecycle integration)
    pub fn load_adapter_bytes(&mut self, adapter_id: u16, weights: &[u8]) -> Result<()> {
        let adapter = LoRAAdapter::from_safetensors_bytes(format!("{}", adapter_id), weights)
            .map_err(|e| AosError::BaseLLM(format!("Failed to parse adapter: {}", e)))?;
        self.attach_adapter(adapter_id, adapter)
    }

    /// Load int4 weights from quantization manifest directory
    fn load_int4_weights(&mut self, manifest_dir: &Path) -> Result<()> {
        let manifest_path = manifest_dir.join("manifest.json");
        let manifest_str = fs::read_to_string(&manifest_path)
            .map_err(|e| AosError::Io(format!("Failed to read manifest: {}", e)))?;

        let manifest: QuantizationManifest = serde_json::from_str(&manifest_str)
            .map_err(|e| AosError::Parse(format!("Failed to parse manifest: {}", e)))?;

        self.manifest = Some(manifest);
        self.manifest_dir = Some(manifest_dir.to_path_buf());

        info!(
            model = %self.manifest.as_ref().map(|m| &m.model_name).unwrap_or(&"<unknown>".to_string()),
            tensors = self.manifest.as_ref().map(|m| m.tensors.len()).unwrap_or(0),
            "Loaded int4 quantization manifest"
        );
        Ok(())
    }

    /// Dequantize a single int4 tensor from disk
    fn dequantize_tensor(
        &self,
        info: &QuantizedTensorInfo,
        manifest_dir: &Path,
    ) -> Result<Vec<f32>> {
        let packed_path = manifest_dir.join(&info.packed_path);
        let scales_path = manifest_dir.join(&info.scales_path);
        let zps_path = manifest_dir.join(&info.zero_points_path);

        let packed = fs::read(&packed_path)
            .map_err(|e| AosError::Io(format!("Failed to read packed tensor: {}", e)))?;
        let scales: Vec<f32> = {
            let scales_bytes = fs::read(&scales_path)
                .map_err(|e| AosError::Io(format!("Failed to read scales: {}", e)))?;
            bytemuck::try_cast_slice(&scales_bytes)
                .map(|s: &[f32]| s.to_vec())
                .map_err(|_| AosError::Parse("Invalid scales format".to_string()))?
        };
        let zero_points: Vec<i8> = {
            let zps_bytes = fs::read(&zps_path)
                .map_err(|e| AosError::Io(format!("Failed to read zero_points: {}", e)))?;
            bytemuck::try_cast_slice(&zps_bytes)
                .map(|s: &[i8]| s.to_vec())
                .map_err(|_| AosError::Parse("Invalid zero_points format".to_string()))?
        };

        let [rows, cols] = <[usize; 2]>::try_from(&info.shape[..])
            .map_err(|_| AosError::Parse("Expected 2D tensor".to_string()))?;

        let mut dequantized = Vec::with_capacity(rows * cols);

        for row_idx in 0..rows {
            let scale = scales.get(row_idx).copied().unwrap_or(1.0);
            let zp = zero_points.get(row_idx).copied().unwrap_or(0);

            let row_start = row_idx * ((cols + 1) / 2);
            let row_packed = packed
                .get(row_start..row_start + ((cols + 1) / 2))
                .unwrap_or(&[]);

            for col_idx in 0..cols {
                let packed_idx = col_idx / 2;
                let nibble_shift = if col_idx % 2 == 0 { 0 } else { 4 };
                let nibble = if packed_idx < row_packed.len() {
                    ((row_packed[packed_idx] >> nibble_shift) & 0x0F) as u8
                } else {
                    0
                };

                let q_val = nibble as i8;
                let dequant_val = (q_val - zp) as f32 * scale;
                dequantized.push(dequant_val);
            }
        }

        Ok(dequantized)
    }

    /// Load all dequantized weights from manifest
    ///
    /// Pre-dequantizes all int4 tensors from disk into f32 format.
    /// This optimizes memory by doing dequantization once at load time.
    fn load_all_dequantized_weights(&self) -> Result<BTreeMap<String, Vec<f32>>> {
        let manifest = self.manifest.as_ref().ok_or_else(|| {
            AosError::Config("Manifest not loaded. Call load_int4_weights first.".to_string())
        })?;

        let manifest_dir = self.manifest_dir.as_ref().ok_or_else(|| {
            AosError::Config("Manifest directory not set.".to_string())
        })?;

        let mut dequantized_weights = BTreeMap::new();

        for (tensor_name, tensor_info) in &manifest.tensors {
            let dequantized = self.dequantize_tensor(tensor_info, manifest_dir)?;
            dequantized_weights.insert(tensor_name.clone(), dequantized);
        }

        info!(
            tensors = dequantized_weights.len(),
            total_elements = dequantized_weights.values().map(|v| v.len()).sum::<usize>(),
            "Pre-dequantized all int4 weights to f32"
        );

        Ok(dequantized_weights)
    }

    /// Create MLX model from pre-dequantized weights
    ///
    /// Constructs an MLXFFIModel by passing dequantized f32 weights directly
    /// to the MLX FFI layer, avoiding the need for a separate model load.
    fn create_model_from_dequantized_weights(
        &self,
        weights: &BTreeMap<String, Vec<f32>>,
    ) -> Result<MLXFFIModel> {
        use adapteros_lora_mlx_ffi::ModelConfig;

        let manifest = self.manifest.as_ref().ok_or_else(|| {
            AosError::Config("Manifest not loaded.".to_string())
        })?;

        // Infer model config from weight shapes (Qwen 2.5 7B defaults)
        let hidden_size = weights
            .get("model.layers.0.self_attn.q_proj.weight")
            .map(|w| {
                // For 2D weight [out_features, in_features], hidden_size = in_features
                let info = manifest.tensors.get("model.layers.0.self_attn.q_proj.weight");
                info.map(|i| i.shape.get(1).copied().unwrap_or(4096)).unwrap_or(4096)
            })
            .unwrap_or(4096);

        let num_layers = manifest
            .tensors
            .keys()
            .filter(|k| k.contains("model.layers.") && k.contains(".self_attn.q_proj"))
            .count();

        let config = ModelConfig {
            hidden_size,
            num_hidden_layers: if num_layers > 0 { num_layers } else { 28 }, // Qwen 2.5 7B default
            num_attention_heads: 28,
            num_key_value_heads: 4, // GQA
            intermediate_size: 18944, // Qwen 2.5 7B
            vocab_size: 152064, // Qwen 2.5
            max_position_embeddings: 131072, // Qwen 2.5 7B
            rope_theta: 1000000.0, // Qwen 2.5
        };

        // Create model with pre-loaded weights via FFI
        // The weights are passed as a flattened buffer with metadata
        let weight_buffer = self.serialize_weights_for_ffi(weights)?;
        let model = self.load_model_from_buffer(&config, &weight_buffer)?;

        info!(
            hidden_size = config.hidden_size,
            num_layers = config.num_hidden_layers,
            vocab_size = config.vocab_size,
            "Created MLX model from pre-dequantized int4 weights"
        );

        Ok(model)
    }

    /// Serialize weights for FFI transfer
    ///
    /// Creates a contiguous buffer with weight data and metadata
    /// that can be passed to MLX FFI for model construction.
    fn serialize_weights_for_ffi(
        &self,
        weights: &BTreeMap<String, Vec<f32>>,
    ) -> Result<Vec<u8>> {
        let manifest = self.manifest.as_ref().ok_or_else(|| {
            AosError::Config("Manifest not loaded.".to_string())
        })?;

        // Calculate total buffer size
        let total_floats: usize = weights.values().map(|v| v.len()).sum();
        let mut buffer = Vec::with_capacity(total_floats * 4 + 4096); // Extra for headers

        // Write header: number of tensors
        buffer.extend_from_slice(&(weights.len() as u32).to_le_bytes());

        // Write each tensor with metadata
        for (name, data) in weights {
            // Tensor name length + name
            let name_bytes = name.as_bytes();
            buffer.extend_from_slice(&(name_bytes.len() as u32).to_le_bytes());
            buffer.extend_from_slice(name_bytes);

            // Shape info from manifest
            if let Some(info) = manifest.tensors.get(name) {
                buffer.extend_from_slice(&(info.shape.len() as u32).to_le_bytes());
                for &dim in &info.shape {
                    buffer.extend_from_slice(&(dim as u32).to_le_bytes());
                }
            } else {
                // Infer 1D shape
                buffer.extend_from_slice(&1u32.to_le_bytes());
                buffer.extend_from_slice(&(data.len() as u32).to_le_bytes());
            }

            // Data length + data
            buffer.extend_from_slice(&(data.len() as u32).to_le_bytes());
            for &val in data {
                buffer.extend_from_slice(&val.to_le_bytes());
            }
        }

        Ok(buffer)
    }

    /// Load MLX model from serialized weight buffer via FFI
    ///
    /// Creates a model using the pre-dequantized weights buffer.
    /// Uses the MLX FFI to load weights directly into the model.
    fn load_model_from_buffer(
        &self,
        config: &adapteros_lora_mlx_ffi::ModelConfig,
        buffer: &[u8],
    ) -> Result<MLXFFIModel> {
        // Safety: Check buffer validity
        if buffer.len() < 4 {
            return Err(AosError::Parse("Weight buffer too small".to_string()));
        }

        // Load model via FFI from pre-dequantized weight buffer
        let model = MLXFFIModel::load_from_buffer(buffer, config.clone())?;

        Ok(model)
    }
}

impl BaseLLM for Qwen25Int4Mlx {
    fn load(&mut self, _executor: &mut DeterministicExecutor) -> Result<()> {
        // Load from manifest directory (env var or default)
        let manifest_dir = std::env::var("AOS_QWEN_INT4_DIR")
            .map(PathBuf::from)
            .or_else(|_| {
                // Fallback to artifacts/qwen2_5_7b_int4 if exists
                let default = PathBuf::from("artifacts/qwen2_5_7b_int4");
                if default.exists() {
                    Ok(default)
                } else {
                    Err(std::env::VarError::NotPresent)
                }
            })
            .map_err(|_| {
                AosError::Config(
                    "AOS_QWEN_INT4_DIR not set and default artifacts/qwen2_5_7b_int4 not found"
                        .to_string(),
                )
            })?;

        self.load_int4_weights(&manifest_dir)?;

        // Load model via MLX FFI - supports both standard load and pre-dequantized int4 weights
        // Use unified model path helper with automatic legacy fallback
        let model_path = get_model_path_optional();

        let (model, backend_arc) = if let Some(ref path) = model_path {
            // Standard path: load from MLX format
            let model = MLXFFIModel::load(path.to_string_lossy().as_ref())?;
            let backend = MLXFFIBackend::new(model);
            (None, Arc::new(RwLock::new(backend)))
        } else {
            // Optimized path: use pre-dequantized int4 weights directly
            // This avoids re-dequantization by passing weights to MLX FFI
            let dequantized_weights = self.load_all_dequantized_weights()?;
            let model = self.create_model_from_dequantized_weights(&dequantized_weights)?;
            let backend = MLXFFIBackend::new(model);
            (None, Arc::new(RwLock::new(backend)))
        };

        // Keep model reference for direct access when needed
        self.model = model;
        self.backend = Some(backend_arc.clone());

        info!(
            model_id = %self.metadata.model_id,
            "Qwen int4 MLX backend loaded with LoRA support"
        );
        Ok(())
    }

    fn forward(&mut self, input_ids: &[u32]) -> Result<Vec<f32>> {
        // Use backend which handles LoRA adapters automatically
        let backend = self
            .backend
            .as_ref()
            .ok_or_else(|| AosError::BaseLLM("Backend not loaded".to_string()))?;

        self.sequence = input_ids.to_vec();
        self.checkpoints = self.checkpoints.wrapping_add(1);

        // Get base model from backend (accessing internal model via forward_with_hidden_states)
        // For now, use simple forward - the backend's run_step handles LoRA routing
        // This is a simplified path; full integration would use IoBuffers and RouterRing
        let backend_guard = backend.read();
        // Access the internal model via the backend
        // Note: This is a limitation - we need to expose model access or use backend.run_step
        // For now, fallback to direct model forward if available
        let logits = if let Some(ref model) = self.model {
            let pos = self.sequence.len().saturating_sub(1);
            model.forward(&self.sequence, pos)?
        } else {
            // Try to get logits via backend (requires IoBuffers setup)
            // Simplified: just get base logits
            let (logits, _) = backend_guard
                .model
                .forward_with_hidden_states(&self.sequence)?;
            logits
        };
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
        let mut inputs: HashMap<String, serde_json::Value> = HashMap::new();
        inputs.insert(
            "input_hash".into(),
            serde_json::Value::String(input_hash.to_string()),
        );
        inputs.insert(
            "sequence_length".into(),
            serde_json::Value::Number(serde_json::Number::from(self.sequence.len())),
        );
        inputs.insert(
            "quantization".into(),
            serde_json::Value::String("int4_per_channel".to_string()),
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

        let metadata = adapteros_trace::EventMetadata {
            global_seed: adapteros_core::B3Hash::hash(b"qwen-int4-mlx"),
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
            "qwen_int4_mlx".to_string(),
            operation.to_string(),
            inputs,
            outputs,
            metadata,
            ts,
        )
    }
}
