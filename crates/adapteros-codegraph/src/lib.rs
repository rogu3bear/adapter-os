//! CodeGraph - Deterministic Code Analysis
//!
//! This crate re-exports the codegraph implementation from `adapteros-retrieval`.
//! It exists for backwards compatibility and to provide a standalone entry point.
//!
//! ## Features
//!
//! - Tree-sitter based AST parsing with deterministic output
//! - Call graph extraction with recursion and trait method linking
//! - Symbol type annotation and analysis
//! - SQLite persistence for efficient querying
//! - Deterministic hashing for reproducible builds

// Re-export entire codegraph module from adapteros-retrieval
pub use adapteros_retrieval::codegraph::*;

// Re-export specific items for backwards compatibility
pub use adapteros_retrieval::codegraph::{
    callgraph, change_detector, directory_analyzer, framework_detector, parsers, sqlite, types,
};

// Module-level re-exports for common types
pub use parsers::{detect_language, parse_directory, parse_file, LanguageParser, ParserFactory};
pub use types::{
    Language, ParseResult, Span, SymbolId, SymbolKind, SymbolNode, TypeAnnotation, Visibility,
};

#[cfg(test)]
mod tests {
    use super::*;

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
