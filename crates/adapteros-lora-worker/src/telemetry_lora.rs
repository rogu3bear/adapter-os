//! LoRA utilities for telemetry signal adapters.

use std::collections::HashMap;
use std::convert::TryInto;
use std::sync::Arc;

use adapteros_core::{AosError, Result};
use safetensors::{tensor::TensorView, SafeTensors};

/// Tasks supported by telemetry LoRA adapters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TelemetryTask {
    Monitoring,
    Forecasting,
    Control,
}

impl TelemetryTask {
    fn as_str(&self) -> &'static str {
        match self {
            TelemetryTask::Monitoring => "monitoring",
            TelemetryTask::Forecasting => "forecasting",
            TelemetryTask::Control => "control",
        }
    }
}

/// LoRA weight container for telemetry processing layers.
#[derive(Debug, Clone)]
pub struct TelemetryLoraWeights {
    task: TelemetryTask,
    weights: Arc<Vec<f32>>,
    bias: Arc<Vec<f32>>,
}

impl TelemetryLoraWeights {
    pub fn new(task: TelemetryTask, weights: Vec<f32>, bias: Vec<f32>) -> Result<Self> {
        if weights.is_empty() {
            return Err(AosError::Validation(
                "telemetry LoRA requires weights".into(),
            ));
        }

        Ok(Self {
            task,
            weights: Arc::new(weights),
            bias: Arc::new(bias),
        })
    }

    pub fn composed_weights(&self, base: &[f32], alpha: f32) -> Result<Vec<f32>> {
        if base.len() != Arc::as_ref(&self.weights).len() {
            return Err(AosError::Validation(format!(
                "base buffer length {} does not match telemetry LoRA {}",
                base.len(),
                Arc::as_ref(&self.weights).len()
            )));
        }

        // Invariant: the caller's base slice is treated as immutable; work on a scratch
        // copy so shared base buffers are never mutated in place.
        let mut scratch = base.to_vec();
        for (target, delta) in scratch.iter_mut().zip(Arc::as_ref(&self.weights).iter()) {
            *target += delta * alpha;
        }

        Ok(scratch)
    }

    pub fn composed_bias(&self, base: &[f32], alpha: f32) -> Result<Vec<f32>> {
        if base.len() != Arc::as_ref(&self.bias).len() {
            return Err(AosError::Validation(format!(
                "bias buffer length {} does not match telemetry LoRA {}",
                base.len(),
                Arc::as_ref(&self.bias).len()
            )));
        }

        let mut scratch = base.to_vec();
        for (target, delta) in scratch.iter_mut().zip(Arc::as_ref(&self.bias).iter()) {
            *target += delta * alpha;
        }

        Ok(scratch)
    }

    pub fn task(&self) -> TelemetryTask {
        self.task
    }
}

/// Registry for telemetry LoRA weights.
#[derive(Debug, Default)]
pub struct TelemetryLoraRegistry {
    weights: HashMap<TelemetryTask, TelemetryLoraWeights>,
}

impl TelemetryLoraRegistry {
    pub fn insert(&mut self, weights: TelemetryLoraWeights) {
        self.weights.insert(weights.task(), weights);
    }

    pub fn get(&self, task: TelemetryTask) -> Option<&TelemetryLoraWeights> {
        self.weights.get(&task)
    }
}

/// Load telemetry LoRA weights from safetensors bytes.
pub fn load_telemetry_lora(bytes: &[u8], task: TelemetryTask) -> Result<TelemetryLoraWeights> {
    let tensors = SafeTensors::deserialize(bytes)
        .map_err(|e| AosError::Validation(format!("invalid safetensors: {e}")))?;

    let weight = extract_tensor(&tensors, "telemetry_lora.weight")?;
    let bias = extract_tensor(&tensors, "telemetry_lora.bias")?;

    let weights = convert_tensor(weight);
    let bias = convert_tensor(bias);

    TelemetryLoraWeights::new(task, weights, bias)
}

fn extract_tensor<'a>(tensors: &'a SafeTensors<'a>, name: &str) -> Result<TensorView<'a>> {
    tensors
        .tensor(name)
        .map_err(|_| AosError::Validation(format!("tensor '{name}' missing from telemetry LoRA")))
}

fn convert_tensor(view: TensorView<'_>) -> Vec<f32> {
    view.data()
        .chunks_exact(std::mem::size_of::<f32>())
        .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap()))
        .collect()
}

/// Merge plan for telemetry adapters.
#[derive(Debug, Clone)]
pub struct TelemetryMergePlan {
    pub task: TelemetryTask,
    pub alpha: f32,
}

impl TelemetryMergePlan {
    pub fn apply(
        &self,
        registry: &TelemetryLoraRegistry,
        weights: &[f32],
        bias: &[f32],
    ) -> Result<(Vec<f32>, Vec<f32>)> {
        let adapter = registry.get(self.task).ok_or_else(|| {
            AosError::Validation(format!("no telemetry LoRA for {}", self.task.as_str()))
        })?;
        let composed_weights = adapter.composed_weights(weights, self.alpha)?;
        let composed_bias = adapter.composed_bias(bias, self.alpha)?;
        Ok((composed_weights, composed_bias))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use safetensors::serialize;

    #[test]
    fn test_merge_plan() {
        let weights = TelemetryLoraWeights::new(
            TelemetryTask::Monitoring,
            vec![1.0, 2.0, 3.0],
            vec![0.1, 0.2, 0.3],
        )
        .unwrap();
        let mut registry = TelemetryLoraRegistry::default();
        registry.insert(weights);

        let base = vec![0.0f32; 3];
        let bias = vec![0.0f32; 3];
        let plan = TelemetryMergePlan {
            task: TelemetryTask::Monitoring,
            alpha: 0.5,
        };
        let (composed_weights, composed_bias) = plan.apply(&registry, &base, &bias).unwrap();

        assert_eq!(base, vec![0.0, 0.0, 0.0]);
        assert_eq!(bias, vec![0.0, 0.0, 0.0]);
        assert_eq!(composed_weights, vec![0.5, 1.0, 1.5]);
        assert_eq!(composed_bias, vec![0.05, 0.1, 0.15]);
    }

    #[test]
    fn test_load_from_safetensors() {
        let weights = [1.0f32, 2.0, 3.0];
        let bias = [0.5f32, 0.6, 0.7];
        let tensors = [
            (
                "telemetry_lora.weight".to_string(),
                TensorView::new(safetensors::Dtype::F32, vec![3, 1], unsafe {
                    std::slice::from_raw_parts(weights.as_ptr() as *const u8, weights.len() * 4)
                })
                .unwrap(),
            ),
            (
                "telemetry_lora.bias".to_string(),
                TensorView::new(safetensors::Dtype::F32, vec![3], unsafe {
                    std::slice::from_raw_parts(bias.as_ptr() as *const u8, bias.len() * 4)
                })
                .unwrap(),
            ),
        ];
        let serialized = serialize(tensors, Default::default()).unwrap();

        let weights = load_telemetry_lora(&serialized, TelemetryTask::Control).unwrap();
        assert_eq!(Arc::as_ref(&weights.weights)[2], 3.0);
        assert_eq!(weights.task(), TelemetryTask::Control);
    }

    #[test]
    fn test_composed_paths_do_not_mutate_base() {
        let weights = TelemetryLoraWeights::new(
            TelemetryTask::Forecasting,
            vec![0.25, -0.5, 1.0],
            vec![0.1, 0.2, -0.3],
        )
        .unwrap();
        let base_weights = vec![1.0f32, 2.0, 3.0];
        let base_bias = vec![0.0f32, 0.0, 0.0];

        let composed_w = weights.composed_weights(&base_weights, 0.2).unwrap();
        let composed_b = weights.composed_bias(&base_bias, 0.2).unwrap();

        assert_eq!(base_weights, vec![1.0, 2.0, 3.0], "base weights mutated");
        assert_eq!(base_bias, vec![0.0, 0.0, 0.0], "base bias mutated");
        let expected_w = [1.05, 1.9, 3.2];
        for (got, exp) in composed_w.iter().zip(expected_w.iter()) {
            assert!(
                (got - exp).abs() < 1e-6,
                "composed weight mismatch: got {got}, expected {exp}"
            );
        }
        let expected_b = [0.02, 0.04, -0.06];
        for (got, exp) in composed_b.iter().zip(expected_b.iter()) {
            assert!(
                (got - exp).abs() < 1e-6,
                "composed bias mismatch: got {got}, expected {exp}"
            );
        }
    }
}
