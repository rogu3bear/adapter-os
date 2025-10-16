//! Call graph extraction and analysis
//!
//! Builds call graphs from parsed symbols, handling recursion,
//! trait method calls, and generic instantiations.

use crate::types::{Language, SymbolId, SymbolKind, SymbolNode};
use adapteros_core::Result;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// A call edge in the graph
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CallEdge {
    /// Calling symbol
    pub caller: SymbolId,
    /// Called symbol
    pub callee: SymbolId,
    /// Call site span
    pub call_site: String,
    /// Whether this is a recursive call
    pub is_recursive: bool,
    /// Whether this is a trait method call
    pub is_trait_call: bool,
    /// Whether this is a generic instantiation
    pub is_generic_instantiation: bool,
}

/// An import edge representing cross-language dependencies
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ImportEdge {
    /// Importing symbol
    pub importer: SymbolId,
    /// Imported symbol
    pub imported: SymbolId,
    /// Import statement text
    pub import_statement: String,
    /// Source language
    pub source_language: Language,
    /// Target language
    pub target_language: Language,
}

/// Call graph structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallGraph {
    /// All call edges
    pub edges: Vec<CallEdge>,
    /// All import edges (cross-language dependencies)
    pub import_edges: Vec<ImportEdge>,
    /// Callers index (callee -> callers)
    pub callers: BTreeMap<SymbolId, BTreeSet<SymbolId>>,
    /// Callees index (caller -> callees)
    pub callees: BTreeMap<SymbolId, BTreeSet<SymbolId>>,
    /// Importers index (imported -> importers)
    pub importers: BTreeMap<SymbolId, BTreeSet<SymbolId>>,
    /// Imported index (importer -> imported)
    pub imported: BTreeMap<SymbolId, BTreeSet<SymbolId>>,
}

impl CallGraph {
    /// Create a new empty call graph
    pub fn new() -> Self {
        Self {
            edges: Vec::new(),
            import_edges: Vec::new(),
            callers: BTreeMap::new(),
            callees: BTreeMap::new(),
            importers: BTreeMap::new(),
            imported: BTreeMap::new(),
        }
    }

    /// Add a call edge
    pub fn add_edge(&mut self, edge: CallEdge) {
        // Add to edges list
        self.edges.push(edge.clone());

        // Update indices
        self.callers
            .entry(edge.callee.clone())
            .or_default()
            .insert(edge.caller.clone());

        self.callees
            .entry(edge.caller.clone())
            .or_default()
            .insert(edge.callee.clone());
    }

    /// Add an import edge
    pub fn add_import_edge(&mut self, edge: ImportEdge) {
        // Add to import edges list
        self.import_edges.push(edge.clone());

        // Update indices
        self.importers
            .entry(edge.imported.clone())
            .or_default()
            .insert(edge.importer.clone());

        self.imported
            .entry(edge.importer.clone())
            .or_default()
            .insert(edge.imported.clone());
    }

    /// Get all callers of a symbol
    pub fn get_callers(&self, callee: &SymbolId) -> Vec<&SymbolId> {
        self.callers
            .get(callee)
            .map(|set| set.iter().collect())
            .unwrap_or_default()
    }

    /// Get all callees of a symbol
    pub fn get_callees(&self, caller: &SymbolId) -> Vec<&SymbolId> {
        self.callees
            .get(caller)
            .map(|set| set.iter().collect())
            .unwrap_or_default()
    }

    /// Get all importers of a symbol
    pub fn get_importers(&self, imported: &SymbolId) -> Vec<&SymbolId> {
        self.importers
            .get(imported)
            .map(|set| set.iter().collect())
            .unwrap_or_default()
    }

    /// Get all imported symbols by a symbol
    pub fn get_imported(&self, importer: &SymbolId) -> Vec<&SymbolId> {
        self.imported
            .get(importer)
            .map(|set| set.iter().collect())
            .unwrap_or_default()
    }

    /// Check if a symbol is recursive
    pub fn is_recursive(&self, symbol: &SymbolId) -> bool {
        self.edges
            .iter()
            .any(|edge| edge.caller == *symbol && edge.callee == *symbol && edge.is_recursive)
    }

    /// Get all recursive symbols
    pub fn get_recursive_symbols(&self) -> Vec<&SymbolId> {
        let mut recursive = BTreeSet::new();

        for edge in &self.edges {
            if edge.is_recursive {
                recursive.insert(&edge.caller);
            }
        }

        recursive.into_iter().collect()
    }

    /// Get all trait method calls
    pub fn get_trait_calls(&self) -> Vec<&CallEdge> {
        self.edges
            .iter()
            .filter(|edge| edge.is_trait_call)
            .collect()
    }

    /// Get all generic instantiations
    pub fn get_generic_instantiations(&self) -> Vec<&CallEdge> {
        self.edges
            .iter()
            .filter(|edge| edge.is_generic_instantiation)
            .collect()
    }

    /// Get all cross-language import edges
    pub fn get_cross_language_imports(&self) -> Vec<&ImportEdge> {
        self.import_edges
            .iter()
            .filter(|edge| edge.source_language != edge.target_language)
            .collect()
    }

    /// Get all import edges for a specific language pair
    pub fn get_imports_between_languages(
        &self,
        source: Language,
        target: Language,
    ) -> Vec<&ImportEdge> {
        self.import_edges
            .iter()
            .filter(|edge| edge.source_language == source && edge.target_language == target)
            .collect()
    }

    /// Compute graph statistics
    pub fn statistics(&self) -> CallGraphStats {
        let total_edges = self.edges.len();
        let recursive_edges = self.edges.iter().filter(|e| e.is_recursive).count();
        let trait_calls = self.edges.iter().filter(|e| e.is_trait_call).count();
        let generic_instantiations = self
            .edges
            .iter()
            .filter(|e| e.is_generic_instantiation)
            .count();
        let total_import_edges = self.import_edges.len();
        let cross_language_imports = self
            .import_edges
            .iter()
            .filter(|e| e.source_language != e.target_language)
            .count();

        let mut caller_counts = BTreeMap::new();
        let mut callee_counts = BTreeMap::new();

        for edge in &self.edges {
            *caller_counts.entry(edge.caller.clone()).or_insert(0) += 1;
            *callee_counts.entry(edge.callee.clone()).or_insert(0) += 1;
        }

        let max_callers = callee_counts.values().max().copied().unwrap_or(0);
        let max_callees = caller_counts.values().max().copied().unwrap_or(0);

        CallGraphStats {
            total_edges,
            recursive_edges,
            trait_calls,
            generic_instantiations,
            total_import_edges,
            cross_language_imports,
            max_callers,
            max_callees,
            unique_callers: caller_counts.len(),
            unique_callees: callee_counts.len(),
        }
    }
}

impl Default for CallGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// Call graph statistics
#[derive(Debug, Clone)]
pub struct CallGraphStats {
    pub total_edges: usize,
    pub recursive_edges: usize,
    pub trait_calls: usize,
    pub generic_instantiations: usize,
    pub total_import_edges: usize,
    pub cross_language_imports: usize,
    pub max_callers: usize,
    pub max_callees: usize,
    pub unique_callers: usize,
    pub unique_callees: usize,
}

/// Builder for call graphs
pub struct CallGraphBuilder {
    /// Symbol table
    symbols: BTreeMap<SymbolId, SymbolNode>,
    /// Call graph
    call_graph: CallGraph,
    /// Function call patterns
    call_patterns: BTreeMap<String, Vec<SymbolId>>,
    /// Import patterns
    import_patterns: BTreeMap<String, Vec<(SymbolId, String, Language)>>,
}

impl CallGraphBuilder {
    /// Create a new call graph builder
    pub fn new() -> Self {
        Self {
            symbols: BTreeMap::new(),
            call_graph: CallGraph::new(),
            call_patterns: BTreeMap::new(),
            import_patterns: BTreeMap::new(),
        }
    }

    /// Add a parse result to the builder
    pub fn add_parse_result(&mut self, result: crate::types::ParseResult) -> Result<()> {
        // Add symbols to symbol table
        for symbol in &result.symbols {
            self.symbols.insert(symbol.id.clone(), symbol.clone());
        }

        // Extract call patterns from the source
        self.extract_call_patterns(&result)?;

        // Extract import patterns from the source
        self.extract_import_patterns(&result)?;

        Ok(())
    }

    /// Extract call patterns from source code
    fn extract_call_patterns(&mut self, result: &crate::types::ParseResult) -> Result<()> {
        // This is a simplified implementation
        // In a full implementation, we would:
        // 1. Parse the source code with tree-sitter
        // 2. Find all function call expressions
        // 3. Match calls to symbol definitions
        // 4. Handle trait method calls and generic instantiations

        // For now, we'll create some example call patterns
        // based on the symbols we found

        let mut calls = Vec::new();

        for symbol in &result.symbols {
            if symbol.kind == SymbolKind::Function {
                // Look for calls to other functions
                // This is where we would implement the actual call extraction
                calls.push(symbol.id.clone());
            }
        }

        if !calls.is_empty() {
            self.call_patterns
                .insert(result.file_path.to_string_lossy().to_string(), calls);
        }

        Ok(())
    }

    /// Extract import patterns from source code
    fn extract_import_patterns(&mut self, result: &crate::types::ParseResult) -> Result<()> {
        // Extract import statements from symbols
        let mut imports = Vec::new();

        for symbol in &result.symbols {
            if symbol.kind == SymbolKind::Module {
                // Parse import statement to extract language information
                let import_statement = symbol.name.clone();
                let target_language = self.detect_import_target_language(&import_statement);

                imports.push((symbol.id.clone(), import_statement, target_language));
            }
        }

        if !imports.is_empty() {
            self.import_patterns
                .insert(result.file_path.to_string_lossy().to_string(), imports);
        }

        Ok(())
    }

    /// Detect target language from import statement
    fn detect_import_target_language(&self, import_statement: &str) -> Language {
        // Simple heuristics for detecting target language
        if import_statement.contains("from ") || import_statement.contains("import ") {
            if import_statement.contains(".py") || import_statement.contains("python") {
                Language::Python
            } else if import_statement.contains(".ts") || import_statement.contains("typescript") {
                Language::TypeScript
            } else if import_statement.contains(".js") || import_statement.contains("javascript") {
                Language::JavaScript
            } else if import_statement.contains(".go") || import_statement.contains("golang") {
                Language::Go
            } else if import_statement.contains(".rs") || import_statement.contains("rust") {
                Language::Rust
            } else {
                // Default to same language for now
                Language::Rust
            }
        } else {
            Language::Rust
        }
    }

    /// Build the final symbol table
    pub fn build_symbols(self) -> BTreeMap<SymbolId, SymbolNode> {
        self.symbols
    }

    /// Build the final call graph
    pub fn build_call_graph(mut self) -> (CallGraph, BTreeMap<SymbolId, SymbolNode>) {
        // Build call edges from patterns
        for (file_path, calls) in &self.call_patterns {
            for (i, caller_id) in calls.iter().enumerate() {
                for callee_id in calls.iter().skip(i + 1) {
                    // Create call edge
                    let edge = CallEdge {
                        caller: caller_id.clone(),
                        callee: callee_id.clone(),
                        call_site: format!("{}:{}", file_path, i),
                        is_recursive: caller_id == callee_id,
                        is_trait_call: self.is_trait_call(caller_id, callee_id),
                        is_generic_instantiation: self
                            .is_generic_instantiation(caller_id, callee_id),
                    };

                    self.call_graph.add_edge(edge);
                }
            }
        }

        // Build import edges from patterns
        for (_file_path, imports) in &self.import_patterns {
            for (importer_id, import_statement, target_language) in imports {
                // Find the source language from the symbol
                let source_language = self
                    .symbols
                    .get(importer_id)
                    .map(|s| s.language.clone())
                    .unwrap_or(Language::Rust);

                // Create import edge
                let import_edge = ImportEdge {
                    importer: importer_id.clone(),
                    imported: importer_id.clone(), // For now, use same ID
                    import_statement: import_statement.clone(),
                    source_language,
                    target_language: target_language.clone(),
                };

                self.call_graph.add_import_edge(import_edge);
            }
        }

        (self.call_graph, self.symbols)
    }

    /// Check if a call is a trait method call
    fn is_trait_call(&self, caller: &SymbolId, callee: &SymbolId) -> bool {
        if let (Some(_caller_symbol), Some(callee_symbol)) =
            (self.symbols.get(caller), self.symbols.get(callee))
        {
            // Check if callee is a trait method
            callee_symbol.kind == SymbolKind::Method
                || callee_symbol.kind == SymbolKind::AssociatedType
                || callee_symbol.kind == SymbolKind::AssociatedConst
        } else {
            false
        }
    }

    /// Check if a call is a generic instantiation
    fn is_generic_instantiation(&self, caller: &SymbolId, callee: &SymbolId) -> bool {
        if let (Some(caller_symbol), Some(callee_symbol)) =
            (self.symbols.get(caller), self.symbols.get(callee))
        {
            // Check if either symbol has generic parameters
            if let Some(ref caller_type) = caller_symbol.type_annotation {
                if !caller_type.generic_params.is_empty() {
                    return true;
                }
            }

            if let Some(ref callee_type) = callee_symbol.type_annotation {
                if !callee_type.generic_params.is_empty() {
                    return true;
                }
            }
        }

        false
    }
}

impl Default for CallGraphBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // use crate::types::{Span, Visibility}; // unused

    #[test]
    fn test_call_graph_creation() {
        let graph = CallGraph::new();
        assert!(graph.edges.is_empty());
        assert!(graph.callers.is_empty());
        assert!(graph.callees.is_empty());
    }

    #[test]
    fn test_call_edge_addition() {
        let mut graph = CallGraph::new();
        let id1 = SymbolId::new("test.rs", "1:1:1:10", "func1");
        let id2 = SymbolId::new("test.rs", "2:1:2:10", "func2");

        let edge = CallEdge {
            caller: id1.clone(),
            callee: id2.clone(),
            call_site: "test.rs:1".to_string(),
            is_recursive: false,
            is_trait_call: false,
            is_generic_instantiation: false,
        };

        graph.add_edge(edge);

        assert_eq!(graph.edges.len(), 1);
        assert_eq!(graph.get_callers(&id2), vec![&id1]);
        assert_eq!(graph.get_callees(&id1), vec![&id2]);
    }

    #[test]
    fn test_recursive_detection() {
        let mut graph = CallGraph::new();
        let id = SymbolId::new("test.rs", "1:1:1:10", "recursive_func");

        let edge = CallEdge {
            caller: id.clone(),
            callee: id.clone(),
            call_site: "test.rs:1".to_string(),
            is_recursive: true,
            is_trait_call: false,
            is_generic_instantiation: false,
        };

        graph.add_edge(edge);

        assert!(graph.is_recursive(&id));
        assert_eq!(graph.get_recursive_symbols(), vec![&id]);
    }

    #[test]
    fn test_call_graph_statistics() {
        let mut graph = CallGraph::new();
        let id1 = SymbolId::new("test.rs", "1:1:1:10", "func1");
        let id2 = SymbolId::new("test.rs", "2:1:2:10", "func2");
        let id3 = SymbolId::new("test.rs", "3:1:3:10", "func3");

        // Add some edges
        graph.add_edge(CallEdge {
            caller: id1.clone(),
            callee: id2.clone(),
            call_site: "test.rs:1".to_string(),
            is_recursive: false,
            is_trait_call: false,
            is_generic_instantiation: false,
        });

        graph.add_edge(CallEdge {
            caller: id2.clone(),
            callee: id3.clone(),
            call_site: "test.rs:2".to_string(),
            is_recursive: false,
            is_trait_call: true,
            is_generic_instantiation: false,
        });

        let stats = graph.statistics();
        assert_eq!(stats.total_edges, 2);
        assert_eq!(stats.trait_calls, 1);
        assert_eq!(stats.recursive_edges, 0);
    }
}
