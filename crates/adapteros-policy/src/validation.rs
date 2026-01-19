//! Policy Customization Validation
//!
//! Server-side validation for tenant policy customizations against canonical schema bounds.
//! Citation: AGENTS.md - Policy Studio feature validation requirements

use adapteros_core::{AosError, Result};
use serde_json::Value;
use std::collections::HashMap;
use tracing::{debug, warn};

/// Policy field bounds and validation rules
#[derive(Debug, Clone)]
pub struct PolicyFieldSchema {
    pub field_name: String,
    pub field_type: FieldType,
    pub required: bool,
    pub min_value: Option<f64>,
    pub max_value: Option<f64>,
    pub allowed_values: Option<Vec<String>>,
    pub safety_constraint: Option<SafetyConstraint>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FieldType {
    String,
    Number,
    Boolean,
    Array,
    Object,
}

#[derive(Debug, Clone)]
pub struct SafetyConstraint {
    pub description: String,
    pub validator: fn(&Value) -> bool,
}

/// Validation result
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ValidationResult {
    pub valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

impl ValidationResult {
    pub fn success() -> Self {
        Self {
            valid: true,
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    pub fn with_error(error: String) -> Self {
        Self {
            valid: false,
            errors: vec![error],
            warnings: Vec::new(),
        }
    }

    pub fn add_error(&mut self, error: String) {
        self.errors.push(error);
        self.valid = false;
    }

    pub fn add_warning(&mut self, warning: String) {
        self.warnings.push(warning);
    }
}

/// Get policy schema for a given policy type
pub fn get_policy_schema(policy_type: &str) -> Result<HashMap<String, PolicyFieldSchema>> {
    let schema = match policy_type {
        "egress" => egress_schema(),
        "determinism" => determinism_schema(),
        "router" => router_schema(),
        "evidence" => evidence_schema(),
        "refusal" => refusal_schema(),
        "numeric" => numeric_schema(),
        "rag" => rag_schema(),
        "isolation" => isolation_schema(),
        "telemetry" => telemetry_schema(),
        "retention" => retention_schema(),
        "performance" => performance_schema(),
        "memory" => memory_schema(),
        "artifacts" => artifacts_schema(),
        "secrets" => secrets_schema(),
        "build_release" => build_release_schema(),
        "compliance" => compliance_schema(),
        "incident" => incident_schema(),
        "output" => output_schema(),
        "adapters" => adapters_schema(),
        _ => {
            return Err(AosError::Validation(format!(
                "Unknown policy type: {}",
                policy_type
            )))
        }
    };

    Ok(schema)
}

/// Validate policy customization JSON
pub fn validate_customization(
    policy_type: &str,
    customizations_json: &str,
) -> Result<ValidationResult> {
    let schema = get_policy_schema(policy_type)?;
    let customizations: Value = serde_json::from_str(customizations_json)
        .map_err(|e| AosError::Validation(format!("Invalid JSON: {}", e)))?;

    let mut result = ValidationResult::success();

    let obj = match customizations.as_object() {
        Some(obj) => obj,
        None => {
            result.add_error("Customizations must be a JSON object".to_string());
            return Ok(result);
        }
    };

    // Validate each field
    for (field_name, value) in obj {
        match schema.get(field_name) {
            Some(field_schema) => {
                validate_field(field_name, value, field_schema, &mut result);
            }
            None => {
                result.add_warning(format!("Unknown field: {}", field_name));
            }
        }
    }

    // Check required fields
    for (field_name, field_schema) in &schema {
        if field_schema.required && !obj.contains_key(field_name) {
            result.add_error(format!("Missing required field: {}", field_name));
        }
    }

    Ok(result)
}

/// Validate a single field
fn validate_field(
    field_name: &str,
    value: &Value,
    schema: &PolicyFieldSchema,
    result: &mut ValidationResult,
) {
    // Type validation
    let type_matches = match schema.field_type {
        FieldType::String => value.is_string(),
        FieldType::Number => value.is_number(),
        FieldType::Boolean => value.is_boolean(),
        FieldType::Array => value.is_array(),
        FieldType::Object => value.is_object(),
    };

    if !type_matches {
        result.add_error(format!(
            "Field '{}' has wrong type (expected {:?})",
            field_name, schema.field_type
        ));
        return;
    }

    // Numeric bounds validation
    if let Some(min) = schema.min_value {
        if let Some(num) = value.as_f64() {
            if num < min {
                result.add_error(format!(
                    "Field '{}' value {} is below minimum {}",
                    field_name, num, min
                ));
            }
        }
    }

    if let Some(max) = schema.max_value {
        if let Some(num) = value.as_f64() {
            if num > max {
                result.add_error(format!(
                    "Field '{}' value {} exceeds maximum {}",
                    field_name, num, max
                ));
            }
        }
    }

    // Enum validation
    if let Some(allowed) = &schema.allowed_values {
        if let Some(str_val) = value.as_str() {
            if !allowed.contains(&str_val.to_string()) {
                result.add_error(format!(
                    "Field '{}' value '{}' is not in allowed values: {:?}",
                    field_name, str_val, allowed
                ));
            }
        }
    }

    // Safety constraint validation
    if let Some(constraint) = &schema.safety_constraint {
        if !(constraint.validator)(value) {
            result.add_error(format!(
                "Field '{}' violates safety constraint: {}",
                field_name, constraint.description
            ));
        }
    }
}

// Schema definitions for each policy type

fn router_schema() -> HashMap<String, PolicyFieldSchema> {
    let mut schema = HashMap::new();

    schema.insert(
        "k_sparse".to_string(),
        PolicyFieldSchema {
            field_name: "k_sparse".to_string(),
            field_type: FieldType::Number,
            required: true,
            min_value: Some(1.0),
            max_value: Some(8.0),
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema.insert(
        "gate_quant".to_string(),
        PolicyFieldSchema {
            field_name: "gate_quant".to_string(),
            field_type: FieldType::String,
            required: true,
            min_value: None,
            max_value: None,
            allowed_values: Some(vec!["q15".to_string(), "q8".to_string(), "f16".to_string()]),
            safety_constraint: None,
        },
    );

    schema.insert(
        "entropy_floor".to_string(),
        PolicyFieldSchema {
            field_name: "entropy_floor".to_string(),
            field_type: FieldType::Number,
            required: true,
            min_value: Some(0.0),
            max_value: Some(1.0),
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema.insert(
        "sample_tokens_full".to_string(),
        PolicyFieldSchema {
            field_name: "sample_tokens_full".to_string(),
            field_type: FieldType::Number,
            required: true,
            min_value: Some(0.0),
            max_value: None,
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema.insert(
        "allowed_clusters".to_string(),
        PolicyFieldSchema {
            field_name: "allowed_clusters".to_string(),
            field_type: FieldType::Array,
            required: false,
            min_value: None,
            max_value: None,
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema.insert(
        "denied_clusters".to_string(),
        PolicyFieldSchema {
            field_name: "denied_clusters".to_string(),
            field_type: FieldType::Array,
            required: false,
            min_value: None,
            max_value: None,
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema.insert(
        "max_reasoning_depth".to_string(),
        PolicyFieldSchema {
            field_name: "max_reasoning_depth".to_string(),
            field_type: FieldType::Number,
            required: false,
            min_value: Some(0.0),
            max_value: None,
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema.insert(
        "cluster_fallback".to_string(),
        PolicyFieldSchema {
            field_name: "cluster_fallback".to_string(),
            field_type: FieldType::String,
            required: false,
            min_value: None,
            max_value: None,
            allowed_values: Some(vec![
                "stay_on_current".to_string(),
                "fallback_to_base".to_string(),
            ]),
            safety_constraint: None,
        },
    );

    schema
}

fn memory_schema() -> HashMap<String, PolicyFieldSchema> {
    let mut schema = HashMap::new();

    schema.insert(
        "min_headroom_pct".to_string(),
        PolicyFieldSchema {
            field_name: "min_headroom_pct".to_string(),
            field_type: FieldType::Number,
            required: true,
            min_value: Some(5.0), // Safety: minimum 5% headroom
            max_value: Some(100.0),
            allowed_values: None,
            safety_constraint: Some(SafetyConstraint {
                description: "Minimum headroom must be at least 5% for system stability"
                    .to_string(),
                validator: |v| v.as_f64().is_some_and(|n| n >= 5.0),
            }),
        },
    );

    schema.insert(
        "evict_order".to_string(),
        PolicyFieldSchema {
            field_name: "evict_order".to_string(),
            field_type: FieldType::Array,
            required: true,
            min_value: None,
            max_value: None,
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema.insert(
        "k_reduce_before_evict".to_string(),
        PolicyFieldSchema {
            field_name: "k_reduce_before_evict".to_string(),
            field_type: FieldType::Boolean,
            required: true,
            min_value: None,
            max_value: None,
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema
}

fn performance_schema() -> HashMap<String, PolicyFieldSchema> {
    let mut schema = HashMap::new();

    schema.insert(
        "latency_p95_ms".to_string(),
        PolicyFieldSchema {
            field_name: "latency_p95_ms".to_string(),
            field_type: FieldType::Number,
            required: true,
            min_value: Some(1.0),
            max_value: None,
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema.insert(
        "router_overhead_pct_max".to_string(),
        PolicyFieldSchema {
            field_name: "router_overhead_pct_max".to_string(),
            field_type: FieldType::Number,
            required: true,
            min_value: Some(0.0),
            max_value: Some(100.0),
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema.insert(
        "throughput_tokens_per_s_min".to_string(),
        PolicyFieldSchema {
            field_name: "throughput_tokens_per_s_min".to_string(),
            field_type: FieldType::Number,
            required: true,
            min_value: Some(1.0),
            max_value: None,
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema
}

// Stub implementations for other policy types
fn egress_schema() -> HashMap<String, PolicyFieldSchema> {
    let mut schema = HashMap::new();

    schema.insert(
        "mode".to_string(),
        PolicyFieldSchema {
            field_name: "mode".to_string(),
            field_type: FieldType::String,
            required: true,
            min_value: None,
            max_value: None,
            allowed_values: Some(vec!["deny_all".to_string(), "allow_list".to_string()]),
            safety_constraint: None,
        },
    );

    schema.insert(
        "serve_requires_pf".to_string(),
        PolicyFieldSchema {
            field_name: "serve_requires_pf".to_string(),
            field_type: FieldType::Boolean,
            required: true,
            min_value: None,
            max_value: None,
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema.insert(
        "allow_tcp".to_string(),
        PolicyFieldSchema {
            field_name: "allow_tcp".to_string(),
            field_type: FieldType::Boolean,
            required: true,
            min_value: None,
            max_value: None,
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema.insert(
        "allow_udp".to_string(),
        PolicyFieldSchema {
            field_name: "allow_udp".to_string(),
            field_type: FieldType::Boolean,
            required: true,
            min_value: None,
            max_value: None,
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema.insert(
        "uds_paths".to_string(),
        PolicyFieldSchema {
            field_name: "uds_paths".to_string(),
            field_type: FieldType::Array,
            required: true,
            min_value: None,
            max_value: None,
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema
}

fn determinism_schema() -> HashMap<String, PolicyFieldSchema> {
    let mut schema = HashMap::new();

    schema.insert(
        "require_metallib_embed".to_string(),
        PolicyFieldSchema {
            field_name: "require_metallib_embed".to_string(),
            field_type: FieldType::Boolean,
            required: true,
            min_value: None,
            max_value: None,
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema.insert(
        "require_kernel_hash_match".to_string(),
        PolicyFieldSchema {
            field_name: "require_kernel_hash_match".to_string(),
            field_type: FieldType::Boolean,
            required: true,
            min_value: None,
            max_value: None,
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema.insert(
        "rng".to_string(),
        PolicyFieldSchema {
            field_name: "rng".to_string(),
            field_type: FieldType::String,
            required: true,
            min_value: None,
            max_value: None,
            allowed_values: Some(vec!["hkdf_seeded".to_string(), "deterministic".to_string()]),
            safety_constraint: None,
        },
    );

    schema.insert(
        "retrieval_tie_break".to_string(),
        PolicyFieldSchema {
            field_name: "retrieval_tie_break".to_string(),
            field_type: FieldType::Array,
            required: true,
            min_value: None,
            max_value: None,
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema
}

fn evidence_schema() -> HashMap<String, PolicyFieldSchema> {
    let mut schema = HashMap::new();

    schema.insert(
        "require_open_book".to_string(),
        PolicyFieldSchema {
            field_name: "require_open_book".to_string(),
            field_type: FieldType::Boolean,
            required: true,
            min_value: None,
            max_value: None,
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema.insert(
        "min_spans".to_string(),
        PolicyFieldSchema {
            field_name: "min_spans".to_string(),
            field_type: FieldType::Number,
            required: true,
            min_value: Some(0.0),
            max_value: None,
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema.insert(
        "prefer_latest_revision".to_string(),
        PolicyFieldSchema {
            field_name: "prefer_latest_revision".to_string(),
            field_type: FieldType::Boolean,
            required: true,
            min_value: None,
            max_value: None,
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema.insert(
        "warn_on_superseded".to_string(),
        PolicyFieldSchema {
            field_name: "warn_on_superseded".to_string(),
            field_type: FieldType::Boolean,
            required: true,
            min_value: None,
            max_value: None,
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema
}

fn refusal_schema() -> HashMap<String, PolicyFieldSchema> {
    let mut schema = HashMap::new();

    schema.insert(
        "abstain_threshold".to_string(),
        PolicyFieldSchema {
            field_name: "abstain_threshold".to_string(),
            field_type: FieldType::Number,
            required: true,
            min_value: Some(0.0),
            max_value: Some(1.0),
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema.insert(
        "missing_fields_templates".to_string(),
        PolicyFieldSchema {
            field_name: "missing_fields_templates".to_string(),
            field_type: FieldType::Object,
            required: false,
            min_value: None,
            max_value: None,
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema
}

fn numeric_schema() -> HashMap<String, PolicyFieldSchema> {
    let mut schema = HashMap::new();

    schema.insert(
        "canonical_units".to_string(),
        PolicyFieldSchema {
            field_name: "canonical_units".to_string(),
            field_type: FieldType::Object,
            required: true,
            min_value: None,
            max_value: None,
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema.insert(
        "max_rounding_error".to_string(),
        PolicyFieldSchema {
            field_name: "max_rounding_error".to_string(),
            field_type: FieldType::Number,
            required: true,
            min_value: Some(0.0),
            max_value: None,
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema.insert(
        "require_units_in_trace".to_string(),
        PolicyFieldSchema {
            field_name: "require_units_in_trace".to_string(),
            field_type: FieldType::Boolean,
            required: true,
            min_value: None,
            max_value: None,
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema
}

fn rag_schema() -> HashMap<String, PolicyFieldSchema> {
    let mut schema = HashMap::new();

    schema.insert(
        "index_scope".to_string(),
        PolicyFieldSchema {
            field_name: "index_scope".to_string(),
            field_type: FieldType::String,
            required: true,
            min_value: None,
            max_value: None,
            allowed_values: Some(vec!["per_tenant".to_string(), "shared".to_string()]),
            safety_constraint: None,
        },
    );

    schema.insert(
        "doc_tags_required".to_string(),
        PolicyFieldSchema {
            field_name: "doc_tags_required".to_string(),
            field_type: FieldType::Array,
            required: true,
            min_value: None,
            max_value: None,
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema.insert(
        "embedding_model_hash".to_string(),
        PolicyFieldSchema {
            field_name: "embedding_model_hash".to_string(),
            field_type: FieldType::String,
            required: true,
            min_value: None,
            max_value: None,
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema.insert(
        "topk".to_string(),
        PolicyFieldSchema {
            field_name: "topk".to_string(),
            field_type: FieldType::Number,
            required: true,
            min_value: Some(1.0),
            max_value: Some(100.0),
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema.insert(
        "order".to_string(),
        PolicyFieldSchema {
            field_name: "order".to_string(),
            field_type: FieldType::Array,
            required: true,
            min_value: None,
            max_value: None,
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema
}

fn isolation_schema() -> HashMap<String, PolicyFieldSchema> {
    let mut schema = HashMap::new();

    schema.insert(
        "process_model".to_string(),
        PolicyFieldSchema {
            field_name: "process_model".to_string(),
            field_type: FieldType::String,
            required: true,
            min_value: None,
            max_value: None,
            allowed_values: Some(vec!["per_tenant".to_string(), "shared".to_string()]),
            safety_constraint: None,
        },
    );

    schema.insert(
        "uds_root".to_string(),
        PolicyFieldSchema {
            field_name: "uds_root".to_string(),
            field_type: FieldType::String,
            required: true,
            min_value: None,
            max_value: None,
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema.insert(
        "forbid_shm".to_string(),
        PolicyFieldSchema {
            field_name: "forbid_shm".to_string(),
            field_type: FieldType::Boolean,
            required: true,
            min_value: None,
            max_value: None,
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema
}

fn telemetry_schema() -> HashMap<String, PolicyFieldSchema> {
    let mut schema = HashMap::new();

    schema.insert(
        "schema_hash".to_string(),
        PolicyFieldSchema {
            field_name: "schema_hash".to_string(),
            field_type: FieldType::String,
            required: true,
            min_value: None,
            max_value: None,
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema.insert(
        "router_full_tokens".to_string(),
        PolicyFieldSchema {
            field_name: "router_full_tokens".to_string(),
            field_type: FieldType::Number,
            required: true,
            min_value: Some(0.0),
            max_value: None,
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema
}

fn retention_schema() -> HashMap<String, PolicyFieldSchema> {
    let mut schema = HashMap::new();

    schema.insert(
        "keep_bundles_per_cpid".to_string(),
        PolicyFieldSchema {
            field_name: "keep_bundles_per_cpid".to_string(),
            field_type: FieldType::Number,
            required: true,
            min_value: Some(1.0),
            max_value: None,
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema.insert(
        "keep_incident_bundles".to_string(),
        PolicyFieldSchema {
            field_name: "keep_incident_bundles".to_string(),
            field_type: FieldType::Boolean,
            required: true,
            min_value: None,
            max_value: None,
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema.insert(
        "keep_promotion_bundles".to_string(),
        PolicyFieldSchema {
            field_name: "keep_promotion_bundles".to_string(),
            field_type: FieldType::Boolean,
            required: true,
            min_value: None,
            max_value: None,
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema.insert(
        "evict_strategy".to_string(),
        PolicyFieldSchema {
            field_name: "evict_strategy".to_string(),
            field_type: FieldType::String,
            required: true,
            min_value: None,
            max_value: None,
            allowed_values: Some(vec![
                "oldest_first_safe".to_string(),
                "lru".to_string(),
                "fifo".to_string(),
            ]),
            safety_constraint: None,
        },
    );

    schema
}

fn artifacts_schema() -> HashMap<String, PolicyFieldSchema> {
    let mut schema = HashMap::new();

    schema.insert(
        "require_signature".to_string(),
        PolicyFieldSchema {
            field_name: "require_signature".to_string(),
            field_type: FieldType::Boolean,
            required: true,
            min_value: None,
            max_value: None,
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema.insert(
        "require_sbom".to_string(),
        PolicyFieldSchema {
            field_name: "require_sbom".to_string(),
            field_type: FieldType::Boolean,
            required: true,
            min_value: None,
            max_value: None,
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema.insert(
        "cas_only".to_string(),
        PolicyFieldSchema {
            field_name: "cas_only".to_string(),
            field_type: FieldType::Boolean,
            required: true,
            min_value: None,
            max_value: None,
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema
}

fn secrets_schema() -> HashMap<String, PolicyFieldSchema> {
    let mut schema = HashMap::new();

    schema.insert(
        "env_allowed".to_string(),
        PolicyFieldSchema {
            field_name: "env_allowed".to_string(),
            field_type: FieldType::Array,
            required: true,
            min_value: None,
            max_value: None,
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema.insert(
        "keystore".to_string(),
        PolicyFieldSchema {
            field_name: "keystore".to_string(),
            field_type: FieldType::String,
            required: true,
            min_value: None,
            max_value: None,
            allowed_values: Some(vec!["secure_enclave".to_string(), "file".to_string()]),
            safety_constraint: None,
        },
    );

    schema.insert(
        "rotate_on_promotion".to_string(),
        PolicyFieldSchema {
            field_name: "rotate_on_promotion".to_string(),
            field_type: FieldType::Boolean,
            required: true,
            min_value: None,
            max_value: None,
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema
}

fn build_release_schema() -> HashMap<String, PolicyFieldSchema> {
    let mut schema = HashMap::new();

    schema.insert(
        "require_replay_zero_diff".to_string(),
        PolicyFieldSchema {
            field_name: "require_replay_zero_diff".to_string(),
            field_type: FieldType::Boolean,
            required: true,
            min_value: None,
            max_value: None,
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema.insert(
        "require_signed_plan".to_string(),
        PolicyFieldSchema {
            field_name: "require_signed_plan".to_string(),
            field_type: FieldType::Boolean,
            required: true,
            min_value: None,
            max_value: None,
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema.insert(
        "require_rollback_plan".to_string(),
        PolicyFieldSchema {
            field_name: "require_rollback_plan".to_string(),
            field_type: FieldType::Boolean,
            required: true,
            min_value: None,
            max_value: None,
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema
}

fn compliance_schema() -> HashMap<String, PolicyFieldSchema> {
    let mut schema = HashMap::new();

    schema.insert(
        "control_matrix_hash".to_string(),
        PolicyFieldSchema {
            field_name: "control_matrix_hash".to_string(),
            field_type: FieldType::String,
            required: true,
            min_value: None,
            max_value: None,
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema.insert(
        "require_evidence_links".to_string(),
        PolicyFieldSchema {
            field_name: "require_evidence_links".to_string(),
            field_type: FieldType::Boolean,
            required: true,
            min_value: None,
            max_value: None,
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema.insert(
        "require_itar_suite_green".to_string(),
        PolicyFieldSchema {
            field_name: "require_itar_suite_green".to_string(),
            field_type: FieldType::Boolean,
            required: true,
            min_value: None,
            max_value: None,
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema
}

fn incident_schema() -> HashMap<String, PolicyFieldSchema> {
    let mut schema = HashMap::new();

    schema.insert(
        "memory".to_string(),
        PolicyFieldSchema {
            field_name: "memory".to_string(),
            field_type: FieldType::Array,
            required: true,
            min_value: None,
            max_value: None,
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema.insert(
        "router_skew".to_string(),
        PolicyFieldSchema {
            field_name: "router_skew".to_string(),
            field_type: FieldType::Array,
            required: true,
            min_value: None,
            max_value: None,
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema.insert(
        "determinism".to_string(),
        PolicyFieldSchema {
            field_name: "determinism".to_string(),
            field_type: FieldType::Array,
            required: true,
            min_value: None,
            max_value: None,
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema.insert(
        "violation".to_string(),
        PolicyFieldSchema {
            field_name: "violation".to_string(),
            field_type: FieldType::Array,
            required: true,
            min_value: None,
            max_value: None,
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema
}

fn output_schema() -> HashMap<String, PolicyFieldSchema> {
    let mut schema = HashMap::new();

    schema.insert(
        "format".to_string(),
        PolicyFieldSchema {
            field_name: "format".to_string(),
            field_type: FieldType::String,
            required: true,
            min_value: None,
            max_value: None,
            allowed_values: Some(vec!["json".to_string(), "text".to_string()]),
            safety_constraint: None,
        },
    );

    schema.insert(
        "require_trace".to_string(),
        PolicyFieldSchema {
            field_name: "require_trace".to_string(),
            field_type: FieldType::Boolean,
            required: true,
            min_value: None,
            max_value: None,
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema.insert(
        "forbidden_topics".to_string(),
        PolicyFieldSchema {
            field_name: "forbidden_topics".to_string(),
            field_type: FieldType::Array,
            required: true,
            min_value: None,
            max_value: None,
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema
}

fn adapters_schema() -> HashMap<String, PolicyFieldSchema> {
    let mut schema = HashMap::new();

    schema.insert(
        "min_activation_pct".to_string(),
        PolicyFieldSchema {
            field_name: "min_activation_pct".to_string(),
            field_type: FieldType::Number,
            required: true,
            min_value: Some(0.0),
            max_value: Some(100.0),
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema.insert(
        "min_quality_delta".to_string(),
        PolicyFieldSchema {
            field_name: "min_quality_delta".to_string(),
            field_type: FieldType::Number,
            required: true,
            min_value: Some(0.0),
            max_value: None,
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema.insert(
        "require_registry_admit".to_string(),
        PolicyFieldSchema {
            field_name: "require_registry_admit".to_string(),
            field_type: FieldType::Boolean,
            required: true,
            min_value: None,
            max_value: None,
            allowed_values: None,
            safety_constraint: None,
        },
    );

    schema
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_router_customization() {
        let valid_json = r#"{"k_sparse": 4, "gate_quant": "q15", "entropy_floor": 0.02, "sample_tokens_full": 128}"#;
        let result = validate_customization("router", valid_json).unwrap();
        assert!(result.valid);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_validate_router_invalid_k_sparse() {
        let invalid_json = r#"{"k_sparse": 20, "gate_quant": "q15", "entropy_floor": 0.02, "sample_tokens_full": 128}"#;
        let result = validate_customization("router", invalid_json).unwrap();
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.contains("exceeds maximum")));
    }

    #[test]
    fn test_validate_memory_safety_constraint() {
        let unsafe_json =
            r#"{"min_headroom_pct": 2, "evict_order": [], "k_reduce_before_evict": true}"#;
        let result = validate_customization("memory", unsafe_json).unwrap();
        assert!(!result.valid);
        assert!(result
            .errors
            .iter()
            .any(|e| e.contains("safety constraint")));
    }

    #[test]
    fn test_validate_egress_enum_values() {
        let valid_json = r#"{"mode": "deny_all", "serve_requires_pf": true, "allow_tcp": false, "allow_udp": false, "uds_paths": ["./var/run/aos/tenant/*.sock"]}"#;
        let result = validate_customization("egress", valid_json).unwrap();
        assert!(result.valid);

        let invalid_json = r#"{"mode": "allow_all", "serve_requires_pf": true, "allow_tcp": false, "allow_udp": false, "uds_paths": []}"#;
        let result = validate_customization("egress", invalid_json).unwrap();
        assert!(!result.valid);
        assert!(result
            .errors
            .iter()
            .any(|e| e.contains("not in allowed values")));
    }

    #[test]
    fn test_validate_performance_bounds() {
        let valid_json = r#"{"latency_p95_ms": 24, "router_overhead_pct_max": 8, "throughput_tokens_per_s_min": 40}"#;
        let result = validate_customization("performance", valid_json).unwrap();
        assert!(result.valid);

        let invalid_latency = r#"{"latency_p95_ms": 0, "router_overhead_pct_max": 8, "throughput_tokens_per_s_min": 40}"#;
        let result = validate_customization("performance", invalid_latency).unwrap();
        assert!(!result.valid);

        let invalid_pct = r#"{"latency_p95_ms": 24, "router_overhead_pct_max": 150, "throughput_tokens_per_s_min": 40}"#;
        let result = validate_customization("performance", invalid_pct).unwrap();
        assert!(!result.valid);
    }

    #[test]
    fn test_validate_determinism_rng() {
        let valid_json = r#"{"require_metallib_embed": true, "require_kernel_hash_match": true, "rng": "hkdf_seeded", "retrieval_tie_break": ["score_desc", "doc_id_asc"]}"#;
        let result = validate_customization("determinism", valid_json).unwrap();
        assert!(result.valid);

        let invalid_rng = r#"{"require_metallib_embed": true, "require_kernel_hash_match": true, "rng": "random", "retrieval_tie_break": []}"#;
        let result = validate_customization("determinism", invalid_rng).unwrap();
        assert!(!result.valid);
        assert!(result
            .errors
            .iter()
            .any(|e| e.contains("not in allowed values")));
    }

    #[test]
    fn test_validate_evidence_min_spans() {
        let valid_json = r#"{"require_open_book": true, "min_spans": 2, "prefer_latest_revision": true, "warn_on_superseded": true}"#;
        let result = validate_customization("evidence", valid_json).unwrap();
        assert!(result.valid);

        let invalid_json = r#"{"require_open_book": true, "min_spans": -1, "prefer_latest_revision": true, "warn_on_superseded": true}"#;
        let result = validate_customization("evidence", invalid_json).unwrap();
        assert!(!result.valid);
    }

    #[test]
    fn test_validate_refusal_threshold() {
        let valid_json = r#"{"abstain_threshold": 0.55, "missing_fields_templates": {}}"#;
        let result = validate_customization("refusal", valid_json).unwrap();
        assert!(result.valid);

        let invalid_json = r#"{"abstain_threshold": 1.5, "missing_fields_templates": {}}"#;
        let result = validate_customization("refusal", invalid_json).unwrap();
        assert!(!result.valid);
    }

    #[test]
    fn test_validate_rag_topk() {
        let valid_json = r#"{"index_scope": "per_tenant", "doc_tags_required": ["doc_id", "rev"], "embedding_model_hash": "b3:abc", "topk": 5, "order": ["score_desc"]}"#;
        let result = validate_customization("rag", valid_json).unwrap();
        assert!(result.valid);

        let invalid_json = r#"{"index_scope": "per_tenant", "doc_tags_required": [], "embedding_model_hash": "b3:abc", "topk": 150, "order": []}"#;
        let result = validate_customization("rag", invalid_json).unwrap();
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.contains("exceeds maximum")));
    }

    #[test]
    fn test_validate_isolation_process_model() {
        let valid_json = r#"{"process_model": "per_tenant", "uds_root": "./var/run/aos/tenant", "forbid_shm": true}"#;
        let result = validate_customization("isolation", valid_json).unwrap();
        assert!(result.valid);

        let invalid_json = r#"{"process_model": "shared_all", "uds_root": "./var/run/aos/tenant", "forbid_shm": true}"#;
        let result = validate_customization("isolation", invalid_json).unwrap();
        assert!(!result.valid);
    }

    #[test]
    fn test_validate_retention_bundles() {
        let valid_json = r#"{"keep_bundles_per_cpid": 12, "keep_incident_bundles": true, "keep_promotion_bundles": true, "evict_strategy": "oldest_first_safe"}"#;
        let result = validate_customization("retention", valid_json).unwrap();
        assert!(result.valid);

        let invalid_json = r#"{"keep_bundles_per_cpid": 0, "keep_incident_bundles": true, "keep_promotion_bundles": true, "evict_strategy": "oldest_first_safe"}"#;
        let result = validate_customization("retention", invalid_json).unwrap();
        assert!(!result.valid);
    }

    #[test]
    fn test_validate_secrets_keystore() {
        let valid_json =
            r#"{"env_allowed": [], "keystore": "secure_enclave", "rotate_on_promotion": true}"#;
        let result = validate_customization("secrets", valid_json).unwrap();
        assert!(result.valid);

        let invalid_json =
            r#"{"env_allowed": [], "keystore": "plaintext", "rotate_on_promotion": true}"#;
        let result = validate_customization("secrets", invalid_json).unwrap();
        assert!(!result.valid);
    }

    #[test]
    fn test_validate_output_format() {
        let valid_json =
            r#"{"format": "json", "require_trace": true, "forbidden_topics": ["tenant_crossing"]}"#;
        let result = validate_customization("output", valid_json).unwrap();
        assert!(result.valid);

        let invalid_json = r#"{"format": "xml", "require_trace": true, "forbidden_topics": []}"#;
        let result = validate_customization("output", invalid_json).unwrap();
        assert!(!result.valid);
    }

    #[test]
    fn test_validate_adapters_activation() {
        let valid_json = r#"{"min_activation_pct": 2.0, "min_quality_delta": 0.5, "require_registry_admit": true}"#;
        let result = validate_customization("adapters", valid_json).unwrap();
        assert!(result.valid);

        let invalid_json = r#"{"min_activation_pct": 150.0, "min_quality_delta": 0.5, "require_registry_admit": true}"#;
        let result = validate_customization("adapters", invalid_json).unwrap();
        assert!(!result.valid);
    }

    #[test]
    fn test_validate_missing_required_fields() {
        let incomplete_json = r#"{"k_sparse": 4}"#;
        let result = validate_customization("router", incomplete_json).unwrap();
        assert!(!result.valid);
        assert!(result
            .errors
            .iter()
            .any(|e| e.contains("Missing required field")));
    }

    #[test]
    fn test_validate_wrong_field_type() {
        let invalid_json = r#"{"k_sparse": "not_a_number", "gate_quant": "q15", "entropy_floor": 0.02, "sample_tokens_full": 128}"#;
        let result = validate_customization("router", invalid_json).unwrap();
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.contains("wrong type")));
    }

    #[test]
    fn test_validate_unknown_policy_type() {
        let json = r#"{"some": "config"}"#;
        let result = validate_customization("unknown_policy", json);
        assert!(result.is_err());
    }
}
