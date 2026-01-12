//! Perplexity computation for training evaluation.
//!
//! Perplexity (PPL) is the standard metric for language model evaluation.
//! It measures how "surprised" the model is by the test data - lower is better.
//!
//! Formula: PPL = exp(cross_entropy_loss)
//!
//! Interpretation:
//! - PPL ≈ 1: Perfect prediction (impossible in practice)
//! - PPL < 10: Very good model
//! - PPL 10-50: Good model
//! - PPL 50-100: Reasonable model
//! - PPL > 100: Model needs improvement

/// Compute perplexity from cross-entropy loss.
///
/// # Arguments
/// * `cross_entropy_loss` - The cross-entropy loss value
///
/// # Returns
/// Perplexity value (exp of loss). Returns f32::INFINITY for invalid inputs.
///
/// # Example
/// ```
/// use adapteros_lora_worker::training::perplexity::compute_perplexity;
///
/// let loss = 2.0;
/// let ppl = compute_perplexity(loss);
/// assert!((ppl - 7.389).abs() < 0.01); // exp(2) ≈ 7.389
/// ```
pub fn compute_perplexity(cross_entropy_loss: f32) -> f32 {
    if cross_entropy_loss.is_nan() || cross_entropy_loss.is_infinite() || cross_entropy_loss < 0.0 {
        return f32::INFINITY;
    }

    // Clamp to prevent overflow (exp(88) is near f32::MAX)
    let clamped_loss = cross_entropy_loss.clamp(0.0, 87.0);
    clamped_loss.exp()
}

/// Compute average perplexity from a loss curve.
///
/// # Arguments
/// * `loss_curve` - Vector of per-epoch loss values
///
/// # Returns
/// Average perplexity across all epochs, or f32::INFINITY if empty.
pub fn average_perplexity(loss_curve: &[f32]) -> f32 {
    if loss_curve.is_empty() {
        return f32::INFINITY;
    }

    let sum: f32 = loss_curve.iter().map(|&l| compute_perplexity(l)).sum();
    sum / loss_curve.len() as f32
}

/// Convert loss curve to perplexity curve.
///
/// # Arguments
/// * `loss_curve` - Vector of per-epoch loss values
///
/// # Returns
/// Vector of per-epoch perplexity values.
pub fn loss_to_perplexity_curve(loss_curve: &[f32]) -> Vec<f32> {
    loss_curve.iter().map(|&l| compute_perplexity(l)).collect()
}

/// Perplexity improvement metrics.
#[derive(Debug, Clone)]
pub struct PerplexityImprovement {
    /// Initial perplexity (first epoch)
    pub initial: f32,
    /// Final perplexity (last epoch)
    pub final_ppl: f32,
    /// Best perplexity achieved
    pub best: f32,
    /// Epoch where best perplexity was achieved (0-indexed)
    pub best_epoch: usize,
    /// Improvement ratio: initial / final (>1 means improvement)
    pub improvement_ratio: f32,
    /// Percentage improvement: (initial - final) / initial * 100
    pub improvement_percent: f32,
}

impl PerplexityImprovement {
    /// Compute improvement metrics from a perplexity curve.
    ///
    /// # Arguments
    /// * `ppl_curve` - Vector of per-epoch perplexity values
    ///
    /// # Returns
    /// None if curve is empty, otherwise improvement metrics.
    pub fn from_curve(ppl_curve: &[f32]) -> Option<Self> {
        if ppl_curve.is_empty() {
            return None;
        }

        let initial = ppl_curve[0];
        let final_ppl = *ppl_curve.last().unwrap();

        let (best_epoch, &best) = ppl_curve
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))?;

        let improvement_ratio = if final_ppl > 0.0 && final_ppl.is_finite() {
            initial / final_ppl
        } else {
            0.0
        };

        let improvement_percent = if initial > 0.0 && initial.is_finite() {
            ((initial - final_ppl) / initial) * 100.0
        } else {
            0.0
        };

        Some(Self {
            initial,
            final_ppl,
            best,
            best_epoch,
            improvement_ratio,
            improvement_percent,
        })
    }

    /// Compute improvement metrics from a loss curve.
    pub fn from_loss_curve(loss_curve: &[f32]) -> Option<Self> {
        let ppl_curve = loss_to_perplexity_curve(loss_curve);
        Self::from_curve(&ppl_curve)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_perplexity_computation() {
        // exp(0) = 1
        assert!((compute_perplexity(0.0) - 1.0).abs() < 0.001);

        // exp(1) = e
        assert!((compute_perplexity(1.0) - std::f32::consts::E).abs() < 0.01);

        // exp(2) ≈ 7.389
        assert!((compute_perplexity(2.0) - 7.389).abs() < 0.01);
    }

    #[test]
    fn test_perplexity_edge_cases() {
        // NaN returns infinity
        assert!(compute_perplexity(f32::NAN).is_infinite());

        // Infinity returns infinity
        assert!(compute_perplexity(f32::INFINITY).is_infinite());

        // Negative returns infinity
        assert!(compute_perplexity(-1.0).is_infinite());

        // Very large value is clamped (doesn't overflow)
        let ppl = compute_perplexity(100.0);
        assert!(ppl.is_finite());
        assert!(ppl > 0.0);
    }

    #[test]
    fn test_loss_to_perplexity_curve() {
        let losses = vec![2.0, 1.5, 1.0, 0.5];
        let ppls = loss_to_perplexity_curve(&losses);

        assert_eq!(ppls.len(), 4);

        // Perplexity should decrease as loss decreases
        for i in 1..ppls.len() {
            assert!(
                ppls[i] < ppls[i - 1],
                "PPL should decrease: {} < {}",
                ppls[i],
                ppls[i - 1]
            );
        }
    }

    #[test]
    fn test_improvement_metrics() {
        let losses = vec![3.0, 2.5, 2.0, 1.5, 1.0];
        let improvement = PerplexityImprovement::from_loss_curve(&losses).unwrap();

        // Initial perplexity should be exp(3.0)
        assert!((improvement.initial - 20.086).abs() < 0.01);

        // Final perplexity should be exp(1.0)
        assert!((improvement.final_ppl - std::f32::consts::E).abs() < 0.01);

        // Best should be final (since loss keeps decreasing)
        assert_eq!(improvement.best_epoch, 4);

        // Improvement ratio should be > 1 (improvement occurred)
        assert!(improvement.improvement_ratio > 1.0);

        // Improvement percent should be positive
        assert!(improvement.improvement_percent > 0.0);
    }
}
