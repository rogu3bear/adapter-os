use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Cluster definition within the semantic topology.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ClusterDefinition {
    pub id: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_adapter_id: Option<String>,
    pub version: String,
    /// Human-readable display name derived from the cluster's typed ID word alias.
    /// Populated when the ID uses the TypedId format.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

/// Adapter-level topology metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct AdapterTopology {
    pub adapter_id: String,
    pub name: String,
    pub cluster_ids: Vec<String>,
    pub transition_probabilities: HashMap<String, f64>,
}

/// Edge in the adjacency matrix from one cluster to the next.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct AdjacencyEdge {
    pub to_cluster_id: String,
    pub probability: f64,
}

/// Node prediction returned alongside the topology graph when context is provided.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct PredictedPathNode {
    /// Node identifier (adapter or cluster)
    pub id: String,
    /// Adapter identifier when this node represents an adapter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapter_id: Option<String>,
    /// Cluster the adapter belongs to (best-effort, optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cluster_id: Option<String>,
    /// Confidence or gate value produced by the router (0..1).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    /// Node kind hint (adapter | cluster).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

/// Complete topology graph returned by the API.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct TopologyGraph {
    pub clusters_version: String,
    pub clusters: Vec<ClusterDefinition>,
    pub adapters: Vec<AdapterTopology>,
    pub adjacency: HashMap<String, Vec<AdjacencyEdge>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub predicted_path: Option<Vec<PredictedPathNode>>,
}
