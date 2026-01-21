//! Type definitions for frontend-backend communication

use serde::{Deserialize, Serialize};

/// Serialized graph data for frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphData {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub stats: GraphStats,
}

/// A node in the graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub file_path: String,
    pub span: SpanData,
    pub visibility: String,
    pub has_type_annotation: bool,
    pub is_recursive: bool,
    pub is_async: bool,
    pub is_unsafe: bool,
    pub qualified_name: String,
}

/// An edge in the graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub call_site: String,
    pub is_recursive: bool,
    pub is_trait_call: bool,
    pub is_generic_instantiation: bool,
}

/// Source code span
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanData {
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
    pub byte_start: usize,
    pub byte_length: usize,
}

/// Graph statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphStats {
    pub node_count: usize,
    pub edge_count: usize,
    pub recursive_count: usize,
    pub trait_call_count: usize,
    pub generic_instantiation_count: usize,
}

/// Symbol match from search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolMatch {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub file_path: String,
    pub qualified_name: String,
    pub span: SpanData,
}

/// Detailed symbol information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolDetails {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub qualified_name: String,
    pub file_path: String,
    pub span: SpanData,
    pub visibility: String,
    pub type_annotation: Option<String>,
    pub signature: Option<String>,
    pub docstring: Option<String>,
    pub is_recursive: bool,
    pub is_async: bool,
    pub is_unsafe: bool,
    pub callers: Vec<SymbolRef>,
    pub callees: Vec<SymbolRef>,
}

/// Reference to another symbol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolRef {
    pub id: String,
    pub name: String,
    pub kind: String,
}

/// Neighbors of a symbol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Neighbors {
    pub callers: Vec<SymbolMatch>,
    pub callees: Vec<SymbolMatch>,
}

/// Diff between two graphs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphDiffData {
    pub nodes_added: Vec<GraphNode>,
    pub nodes_removed: Vec<GraphNode>,
    pub nodes_modified: Vec<(GraphNode, GraphNode)>,
    pub edges_added: Vec<GraphEdge>,
    pub edges_removed: Vec<GraphEdge>,
    pub stats: DiffStats,
}

/// Diff statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffStats {
    pub nodes_added: usize,
    pub nodes_removed: usize,
    pub nodes_modified: usize,
    pub edges_added: usize,
    pub edges_removed: usize,
}

