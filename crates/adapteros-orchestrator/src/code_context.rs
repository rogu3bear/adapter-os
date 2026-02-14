//! Code context retrieval for RAG-grounded code generation.
//!
//! Assembles structured context from multiple retrieval signals (CodeGraph,
//! FTS5, vector similarity) for use in code generation prompts. Enforces
//! token budgets and tracks provenance via citations.

use adapteros_core::Result;
use adapteros_retrieval::codegraph::{CodeGraph, SymbolId, SymbolKind, SymbolNode};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

// ─── Context types ───────────────────────────────────────────────────────

/// Assembled code context for a generation request.
///
/// Contains all retrieved context organized by signal type, with citations
/// tracking which files and symbols contributed to the context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeContext {
    /// Type definitions referenced in the target signature
    pub type_definitions: Vec<TypeSnippet>,
    /// Similar functions from the codebase (by embedding similarity or name)
    pub similar_functions: Vec<FunctionSnippet>,
    /// Functions the target is likely to call (from call graph)
    pub callee_signatures: Vec<String>,
    /// Functions that call the target (from call graph)
    pub caller_signatures: Vec<String>,
    /// Required import statements
    pub imports: Vec<String>,
    /// Test examples for the target or related functions
    pub test_examples: Vec<TestSnippet>,
    /// Estimated total token count of assembled context
    pub estimated_tokens: usize,
    /// Provenance citations
    pub citations: Vec<ContextCitation>,
}

/// A type definition snippet included in context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeSnippet {
    pub name: String,
    pub kind: String,
    pub code: String,
    pub file_path: String,
    pub line_start: u32,
    pub line_end: u32,
}

/// A function snippet included as a pattern example.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionSnippet {
    pub name: String,
    pub signature: String,
    pub code: String,
    pub file_path: String,
    pub line_start: u32,
    pub line_end: u32,
    /// Why this function was included (e.g., "similar_name", "same_module", "callee")
    pub reason: String,
}

/// A test snippet showing usage patterns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSnippet {
    pub test_name: String,
    pub code: String,
    pub file_path: String,
    pub target_function: Option<String>,
}

/// Provenance citation for a context element.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextCitation {
    pub file_path: String,
    pub line_start: u32,
    pub line_end: u32,
    pub snippet_type: String,
    pub symbol_name: Option<String>,
}

// ─── Token budget ────────────────────────────────────────────────────────

/// Token budget allocation for context assembly.
///
/// Prioritizes: types > callees > similar functions > tests > imports.
#[derive(Debug, Clone)]
pub struct TokenBudget {
    pub total: usize,
    pub type_definitions: usize,
    pub callee_signatures: usize,
    pub similar_functions: usize,
    pub test_examples: usize,
    pub imports: usize,
}

impl TokenBudget {
    /// Create a budget from a total token count.
    ///
    /// Allocation ratios: types 30%, callees 15%, similar 30%, tests 15%, imports 10%.
    pub fn from_total(total: usize) -> Self {
        Self {
            total,
            type_definitions: total * 30 / 100,
            callee_signatures: total * 15 / 100,
            similar_functions: total * 30 / 100,
            test_examples: total * 15 / 100,
            imports: total * 10 / 100,
        }
    }
}

impl Default for TokenBudget {
    fn default() -> Self {
        Self::from_total(4096)
    }
}

// ─── Retriever ───────────────────────────────────────────────────────────

/// Configuration for code context retrieval.
#[derive(Debug, Clone)]
pub struct CodeContextConfig {
    /// Maximum context tokens to assemble
    pub max_context_tokens: usize,
    /// Maximum number of similar functions to include
    pub max_similar_functions: usize,
    /// Maximum number of test examples to include
    pub max_test_examples: usize,
    /// Maximum depth for call graph traversal
    pub call_graph_depth: usize,
    /// Include private symbols in context
    pub include_private: bool,
}

impl Default for CodeContextConfig {
    fn default() -> Self {
        Self {
            max_context_tokens: 4096,
            max_similar_functions: 3,
            max_test_examples: 2,
            call_graph_depth: 1,
            include_private: false,
        }
    }
}

/// Retrieves and assembles code context from a CodeGraph.
///
/// Combines structural analysis (call graph, type dependencies) with the
/// code graph's symbol table to build rich context for code generation.
/// FTS and vector retrieval are delegated to the caller when available;
/// this struct handles the graph-based context that is always available.
pub struct CodeContextRetriever<'a> {
    graph: &'a CodeGraph,
    config: CodeContextConfig,
    /// Cache of file contents for extracting code snippets.
    file_cache: BTreeMap<String, String>,
}

impl<'a> CodeContextRetriever<'a> {
    pub fn new(graph: &'a CodeGraph, config: CodeContextConfig) -> Self {
        Self {
            graph,
            config,
            file_cache: BTreeMap::new(),
        }
    }

    /// Load file content into cache for code extraction.
    pub fn preload_file(&mut self, path: &str, content: String) {
        self.file_cache.insert(path.to_string(), content);
    }

    /// Retrieve context for generating a target function.
    ///
    /// Uses the code graph to find:
    /// 1. Type definitions used in the function signature
    /// 2. Functions called by the target (callees)
    /// 3. Functions that call the target (callers, as usage examples)
    /// 4. Similar functions in the same module
    pub fn retrieve_for_symbol(&self, target_id: &SymbolId) -> Result<CodeContext> {
        let target = self
            .graph
            .get_symbol(target_id)
            .ok_or_else(|| adapteros_core::AosError::NotFound("target symbol".into()))?;

        let budget = TokenBudget::from_total(self.config.max_context_tokens);
        let mut citations = Vec::new();
        let mut token_count = 0usize;

        // 1. Extract type dependencies from signature
        let type_defs =
            self.resolve_type_dependencies(target, &budget, &mut token_count, &mut citations);

        // 2. Callee signatures (functions the target calls)
        let (callee_sigs, caller_sigs) =
            self.resolve_call_graph(target_id, &budget, &mut token_count, &mut citations);

        // 3. Similar functions in the same module
        let similar =
            self.find_similar_functions(target, &budget, &mut token_count, &mut citations);

        // 4. Imports from the target's file
        let imports = self.extract_imports(target);

        Ok(CodeContext {
            type_definitions: type_defs,
            similar_functions: similar,
            callee_signatures: callee_sigs,
            caller_signatures: caller_sigs,
            imports,
            test_examples: Vec::new(), // Populated by FTS when available
            estimated_tokens: token_count,
            citations,
        })
    }

    /// Resolve type dependencies from the target's type annotations.
    fn resolve_type_dependencies(
        &self,
        target: &SymbolNode,
        budget: &TokenBudget,
        token_count: &mut usize,
        citations: &mut Vec<ContextCitation>,
    ) -> Vec<TypeSnippet> {
        let mut type_defs = Vec::new();
        let mut seen_types = BTreeSet::new();

        // Collect type names from the target's annotations
        let type_names = self.extract_type_names(target);

        for type_name in &type_names {
            if *token_count >= budget.type_definitions {
                break;
            }

            // Search the graph for a matching type/struct/enum
            if let Some((id, symbol)) = self.find_type_symbol(type_name) {
                if seen_types.contains(&id) {
                    continue;
                }
                seen_types.insert(id);

                let code = self.extract_symbol_code(symbol);
                let estimated = estimate_tokens(&code);

                if *token_count + estimated > budget.type_definitions {
                    break;
                }

                citations.push(ContextCitation {
                    file_path: symbol.file_path.clone(),
                    line_start: symbol.span.start_line,
                    line_end: symbol.span.end_line,
                    snippet_type: "type_definition".into(),
                    symbol_name: Some(symbol.name.clone()),
                });

                type_defs.push(TypeSnippet {
                    name: symbol.name.clone(),
                    kind: format!("{:?}", symbol.kind),
                    code,
                    file_path: symbol.file_path.clone(),
                    line_start: symbol.span.start_line,
                    line_end: symbol.span.end_line,
                });

                *token_count += estimated;
            }
        }

        type_defs
    }

    /// Resolve call graph relationships.
    fn resolve_call_graph(
        &self,
        target_id: &SymbolId,
        budget: &TokenBudget,
        token_count: &mut usize,
        citations: &mut Vec<ContextCitation>,
    ) -> (Vec<String>, Vec<String>) {
        let mut callee_sigs = Vec::new();
        let mut caller_sigs = Vec::new();

        // Callees: functions the target calls
        for callee_id in self.graph.get_callees(target_id) {
            if *token_count >= budget.callee_signatures {
                break;
            }
            if let Some(callee) = self.graph.get_symbol(callee_id) {
                if let Some(sig) = &callee.signature {
                    let estimated = estimate_tokens(sig);
                    if *token_count + estimated <= budget.callee_signatures {
                        callee_sigs.push(sig.clone());
                        citations.push(ContextCitation {
                            file_path: callee.file_path.clone(),
                            line_start: callee.span.start_line,
                            line_end: callee.span.start_line,
                            snippet_type: "callee".into(),
                            symbol_name: Some(callee.name.clone()),
                        });
                        *token_count += estimated;
                    }
                }
            }
        }

        // Callers: functions that call the target (usage examples)
        for caller_id in self.graph.get_callers(target_id) {
            if caller_sigs.len() >= 3 {
                break;
            }
            if let Some(caller) = self.graph.get_symbol(caller_id) {
                if let Some(sig) = &caller.signature {
                    caller_sigs.push(sig.clone());
                }
            }
        }

        (callee_sigs, caller_sigs)
    }

    /// Find similar functions in the same module or file.
    fn find_similar_functions(
        &self,
        target: &SymbolNode,
        budget: &TokenBudget,
        token_count: &mut usize,
        citations: &mut Vec<ContextCitation>,
    ) -> Vec<FunctionSnippet> {
        let mut similar = Vec::new();
        let target_module = &target.module_path;

        // Iterate graph symbols, find functions in the same module
        for (id, symbol) in self.graph.symbols.iter() {
            if similar.len() >= self.config.max_similar_functions {
                break;
            }
            if *token_count >= budget.similar_functions {
                break;
            }

            // Must be a function, in the same module, but not the target itself
            if !matches!(symbol.kind, SymbolKind::Function | SymbolKind::Method) {
                continue;
            }
            if symbol.module_path != *target_module {
                continue;
            }
            if symbol.name == target.name {
                continue;
            }
            if !self.config.include_private
                && !matches!(
                    symbol.visibility,
                    adapteros_retrieval::codegraph::Visibility::Public
                )
            {
                continue;
            }

            let code = self.extract_symbol_code(symbol);
            let estimated = estimate_tokens(&code);

            if *token_count + estimated > budget.similar_functions {
                continue;
            }

            let sig = symbol
                .signature
                .clone()
                .unwrap_or_else(|| symbol.name.clone());

            citations.push(ContextCitation {
                file_path: symbol.file_path.clone(),
                line_start: symbol.span.start_line,
                line_end: symbol.span.end_line,
                snippet_type: "similar_function".into(),
                symbol_name: Some(symbol.name.clone()),
            });

            similar.push(FunctionSnippet {
                name: symbol.name.clone(),
                signature: sig,
                code,
                file_path: symbol.file_path.clone(),
                line_start: symbol.span.start_line,
                line_end: symbol.span.end_line,
                reason: "same_module".into(),
            });

            *token_count += estimated;
        }

        similar
    }

    /// Extract type names referenced in a symbol's annotations.
    fn extract_type_names(&self, symbol: &SymbolNode) -> Vec<String> {
        let mut names = Vec::new();

        if let Some(ref ann) = symbol.type_annotation {
            // Parameter types
            for param_type in &ann.parameter_types {
                names.extend(extract_rust_type_names(param_type));
            }
            // Return type
            if let Some(ref ret) = ann.return_type {
                names.extend(extract_rust_type_names(ret));
            }
            // Generic params
            for gp in &ann.generic_params {
                names.extend(extract_rust_type_names(gp));
            }
        }

        names.sort();
        names.dedup();
        names
    }

    /// Find a type/struct/enum symbol by name.
    fn find_type_symbol(&self, name: &str) -> Option<(SymbolId, &SymbolNode)> {
        for (id, symbol) in self.graph.symbols.iter() {
            if matches!(
                symbol.kind,
                SymbolKind::Struct | SymbolKind::Enum | SymbolKind::Trait | SymbolKind::Type
            ) && symbol.name == name
            {
                return Some((*id, symbol));
            }
        }
        None
    }

    /// Extract the full code text for a symbol from the file cache.
    fn extract_symbol_code(&self, symbol: &SymbolNode) -> String {
        if let Some(content) = self.file_cache.get(&symbol.file_path) {
            let lines: Vec<&str> = content.lines().collect();
            let start = (symbol.span.start_line as usize).saturating_sub(1);
            let end = (symbol.span.end_line as usize).min(lines.len());
            if start < end {
                return lines[start..end].join("\n");
            }
        }
        // Fallback: return signature if available
        symbol
            .signature
            .clone()
            .unwrap_or_else(|| format!("// {} (source unavailable)", symbol.name))
    }

    /// Extract import lines from the target's file.
    fn extract_imports(&self, target: &SymbolNode) -> Vec<String> {
        if let Some(content) = self.file_cache.get(&target.file_path) {
            content
                .lines()
                .filter(|line| {
                    let trimmed = line.trim();
                    trimmed.starts_with("use ") || trimmed.starts_with("pub use ")
                })
                .map(|s| s.to_string())
                .collect()
        } else {
            Vec::new()
        }
    }
}

// ─── Prompt builder ──────────────────────────────────────────────────────

/// Build a RAG-enriched prompt for code generation from assembled context.
pub fn build_rag_enriched_prompt(
    signature: &str,
    docstring: Option<&str>,
    context: &CodeContext,
) -> String {
    let mut prompt = String::new();

    // Type definitions
    if !context.type_definitions.is_empty() {
        prompt.push_str("// Relevant type definitions:\n");
        for td in &context.type_definitions {
            prompt.push_str(&format!("// From {}:\n{}\n\n", td.file_path, td.code));
        }
    }

    // Callee signatures (functions this will call)
    if !context.callee_signatures.is_empty() {
        prompt.push_str("// Functions available to call:\n");
        for sig in &context.callee_signatures {
            prompt.push_str(&format!("//   {}\n", sig));
        }
        prompt.push('\n');
    }

    // Similar functions (pattern examples)
    if !context.similar_functions.is_empty() {
        prompt.push_str("// Similar functions in the codebase:\n");
        for sf in &context.similar_functions {
            prompt.push_str(&format!(
                "// From {} ({}):\n{}\n\n",
                sf.file_path, sf.reason, sf.code
            ));
        }
    }

    // Test examples
    if !context.test_examples.is_empty() {
        prompt.push_str("// Test examples:\n");
        for te in &context.test_examples {
            prompt.push_str(&format!("// {}:\n{}\n\n", te.test_name, te.code));
        }
    }

    // Imports
    if !context.imports.is_empty() {
        prompt.push_str("// Current file imports:\n");
        for imp in context.imports.iter().take(20) {
            prompt.push_str(&format!("{}\n", imp));
        }
        prompt.push('\n');
    }

    // Generation request
    prompt.push_str("// Implement the following function:\n");
    if let Some(doc) = docstring {
        for line in doc.lines() {
            prompt.push_str(&format!("/// {}\n", line));
        }
    }
    prompt.push_str(signature);
    prompt.push('\n');

    prompt
}

// ─── Helpers ─────────────────────────────────────────────────────────────

/// Rough token estimate: ~4 chars per token for code.
fn estimate_tokens(text: &str) -> usize {
    (text.len() + 3) / 4
}

/// Extract Rust type names from a type string.
///
/// Handles common patterns: `Vec<Foo>`, `Option<Bar>`, `Result<Baz, Error>`,
/// `&str`, `&mut Foo`, `impl Trait`, references, etc.
fn extract_rust_type_names(type_str: &str) -> Vec<String> {
    let mut names = Vec::new();

    // Remove references, lifetimes, mut
    let cleaned = type_str
        .replace('&', "")
        .replace("mut ", "")
        .replace("'_ ", "")
        .replace("'static ", "");

    // Split on generic delimiters and commas
    for part in cleaned.split(|c: char| c == '<' || c == '>' || c == ',' || c == '(' || c == ')') {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Skip primitives and common std types
        let skip = [
            "bool", "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64", "i128",
            "isize", "f32", "f64", "str", "String", "Vec", "Option", "Result", "Box", "Arc", "Rc",
            "Cell", "RefCell", "Mutex", "RwLock", "HashMap", "BTreeMap", "HashSet", "BTreeSet",
            "impl", "dyn", "Self", "self",
        ];

        if skip.contains(&trimmed) {
            continue;
        }

        // Must start with uppercase (Rust type convention) or be a path
        if trimmed.starts_with(char::is_uppercase) || trimmed.contains("::") {
            // Take the last segment of a path
            let name = trimmed.rsplit("::").next().unwrap_or(trimmed);
            if name.starts_with(char::is_uppercase) {
                names.push(name.to_string());
            }
        }
    }

    names
}

// ─── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_rust_type_names_simple() {
        let names = extract_rust_type_names("Vec<MyStruct>");
        assert!(names.contains(&"MyStruct".to_string()));
        assert!(!names.contains(&"Vec".to_string()));
    }

    #[test]
    fn test_extract_rust_type_names_nested() {
        let names = extract_rust_type_names("Result<FooBar, AosError>");
        assert!(names.contains(&"FooBar".to_string()));
        assert!(names.contains(&"AosError".to_string()));
        assert!(!names.contains(&"Result".to_string()));
    }

    #[test]
    fn test_extract_rust_type_names_primitives_skipped() {
        let names = extract_rust_type_names("&str");
        assert!(names.is_empty());

        let names = extract_rust_type_names("u32");
        assert!(names.is_empty());
    }

    #[test]
    fn test_extract_rust_type_names_path() {
        let names = extract_rust_type_names("adapteros_core::AosError");
        assert!(names.contains(&"AosError".to_string()));
    }

    #[test]
    fn test_estimate_tokens() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("fn foo()"), 2); // 8 chars / 4
        assert_eq!(estimate_tokens("pub fn hello_world() -> Result<()>"), 9); // 35/4 rounded
    }

    #[test]
    fn test_token_budget_allocation() {
        let budget = TokenBudget::from_total(1000);
        assert_eq!(budget.type_definitions, 300);
        assert_eq!(budget.callee_signatures, 150);
        assert_eq!(budget.similar_functions, 300);
        assert_eq!(budget.test_examples, 150);
        assert_eq!(budget.imports, 100);
    }

    #[test]
    fn test_build_rag_enriched_prompt_empty_context() {
        let ctx = CodeContext {
            type_definitions: vec![],
            similar_functions: vec![],
            callee_signatures: vec![],
            caller_signatures: vec![],
            imports: vec![],
            test_examples: vec![],
            estimated_tokens: 0,
            citations: vec![],
        };

        let prompt = build_rag_enriched_prompt("fn foo() -> u32", Some("Returns 42"), &ctx);
        assert!(prompt.contains("fn foo() -> u32"));
        assert!(prompt.contains("/// Returns 42"));
        assert!(prompt.contains("Implement the following function"));
    }

    #[test]
    fn test_build_rag_enriched_prompt_with_context() {
        let ctx = CodeContext {
            type_definitions: vec![TypeSnippet {
                name: "MyConfig".into(),
                kind: "Struct".into(),
                code: "pub struct MyConfig { pub name: String }".into(),
                file_path: "src/config.rs".into(),
                line_start: 10,
                line_end: 12,
            }],
            similar_functions: vec![FunctionSnippet {
                name: "bar".into(),
                signature: "fn bar() -> u32".into(),
                code: "fn bar() -> u32 { 42 }".into(),
                file_path: "src/lib.rs".into(),
                line_start: 5,
                line_end: 7,
                reason: "same_module".into(),
            }],
            callee_signatures: vec!["fn helper() -> String".into()],
            caller_signatures: vec![],
            imports: vec!["use crate::config::MyConfig;".into()],
            test_examples: vec![],
            estimated_tokens: 100,
            citations: vec![],
        };

        let prompt = build_rag_enriched_prompt("fn foo(cfg: &MyConfig) -> u32", None, &ctx);
        assert!(prompt.contains("MyConfig"));
        assert!(prompt.contains("fn helper() -> String"));
        assert!(prompt.contains("fn bar() -> u32 { 42 }"));
        assert!(prompt.contains("use crate::config::MyConfig;"));
    }
}
