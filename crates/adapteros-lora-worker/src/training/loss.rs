use adapteros_core::{AosError, Result};
use std::collections::HashSet;

use super::trainer::{LoRAWeights, TrainingConfig};

pub const LOSS_IGNORE_INDEX: i32 = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LossKind {
    CrossEntropy,
    LegacyMse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LossNormalization {
    MeanTokens,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LossLogitsSource {
    HiddenPlusLoraProjection,
    HiddenPlusLora,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LossSpec {
    pub kind: LossKind,
    pub normalization: LossNormalization,
    pub ignore_index: i32,
    pub logits_source: LossLogitsSource,
}

impl LossSpec {
    pub fn summary(&self) -> String {
        let kind = match self.kind {
            LossKind::CrossEntropy => "cross_entropy",
            LossKind::LegacyMse => "legacy_mse",
        };
        let logits = match self.logits_source {
            LossLogitsSource::HiddenPlusLoraProjection => "hidden_plus_lora@lm_head",
            LossLogitsSource::HiddenPlusLora => "hidden_plus_lora",
        };
        format!(
            "loss={} normalization=mean_tokens ignore_index={} logits={}",
            kind, self.ignore_index, logits
        )
    }

    pub fn diffs(&self, other: &Self) -> Vec<String> {
        let mut diffs = Vec::new();
        if self.kind != other.kind {
            diffs.push(format!("loss_kind: {:?} vs {:?}", self.kind, other.kind));
        }
        if self.normalization != other.normalization {
            diffs.push(format!(
                "normalization: {:?} vs {:?}",
                self.normalization, other.normalization
            ));
        }
        if self.ignore_index != other.ignore_index {
            diffs.push(format!(
                "ignore_index: {} vs {}",
                self.ignore_index, other.ignore_index
            ));
        }
        if self.logits_source != other.logits_source {
            diffs.push(format!(
                "logits_source: {:?} vs {:?}",
                self.logits_source, other.logits_source
            ));
        }
        diffs
    }
}

#[derive(Debug, Clone)]
pub struct LossReport {
    pub loss: f32,
    pub valid_tokens: usize,
    pub masked_tokens: usize,
    pub spec: LossSpec,
    pub warnings: Vec<String>,
}

pub fn training_loss_spec(ignore_index: i32) -> LossSpec {
    LossSpec {
        kind: LossKind::CrossEntropy,
        normalization: LossNormalization::MeanTokens,
        ignore_index,
        logits_source: LossLogitsSource::HiddenPlusLoraProjection,
    }
}

pub fn validation_loss_spec(ignore_index: i32) -> LossSpec {
    training_loss_spec(ignore_index)
}

pub fn legacy_training_loss_spec(ignore_index: i32) -> LossSpec {
    LossSpec {
        kind: LossKind::LegacyMse,
        normalization: LossNormalization::MeanTokens,
        ignore_index,
        logits_source: LossLogitsSource::HiddenPlusLora,
    }
}

pub fn legacy_validation_loss_spec(ignore_index: i32) -> LossSpec {
    legacy_training_loss_spec(ignore_index)
}

fn audit_targets(targets: &[u32], spec: &LossSpec) -> (usize, usize, Vec<String>) {
    let mut warnings = Vec::new();
    let mut masked_tokens = 0usize;
    if spec.ignore_index >= 0 {
        masked_tokens = targets
            .iter()
            .filter(|&&token| token as i32 == spec.ignore_index)
            .count();
        if masked_tokens > 0 {
            warnings.push(format!(
                "targets contain {} token(s) equal to ignore_index={}; those positions are ignored in loss",
                masked_tokens, spec.ignore_index
            ));
        }
    }
    let valid_tokens = targets.len().saturating_sub(masked_tokens);
    if valid_tokens == 0 {
        warnings.push("all target tokens are masked; loss is not comparable".to_string());
    }
    (valid_tokens, masked_tokens, warnings)
}

pub fn merge_loss_warnings(into: &mut HashSet<String>, report: &LossReport) {
    for warning in &report.warnings {
        into.insert(warning.clone());
    }
}

pub fn build_loss_report(loss: f32, spec: LossSpec, targets: &[u32]) -> LossReport {
    let (valid_tokens, masked_tokens, warnings) = audit_targets(targets, &spec);
    LossReport {
        loss,
        valid_tokens,
        masked_tokens,
        spec,
        warnings,
    }
}

#[cfg(feature = "multi-backend")]
fn build_logits(
    config: &TrainingConfig,
    weights: &LoRAWeights,
    hidden: &[f32],
    output_proj: &adapteros_lora_mlx_ffi::MLXFFITensor,
) -> Result<adapteros_lora_mlx_ffi::MLXFFITensor> {
    use adapteros_lora_mlx_ffi::MLXFFITensor;

    if hidden.len() != config.hidden_dim {
        return Err(AosError::Training(format!(
            "Hidden state length {} does not match hidden_dim {}",
            hidden.len(),
            config.hidden_dim
        )));
    }

    let rank = config.rank;
    let hidden_dim = config.hidden_dim;
    let alpha = config.alpha;
    let scale = alpha / rank as f32;

    let hidden_tensor = MLXFFITensor::from_data(hidden, vec![1, hidden_dim])?;

    let lora_a_flat: Vec<f32> = weights.lora_a.iter().flatten().copied().collect();
    let lora_b_flat: Vec<f32> = weights.lora_b.iter().flatten().copied().collect();
    let lora_a_tensor = MLXFFITensor::from_data(&lora_a_flat, vec![rank, hidden_dim])?;
    let lora_b_tensor = MLXFFITensor::from_data(&lora_b_flat, vec![hidden_dim, rank])?;

    let lora_intermediate = hidden_tensor.matmul(&lora_a_tensor.transpose()?)?;
    let lora_out = lora_intermediate.matmul(&lora_b_tensor.transpose()?)?;
    let scale_tensor = MLXFFITensor::from_data(&[scale], vec![1])?;
    let lora_scaled = lora_out.multiply(&scale_tensor)?;
    let h_prime = hidden_tensor.add(&lora_scaled)?;
    let logits = h_prime.matmul(&output_proj.transpose()?)?;

    Ok(logits)
}

#[cfg(feature = "multi-backend")]
pub fn compute_validation_loss_with_output_proj(
    config: &TrainingConfig,
    weights: &LoRAWeights,
    hidden: &[f32],
    targets: &[u32],
    output_proj: &adapteros_lora_mlx_ffi::MLXFFITensor,
) -> Result<LossReport> {
    use adapteros_lora_mlx_ffi::training::mlx_cross_entropy_loss_gpu;
    use adapteros_lora_mlx_ffi::MLXFFITensor;

    if targets.is_empty() {
        return Err(AosError::Training(
            "Validation targets must be non-empty to compute loss".to_string(),
        ));
    }

    let logits = build_logits(config, weights, hidden, output_proj)?;
    let targets_i32: Vec<i32> = targets.iter().map(|&t| t as i32).collect();
    let targets_tensor = MLXFFITensor::from_ints(&targets_i32, vec![1, targets.len()])?;
    let loss_tensor = mlx_cross_entropy_loss_gpu(&logits, &targets_tensor, LOSS_IGNORE_INDEX)?;
    let loss_vec = loss_tensor.to_float_vec()?;
    let loss = loss_vec
        .first()
        .copied()
        .ok_or_else(|| AosError::Training("Loss tensor is empty".to_string()))?;

    Ok(build_loss_report(
        loss,
        validation_loss_spec(LOSS_IGNORE_INDEX),
        targets,
    ))
}

#[cfg(not(feature = "multi-backend"))]
pub fn compute_validation_loss_with_output_proj(
    _config: &TrainingConfig,
    _weights: &LoRAWeights,
    _hidden: &[f32],
    _targets: &[u32],
    _output_proj: &(),
) -> Result<LossReport> {
    Err(AosError::Training(
        "Cross-entropy validation loss requires multi-backend (MLX) support".to_string(),
    ))
}

#[cfg(test)]
pub fn reference_cross_entropy_loss(
    logits: &[Vec<f32>],
    targets: &[u32],
    ignore_index: i32,
) -> f32 {
    let mut total = 0.0f32;
    let mut count = 0usize;
    for (row_idx, row) in logits.iter().enumerate() {
        let target = *targets.get(row_idx).unwrap_or(&0);
        if ignore_index >= 0 && target as i32 == ignore_index {
            continue;
        }
        let max = row.iter().copied().fold(f32::NEG_INFINITY, |a, b| a.max(b));
        let mut sum_exp = 0.0f32;
        for &v in row {
            sum_exp += (v - max).exp();
        }
        let log_sum_exp = sum_exp.ln() + max;
        let target_logit = row.get(target as usize).copied().unwrap_or(0.0);
        total += log_sum_exp - target_logit;
        count += 1;
    }
    if count == 0 {
        0.0
    } else {
        total / count as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loss_spec_is_consistent() {
        let train = training_loss_spec(LOSS_IGNORE_INDEX);
        let validation = validation_loss_spec(LOSS_IGNORE_INDEX);
        assert!(train.diffs(&validation).is_empty());
        assert!(!train.summary().is_empty());
    }

    #[test]
    fn audit_targets_reports_ignore_index() {
        let spec = training_loss_spec(LOSS_IGNORE_INDEX);
        let targets = vec![1, LOSS_IGNORE_INDEX as u32, 2, LOSS_IGNORE_INDEX as u32];
        let report = build_loss_report(0.5, spec, &targets);
        assert_eq!(report.masked_tokens, 2);
        assert_eq!(report.valid_tokens, 2);
        assert!(!report.warnings.is_empty());
    }

    #[test]
    fn audit_targets_reports_all_masked() {
        let spec = training_loss_spec(LOSS_IGNORE_INDEX);
        let targets = vec![LOSS_IGNORE_INDEX as u32, LOSS_IGNORE_INDEX as u32];
        let report = build_loss_report(0.5, spec, &targets);
        assert_eq!(report.valid_tokens, 0);
        assert!(report
            .warnings
            .iter()
            .any(|warning| warning.contains("all target tokens are masked")));
    }

    #[test]
    fn reference_loss_respects_ignore_index() {
        let logits = vec![vec![0.0, 0.0], vec![0.0, 0.0]];
        let targets = vec![LOSS_IGNORE_INDEX as u32, 1];
        let loss = reference_cross_entropy_loss(&logits, &targets, LOSS_IGNORE_INDEX);
        let expected = (2.0f32).ln();
        assert!((loss - expected).abs() < 1e-6);
    }
}
