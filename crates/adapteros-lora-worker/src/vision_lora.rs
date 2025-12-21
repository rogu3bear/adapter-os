//! LoRA integration utilities for the vision adapter.
//!
//! The worker runtime stores LoRA weights in lightweight `Arc<Vec<f32>>`
//! containers so that multiple adapters can share the same memory mapped
//! buffers.  The helpers in this module focus on deterministic merging of
//! adapter weights for different vision tasks.

use std::collections::HashMap;
use std::convert::TryInto;
use std::sync::Arc;

use adapteros_core::{AosError, Result};
use safetensors::{tensor::TensorView, SafeTensors};

/// Vision task type used to group LoRA weights.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VisionTask {
    Classification,
    Detection,
    Segmentation,
    Multimodal,
}

impl VisionTask {
    fn as_str(&self) -> &'static str {
        match self {
            VisionTask::Classification => "classification",
            VisionTask::Detection => "detection",
            VisionTask::Segmentation => "segmentation",
            VisionTask::Multimodal => "multimodal",
        }
    }
}

/// Memory efficient container for LoRA weights.
#[derive(Debug, Clone)]
pub struct VisionLoraWeights {
    task: VisionTask,
    #[allow(dead_code)]
    rank: usize,
    channels: usize,
    weights: Arc<Vec<f32>>,
    biases: Arc<Vec<f32>>,
}

impl VisionLoraWeights {
    /// Construct new weights from the provided components.
    pub fn new(
        task: VisionTask,
        rank: usize,
        channels: usize,
        weights: Vec<f32>,
        biases: Vec<f32>,
    ) -> Result<Self> {
        if weights.is_empty() {
            return Err(AosError::Validation("empty LoRA weight matrix".into()));
        }
        if biases.len() != channels {
            return Err(AosError::Validation(format!(
                "expected {} bias values, found {}",
                channels,
                biases.len()
            )));
        }

        Ok(Self {
            task,
            rank,
            channels,
            weights: Arc::new(weights),
            biases: Arc::new(biases),
        })
    }

    /// Produce a composed weight buffer without mutating the provided base slice.
    ///
    /// The base model weights are treated as immutable; callers receive a new
    /// buffer that includes the LoRA deltas scaled by `alpha`.
    pub fn composed_weights(&self, base: &[f32], alpha: f32) -> Result<Vec<f32>> {
        if base.len() != self.weights.len() {
            return Err(AosError::Validation(format!(
                "base weight buffer has {} elements but {} expected",
                base.len(),
                self.weights.len()
            )));
        }

        // Invariant: base model weights are immutable after load. Work on a scratch
        // copy so shared base buffers (Arc in the cache) are never mutated.
        let mut scratch = base.to_vec();
        for (target, delta) in scratch.iter_mut().zip(Arc::as_ref(&self.weights).iter()) {
            *target += delta * alpha;
        }

        Ok(scratch)
    }

    /// Produce a composed bias buffer without mutating the provided base slice.
    pub fn composed_bias(&self, base: &[f32], alpha: f32) -> Result<Vec<f32>> {
        if base.len() != self.channels {
            return Err(AosError::Validation(format!(
                "expected {} bias entries, found {}",
                self.channels,
                base.len()
            )));
        }

        let mut scratch = base.to_vec();
        for (target, delta) in scratch.iter_mut().zip(Arc::as_ref(&self.biases).iter()) {
            *target += delta * alpha;
        }

        Ok(scratch)
    }

    /// Access underlying weights.
    pub fn weights(&self) -> Arc<Vec<f32>> {
        self.weights.clone()
    }

    /// Access underlying biases.
    pub fn biases(&self) -> Arc<Vec<f32>> {
        self.biases.clone()
    }

    /// Associated task.
    pub fn task(&self) -> VisionTask {
        self.task
    }
}

/// Collection of LoRA weights indexed by task name.
#[derive(Debug, Default)]
pub struct VisionLoraRegistry {
    weights: HashMap<VisionTask, VisionLoraWeights>,
}

impl VisionLoraRegistry {
    pub fn insert(&mut self, weights: VisionLoraWeights) {
        self.weights.insert(weights.task(), weights);
    }

    pub fn get(&self, task: VisionTask) -> Option<&VisionLoraWeights> {
        self.weights.get(&task)
    }

    pub fn is_empty(&self) -> bool {
        self.weights.is_empty()
    }
}

/// Load LoRA weights from a safetensors file.
pub fn load_vision_lora(
    bytes: &[u8],
    task: VisionTask,
    channels: usize,
) -> Result<VisionLoraWeights> {
    let tensors = SafeTensors::deserialize(bytes)
        .map_err(|e| AosError::Validation(format!("invalid safetensors: {e}")))?;

    let weight = extract_tensor(&tensors, "vision_lora.weight")?;
    let bias = extract_tensor(&tensors, "vision_lora.bias")?;

    let rank = weight.shape().first().copied().unwrap_or(1) as usize;
    let weights: Vec<f32> = weight
        .data()
        .chunks_exact(std::mem::size_of::<f32>())
        .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap()))
        .collect();

    let biases: Vec<f32> = bias
        .data()
        .chunks_exact(std::mem::size_of::<f32>())
        .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap()))
        .collect();

    VisionLoraWeights::new(task, rank, channels, weights, biases)
}

fn extract_tensor<'a>(tensors: &'a SafeTensors<'a>, name: &str) -> Result<TensorView<'a>> {
    tensors
        .tensor(name)
        .map_err(|_| AosError::Validation(format!("tensor '{name}' missing from LoRA weights")))
}

/// Merge plan describing how a LoRA adapter should be applied.
#[derive(Debug, Clone)]
pub struct VisionMergePlan {
    pub task: VisionTask,
    pub alpha: f32,
}

impl VisionMergePlan {
    pub fn apply(
        &self,
        registry: &VisionLoraRegistry,
        base_weights: &[f32],
        base_bias: &[f32],
    ) -> Result<(Vec<f32>, Vec<f32>)> {
        let weights = registry.get(self.task).ok_or_else(|| {
            AosError::Validation(format!(
                "no vision LoRA registered for {}",
                self.task.as_str()
            ))
        })?;
        let composed_weights = weights.composed_weights(base_weights, self.alpha)?;
        let composed_bias = weights.composed_bias(base_bias, self.alpha)?;
        Ok((composed_weights, composed_bias))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use safetensors::serialize;
    use safetensors::tensor::TensorView;

    #[test]
    fn test_merge_plan_application() {
        let weights = vec![1.0f32, 2.0, 3.0, 4.0];
        let bias = vec![0.5, -0.25];
        let weights = VisionLoraWeights::new(
            VisionTask::Classification,
            2,
            2,
            weights.clone(),
            bias.clone(),
        )
        .unwrap();

        let base = vec![0.0f32; 4];
        let base_bias = vec![0.0f32; 2];

        let mut registry = VisionLoraRegistry::default();
        registry.insert(weights);

        let plan = VisionMergePlan {
            task: VisionTask::Classification,
            alpha: 0.5,
        };

        let (composed_weights, composed_bias) = plan.apply(&registry, &base, &base_bias).unwrap();

        assert_eq!(base, vec![0.0, 0.0, 0.0, 0.0]); // unchanged
        assert_eq!(base_bias, vec![0.0, 0.0]); // unchanged
        assert_eq!(composed_weights, vec![0.5, 1.0, 1.5, 2.0]);
        assert_eq!(composed_bias, vec![0.25, -0.125]);
    }

    #[test]
    fn test_load_from_safetensors() {
        let weights = vec![1.0f32, 2.0, 3.0, 4.0];
        let bias = vec![0.1f32, 0.2, 0.3, 0.4];
        let tensors = [
            (
                "vision_lora.weight".to_string(),
                TensorView::new(safetensors::Dtype::F32, vec![2, 2], unsafe {
                    std::slice::from_raw_parts(weights.as_ptr() as *const u8, weights.len() * 4)
                })
                .unwrap(),
            ),
            (
                "vision_lora.bias".to_string(),
                TensorView::new(safetensors::Dtype::F32, vec![4], unsafe {
                    std::slice::from_raw_parts(bias.as_ptr() as *const u8, bias.len() * 4)
                })
                .unwrap(),
            ),
        ];
        let serialized = serialize(tensors, &Default::default()).unwrap();

        let weights = load_vision_lora(&serialized, VisionTask::Detection, 4).unwrap();
        assert_eq!(Arc::as_ref(&weights.weights)[0], 1.0);
        assert_eq!(weights.task(), VisionTask::Detection);
    }

    #[test]
    fn test_composed_weights_do_not_mutate_base_model_bytes() {
        use adapteros_core::B3Hash;
        use safetensors::tensor::TensorView;

        // Build a tiny in-memory safetensors blob to avoid external fixtures. The
        // blob represents immutable base weights that must remain unchanged after
        // any LoRA composition.
        let base_floats = vec![0.1f32, -0.2, 0.3, 0.4];
        let tensor = TensorView::new(safetensors::Dtype::F32, vec![base_floats.len()], unsafe {
            std::slice::from_raw_parts(
                base_floats.as_ptr() as *const u8,
                base_floats.len() * std::mem::size_of::<f32>(),
            )
        })
        .unwrap();
        let base_bytes = safetensors::serialize(
            vec![("dummy.weight".to_string(), tensor)],
            &Default::default(),
        )
        .unwrap();
        let hash_before = B3Hash::hash(&base_bytes);

        let tensors = SafeTensors::deserialize(&base_bytes).expect("valid in-memory safetensors");
        let (_, first_tensor) = tensors
            .tensors()
            .into_iter()
            .next()
            .expect("model should expose at least one tensor");
        let base_floats: Vec<f32> = first_tensor
            .data()
            .chunks_exact(std::mem::size_of::<f32>())
            .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap()))
            .collect();

        let base_clone = base_floats.clone();
        let lora = VisionLoraWeights::new(
            VisionTask::Classification,
            1,
            base_floats.len(),
            vec![0.01f32; base_floats.len()],
            vec![0.0; base_floats.len()],
        )
        .unwrap();

        let _ = lora.composed_weights(&base_floats, 0.25).unwrap();
        assert_eq!(base_floats, base_clone, "base slice mutated unexpectedly");

        let hash_after = B3Hash::hash(&base_bytes);
        assert_eq!(hash_before, hash_after, "base model bytes changed");
    }
}
