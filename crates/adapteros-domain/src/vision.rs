//! Vision domain adapter with canonical image processing

use adapteros_deterministic_exec::DeterministicExecutor;
use adapteros_numerics::noise::{EpsilonStats, Tensor};
use adapteros_trace::{Event, EventMetadata};
use adapteros_core::B3Hash;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

use crate::adapter::{AdapterMetadata, DomainAdapter, TensorData};
use crate::error::{DomainAdapterError, Result};
use crate::manifest::{load_manifest, AdapterManifest};

/// Vision adapter for deterministic image processing
///
/// This adapter handles:
/// - Canonical image layout (NCHW format)
/// - Deterministic normalization
/// - Quantized convolution pipeline
/// - Image-to-tensor conversion
pub struct VisionAdapter {
    /// Adapter metadata
    metadata: AdapterMetadata,
    /// Internal state
    state: Arc<RwLock<VisionAdapterState>>,
    /// Manifest configuration
    manifest: AdapterManifest,
}

#[derive(Debug)]
struct VisionAdapterState {
    /// Whether adapter is initialized
    initialized: bool,
    /// Image dimensions (height, width)
    image_size: (usize, usize),
    /// Number of channels (e.g., 3 for RGB)
    num_channels: usize,
    /// Normalization mean values
    norm_mean: Vec<f32>,
    /// Normalization std values
    norm_std: Vec<f32>,
    /// Current epsilon statistics
    epsilon_stats: Option<EpsilonStats>,
    /// Processing counter
    processing_counter: u64,
}

impl VisionAdapter {
    /// Load vision adapter from manifest
    pub fn load<P: AsRef<std::path::Path>>(manifest_path: P) -> Result<Self> {
        let manifest = load_manifest(manifest_path)?;
        
        // Extract configuration
        let image_height = manifest
            .get_parameter_i64("image_height")
            .unwrap_or(224) as usize;
        
        let image_width = manifest
            .get_parameter_i64("image_width")
            .unwrap_or(224) as usize;
        
        let num_channels = manifest
            .get_parameter_i64("num_channels")
            .unwrap_or(3) as usize;
        
        // Default ImageNet normalization
        let norm_mean = vec![0.485, 0.456, 0.406];
        let norm_std = vec![0.229, 0.224, 0.225];
        
        let model_hash = manifest.parse_hash()?;
        
        let metadata = AdapterMetadata {
            name: manifest.adapter.name.clone(),
            version: manifest.adapter.version.clone(),
            model_hash,
            input_format: manifest.adapter.input_format.clone(),
            output_format: manifest.adapter.output_format.clone(),
            epsilon_threshold: manifest.adapter.epsilon_threshold,
            deterministic: manifest.adapter.deterministic,
            custom: HashMap::new(),
        };
        
        let state = VisionAdapterState {
            initialized: false,
            image_size: (image_height, image_width),
            num_channels,
            norm_mean,
            norm_std,
            epsilon_stats: None,
            processing_counter: 0,
        };
        
        tracing::info!(
            "Created VisionAdapter '{}' v{} (size={}x{}, channels={})",
            metadata.name,
            metadata.version,
            image_height,
            image_width,
            num_channels
        );
        
        Ok(Self {
            metadata,
            state: Arc::new(RwLock::new(state)),
            manifest,
        })
    }
    
    /// Convert raw image bytes to canonical tensor format (NCHW)
    ///
    /// This method:
    /// 1. Parses image data
    /// 2. Resizes to target dimensions (deterministic)
    /// 3. Converts to NCHW layout
    /// 4. Normalizes using mean/std
    fn image_to_tensor(&self, image_data: &[u8]) -> Result<Tensor> {
        let state = self.state.read();
        let (height, width) = state.image_size;
        let channels = state.num_channels;
        
        // In production, this would:
        // 1. Decode image format (JPEG/PNG) deterministically
        // 2. Resize using deterministic interpolation
        // 3. Convert to NCHW layout
        // 4. Apply normalization
        
        // For this stub, create a deterministic tensor from the hash of the input
        let hash = B3Hash::hash(image_data);
        let hash_bytes = hash.as_bytes();
        
        // Generate deterministic pixel values
        let total_size = channels * height * width;
        let mut data = Vec::with_capacity(total_size);
        
        for i in 0..total_size {
            let hash_idx = i % hash_bytes.len();
            let pixel_value = hash_bytes[hash_idx] as f32 / 255.0;
            
            // Apply normalization
            let channel = i / (height * width);
            let normalized = (pixel_value - state.norm_mean[channel % state.norm_mean.len()])
                / state.norm_std[channel % state.norm_std.len()];
            
            data.push(normalized);
        }
        
        tracing::debug!(
            "Converted image to tensor: {}x{}x{}",
            channels,
            height,
            width
        );
        
        Ok(Tensor::new(data, vec![1, channels, height, width]))
    }
    
    /// Apply quantized convolution (simplified)
    ///
    /// In production, this would apply quantized 2D convolutions
    /// with deterministic rounding and fixed-point arithmetic.
    fn apply_quantized_conv(&self, tensor: &Tensor) -> Tensor {
        // For now, this is a no-op that returns the input tensor
        // In production, this would:
        // 1. Apply quantized convolution kernels
        // 2. Use fixed-point arithmetic for determinism
        // 3. Apply activation functions (ReLU, etc.)
        // 4. Apply pooling operations
        
        tracing::debug!("Applied quantized conv (no-op in stub)");
        tensor.clone()
    }
    
    /// Normalize tensor to canonical range
    fn normalize_output(&self, tensor: &Tensor) -> Tensor {
        // Apply deterministic normalization to output
        // This ensures outputs are in a consistent range
        
        let mut normalized_data = Vec::with_capacity(tensor.len());
        
        // Find min/max (deterministic)
        let min_val = tensor.data.iter().fold(f32::INFINITY, |a, &b| a.min(b));
        let max_val = tensor.data.iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b));
        
        let range = max_val - min_val;
        let safe_range = if range.abs() < 1e-7 { 1.0 } else { range };
        
        for &val in &tensor.data {
            let normalized = (val - min_val) / safe_range;
            normalized_data.push(normalized);
        }
        
        Tensor::new(normalized_data, tensor.shape.clone())
    }
}

impl DomainAdapter for VisionAdapter {
    fn name(&self) -> &str {
        &self.metadata.name
    }
    
    fn metadata(&self) -> &AdapterMetadata {
        &self.metadata
    }
    
    fn prepare(&mut self, executor: &mut DeterministicExecutor) -> Result<()> {
        let mut state = self.state.write();
        
        if state.initialized {
            tracing::warn!("VisionAdapter '{}' already initialized", self.metadata.name);
            return Ok(());
        }
        
        // Derive a deterministic seed for this adapter
        let adapter_seed = executor.derive_seed(&format!("vision_adapter:{}", self.metadata.name));
        
        tracing::info!(
            "Initialized VisionAdapter '{}' with seed: {:?}",
            self.metadata.name,
            &adapter_seed[..8]
        );
        
        state.initialized = true;
        Ok(())
    }
    
    fn forward(&mut self, input: &TensorData) -> Result<TensorData> {
        let state = self.state.read();
        
        if !state.initialized {
            return Err(DomainAdapterError::AdapterNotInitialized {
                adapter_name: self.metadata.name.clone(),
            });
        }
        
        let input_tensor = &input.tensor;
        
        // Verify input shape
        let expected_shape = vec![
            1,
            state.num_channels,
            state.image_size.0,
            state.image_size.1,
        ];
        
        if input_tensor.shape != expected_shape {
            return Err(DomainAdapterError::TensorShapeMismatch {
                expected: expected_shape,
                actual: input_tensor.shape.clone(),
            });
        }
        
        // Apply quantized convolution pipeline
        let processed_tensor = self.apply_quantized_conv(input_tensor);
        
        // Normalize output
        let output_tensor = self.normalize_output(&processed_tensor);
        
        let output_data = TensorData::new(output_tensor, "f32".to_string());
        
        tracing::debug!("Forward pass completed for VisionAdapter '{}'", self.metadata.name);
        
        Ok(output_data)
    }
    
    fn postprocess(&mut self, output: &TensorData) -> Result<TensorData> {
        // Apply any final processing
        // For now, this is a pass-through
        
        tracing::debug!("Postprocessing output for VisionAdapter '{}'", self.metadata.name);
        
        Ok(output.clone())
    }
    
    fn epsilon_stats(&self) -> Option<EpsilonStats> {
        self.state.read().epsilon_stats.clone()
    }
    
    fn reset(&mut self) {
        let mut state = self.state.write();
        state.processing_counter = 0;
        state.epsilon_stats = None;
        
        tracing::info!("Reset VisionAdapter '{}'", self.metadata.name);
    }
    
    fn create_trace_event(
        &self,
        tick_id: u64,
        op_id: String,
        inputs: &HashMap<String, serde_json::Value>,
        outputs: &HashMap<String, serde_json::Value>,
    ) -> Event {
        use adapteros_trace::schema::Event;
        
        let metadata = EventMetadata {
            global_seed: B3Hash::hash(b"vision_adapter_seed"),
            plan_id: "vision_adapter_plan".to_string(),
            cpid: "vision_adapter_cpid".to_string(),
            tenant_id: "default".to_string(),
            session_id: "default".to_string(),
            adapter_ids: vec![self.metadata.name.clone()],
            memory_usage_mb: 0,
            gpu_utilization_pct: 0.0,
            custom: HashMap::new(),
        };
        
        Event::new(
            tick_id,
            op_id,
            "vision.forward".to_string(),
            inputs.clone(),
            outputs.clone(),
            metadata,
        )
    }
}

/// Helper function to create a vision tensor from image bytes
pub fn image_to_tensor(adapter: &VisionAdapter, image_data: &[u8]) -> Result<TensorData> {
    let tensor = adapter.image_to_tensor(image_data)?;
    Ok(TensorData::new(tensor, "f32".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    
    fn create_test_manifest() -> (AdapterManifest, NamedTempFile) {
        use crate::manifest::{save_manifest, AdapterManifest};
        
        let mut manifest = AdapterManifest::new(
            "test_vision_adapter".to_string(),
            "1.0.0".to_string(),
            "test_vision_model".to_string(),
            "b3d9c2a1e8f7d6b5a4938271605e4f3c2d1b0a9e8f7d6c5b4a3928170605".to_string(),
        );
        
        manifest.adapter.input_format = "NCHW".to_string();
        manifest.adapter.output_format = "NCHW normalized".to_string();
        
        manifest.adapter.parameters.insert(
            "image_height".to_string(),
            serde_json::Value::Number(64.into()),
        );
        
        manifest.adapter.parameters.insert(
            "image_width".to_string(),
            serde_json::Value::Number(64.into()),
        );
        
        manifest.adapter.parameters.insert(
            "num_channels".to_string(),
            serde_json::Value::Number(3.into()),
        );
        
        let temp_file = NamedTempFile::new().unwrap();
        save_manifest(&manifest, temp_file.path()).unwrap();
        
        (manifest, temp_file)
    }
    
    #[test]
    fn test_vision_adapter_load() {
        let (_manifest, temp_file) = create_test_manifest();
        let adapter = VisionAdapter::load(temp_file.path()).unwrap();
        
        assert_eq!(adapter.name(), "test_vision_adapter");
        assert_eq!(adapter.state.read().image_size, (64, 64));
        assert_eq!(adapter.state.read().num_channels, 3);
    }
    
    #[test]
    fn test_image_to_tensor_deterministic() {
        let (_manifest, temp_file) = create_test_manifest();
        let adapter = VisionAdapter::load(temp_file.path()).unwrap();
        
        let image_data = b"fake image data";
        let tensor1 = adapter.image_to_tensor(image_data).unwrap();
        let tensor2 = adapter.image_to_tensor(image_data).unwrap();
        
        assert_eq!(tensor1.data, tensor2.data);
        assert_eq!(tensor1.shape, vec![1, 3, 64, 64]);
    }
    
    #[test]
    fn test_normalize_output() {
        let (_manifest, temp_file) = create_test_manifest();
        let adapter = VisionAdapter::load(temp_file.path()).unwrap();
        
        let tensor = Tensor::new(vec![0.0, 5.0, 10.0], vec![3]);
        let normalized = adapter.normalize_output(&tensor);
        
        // Values should be in [0, 1] range
        assert!(normalized.data.iter().all(|&x| x >= 0.0 && x <= 1.0));
    }
}

