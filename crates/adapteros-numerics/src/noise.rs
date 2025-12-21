use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

/// Represents a tensor for numerical computation
/// This is a simplified tensor representation for error measurement
#[derive(Debug, Clone)]
pub struct Tensor {
    pub data: Vec<f32>,
    pub shape: Vec<usize>,
}

impl Tensor {
    pub fn new(data: Vec<f32>, shape: Vec<usize>) -> Self {
        Self { data, shape }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, f32> {
        self.data.iter()
    }
}

/// Error statistics for a single kernel layer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpsilonStats {
    /// Unique identifier for the kernel layer
    pub layer_id: String,
    /// L2 norm of the error vector between reference and quantized outputs
    pub l2_error: f64,
    /// Maximum absolute error across all elements
    pub max_error: f64,
    /// Mean absolute error
    pub mean_error: f64,
    /// Number of elements compared
    pub element_count: usize,
    /// Timestamp when measurement was taken
    pub timestamp: u64,
}

impl EpsilonStats {
    pub fn new(
        layer_id: String,
        l2_error: f64,
        max_error: f64,
        mean_error: f64,
        element_count: usize,
    ) -> Self {
        Self {
            layer_id,
            l2_error,
            max_error,
            mean_error,
            element_count,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        }
    }

    /// Check if the error exceeds the specified threshold
    pub fn exceeds_threshold(&self, threshold: f64) -> bool {
        self.l2_error > threshold
    }

    /// Get error rate as percentage of reference range
    pub fn error_rate(&self, reference_range: f64) -> f64 {
        if reference_range == 0.0 {
            0.0
        } else {
            (self.l2_error / reference_range) * 100.0
        }
    }
}

/// Global stability report aggregating statistics across all layers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalStabilityReport {
    /// Per-layer error statistics
    pub layer_stats: HashMap<String, EpsilonStats>,
    /// Overall system stability metrics
    pub total_l2_error: f64,
    pub max_layer_error: f64,
    pub mean_layer_error: f64,
    /// Number of layers measured
    pub layer_count: usize,
    /// Total number of elements processed
    pub total_elements: usize,
    /// Report generation timestamp
    pub generated_at: u64,
    /// Whether any layer exceeded the error threshold
    pub threshold_violations: Vec<String>,
}

impl Default for GlobalStabilityReport {
    fn default() -> Self {
        Self::new()
    }
}

impl GlobalStabilityReport {
    pub fn new() -> Self {
        Self {
            layer_stats: HashMap::new(),
            total_l2_error: 0.0,
            max_layer_error: 0.0,
            mean_layer_error: 0.0,
            layer_count: 0,
            total_elements: 0,
            generated_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            threshold_violations: Vec::new(),
        }
    }

    /// Add a layer's statistics to the report
    pub fn add_layer_stats(&mut self, stats: EpsilonStats) {
        self.layer_count += 1;
        self.total_elements += stats.element_count;

        // Update running totals
        self.total_l2_error += stats.l2_error;
        self.max_layer_error = self.max_layer_error.max(stats.max_error);

        // Update mean
        self.mean_layer_error = self.total_l2_error / self.layer_count as f64;

        // Check for threshold violations
        if stats.exceeds_threshold(1e-6) {
            // Default threshold: 1e-6
            self.threshold_violations.push(stats.layer_id.clone());
        }

        self.layer_stats.insert(stats.layer_id.clone(), stats);
    }

    /// Get stability score (0.0 = perfect, higher = more error)
    pub fn stability_score(&self) -> f64 {
        if self.layer_count == 0 {
            return 0.0;
        }

        // Weighted score based on L2 error and max error
        let l2_component = self.total_l2_error / self.layer_count as f64;
        let max_component = self.max_layer_error;

        l2_component + (max_component * 0.1) // Max error gets 10% weight
    }

    /// Check if the system meets stability requirements
    pub fn is_stable(&self, threshold: f64) -> bool {
        self.threshold_violations.is_empty() && self.stability_score() <= threshold
    }
}

/// Errors that can occur during numerical measurement
#[derive(Error, Debug)]
pub enum NumericsError {
    #[error("Tensor dimension mismatch: expected {expected}, got {actual}")]
    DimensionMismatch { expected: usize, actual: usize },

    #[error("Tensor shape mismatch: expected {expected:?}, got {actual:?}")]
    ShapeMismatch {
        expected: Vec<usize>,
        actual: Vec<usize>,
    },

    #[error("Empty tensor provided")]
    EmptyTensor,

    #[error("Numerical overflow in computation")]
    NumericalOverflow,

    #[error("Invalid layer ID: {layer_id}")]
    InvalidLayerId { layer_id: String },
}

/// Measure numerical error between reference and quantized tensor outputs
///
/// This function computes the L2 norm of the difference vector and other
/// error metrics between reference (high-precision) and quantized outputs.
///
/// # Arguments
/// * `ref_out` - Reference tensor output (high precision)
/// * `quant_out` - Quantized tensor output
/// * `layer_id` - Identifier for the kernel layer
///
/// # Returns
/// * `Result<EpsilonStats, NumericsError>` - Error statistics or error
///
/// # Example
/// ```rust
/// use adapteros_numerics::noise::{Tensor, measure_error};
///
/// let ref_tensor = Tensor::new(vec![1.0, 2.0, 3.0], vec![3]);
/// let quant_tensor = Tensor::new(vec![1.01, 1.99, 3.01], vec![3]);
///
/// let stats = measure_error(&ref_tensor, &quant_tensor, "attention_layer".to_string()).unwrap();
/// println!("L2 error: {}", stats.l2_error);
/// ```
pub fn measure_error(
    ref_out: &Tensor,
    quant_out: &Tensor,
    layer_id: String,
) -> Result<EpsilonStats, NumericsError> {
    // Validate inputs
    if ref_out.data.is_empty() || quant_out.data.is_empty() {
        return Err(NumericsError::EmptyTensor);
    }

    if ref_out.len() != quant_out.len() {
        return Err(NumericsError::DimensionMismatch {
            expected: ref_out.len(),
            actual: quant_out.len(),
        });
    }

    if ref_out.shape != quant_out.shape {
        return Err(NumericsError::ShapeMismatch {
            expected: ref_out.shape.clone(),
            actual: quant_out.shape.clone(),
        });
    }

    if layer_id.is_empty() {
        return Err(NumericsError::InvalidLayerId { layer_id });
    }

    let element_count = ref_out.len();
    let mut l2_error_squared = 0.0f64;
    let mut max_error = 0.0f64;
    let mut total_error = 0.0f64;

    // Compute error metrics
    for (ref_val, quant_val) in ref_out.iter().zip(quant_out.iter()) {
        let error = (ref_val - quant_val).abs() as f64;

        // Check for numerical overflow
        if error.is_infinite() || error.is_nan() {
            return Err(NumericsError::NumericalOverflow);
        }

        l2_error_squared += error * error;
        max_error = max_error.max(error);
        total_error += error;
    }

    let l2_error = l2_error_squared.sqrt();
    let mean_error = total_error / element_count as f64;

    Ok(EpsilonStats::new(
        layer_id,
        l2_error,
        max_error,
        mean_error,
        element_count,
    ))
}

/// Aggregate multiple epsilon statistics into a global stability report
///
/// This function combines error statistics from multiple kernel layers
/// into a comprehensive stability report for the entire system.
///
/// # Arguments
/// * `stats` - Vector of epsilon statistics from different layers
///
/// # Returns
/// * `GlobalStabilityReport` - Aggregated stability report
///
/// # Example
/// ```rust
/// use adapteros_numerics::noise::{EpsilonStats, aggregate_stats};
///
/// let stats = vec![
///     EpsilonStats::new("layer1".to_string(), 0.001, 0.01, 0.005, 1000),
///     EpsilonStats::new("layer2".to_string(), 0.002, 0.02, 0.008, 2000),
/// ];
///
/// let report = aggregate_stats(&stats);
/// println!("Total L2 error: {}", report.total_l2_error);
/// ```
pub fn aggregate_stats(stats: &[EpsilonStats]) -> GlobalStabilityReport {
    let mut report = GlobalStabilityReport::new();

    for stat in stats {
        report.add_layer_stats(stat.clone());
    }

    report
}

/// Compute the reference range for error rate calculation
///
/// This function calculates the range (max - min) of the reference tensor
/// to provide context for error measurements.
///
/// # Arguments
/// * `tensor` - Reference tensor
///
/// # Returns
/// * `f64` - Range of the tensor values
pub fn compute_reference_range(tensor: &Tensor) -> f64 {
    if tensor.data.is_empty() {
        return 0.0;
    }

    let min_val = tensor.data.iter().fold(f32::INFINITY, |a, &b| a.min(b));
    let max_val = tensor.data.iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b));

    if min_val.is_infinite() || max_val.is_infinite() {
        0.0
    } else {
        (max_val - min_val) as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_measure_error_basic() {
        let ref_tensor = Tensor::new(vec![1.0, 2.0, 3.0], vec![3]);
        let quant_tensor = Tensor::new(vec![1.01, 1.99, 3.01], vec![3]);

        let stats = measure_error(&ref_tensor, &quant_tensor, "test_layer".to_string()).unwrap();

        assert_eq!(stats.layer_id, "test_layer");
        assert_eq!(stats.element_count, 3);
        assert!(stats.l2_error > 0.0);
        assert!(stats.max_error > 0.0);
        assert!(stats.mean_error > 0.0);
    }

    #[test]
    fn test_measure_error_identical() {
        let ref_tensor = Tensor::new(vec![1.0, 2.0, 3.0], vec![3]);
        let quant_tensor = Tensor::new(vec![1.0, 2.0, 3.0], vec![3]);

        let stats = measure_error(&ref_tensor, &quant_tensor, "test_layer".to_string()).unwrap();

        assert_eq!(stats.l2_error, 0.0);
        assert_eq!(stats.max_error, 0.0);
        assert_eq!(stats.mean_error, 0.0);
    }

    #[test]
    fn test_measure_error_dimension_mismatch() {
        let ref_tensor = Tensor::new(vec![1.0, 2.0], vec![2]);
        let quant_tensor = Tensor::new(vec![1.0, 2.0, 3.0], vec![3]);

        let result = measure_error(&ref_tensor, &quant_tensor, "test_layer".to_string());
        assert!(matches!(
            result,
            Err(NumericsError::DimensionMismatch { .. })
        ));
    }

    #[test]
    fn test_aggregate_stats() {
        let stats = vec![
            EpsilonStats::new("layer1".to_string(), 0.001, 0.01, 0.005, 1000),
            EpsilonStats::new("layer2".to_string(), 0.002, 0.02, 0.008, 2000),
        ];

        let report = aggregate_stats(&stats);

        assert_eq!(report.layer_count, 2);
        assert_eq!(report.total_elements, 3000);
        assert_eq!(report.total_l2_error, 0.003);
        assert_eq!(report.max_layer_error, 0.02);
    }

    #[test]
    fn test_compute_reference_range() {
        let tensor = Tensor::new(vec![1.0, 5.0, 3.0], vec![3]);
        let range = compute_reference_range(&tensor);
        assert_eq!(range, 4.0);
    }

    #[test]
    fn test_epsilon_stats_threshold() {
        let stats = EpsilonStats::new("test".to_string(), 1e-7, 1e-6, 1e-7, 1000);
        assert!(!stats.exceeds_threshold(1e-6));
        assert!(stats.exceeds_threshold(1e-8));
    }

    #[test]
    fn test_global_stability_report() {
        let mut report = GlobalStabilityReport::new();
        let stats = EpsilonStats::new("layer1".to_string(), 0.001, 0.01, 0.005, 1000);

        report.add_layer_stats(stats);

        assert_eq!(report.layer_count, 1);
        assert_eq!(report.total_elements, 1000);
        assert_eq!(report.total_l2_error, 0.001);
        assert_eq!(report.max_layer_error, 0.01);
    }
}
