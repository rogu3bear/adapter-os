//! Telemetry domain adapter with deterministic signal normalization

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

/// Telemetry adapter for deterministic signal processing
///
/// This adapter handles:
/// - Deterministic signal normalization
/// - Canonical ordering of time-series data
/// - Quantized filtering operations
/// - Anomaly detection with fixed thresholds
pub struct TelemetryAdapter {
    /// Adapter metadata
    metadata: AdapterMetadata,
    /// Internal state
    state: Arc<RwLock<TelemetryAdapterState>>,
    /// Manifest configuration
    manifest: AdapterManifest,
}

#[derive(Debug)]
struct TelemetryAdapterState {
    /// Whether adapter is initialized
    initialized: bool,
    /// Number of signal channels
    num_channels: usize,
    /// Window size for temporal processing
    window_size: usize,
    /// Sampling rate (Hz)
    sampling_rate: f32,
    /// Normalization parameters (min, max) per channel
    norm_params: Vec<(f32, f32)>,
    /// Current epsilon statistics
    epsilon_stats: Option<EpsilonStats>,
    /// Signal counter
    signal_counter: u64,
}

impl TelemetryAdapter {
    /// Load telemetry adapter from manifest
    pub fn load<P: AsRef<std::path::Path>>(manifest_path: P) -> Result<Self> {
        let manifest = load_manifest(manifest_path)?;
        
        // Extract configuration
        let num_channels = manifest
            .get_parameter_i64("num_channels")
            .unwrap_or(16) as usize;
        
        let window_size = manifest
            .get_parameter_i64("window_size")
            .unwrap_or(128) as usize;
        
        let sampling_rate = manifest
            .get_parameter_f64("sampling_rate")
            .unwrap_or(100.0) as f32;
        
        // Default normalization parameters (min, max) for each channel
        let norm_params = vec![(0.0, 1.0); num_channels];
        
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
        
        let state = TelemetryAdapterState {
            initialized: false,
            num_channels,
            window_size,
            sampling_rate,
            norm_params,
            epsilon_stats: None,
            signal_counter: 0,
        };
        
        tracing::info!(
            "Created TelemetryAdapter '{}' v{} (channels={}, window={}, rate={}Hz)",
            metadata.name,
            metadata.version,
            num_channels,
            window_size,
            sampling_rate
        );
        
        Ok(Self {
            metadata,
            state: Arc::new(RwLock::new(state)),
            manifest,
        })
    }
    
    /// Normalize signal data deterministically
    ///
    /// This applies min-max normalization to each channel independently.
    /// The normalization is deterministic and uses fixed parameters.
    fn normalize_signal(&self, signal: &Tensor) -> Result<Tensor> {
        let state = self.state.read();
        
        // Expected shape: [batch, channels, time_steps]
        if signal.shape.len() != 3 {
            return Err(DomainAdapterError::TensorShapeMismatch {
                expected: vec![1, state.num_channels, state.window_size],
                actual: signal.shape.clone(),
            });
        }
        
        let batch_size = signal.shape[0];
        let channels = signal.shape[1];
        let time_steps = signal.shape[2];
        
        if channels != state.num_channels {
            return Err(DomainAdapterError::TensorShapeMismatch {
                expected: vec![batch_size, state.num_channels, time_steps],
                actual: signal.shape.clone(),
            });
        }
        
        let mut normalized_data = Vec::with_capacity(signal.len());
        
        for b in 0..batch_size {
            for c in 0..channels {
                let (min_val, max_val) = state.norm_params[c];
                let range = max_val - min_val;
                let safe_range = if range.abs() < 1e-7 { 1.0 } else { range };
                
                for t in 0..time_steps {
                    let idx = b * (channels * time_steps) + c * time_steps + t;
                    let val = signal.data[idx];
                    let normalized = (val - min_val) / safe_range;
                    // Clamp to [0, 1]
                    let clamped = normalized.max(0.0).min(1.0);
                    normalized_data.push(clamped);
                }
            }
        }
        
        tracing::debug!("Normalized signal: {}x{}x{}", batch_size, channels, time_steps);
        
        Ok(Tensor::new(normalized_data, signal.shape.clone()))
    }
    
    /// Apply deterministic filtering
    ///
    /// This applies a simple moving average filter for smoothing.
    /// The filter is deterministic with fixed kernel size.
    fn apply_filtering(&self, signal: &Tensor, kernel_size: usize) -> Tensor {
        if kernel_size == 0 || kernel_size == 1 {
            return signal.clone();
        }
        
        let batch_size = signal.shape[0];
        let channels = signal.shape[1];
        let time_steps = signal.shape[2];
        
        let mut filtered_data = Vec::with_capacity(signal.len());
        
        for b in 0..batch_size {
            for c in 0..channels {
                for t in 0..time_steps {
                    // Compute moving average
                    let start = if t >= kernel_size / 2 {
                        t - kernel_size / 2
                    } else {
                        0
                    };
                    let end = (t + kernel_size / 2 + 1).min(time_steps);
                    
                    let mut sum = 0.0;
                    let mut count = 0;
                    
                    for i in start..end {
                        let idx = b * (channels * time_steps) + c * time_steps + i;
                        sum += signal.data[idx];
                        count += 1;
                    }
                    
                    let filtered = sum / count as f32;
                    filtered_data.push(filtered);
                }
            }
        }
        
        tracing::debug!("Applied filtering with kernel size {}", kernel_size);
        
        Tensor::new(filtered_data, signal.shape.clone())
    }
    
    /// Detect anomalies using fixed thresholds
    ///
    /// Returns a mask tensor with 1.0 for anomalies, 0.0 for normal values.
    fn detect_anomalies(&self, signal: &Tensor, threshold: f32) -> Tensor {
        let mut anomaly_mask = Vec::with_capacity(signal.len());
        
        for &val in &signal.data {
            // Simple threshold-based anomaly detection
            let is_anomaly = if val.abs() > threshold { 1.0 } else { 0.0 };
            anomaly_mask.push(is_anomaly);
        }
        
        let anomaly_count = anomaly_mask.iter().filter(|&&x| x == 1.0).count();
        
        tracing::debug!("Detected {} anomalies (threshold={})", anomaly_count, threshold);
        
        Tensor::new(anomaly_mask, signal.shape.clone())
    }
}

impl DomainAdapter for TelemetryAdapter {
    fn name(&self) -> &str {
        &self.metadata.name
    }
    
    fn metadata(&self) -> &AdapterMetadata {
        &self.metadata
    }
    
    fn prepare(&mut self, executor: &mut DeterministicExecutor) -> Result<()> {
        let mut state = self.state.write();
        
        if state.initialized {
            tracing::warn!("TelemetryAdapter '{}' already initialized", self.metadata.name);
            return Ok(());
        }
        
        // Derive a deterministic seed for this adapter
        let adapter_seed = executor.derive_seed(&format!("telemetry_adapter:{}", self.metadata.name));
        
        tracing::info!(
            "Initialized TelemetryAdapter '{}' with seed: {:?}",
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
        
        // Normalize signal
        let normalized = self.normalize_signal(input_tensor)?;
        
        // Apply filtering (kernel size = 5)
        let filtered = self.apply_filtering(&normalized, 5);
        
        let output_data = TensorData::new(filtered, "f32".to_string());
        
        tracing::debug!("Forward pass completed for TelemetryAdapter '{}'", self.metadata.name);
        
        Ok(output_data)
    }
    
    fn postprocess(&mut self, output: &TensorData) -> Result<TensorData> {
        // Apply anomaly detection
        let anomaly_threshold = 0.95; // Fixed threshold for determinism
        let _anomaly_mask = self.detect_anomalies(&output.tensor, anomaly_threshold);
        
        // For postprocessing, we'll return the anomaly mask as metadata
        let mut output_with_metadata = output.clone();
        output_with_metadata.metadata.custom.insert(
            "anomaly_mask".to_string(),
            serde_json::Value::String("computed".to_string()),
        );
        
        tracing::debug!("Postprocessing output for TelemetryAdapter '{}'", self.metadata.name);
        
        Ok(output_with_metadata)
    }
    
    fn epsilon_stats(&self) -> Option<EpsilonStats> {
        self.state.read().epsilon_stats.clone()
    }
    
    fn reset(&mut self) {
        let mut state = self.state.write();
        state.signal_counter = 0;
        state.epsilon_stats = None;
        
        tracing::info!("Reset TelemetryAdapter '{}'", self.metadata.name);
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
            global_seed: B3Hash::hash(b"telemetry_adapter_seed"),
            plan_id: "telemetry_adapter_plan".to_string(),
            cpid: "telemetry_adapter_cpid".to_string(),
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
            "telemetry.forward".to_string(),
            inputs.clone(),
            outputs.clone(),
            metadata,
        )
    }
}

/// Helper function to create a telemetry tensor from time-series data
pub fn timeseries_to_tensor(
    num_channels: usize,
    window_size: usize,
    data: &[f32],
) -> Result<TensorData> {
    if data.len() != num_channels * window_size {
        return Err(DomainAdapterError::TensorShapeMismatch {
            expected: vec![1, num_channels, window_size],
            actual: vec![data.len()],
        });
    }
    
    let tensor = Tensor::new(data.to_vec(), vec![1, num_channels, window_size]);
    Ok(TensorData::new(tensor, "f32".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    
    fn create_test_manifest() -> (AdapterManifest, NamedTempFile) {
        use crate::manifest::{save_manifest, AdapterManifest};
        
        let mut manifest = AdapterManifest::new(
            "test_telemetry_adapter".to_string(),
            "1.0.0".to_string(),
            "test_telemetry_model".to_string(),
            "b3d9c2a1e8f7d6b5a4938271605e4f3c2d1b0a9e8f7d6c5b4a3928170605".to_string(),
        );
        
        manifest.adapter.input_format = "time_series".to_string();
        manifest.adapter.output_format = "normalized".to_string();
        
        manifest.adapter.parameters.insert(
            "num_channels".to_string(),
            serde_json::Value::Number(4.into()),
        );
        
        manifest.adapter.parameters.insert(
            "window_size".to_string(),
            serde_json::Value::Number(32.into()),
        );
        
        manifest.adapter.parameters.insert(
            "sampling_rate".to_string(),
            serde_json::Value::Number(serde_json::Number::from_f64(50.0).unwrap()),
        );
        
        let temp_file = NamedTempFile::new().unwrap();
        save_manifest(&manifest, temp_file.path()).unwrap();
        
        (manifest, temp_file)
    }
    
    #[test]
    fn test_telemetry_adapter_load() {
        let (_manifest, temp_file) = create_test_manifest();
        let adapter = TelemetryAdapter::load(temp_file.path()).unwrap();
        
        assert_eq!(adapter.name(), "test_telemetry_adapter");
        assert_eq!(adapter.state.read().num_channels, 4);
        assert_eq!(adapter.state.read().window_size, 32);
        assert_eq!(adapter.state.read().sampling_rate, 50.0);
    }
    
    #[test]
    fn test_signal_normalization() {
        let (_manifest, temp_file) = create_test_manifest();
        let adapter = TelemetryAdapter::load(temp_file.path()).unwrap();
        
        // Create test signal: [1, 4, 32]
        let data: Vec<f32> = (0..128).map(|x| x as f32 / 128.0).collect();
        let tensor = Tensor::new(data, vec![1, 4, 32]);
        
        let normalized = adapter.normalize_signal(&tensor).unwrap();
        
        // All values should be in [0, 1]
        assert!(normalized.data.iter().all(|&x| x >= 0.0 && x <= 1.0));
    }
    
    #[test]
    fn test_filtering() {
        let (_manifest, temp_file) = create_test_manifest();
        let adapter = TelemetryAdapter::load(temp_file.path()).unwrap();
        
        let data: Vec<f32> = (0..128).map(|x| x as f32).collect();
        let tensor = Tensor::new(data, vec![1, 4, 32]);
        
        let filtered = adapter.apply_filtering(&tensor, 5);
        
        assert_eq!(filtered.shape, tensor.shape);
    }
    
    #[test]
    fn test_anomaly_detection() {
        let (_manifest, temp_file) = create_test_manifest();
        let adapter = TelemetryAdapter::load(temp_file.path()).unwrap();
        
        // Create signal with some high values (anomalies)
        let mut data: Vec<f32> = vec![0.5; 128];
        data[10] = 2.0; // Anomaly
        data[50] = 3.0; // Anomaly
        
        let tensor = Tensor::new(data, vec![1, 4, 32]);
        let anomaly_mask = adapter.detect_anomalies(&tensor, 0.95);
        
        // Should detect 2 anomalies
        let anomaly_count = anomaly_mask.data.iter().filter(|&&x| x == 1.0).count();
        assert_eq!(anomaly_count, 2);
    }
    
    #[test]
    fn test_timeseries_to_tensor() {
        let data: Vec<f32> = (0..128).map(|x| x as f32).collect();
        let tensor_data = timeseries_to_tensor(4, 32, &data).unwrap();
        
        assert_eq!(tensor_data.tensor.shape, vec![1, 4, 32]);
        assert_eq!(tensor_data.tensor.len(), 128);
    }
}

