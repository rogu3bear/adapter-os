use std::cmp::Ordering;
use std::sync::Arc;

use adapteros_core::AosError;

use crate::conv_pipeline::ConvArchitecture;

/// Supported downstream tasks for the vision adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VisionTask {
    ImageClassification,
    ObjectDetection,
    SemanticSegmentation,
    MultiModalRetrieval,
}

/// LoRA update for a single convolution layer. The weight delta is stored in a
/// compact [`Arc<[f32]>`] slice to avoid unnecessary cloning when switching
/// between tasks.
#[derive(Debug, Clone)]
pub struct LayerLoRA {
    pub layer_index: usize,
    pub weight_delta: Arc<[f32]>,
    pub bias_delta: Arc<[f32]>,
    pub scaling: f32,
}

impl LayerLoRA {
    pub fn new(
        layer_index: usize,
        weight_delta: Vec<f32>,
        bias_delta: Vec<f32>,
        scaling: f32,
    ) -> Self {
        Self {
            layer_index,
            weight_delta: Arc::from(weight_delta.into_boxed_slice()),
            bias_delta: Arc::from(bias_delta.into_boxed_slice()),
            scaling,
        }
    }
}

/// Vision-specific LoRA weights that can be merged into a convolution pipeline.
#[derive(Debug, Clone)]
pub struct VisionLoRAWeights {
    task: VisionTask,
    architecture: Option<ConvArchitecture>,
    updates: Vec<LayerLoRA>,
}

impl VisionLoRAWeights {
    pub fn new(task: VisionTask, mut updates: Vec<LayerLoRA>) -> Self {
        updates.sort_by(|a, b| a.layer_index.cmp(&b.layer_index));
        Self {
            task,
            architecture: None,
            updates,
        }
    }

    pub fn for_architecture(mut self, architecture: ConvArchitecture) -> Self {
        self.architecture = Some(architecture);
        self
    }

    pub fn task(&self) -> VisionTask {
        self.task
    }

    pub fn architecture(&self) -> Option<ConvArchitecture> {
        self.architecture
    }

    pub fn layer_updates(&self) -> &[LayerLoRA] {
        &self.updates
    }

    /// Merge two LoRA weight sets with deterministic ordering. Updates for the
    /// same layer are combined using scaling factors. This allows stacking
    /// adapters for related tasks (e.g., segmentation + detection).
    pub fn merge(
        &self,
        other: &VisionLoRAWeights,
    ) -> std::result::Result<VisionLoRAWeights, AosError> {
        if let (Some(a), Some(b)) = (self.architecture, other.architecture) {
            if a != b {
                return Err(AosError::Adapter(format!(
                    "architecture mismatch: {a:?} vs {b:?}",
                )));
            }
        }

        let mut merged = Vec::with_capacity(self.updates.len() + other.updates.len());
        let mut i = 0;
        let mut j = 0;

        while i < self.updates.len() && j < other.updates.len() {
            match self.updates[i]
                .layer_index
                .cmp(&other.updates[j].layer_index)
            {
                Ordering::Less => {
                    merged.push(self.updates[i].clone());
                    i += 1;
                }
                Ordering::Greater => {
                    merged.push(other.updates[j].clone());
                    j += 1;
                }
                Ordering::Equal => {
                    let lhs = &self.updates[i];
                    let rhs = &other.updates[j];
                    if lhs.weight_delta.len() != rhs.weight_delta.len()
                        || lhs.bias_delta.len() != rhs.bias_delta.len()
                    {
                        return Err(AosError::Adapter(format!(
                            "incompatible LoRA shapes for layer {}",
                            lhs.layer_index
                        )));
                    }
                    let mut weight = Vec::with_capacity(lhs.weight_delta.len());
                    let mut bias = Vec::with_capacity(lhs.bias_delta.len());
                    for (lw, rw) in lhs.weight_delta.iter().zip(rhs.weight_delta.iter()) {
                        weight.push(*lw + *rw);
                    }
                    for (lb, rb) in lhs.bias_delta.iter().zip(rhs.bias_delta.iter()) {
                        bias.push(*lb + *rb);
                    }
                    merged.push(LayerLoRA::new(
                        lhs.layer_index,
                        weight,
                        bias,
                        lhs.scaling + rhs.scaling,
                    ));
                    i += 1;
                    j += 1;
                }
            }
        }

        while i < self.updates.len() {
            merged.push(self.updates[i].clone());
            i += 1;
        }
        while j < other.updates.len() {
            merged.push(other.updates[j].clone());
            j += 1;
        }

        Ok(VisionLoRAWeights {
            task: self.task,
            architecture: self.architecture.or(other.architecture),
            updates: merged,
        })
    }

    /// Memory footprint of the LoRA payload in number of floating point values.
    pub fn parameter_count(&self) -> usize {
        self.updates
            .iter()
            .map(|update| update.weight_delta.len() + update.bias_delta.len())
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn updates_are_sorted_and_mergeable() {
        let a = VisionLoRAWeights::new(
            VisionTask::ImageClassification,
            vec![
                LayerLoRA::new(2, vec![0.1, 0.2], vec![0.05], 0.8),
                LayerLoRA::new(0, vec![0.3], vec![0.01], 0.5),
            ],
        );

        let b = VisionLoRAWeights::new(
            VisionTask::ImageClassification,
            vec![LayerLoRA::new(2, vec![0.4, 0.6], vec![0.02], 0.1)],
        );

        let merged = a.merge(&b).expect("merge succeeds");
        assert_eq!(merged.layer_updates()[0].layer_index, 0);
        assert_eq!(merged.layer_updates()[1].layer_index, 2);
        assert_eq!(merged.layer_updates()[1].weight_delta.len(), 2);
        assert_eq!(merged.parameter_count(), 1 + 2 + 1 + 1);
    }
}
