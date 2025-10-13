//! Loss functions for autograd

use adapteros_core::Result;
use ndarray::{Array1, Array2, ArrayView2};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Type of loss function
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LossType {
    /// Mean squared error
    MSE,
    /// Cross-entropy loss
    CrossEntropy,
    /// Binary cross-entropy loss
    BinaryCrossEntropy,
    /// Huber loss (robust to outliers)
    Huber,
    /// Smooth L1 loss
    SmoothL1,
}

impl fmt::Display for LossType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LossType::MSE => write!(f, "MSE"),
            LossType::CrossEntropy => write!(f, "CrossEntropy"),
            LossType::BinaryCrossEntropy => write!(f, "BinaryCrossEntropy"),
            LossType::Huber => write!(f, "Huber"),
            LossType::SmoothL1 => write!(f, "SmoothL1"),
        }
    }
}

/// Loss function configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LossConfig {
    /// Loss function type
    pub loss_type: LossType,
    /// Reduction method
    pub reduction: Reduction,
    /// Label smoothing factor (for cross-entropy)
    pub label_smoothing: f32,
    /// Huber delta parameter
    pub huber_delta: f32,
    /// Smooth L1 beta parameter
    pub smooth_l1_beta: f32,
}

/// Reduction method for loss computation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Reduction {
    /// Mean reduction
    Mean,
    /// Sum reduction
    Sum,
    /// No reduction
    None,
}

impl fmt::Display for Reduction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Reduction::Mean => write!(f, "Mean"),
            Reduction::Sum => write!(f, "Sum"),
            Reduction::None => write!(f, "None"),
        }
    }
}

impl Default for LossConfig {
    fn default() -> Self {
        Self {
            loss_type: LossType::MSE,
            reduction: Reduction::Mean,
            label_smoothing: 0.0,
            huber_delta: 1.0,
            smooth_l1_beta: 1.0,
        }
    }
}

/// Loss function implementation
#[derive(Debug, Clone)]
pub struct LossFunction {
    config: LossConfig,
}

impl LossFunction {
    /// Create a new loss function
    pub fn new(config: LossConfig) -> Self {
        Self { config }
    }

    /// Create MSE loss function
    pub fn mse() -> Self {
        Self::new(LossConfig {
            loss_type: LossType::MSE,
            ..Default::default()
        })
    }

    /// Create cross-entropy loss function
    pub fn cross_entropy(label_smoothing: f32) -> Self {
        Self::new(LossConfig {
            loss_type: LossType::CrossEntropy,
            label_smoothing,
            ..Default::default()
        })
    }

    /// Create binary cross-entropy loss function
    pub fn binary_cross_entropy() -> Self {
        Self::new(LossConfig {
            loss_type: LossType::BinaryCrossEntropy,
            ..Default::default()
        })
    }

    /// Create Huber loss function
    pub fn huber(delta: f32) -> Self {
        Self::new(LossConfig {
            loss_type: LossType::Huber,
            huber_delta: delta,
            ..Default::default()
        })
    }

    /// Create smooth L1 loss function
    pub fn smooth_l1(beta: f32) -> Self {
        Self::new(LossConfig {
            loss_type: LossType::SmoothL1,
            smooth_l1_beta: beta,
            ..Default::default()
        })
    }

    /// Compute loss value
    pub fn compute_loss(
        &self,
        predictions: ArrayView2<f32>,
        targets: ArrayView2<f32>,
    ) -> Result<f32> {
        let loss_array = match self.config.loss_type {
            LossType::MSE => self.mse_loss(predictions, targets)?,
            LossType::CrossEntropy => self.cross_entropy_loss(predictions, targets)?,
            LossType::BinaryCrossEntropy => self.binary_cross_entropy_loss(predictions, targets)?,
            LossType::Huber => self.huber_loss(predictions, targets)?,
            LossType::SmoothL1 => self.smooth_l1_loss(predictions, targets)?,
        };

        let loss = match self.config.reduction {
            Reduction::Mean => loss_array.mean().unwrap_or(0.0),
            Reduction::Sum => loss_array.sum(),
            Reduction::None => loss_array.mean().unwrap_or(0.0), // For scalar loss
        };

        Ok(loss)
    }

    /// Compute loss gradient
    pub fn compute_gradient(
        &self,
        predictions: ArrayView2<f32>,
        targets: ArrayView2<f32>,
    ) -> Result<Array2<f32>> {
        match self.config.loss_type {
            LossType::MSE => self.mse_gradient(predictions, targets),
            LossType::CrossEntropy => self.cross_entropy_gradient(predictions, targets),
            LossType::BinaryCrossEntropy => {
                self.binary_cross_entropy_gradient(predictions, targets)
            }
            LossType::Huber => self.huber_gradient(predictions, targets),
            LossType::SmoothL1 => self.smooth_l1_gradient(predictions, targets),
        }
    }

    /// MSE loss computation
    fn mse_loss(
        &self,
        predictions: ArrayView2<f32>,
        targets: ArrayView2<f32>,
    ) -> Result<Array1<f32>> {
        let diff = &predictions - &targets;
        let squared_diff = &diff * &diff;
        Ok(squared_diff.mean_axis(ndarray::Axis(1)).unwrap())
    }

    /// MSE gradient computation
    fn mse_gradient(
        &self,
        predictions: ArrayView2<f32>,
        targets: ArrayView2<f32>,
    ) -> Result<Array2<f32>> {
        let diff = &predictions - &targets;
        let gradient = 2.0 * &diff;

        match self.config.reduction {
            Reduction::Mean => Ok(gradient / predictions.nrows() as f32),
            Reduction::Sum => Ok(gradient),
            Reduction::None => Ok(gradient),
        }
    }

    /// Cross-entropy loss computation
    fn cross_entropy_loss(
        &self,
        predictions: ArrayView2<f32>,
        targets: ArrayView2<f32>,
    ) -> Result<Array1<f32>> {
        // Apply softmax to predictions
        let softmax_pred = self.softmax(predictions);

        // Compute cross-entropy loss
        let log_probs = softmax_pred.mapv(|x| x.ln().max(-100.0)); // Clamp to avoid -inf
        let loss = -(&targets * &log_probs).sum_axis(ndarray::Axis(1));

        Ok(loss)
    }

    /// Cross-entropy gradient computation
    fn cross_entropy_gradient(
        &self,
        predictions: ArrayView2<f32>,
        targets: ArrayView2<f32>,
    ) -> Result<Array2<f32>> {
        // Apply softmax to predictions
        let softmax_pred = self.softmax(predictions);

        // Gradient is softmax_pred - targets
        let gradient = &softmax_pred - &targets;

        match self.config.reduction {
            Reduction::Mean => Ok(gradient / predictions.nrows() as f32),
            Reduction::Sum => Ok(gradient),
            Reduction::None => Ok(gradient),
        }
    }

    /// Binary cross-entropy loss computation
    fn binary_cross_entropy_loss(
        &self,
        predictions: ArrayView2<f32>,
        targets: ArrayView2<f32>,
    ) -> Result<Array1<f32>> {
        // Apply sigmoid to predictions
        let sigmoid_pred = predictions.mapv(|x| 1.0 / (1.0 + (-x).exp()));

        // Compute binary cross-entropy loss
        let eps = 1e-8; // Small epsilon to avoid log(0)
        let loss = -(&targets * &sigmoid_pred.mapv(|x| (x + eps).ln())
            + &(1.0 - &targets) * &(1.0 - &sigmoid_pred).mapv(|x| (x + eps).ln()));

        Ok(loss.sum_axis(ndarray::Axis(1)))
    }

    /// Binary cross-entropy gradient computation
    fn binary_cross_entropy_gradient(
        &self,
        predictions: ArrayView2<f32>,
        targets: ArrayView2<f32>,
    ) -> Result<Array2<f32>> {
        // Apply sigmoid to predictions
        let sigmoid_pred = predictions.mapv(|x| 1.0 / (1.0 + (-x).exp()));

        // Gradient is sigmoid_pred - targets
        let gradient = &sigmoid_pred - &targets;

        match self.config.reduction {
            Reduction::Mean => Ok(gradient / predictions.nrows() as f32),
            Reduction::Sum => Ok(gradient),
            Reduction::None => Ok(gradient),
        }
    }

    /// Huber loss computation
    fn huber_loss(
        &self,
        predictions: ArrayView2<f32>,
        targets: ArrayView2<f32>,
    ) -> Result<Array1<f32>> {
        let diff = &predictions - &targets;
        let abs_diff = diff.mapv(|x| x.abs());
        let delta = self.config.huber_delta;

        let loss = abs_diff.mapv(|x| {
            if x <= delta {
                0.5 * x * x
            } else {
                delta * (x - 0.5 * delta)
            }
        });

        Ok(loss.sum_axis(ndarray::Axis(1)))
    }

    /// Huber gradient computation
    fn huber_gradient(
        &self,
        predictions: ArrayView2<f32>,
        targets: ArrayView2<f32>,
    ) -> Result<Array2<f32>> {
        let diff = &predictions - &targets;
        let delta = self.config.huber_delta;

        let gradient = diff.mapv(|x| {
            let abs_x = x.abs();
            if abs_x <= delta {
                x
            } else {
                delta * x.signum()
            }
        });

        match self.config.reduction {
            Reduction::Mean => Ok(gradient / predictions.nrows() as f32),
            Reduction::Sum => Ok(gradient),
            Reduction::None => Ok(gradient),
        }
    }

    /// Smooth L1 loss computation
    fn smooth_l1_loss(
        &self,
        predictions: ArrayView2<f32>,
        targets: ArrayView2<f32>,
    ) -> Result<Array1<f32>> {
        let diff = &predictions - &targets;
        let abs_diff = diff.mapv(|x| x.abs());
        let beta = self.config.smooth_l1_beta;

        let loss = abs_diff.mapv(|x| {
            if x < beta {
                0.5 * x * x / beta
            } else {
                x - 0.5 * beta
            }
        });

        Ok(loss.sum_axis(ndarray::Axis(1)))
    }

    /// Smooth L1 gradient computation
    fn smooth_l1_gradient(
        &self,
        predictions: ArrayView2<f32>,
        targets: ArrayView2<f32>,
    ) -> Result<Array2<f32>> {
        let diff = &predictions - &targets;
        let beta = self.config.smooth_l1_beta;

        let gradient = diff.mapv(|x| {
            let abs_x = x.abs();
            if abs_x < beta {
                x / beta
            } else {
                x.signum()
            }
        });

        match self.config.reduction {
            Reduction::Mean => Ok(gradient / predictions.nrows() as f32),
            Reduction::Sum => Ok(gradient),
            Reduction::None => Ok(gradient),
        }
    }

    /// Softmax function
    fn softmax(&self, x: ArrayView2<f32>) -> Array2<f32> {
        let max_vals = x
            .map_axis(ndarray::Axis(1), |axis| {
                axis.iter().cloned().fold(f32::NEG_INFINITY, f32::max)
            })
            .insert_axis(ndarray::Axis(1));
        let exp_x = (x.to_owned() - &max_vals).mapv(|x| x.exp());
        let sum_exp = exp_x
            .sum_axis(ndarray::Axis(1))
            .insert_axis(ndarray::Axis(1));
        exp_x / &sum_exp
    }

    /// Get loss configuration
    pub fn config(&self) -> &LossConfig {
        &self.config
    }
}

impl fmt::Display for LossFunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "LossFunction({}, {})",
            self.config.loss_type, self.config.reduction
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ndarray::array;

    #[test]
    fn test_mse_loss() {
        let loss_fn = LossFunction::mse();
        let predictions = array![[1.0, 2.0], [3.0, 4.0]];
        let targets = array![[0.0, 1.0], [2.0, 3.0]];

        let loss = loss_fn
            .compute_loss(predictions.view(), targets.view())
            .unwrap();
        assert!(loss > 0.0);
    }

    #[test]
    fn test_cross_entropy_loss() {
        let loss_fn = LossFunction::cross_entropy(0.0);
        let predictions = array![[1.0, 2.0], [3.0, 4.0]];
        let targets = array![[0.0, 1.0], [1.0, 0.0]];

        let loss = loss_fn
            .compute_loss(predictions.view(), targets.view())
            .unwrap();
        assert!(loss > 0.0);
    }

    #[test]
    fn test_loss_gradient() {
        let loss_fn = LossFunction::mse();
        let predictions = array![[1.0, 2.0], [3.0, 4.0]];
        let targets = array![[0.0, 1.0], [2.0, 3.0]];

        let gradient = loss_fn
            .compute_gradient(predictions.view(), targets.view())
            .unwrap();
        assert_eq!(gradient.shape(), predictions.shape());
    }

    #[test]
    fn test_loss_config() {
        let config = LossConfig {
            loss_type: LossType::Huber,
            reduction: Reduction::Sum,
            huber_delta: 2.0,
            ..Default::default()
        };

        let loss_fn = LossFunction::new(config);
        assert_eq!(loss_fn.config().loss_type, LossType::Huber);
        assert_eq!(loss_fn.config().reduction, Reduction::Sum);
        assert_eq!(loss_fn.config().huber_delta, 2.0);
    }
}
