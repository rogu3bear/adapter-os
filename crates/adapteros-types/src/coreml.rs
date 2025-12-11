//! CoreML placement specification shared between training and inference.
//!
//! The placement spec describes where LoRA adapters attach inside a CoreML
//! graph using stable, human-readable identifiers (layer name + op role),
//! along with the projection direction and matrix shapes required to build
//! adapter deltas deterministically.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;

/// CoreML execution mode for inference/training backend selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum CoreMLMode {
    /// CoreML only; fail fast on any CoreML error (no fallback).
    CoremlStrict,
    /// Prefer CoreML; allow fallback when CoreML cannot be used.
    CoremlPreferred,
    /// Allow any backend (current auto behavior).
    BackendAuto,
}

impl Default for CoreMLMode {
    fn default() -> Self {
        CoreMLMode::CoremlPreferred
    }
}

impl CoreMLMode {
    /// Canonical lowercase string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            CoreMLMode::CoremlStrict => "coreml_strict",
            CoreMLMode::CoremlPreferred => "coreml_preferred",
            CoreMLMode::BackendAuto => "backend_auto",
        }
    }
}

impl FromStr for CoreMLMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let normalized = s.to_ascii_lowercase().replace(['-', ' '], "_");
        match normalized.as_str() {
            "coreml_strict" | "strict" => Ok(CoreMLMode::CoremlStrict),
            "coreml_preferred" | "preferred" => Ok(CoreMLMode::CoremlPreferred),
            "backend_auto" | "auto" => Ok(CoreMLMode::BackendAuto),
            other => Err(format!(
                "invalid coreml mode '{}', expected coreml_strict|coreml_preferred|backend_auto",
                other
            )),
        }
    }
}

/// Stable operation classes that can host LoRA adapters in decoder-only models.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CoreMLOpKind {
    /// Attention query projection (Wq)
    AttentionQ,
    /// Attention key projection (Wk)
    AttentionK,
    /// Attention value projection (Wv)
    AttentionV,
    /// Attention output projection (Wo)
    AttentionO,
    /// MLP gating projection
    MlpGate,
    /// MLP up projection
    MlpUp,
    /// MLP down projection
    MlpDown,
}

impl CoreMLOpKind {
    /// Canonical lowercase string for keys/paths.
    pub fn as_str(&self) -> &'static str {
        match self {
            CoreMLOpKind::AttentionQ => "attn_q",
            CoreMLOpKind::AttentionK => "attn_k",
            CoreMLOpKind::AttentionV => "attn_v",
            CoreMLOpKind::AttentionO => "attn_o",
            CoreMLOpKind::MlpGate => "mlp_gate",
            CoreMLOpKind::MlpUp => "mlp_up",
            CoreMLOpKind::MlpDown => "mlp_down",
        }
    }
}

/// Projection direction for the LoRA factorization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CoreMLProjection {
    /// Input (model hidden) -> intermediate hidden
    InputToHidden,
    /// Hidden -> output
    HiddenToOutput,
    /// Hidden -> hidden (residual style)
    HiddenToHidden,
}

/// Matrix shape for a single placement target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CoreMLPlacementShape {
    /// Input dimension (columns)
    pub input_dim: u32,
    /// Output dimension (rows)
    pub output_dim: u32,
}

impl CoreMLPlacementShape {
    /// Total elements in the target weight matrix.
    pub fn elements(&self) -> u32 {
        self.input_dim.saturating_mul(self.output_dim)
    }
}

/// Optional gating defaults for a placement entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, Default)]
pub struct CoreMLGating {
    /// Optional default gate to apply when none is supplied by routing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_gate: Option<f32>,
    /// Whether runtime overrides are allowed; false = enforce default.
    #[serde(default)]
    pub allow_runtime_override: bool,
}

/// Stable reference to a CoreML graph node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CoreMLTargetRef {
    /// Stable layer or block name from the CoreML graph.
    pub layer: String,
    /// Operation class inside the layer (e.g., q_proj).
    pub op_kind: CoreMLOpKind,
    /// Optional subpath hint (for nested blocks or MIL function names).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path_hint: Option<String>,
}

impl CoreMLTargetRef {
    /// Build a deterministic key combining layer + op + optional path.
    pub fn canonical_key(&self) -> String {
        if let Some(path) = &self.path_hint {
            format!("{}::{}::{}", self.layer, self.op_kind.as_str(), path)
        } else {
            format!("{}::{}", self.layer, self.op_kind.as_str())
        }
    }
}

/// Single LoRA binding entry for the CoreML graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CoreMLPlacementBinding {
    /// Stable binding identifier (human readable, persisted in manifests).
    pub binding_id: String,
    /// Graph target this binding attaches to.
    pub target: CoreMLTargetRef,
    /// Projection direction for LoRA factorization.
    pub projection: CoreMLProjection,
    /// LoRA rank.
    pub rank: u32,
    /// Optional LoRA alpha scaling factor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alpha: Option<f32>,
    /// Optional per-binding scale override (post alpha).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<f32>,
    /// Optional gating defaults.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gating: Option<CoreMLGating>,
    /// Matrix shape for this binding.
    pub shape: CoreMLPlacementShape,
}

impl CoreMLPlacementBinding {
    /// Canonical key for maps (prefers explicit id, otherwise target key).
    pub fn key(&self) -> String {
        if self.binding_id.is_empty() {
            self.target.canonical_key()
        } else {
            self.binding_id.clone()
        }
    }
}

/// CoreML placement specification for an entire model.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CoreMLPlacementSpec {
    /// Versioned schema for forward compatibility.
    #[serde(default = "coreml_placement_version_default")]
    pub version: u32,
    /// Optional graph fingerprint (e.g., model hash or CoreML spec hash).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph_id: Option<String>,
    /// Binding entries for LoRA attachment points.
    #[serde(default)]
    pub bindings: Vec<CoreMLPlacementBinding>,
}

impl CoreMLPlacementSpec {
    /// Build a lookup map from binding key to binding.
    pub fn binding_map(&self) -> HashMap<String, &CoreMLPlacementBinding> {
        let mut map = HashMap::with_capacity(self.bindings.len());
        for binding in &self.bindings {
            map.insert(binding.key(), binding);
        }
        map
    }
}

const fn coreml_placement_version_default() -> u32 {
    1
}
