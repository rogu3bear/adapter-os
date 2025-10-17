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

    pub fn merge_into(&self, base: &mut [f32], alpha: f32) -> Result<()> {
        if base.len() != Arc::as_ref(&self.weights).len() {
            return Err(AosError::Validation(format!(
                "base buffer length {} does not match telemetry LoRA {}",
                base.len(),
                Arc::as_ref(&self.weights).len()
            )));
        }

        for (target, delta) in base.iter_mut().zip(Arc::as_ref(&self.weights).iter()) {
            *target += delta * alpha;
        }

        Ok(())
    }

    pub fn apply_bias(&self, base: &mut [f32], alpha: f32) -> Result<()> {
        if base.len() != Arc::as_ref(&self.bias).len() {
            return Err(AosError::Validation(format!(
                "bias buffer length {} does not match telemetry LoRA {}",
                base.len(),
                Arc::as_ref(&self.bias).len()
            )));
        }

        for (target, delta) in base.iter_mut().zip(Arc::as_ref(&self.bias).iter()) {
            *target += delta * alpha;
        }

        Ok(())
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
        weights: &mut [f32],
        bias: &mut [f32],
    ) -> Result<()> {
        let adapter = registry.get(self.task).ok_or_else(|| {
            AosError::Validation(format!("no telemetry LoRA for {}", self.task.as_str()))
        })?;
        adapter.merge_into(weights, self.alpha)?;
        adapter.apply_bias(bias, self.alpha)
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

        let mut base = vec![0.0f32; 3];
        let mut bias = vec![0.0f32; 3];
        let plan = TelemetryMergePlan {
            task: TelemetryTask::Monitoring,
            alpha: 0.5,
        };
        plan.apply(&registry, &mut base, &mut bias).unwrap();

        assert_eq!(base, vec![0.5, 1.0, 1.5]);
        assert_eq!(bias, vec![0.05, 0.1, 0.15]);
    }

    #[test]
    fn test_load_from_safetensors() {
        let weights = vec![1.0f32, 2.0, 3.0];
        let bias = vec![0.5f32, 0.6, 0.7];
        let tensors = [
            (
                "telemetry_lora.weight".to_string(),
                TensorView::new(safetensors::Dtype::F32, vec![3, 1], bytemuck::cast_slice(&weights)).unwrap(),
            ),
            (
                "telemetry_lora.bias".to_string(),
                TensorView::new(safetensors::Dtype::F32, vec![3], bytemuck::cast_slice(&bias)).unwrap(),
            ),
        ];
        let serialized = serialize(tensors, &Default::default()).unwrap();

        let weights = load_telemetry_lora(&serialized, TelemetryTask::Control).unwrap();
        assert_eq!(Arc::as_ref(&weights.weights)[2], 3.0);
        assert_eq!(weights.task(), TelemetryTask::Control);
    }
}
