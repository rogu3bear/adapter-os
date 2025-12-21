//! CoreML graph import and placement resolution for LoRA.
//!
//! This module builds a logical CoreML graph (layer names + op roles) and
//! resolves a `CoreMLPlacementSpec` into concrete nodes so both training and
//! inference can attach LoRA adapters deterministically.

use adapteros_core::{AosError, Result};
use adapteros_types::coreml::{
    CoreMLOpKind, CoreMLPlacementBinding, CoreMLPlacementShape, CoreMLPlacementSpec,
    CoreMLTargetRef,
};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Logical CoreML node that can host a LoRA adapter.
#[derive(Debug, Clone)]
pub struct CoreMLGraphNode {
    /// Stable layer/block name from CoreML graph.
    pub name: String,
    /// Operation class (q/k/v/o or MLP projections).
    pub op_kind: Option<CoreMLOpKind>,
    /// Optional input dimension.
    pub input_dim: Option<u32>,
    /// Optional output dimension.
    pub output_dim: Option<u32>,
    /// Optional path hint for nested blocks.
    pub path_hint: Option<String>,
}

impl CoreMLGraphNode {
    /// Canonical key combining layer + op + optional path.
    pub fn key(&self) -> String {
        let op = self.op_kind.map(|k| k.as_str()).unwrap_or("unknown");
        if let Some(path) = &self.path_hint {
            format!("{}::{}::{}", self.name, op, path)
        } else {
            format!("{}::{}", self.name, op)
        }
    }
}

/// Imported CoreML graph (logical view only).
#[derive(Debug, Clone, Default)]
pub struct CoreMLGraph {
    nodes: Vec<CoreMLGraphNode>,
    by_key: HashMap<String, usize>,
}

impl CoreMLGraph {
    /// Construct from an explicit node list (useful for tests).
    pub fn from_nodes(nodes: Vec<CoreMLGraphNode>) -> Self {
        let mut by_key = HashMap::new();
        for (idx, node) in nodes.iter().enumerate() {
            by_key.insert(node.key(), idx);
            // Also register layer-only key to survive minor op renames.
            if let Some(op) = node.op_kind {
                by_key.insert(format!("{}::{}", node.name, op.as_str()), idx);
            }
        }
        Self { nodes, by_key }
    }

    /// Attempt to import a CoreML package (mlpackage or JSON dump) into a graph.
    pub fn from_package(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let json_path = find_model_json(path).ok_or_else(|| {
            AosError::Kernel(format!(
                "CoreML graph import: no model.json under {}",
                path.display()
            ))
        })?;

        let json_str = fs::read_to_string(&json_path).map_err(|e| {
            AosError::Kernel(format!(
                "CoreML graph import: failed reading {}: {}",
                json_path.display(),
                e
            ))
        })?;

        let value: Value = serde_json::from_str(&json_str).map_err(|e| {
            AosError::Kernel(format!(
                "CoreML graph import: invalid JSON at {}: {}",
                json_path.display(),
                e
            ))
        })?;

        let nodes = extract_nodes_from_json(&value);
        Ok(Self::from_nodes(nodes))
    }

    /// Enumerate candidate nodes (filtering to those with op_kind set).
    pub fn candidate_nodes(&self) -> impl Iterator<Item = &CoreMLGraphNode> {
        self.nodes.iter().filter(|n| n.op_kind.is_some())
    }

    fn lookup(&self, target: &CoreMLTargetRef) -> Option<&CoreMLGraphNode> {
        let key = target.canonical_key();
        if let Some(idx) = self.by_key.get(&key) {
            return self.nodes.get(*idx);
        }

        // Fallback: try without path_hint if provided.
        let fallback_key = format!("{}::{}", target.layer, target.op_kind.as_str());
        self.by_key
            .get(&fallback_key)
            .and_then(|idx| self.nodes.get(*idx))
    }
}

/// Resolved placement entry.
#[derive(Debug, Clone)]
pub struct ResolvedPlacement {
    /// Binding that was resolved.
    pub binding: CoreMLPlacementBinding,
    /// Target node matched in graph.
    pub target: CoreMLGraphNode,
    /// Final shape (uses binding shape; may check against target dims).
    pub shape: CoreMLPlacementShape,
    /// Whether graph dims matched the binding shape (if dims known).
    pub shape_verified: bool,
}

/// Placement resolution result with observability metrics.
#[derive(Debug, Default)]
pub struct PlacementResolution {
    pub resolved: Vec<ResolvedPlacement>,
    pub missing: Vec<String>,
    pub shape_mismatches: Vec<String>,
}

impl PlacementResolution {
    /// High-level metrics for observability.
    pub fn metrics(&self) -> PlacementMetrics {
        PlacementMetrics {
            total: self.resolved.len() + self.missing.len(),
            resolved: self.resolved.len(),
            missing: self.missing.len(),
            shape_mismatches: self.shape_mismatches.len(),
        }
    }

    /// Human-readable dump of resolved and missing entries.
    pub fn dump(&self) -> String {
        let mut lines = Vec::new();
        lines.push(format!(
            "placements={} missing={} shape_mismatches={}",
            self.resolved.len(),
            self.missing.len(),
            self.shape_mismatches.len()
        ));
        for entry in &self.resolved {
            lines.push(format!(
                "[ok] {} -> {} ({:?}) shape {}x{}{}",
                entry.binding.key(),
                entry.target.key(),
                entry.target.op_kind,
                entry.shape.output_dim,
                entry.shape.input_dim,
                if entry.shape_verified {
                    " verified"
                } else {
                    ""
                }
            ));
        }
        for miss in &self.missing {
            lines.push(format!("[missing] {}", miss));
        }
        lines.join("\n")
    }
}

/// Placement metrics for observability/telemetry.
#[derive(Debug, Clone, Copy, Default)]
pub struct PlacementMetrics {
    pub total: usize,
    pub resolved: usize,
    pub missing: usize,
    pub shape_mismatches: usize,
}

/// Resolve placement spec against a graph.
pub fn resolve_placement(graph: &CoreMLGraph, spec: &CoreMLPlacementSpec) -> PlacementResolution {
    let mut resolved = Vec::with_capacity(spec.bindings.len());
    let mut missing = Vec::new();
    let mut mismatches = Vec::new();

    for binding in &spec.bindings {
        if let Some(node) = graph.lookup(&binding.target) {
            let mut shape_verified = false;
            if let (Some(in_dim), Some(out_dim)) = (node.input_dim, node.output_dim) {
                if in_dim != binding.shape.input_dim || out_dim != binding.shape.output_dim {
                    mismatches.push(binding.key());
                } else {
                    shape_verified = true;
                }
            }

            resolved.push(ResolvedPlacement {
                binding: binding.clone(),
                target: node.clone(),
                shape: binding.shape,
                shape_verified,
            });
        } else {
            missing.push(binding.key());
        }
    }

    PlacementResolution {
        resolved,
        missing,
        shape_mismatches: mismatches,
    }
}

fn find_model_json(path: &Path) -> Option<PathBuf> {
    if path.is_file() {
        return Some(path.to_path_buf());
    }

    let candidates = [
        path.join("model.json"),
        path.join("Model.json"),
        path.join("Data").join("model.json"),
    ];
    candidates.into_iter().find(|p| p.exists())
}

fn extract_nodes_from_json(value: &Value) -> Vec<CoreMLGraphNode> {
    // Attempt to walk common CoreML JSON forms:
    // - { "neuralNetwork": { "layers": [ ... ] } }
    // - { "layers": [ ... ] }
    // - MIL export: { "graph": { "nodes": [ ... ] } }
    if let Some(layers) = value
        .get("neuralNetwork")
        .and_then(|nn| nn.get("layers"))
        .and_then(|layers| layers.as_array())
    {
        return layers
            .iter()
            .filter_map(parse_layer_value)
            .collect::<Vec<_>>();
    }

    if let Some(layers) = value.get("layers").and_then(|l| l.as_array()) {
        return layers
            .iter()
            .filter_map(parse_layer_value)
            .collect::<Vec<_>>();
    }

    if let Some(nodes) = value
        .get("graph")
        .and_then(|g| g.get("nodes"))
        .and_then(|n| n.as_array())
    {
        return nodes.iter().filter_map(parse_layer_value).collect();
    }

    Vec::new()
}

fn parse_layer_value(layer: &Value) -> Option<CoreMLGraphNode> {
    let name = layer
        .get("name")
        .and_then(|n| n.as_str())
        .or_else(|| layer.get("id").and_then(|n| n.as_str()))?;

    let op_kind = infer_op_kind(name);
    let (output_dim, input_dim) = infer_dims(layer);
    let path_hint = layer
        .get("path_hint")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Some(CoreMLGraphNode {
        name: name.to_string(),
        op_kind,
        input_dim,
        output_dim,
        path_hint,
    })
}

fn infer_op_kind(name: &str) -> Option<CoreMLOpKind> {
    let lower = name.to_ascii_lowercase();
    if lower.contains("q_proj") || lower.contains(".q_proj") || lower.contains("query") {
        return Some(CoreMLOpKind::AttentionQ);
    }
    if lower.contains("k_proj") || lower.contains(".k_proj") || lower.contains("key") {
        return Some(CoreMLOpKind::AttentionK);
    }
    if lower.contains("v_proj") || lower.contains(".v_proj") || lower.contains("value") {
        return Some(CoreMLOpKind::AttentionV);
    }
    if lower.contains("o_proj") || lower.contains(".o_proj") || lower.contains("out_proj") {
        return Some(CoreMLOpKind::AttentionO);
    }
    if lower.contains("gate_proj") || lower.contains("mlp.gate") || lower.contains("gate") {
        return Some(CoreMLOpKind::MlpGate);
    }
    if lower.contains("up_proj") || lower.contains("mlp.up") || lower.contains("ffn_up") {
        return Some(CoreMLOpKind::MlpUp);
    }
    if lower.contains("down_proj") || lower.contains("mlp.down") || lower.contains("ffn_down") {
        return Some(CoreMLOpKind::MlpDown);
    }
    None
}

fn infer_dims(layer: &Value) -> (Option<u32>, Option<u32>) {
    // Heuristic: look for weight_shape: [out, in]
    if let Some(shape) = layer
        .get("weight_shape")
        .and_then(|s| s.as_array())
        .and_then(|arr| {
            if arr.len() == 2 {
                let out = arr[0].as_u64()?;
                let inp = arr[1].as_u64()?;
                Some((out as u32, inp as u32))
            } else {
                None
            }
        })
    {
        return (Some(shape.0), Some(shape.1));
    }

    // Some exports may use "outputChannels"/"inputChannels"
    if let (Some(out), Some(inp)) = (
        layer
            .get("outputChannels")
            .or_else(|| layer.get("out_channels"))
            .and_then(|v| v.as_u64()),
        layer
            .get("inputChannels")
            .or_else(|| layer.get("in_channels"))
            .and_then(|v| v.as_u64()),
    ) {
        return (Some(out as u32), Some(inp as u32));
    }

    (None, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_spec() -> CoreMLPlacementSpec {
        CoreMLPlacementSpec {
            version: 1,
            graph_id: None,
            bindings: vec![
                CoreMLPlacementBinding {
                    binding_id: "layer0.q".into(),
                    target: CoreMLTargetRef {
                        layer: "layer0.self_attn.q_proj".into(),
                        op_kind: CoreMLOpKind::AttentionQ,
                        path_hint: None,
                    },
                    projection: adapteros_types::coreml::CoreMLProjection::InputToHidden,
                    rank: 8,
                    alpha: Some(16.0),
                    scale: None,
                    gating: None,
                    shape: CoreMLPlacementShape {
                        input_dim: 4096,
                        output_dim: 4096,
                    },
                },
                CoreMLPlacementBinding {
                    binding_id: "missing".into(),
                    target: CoreMLTargetRef {
                        layer: "layer9.self_attn.q_proj".into(),
                        op_kind: CoreMLOpKind::AttentionQ,
                        path_hint: None,
                    },
                    projection: adapteros_types::coreml::CoreMLProjection::InputToHidden,
                    rank: 8,
                    alpha: Some(16.0),
                    scale: None,
                    gating: None,
                    shape: CoreMLPlacementShape {
                        input_dim: 4096,
                        output_dim: 4096,
                    },
                },
            ],
        }
    }

    #[test]
    fn resolves_against_stub_graph() {
        let graph = CoreMLGraph::from_nodes(vec![CoreMLGraphNode {
            name: "layer0.self_attn.q_proj".into(),
            op_kind: Some(CoreMLOpKind::AttentionQ),
            input_dim: Some(4096),
            output_dim: Some(4096),
            path_hint: None,
        }]);

        let spec = sample_spec();
        let resolution = resolve_placement(&graph, &spec);
        let metrics = resolution.metrics();
        assert_eq!(metrics.resolved, 1);
        assert_eq!(metrics.missing, 1);
        assert_eq!(metrics.shape_mismatches, 0);
        assert!(resolution
            .resolved
            .iter()
            .any(|r| r.binding.binding_id == "layer0.q"));
        assert!(resolution.missing.contains(&"missing".to_string()));
    }
}
