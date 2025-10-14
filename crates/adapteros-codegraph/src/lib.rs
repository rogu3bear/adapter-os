//! CodeGraph - Deterministic Code Analysis
//!
//! This crate provides deterministic parsing and analysis of Rust codebases,
//! including call graph extraction, symbol type analysis, and semantic indexing.
//!
//! ## Features
//!
//! - Tree-sitter based AST parsing with deterministic output
//! - Call graph extraction with recursion and trait method linking
//! - Symbol type annotation and analysis
//! - SQLite persistence for efficient querying
//! - Deterministic hashing for reproducible builds

use adapteros_core::{B3Hash, Result};
use std::collections::BTreeMap;
use std::path::Path;

pub mod callgraph;
pub mod parsers;
pub mod sqlite;
pub mod types;

pub use callgraph::{CallEdge, CallGraph, CallGraphBuilder, ImportEdge};
pub use parsers::{detect_language, parse_directory, parse_file, LanguageParser, ParserFactory};
pub use sqlite::{CodeGraphDb, DbConfig};
pub use types::{Language, SymbolId, SymbolKind, SymbolNode, TypeAnnotation, Visibility};

/// Main CodeGraph structure
#[derive(Debug, Clone)]
pub struct CodeGraph {
    /// Symbol nodes indexed by ID
    pub symbols: BTreeMap<SymbolId, SymbolNode>,
    /// Call graph edges
    pub call_graph: CallGraph,
    /// Content hash for determinism
    pub content_hash: B3Hash,
}

impl CodeGraph {
    /// Create a new empty CodeGraph
    pub fn new() -> Self {
        Self {
            symbols: BTreeMap::new(),
            call_graph: CallGraph::new(),
            content_hash: B3Hash::hash(b""),
        }
    }

    /// Build CodeGraph from source directory
    pub async fn from_directory<P: AsRef<Path>>(
        source_dir: P,
        _db_config: Option<DbConfig>,
    ) -> Result<Self> {
        let mut builder = CallGraphBuilder::new();

        // Parse all supported files in directory using multi-language parser
        let parse_results = parse_directory(source_dir.as_ref()).await?;

        // Build symbol table and call graph
        for result in parse_results {
            builder.add_parse_result(result)?;
        }

        let (call_graph, symbols) = builder.build_call_graph();

        // Compute deterministic content hash
        let content_hash = Self::compute_content_hash(&symbols, &call_graph);

        Ok(Self {
            symbols,
            call_graph,
            content_hash,
        })
    }

    /// Compute deterministic content hash
    fn compute_content_hash(
        symbols: &BTreeMap<SymbolId, SymbolNode>,
        call_graph: &CallGraph,
    ) -> B3Hash {
        let mut hasher = blake3::Hasher::new();

        // Hash symbols in deterministic order
        for (id, symbol) in symbols {
            hasher.update(id.as_bytes());
            hasher.update(symbol.name.as_bytes());
            hasher.update(symbol.kind.to_string().as_bytes());
            hasher.update(symbol.language.to_string().as_bytes());
            if let Some(ref type_annotation) = symbol.type_annotation {
                hasher.update(type_annotation.to_string().as_bytes());
            }
        }

        // Hash call graph edges
        for edge in &call_graph.edges {
            hasher.update(edge.caller.as_bytes());
            hasher.update(edge.callee.as_bytes());
        }

        // Hash import edges (cross-language dependencies)
        for edge in &call_graph.import_edges {
            hasher.update(edge.importer.as_bytes());
            hasher.update(edge.imported.as_bytes());
            hasher.update(edge.import_statement.as_bytes());
            hasher.update(edge.source_language.to_string().as_bytes());
            hasher.update(edge.target_language.to_string().as_bytes());
        }

        B3Hash::from_bytes(hasher.finalize().into())
    }

    /// Get symbol by ID
    pub fn get_symbol(&self, id: &SymbolId) -> Option<&SymbolNode> {
        self.symbols.get(id)
    }

    /// Get all callers of a symbol
    pub fn get_callers(&self, callee: &SymbolId) -> Vec<&SymbolId> {
        self.call_graph
            .edges
            .iter()
            .filter(|edge| edge.callee == *callee)
            .map(|edge| &edge.caller)
            .collect()
    }

    /// Get all callees of a symbol
    pub fn get_callees(&self, caller: &SymbolId) -> Vec<&SymbolId> {
        self.call_graph
            .edges
            .iter()
            .filter(|edge| edge.caller == *caller)
            .map(|edge| &edge.callee)
            .collect()
    }

    /// Export to DOT format for Graphviz
    pub fn to_dot(&self) -> String {
        let mut dot = String::new();
        dot.push_str("digraph CodeGraph {\n");
        dot.push_str("  rankdir=TB;\n");
        dot.push_str("  node [shape=box, style=filled];\n\n");

        // Add nodes
        for (id, symbol) in &self.symbols {
            let label = format!("{}\\n{}", symbol.name, symbol.kind);
            let color = match symbol.kind {
                SymbolKind::Function => "lightblue",
                SymbolKind::Struct => "lightgreen",
                SymbolKind::Trait => "lightyellow",
                SymbolKind::Impl => "lightcoral",
                _ => "lightgray",
            };

            dot.push_str(&format!(
                "  \"{}\" [label=\"{}\", fillcolor=\"{}\"];\n",
                id.to_hex(),
                label,
                color
            ));
        }

        dot.push('\n');

        // Add edges
        for edge in &self.call_graph.edges {
            dot.push_str(&format!(
                "  \"{}\" -> \"{}\";\n",
                edge.caller.to_hex(),
                edge.callee.to_hex()
            ));
        }

        dot.push_str("}\n");
        dot
    }

    /// Save to SQLite database
    pub async fn save_to_db(&self, db_path: &Path) -> Result<()> {
        let db = CodeGraphDb::new(db_path).await?;
        db.save_codegraph(self).await?;
        Ok(())
    }

    /// Load from SQLite database
    pub async fn load_from_db(db_path: &Path) -> Result<Self> {
        let db = CodeGraphDb::new(db_path).await?;
        db.load_codegraph().await
    }
}

impl Default for CodeGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // use tempfile::TempDir; // unused

    #[tokio::test]
    async fn test_codegraph_creation() {
        let graph = CodeGraph::new();
        assert!(graph.symbols.is_empty());
        assert!(graph.call_graph.edges.is_empty());
    }

    #[tokio::test]
    async fn test_dot_export() {
        let graph = CodeGraph::new();
        let dot = graph.to_dot();
        assert!(dot.contains("digraph CodeGraph"));
        assert!(dot.contains("rankdir=TB"));
    }

    #[tokio::test]
    async fn test_content_hash_determinism() {
        let graph1 = CodeGraph::new();
        let graph2 = CodeGraph::new();

        // Empty graphs should have same hash
        assert_eq!(graph1.content_hash, graph2.content_hash);
    }
}
