//! Gradient accumulation and tracking

use adapteros_core::Result;
use ndarray::Array2;
use std::collections::HashMap;
use tracing::debug;

use crate::tensor::TensorId;

/// Gradient accumulator for managing gradient updates
#[derive(Debug)]
pub struct GradientAccumulator {
    /// Accumulated gradients by tensor ID
    gradients: HashMap<TensorId, Array2<f32>>,
    /// Gradient step count
    step_count: u64,
}

impl GradientAccumulator {
    /// Create a new gradient accumulator
    pub fn new() -> Self {
        Self {
            gradients: HashMap::new(),
            step_count: 0,
        }
    }

    /// Accumulate gradient for a tensor
    pub fn accumulate(&mut self, tensor_id: TensorId, grad: Array2<f32>) -> Result<()> {
        match self.gradients.get_mut(&tensor_id) {
            Some(existing_grad) => {
                *existing_grad += &grad;
            }
            None => {
                self.gradients.insert(tensor_id, grad);
            }
        }

        debug!("Accumulated gradient for tensor {}", tensor_id.0);
        Ok(())
    }

    /// Get gradient for a tensor
    pub fn get_gradient(&self, tensor_id: TensorId) -> Option<&Array2<f32>> {
        self.gradients.get(&tensor_id)
    }

    /// Get mutable gradient for a tensor
    pub fn get_gradient_mut(&mut self, tensor_id: TensorId) -> Option<&mut Array2<f32>> {
        self.gradients.get_mut(&tensor_id)
    }

    /// Check if tensor has accumulated gradient
    pub fn has_gradient(&self, tensor_id: TensorId) -> bool {
        self.gradients.contains_key(&tensor_id)
    }

    /// Clear all gradients
    pub fn clear(&mut self) {
        self.gradients.clear();
        self.step_count = 0;
        debug!("Cleared all accumulated gradients");
    }

    /// Get gradient count
    pub fn gradient_count(&self) -> usize {
        self.gradients.len()
    }

    /// Get step count
    pub fn step_count(&self) -> u64 {
        self.step_count
    }

    /// Increment step count
    pub fn increment_step(&mut self) {
        self.step_count += 1;
    }

    /// Get all tensor IDs with gradients
    pub fn tensor_ids(&self) -> Vec<TensorId> {
        self.gradients.keys().cloned().collect()
    }

    /// Remove gradient for a tensor
    pub fn remove_gradient(&mut self, tensor_id: TensorId) -> Option<Array2<f32>> {
        self.gradients.remove(&tensor_id)
    }

    /// Scale all gradients by a factor
    pub fn scale_gradients(&mut self, scale: f32) {
        for grad in self.gradients.values_mut() {
            *grad *= scale;
        }
        debug!("Scaled all gradients by {}", scale);
    }

    /// Clip gradients by norm
    pub fn clip_gradients(&mut self, max_norm: f32) -> Result<f32> {
        let mut total_norm = 0.0;

        // Compute total norm
        for grad in self.gradients.values() {
            total_norm += grad.iter().map(|&x| x * x).sum::<f32>();
        }
        total_norm = total_norm.sqrt();

        // Clip if necessary
        if total_norm > max_norm {
            let clip_factor = max_norm / total_norm;
            self.scale_gradients(clip_factor);
            debug!("Clipped gradients: norm {} -> {}", total_norm, max_norm);
        }

        Ok(total_norm)
    }
}

impl Default for GradientAccumulator {
    fn default() -> Self {
        Self::new()
    }
}

/// Gradient tracker for monitoring gradient flow
#[derive(Debug)]
pub struct GradientTracker {
    /// Gradient statistics by tensor ID
    stats: HashMap<TensorId, GradientStats>,
    /// Global gradient statistics
    global_stats: GlobalGradientStats,
}

/// Gradient statistics for a tensor
#[derive(Debug, Clone)]
pub struct GradientStats {
    /// Number of gradient updates
    update_count: u64,
    /// Sum of gradient norms
    norm_sum: f32,
    /// Maximum gradient norm
    max_norm: f32,
    /// Minimum gradient norm
    min_norm: f32,
    /// Last gradient norm
    last_norm: f32,
}

impl GradientStats {
    /// Create new gradient statistics
    pub fn new() -> Self {
        Self {
            update_count: 0,
            norm_sum: 0.0,
            max_norm: 0.0,
            min_norm: f32::INFINITY,
            last_norm: 0.0,
        }
    }

    /// Update statistics with new gradient
    pub fn update(&mut self, grad: &Array2<f32>) {
        let norm = grad.iter().map(|&x| x * x).sum::<f32>().sqrt();

        self.update_count += 1;
        self.norm_sum += norm;
        self.max_norm = self.max_norm.max(norm);
        self.min_norm = self.min_norm.min(norm);
        self.last_norm = norm;
    }

    /// Get average gradient norm
    pub fn avg_norm(&self) -> f32 {
        if self.update_count > 0 {
            self.norm_sum / self.update_count as f32
        } else {
            0.0
        }
    }

    /// Get update count
    pub fn update_count(&self) -> u64 {
        self.update_count
    }

    /// Get maximum gradient norm
    pub fn max_norm(&self) -> f32 {
        self.max_norm
    }

    /// Get minimum gradient norm
    pub fn min_norm(&self) -> f32 {
        if self.min_norm == f32::INFINITY {
            0.0
        } else {
            self.min_norm
        }
    }

    /// Get last gradient norm
    pub fn last_norm(&self) -> f32 {
        self.last_norm
    }
}

impl Default for GradientStats {
    fn default() -> Self {
        Self::new()
    }
}

/// Global gradient statistics
#[derive(Debug, Clone)]
pub struct GlobalGradientStats {
    /// Total gradient updates
    total_updates: u64,
    /// Total gradient norm sum
    total_norm_sum: f32,
    /// Global maximum gradient norm
    global_max_norm: f32,
    /// Global minimum gradient norm
    global_min_norm: f32,
}

impl GlobalGradientStats {
    /// Create new global gradient statistics
    pub fn new() -> Self {
        Self {
            total_updates: 0,
            total_norm_sum: 0.0,
            global_max_norm: 0.0,
            global_min_norm: f32::INFINITY,
        }
    }

    /// Update global statistics
    pub fn update(&mut self, norm: f32) {
        self.total_updates += 1;
        self.total_norm_sum += norm;
        self.global_max_norm = self.global_max_norm.max(norm);
        self.global_min_norm = self.global_min_norm.min(norm);
    }

    /// Get global average gradient norm
    pub fn avg_norm(&self) -> f32 {
        if self.total_updates > 0 {
            self.total_norm_sum / self.total_updates as f32
        } else {
            0.0
        }
    }

    /// Get total updates
    pub fn total_updates(&self) -> u64 {
        self.total_updates
    }

    /// Get global maximum gradient norm
    pub fn global_max_norm(&self) -> f32 {
        self.global_max_norm
    }

    /// Get global minimum gradient norm
    pub fn global_min_norm(&self) -> f32 {
        if self.global_min_norm == f32::INFINITY {
            0.0
        } else {
            self.global_min_norm
        }
    }
}

impl Default for GlobalGradientStats {
    fn default() -> Self {
        Self::new()
    }
}

impl GradientTracker {
    /// Create a new gradient tracker
    pub fn new() -> Self {
        Self {
            stats: HashMap::new(),
            global_stats: GlobalGradientStats::new(),
        }
    }

    /// Track gradient update for a tensor
    pub fn track_gradient(&mut self, tensor_id: TensorId, grad: &Array2<f32>) {
        let norm = grad.iter().map(|&x| x * x).sum::<f32>().sqrt();

        // Update tensor-specific stats
        let tensor_stats = self.stats.entry(tensor_id).or_default();
        tensor_stats.update(grad);

        // Update global stats
        self.global_stats.update(norm);

        debug!(
            "Tracked gradient for tensor {}: norm={:.6}",
            tensor_id.0, norm
        );
    }

    /// Get gradient statistics for a tensor
    pub fn get_stats(&self, tensor_id: TensorId) -> Option<&GradientStats> {
        self.stats.get(&tensor_id)
    }

    /// Get global gradient statistics
    pub fn get_global_stats(&self) -> &GlobalGradientStats {
        &self.global_stats
    }

    /// Get all tensor IDs with statistics
    pub fn tensor_ids(&self) -> Vec<TensorId> {
        self.stats.keys().cloned().collect()
    }

    /// Clear all statistics
    pub fn clear(&mut self) {
        self.stats.clear();
        self.global_stats = GlobalGradientStats::new();
        debug!("Cleared all gradient statistics");
    }

    /// Get statistics count
    pub fn stats_count(&self) -> usize {
        self.stats.len()
    }
}

impl Default for GradientTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ndarray::array;

    #[test]
    fn test_gradient_accumulator() {
        let mut accumulator = GradientAccumulator::new();
        let tensor_id = TensorId(0);
        let grad1 = array![[1.0, 2.0], [3.0, 4.0]];
        let grad2 = array![[0.1, 0.2], [0.3, 0.4]];

        accumulator.accumulate(tensor_id, grad1.clone()).unwrap();
        accumulator.accumulate(tensor_id, grad2.clone()).unwrap();

        let result = accumulator.get_gradient(tensor_id).unwrap();
        let expected = &grad1 + &grad2;
        assert_eq!(result, &expected);
    }

    #[test]
    fn test_gradient_tracker() {
        let mut tracker = GradientTracker::new();
        let tensor_id = TensorId(0);
        let grad = array![[1.0, 2.0], [3.0, 4.0]];

        tracker.track_gradient(tensor_id, &grad);

        let stats = tracker.get_stats(tensor_id).unwrap();
        assert_eq!(stats.update_count(), 1);
        assert!(stats.last_norm() > 0.0);

        let global_stats = tracker.get_global_stats();
        assert_eq!(global_stats.total_updates(), 1);
    }

    #[test]
    fn test_gradient_clipping() {
        let mut accumulator = GradientAccumulator::new();
        let tensor_id = TensorId(0);
        let grad = array![[10.0, 20.0], [30.0, 40.0]];

        accumulator.accumulate(tensor_id, grad).unwrap();
        let norm = accumulator.clip_gradients(1.0).unwrap();

        assert!(norm > 1.0); // Original norm was large
        let clipped_grad = accumulator.get_gradient(tensor_id).unwrap();
        let clipped_norm = clipped_grad.iter().map(|&x| x * x).sum::<f32>().sqrt();
        assert!(clipped_norm <= 1.0); // Clipped norm should be <= 1.0
    }
}
