//! CoreML placement handling

use super::metadata::infer_op_kind_from_target;
use super::types::{
    AdapterPlacement, CoremlPlacementSpec, CoremlTrainingMetadata, PlacementRecord,
};
use crate::training::{QuantizedLoRAWeights, TrainingConfig};
use adapteros_core::{AosError, Result};
use adapteros_types::coreml::{
    CoreMLPlacementBinding, CoreMLPlacementShape, CoreMLProjection, CoreMLTargetRef,
};
use std::collections::HashMap;
use std::collections::HashSet;

pub(crate) fn validate_quantized_shapes(
    weights: &QuantizedLoRAWeights,
    config: &TrainingConfig,
) -> Result<(usize, usize, usize, usize)> {
    // Handle multi-module weights
    if weights.is_multi_module() {
        // Get dimensions from first module
        let first_module = weights.modules.values().next().ok_or_else(|| {
            AosError::Validation("Multi-module weights have no modules".to_string())
        })?;

        let a_rows = first_module.lora_a_q15.len();
        let a_cols = first_module
            .lora_a_q15
            .first()
            .map(|r| r.len())
            .unwrap_or_default();
        let b_rows = first_module.lora_b_q15.len();
        let b_cols = first_module
            .lora_b_q15
            .first()
            .map(|r| r.len())
            .unwrap_or_default();

        if a_rows == 0 || a_cols == 0 || b_rows == 0 || b_cols == 0 {
            return Err(AosError::Validation(
                "Quantized multi-module weights are empty; aborting packaging".to_string(),
            ));
        }

        // For multi-module, rank is in the columns of lora_a (rows x rank)
        // and hidden_dim validation is optional since modules may have different dims
        return Ok((a_rows, a_cols, b_rows, b_cols));
    }

    // Legacy single-module path
    let a_rows = weights.lora_a_q15.len();
    let a_cols = weights
        .lora_a_q15
        .first()
        .map(|r| r.len())
        .unwrap_or_default();
    let b_rows = weights.lora_b_q15.len();
    let b_cols = weights
        .lora_b_q15
        .first()
        .map(|r| r.len())
        .unwrap_or_default();

    if a_rows == 0 || a_cols == 0 || b_rows == 0 || b_cols == 0 {
        return Err(AosError::Validation(
            "Quantized weights are empty; aborting packaging".to_string(),
        ));
    }

    if a_rows != config.rank || b_cols != config.rank {
        return Err(AosError::Validation(format!(
            "LoRA rank mismatch for CoreML placement: expected {}, got A rows {} / B cols {}",
            config.rank, a_rows, b_cols
        )));
    }

    if a_cols != config.hidden_dim || b_rows != config.hidden_dim {
        return Err(AosError::Validation(format!(
            "Hidden dimension mismatch for CoreML placement: expected {}, got A cols {} / B rows {}",
            config.hidden_dim, a_cols, b_rows
        )));
    }

    Ok((a_rows, a_cols, b_rows, b_cols))
}

pub(crate) fn parse_coreml_placement_from_metadata(
    metadata: &HashMap<String, String>,
) -> Result<Option<CoremlPlacementSpec>> {
    if let Some(raw) = metadata.get("coreml_placement") {
        let spec: CoremlPlacementSpec = serde_json::from_str(raw).map_err(|e| {
            AosError::Validation(format!("Invalid CoreML placement spec JSON: {}", e))
        })?;
        return Ok(Some(spec));
    }
    Ok(None)
}

pub(crate) fn default_coreml_placement_spec(
    modules: &[&str],
    rank: usize,
    hidden_dim: usize,
) -> CoremlPlacementSpec {
    CoremlPlacementSpec {
        version: 1,
        graph_id: Some("coreml-default".to_string()),
        bindings: modules
            .iter()
            .map(|m| CoreMLPlacementBinding {
                binding_id: m.to_string(),
                target: CoreMLTargetRef {
                    layer: m.to_string(),
                    op_kind: infer_op_kind_from_target(m),
                    path_hint: None,
                },
                projection: CoreMLProjection::InputToHidden,
                rank: rank as u32,
                alpha: None,
                scale: None,
                gating: None,
                shape: CoreMLPlacementShape {
                    input_dim: hidden_dim as u32,
                    output_dim: hidden_dim as u32,
                },
            })
            .collect(),
    }
}

pub(crate) fn validate_coreml_placement_spec(
    spec: &CoremlPlacementSpec,
    modules: &[&str],
    rank: usize,
    hidden_dim: usize,
) -> Result<()> {
    if spec.version == 0 {
        return Err(AosError::Validation(
            "CoreML placement spec version must be > 0".to_string(),
        ));
    }
    if spec.bindings.is_empty() {
        return Err(AosError::Validation(
            "CoreML placement spec must include at least one entry".to_string(),
        ));
    }

    let mut seen = HashSet::new();
    let allowed: HashSet<String> = modules.iter().map(|m| m.to_string()).collect();

    for binding in &spec.bindings {
        if !seen.insert(binding.binding_id.clone()) {
            return Err(AosError::Validation(format!(
                "Duplicate CoreML placement target '{}'",
                binding.binding_id
            )));
        }

        if !allowed.contains(&binding.target.layer) {
            return Err(AosError::Validation(format!(
                "Unknown CoreML placement target '{}' (expected one of {:?})",
                binding.target.layer, modules
            )));
        }

        if binding.rank as usize != rank {
            return Err(AosError::Validation(format!(
                "CoreML placement rank mismatch for '{}': expected {}, got {}",
                binding.binding_id, rank, binding.rank
            )));
        }
        if binding.shape.input_dim != hidden_dim as u32
            || binding.shape.output_dim != hidden_dim as u32
        {
            return Err(AosError::Validation(format!(
                "CoreML placement shape mismatch for '{}': expected {}x{}, got {}x{}",
                binding.binding_id,
                hidden_dim,
                hidden_dim,
                binding.shape.output_dim,
                binding.shape.input_dim
            )));
        }
    }

    Ok(())
}

pub(crate) fn resolve_coreml_placement_spec(
    metadata: &HashMap<String, String>,
    modules: &[&str],
    rank: usize,
    hidden_dim: usize,
) -> Result<CoremlPlacementSpec> {
    if let Some(spec) = parse_coreml_placement_from_metadata(metadata)? {
        validate_coreml_placement_spec(&spec, modules, rank, hidden_dim)?;
        return Ok(spec);
    }

    let spec = default_coreml_placement_spec(modules, rank, hidden_dim);
    validate_coreml_placement_spec(&spec, modules, rank, hidden_dim)?;
    Ok(spec)
}

pub(crate) fn build_coreml_sections(
    metadata: &HashMap<String, String>,
    training_backend: Option<&str>,
    rank: usize,
) -> Result<(
    Option<CoremlTrainingMetadata>,
    Option<AdapterPlacement>,
    Option<String>,
)> {
    let mut training_backend_details = metadata
        .get("training_backend_details")
        .cloned()
        .or_else(|| training_backend.map(|b| format!("{b}_train")));

    let coreml_requested = metadata
        .get("coreml_used")
        .and_then(|v| v.parse::<bool>().ok())
        .unwrap_or(false)
        || matches!(training_backend, Some("coreml"));

    let coreml_metadata = if coreml_requested {
        if training_backend.is_none() && training_backend_details.is_none() {
            training_backend_details = Some("coreml_train".to_string());
        }
        let device_type = metadata
            .get("coreml_device_type")
            .or_else(|| metadata.get("coreml_device"))
            .cloned()
            .or_else(|| {
                training_backend.map(|b| {
                    if b == "coreml" {
                        "ane".to_string()
                    } else {
                        "unknown".to_string()
                    }
                })
            })
            .unwrap_or_else(|| "unknown".to_string());

        Some(CoremlTrainingMetadata {
            coreml_used: true,
            coreml_device_type: Some(device_type.to_ascii_lowercase()),
            coreml_precision_mode: metadata.get("coreml_precision_mode").cloned(),
            coreml_compile_config_id: metadata.get("coreml_compile_config_id").cloned(),
        })
    } else {
        None
    };

    let placement_records = if let Some(raw) = metadata.get("coreml_placement_records") {
        let parsed: Vec<PlacementRecord> = serde_json::from_str(raw).map_err(|e| {
            AosError::InvalidManifest(format!(
                "coreml_placement_records is not valid JSON array: {}",
                e
            ))
        })?;
        Some(parsed)
    } else if let Some(target) = metadata.get("coreml_graph_target") {
        let direction = metadata
            .get("coreml_projection")
            .cloned()
            .unwrap_or_else(|| "projection".to_string());
        let alpha_override = metadata
            .get("coreml_alpha_override")
            .and_then(|v| v.parse::<f32>().ok());
        Some(vec![PlacementRecord {
            graph_target: target.clone(),
            rank: rank as u32,
            direction,
            alpha_override,
        }])
    } else {
        None
    };

    let placement = placement_records.and_then(|records| {
        if records.is_empty() {
            None
        } else {
            Some(AdapterPlacement { records })
        }
    });

    Ok((coreml_metadata, placement, training_backend_details))
}
