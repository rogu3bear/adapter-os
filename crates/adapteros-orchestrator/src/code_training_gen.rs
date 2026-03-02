//! Code-aware training data generation for code generation adapters.
//!
//! Unlike the comprehension pairs in `codebase_ingestion.rs` (which answer
//! "what does this function do?"), this module produces **generation** pairs
//! that teach a model to **write** code matching a given specification.
//!
//! Four strategies are supported:
//!
//! 1. **SignatureToBody** — given a function signature + docs, produce the body
//! 2. **ContextToFunction** — given surrounding code, fill in the function
//! 3. **DocstringToImplementation** — given a docstring, write the full function
//! 4. **FillInTheMiddle** — FIM-style prefix/suffix → middle completion
//!
//! All strategies read actual source files using [`Span`] information from
//! the existing [`CodeGraph`] infrastructure.

use adapteros_core::Result;
use adapteros_retrieval::codegraph::types::Span;
use adapteros_retrieval::codegraph::{CodeGraph, SymbolKind, SymbolNode, Visibility};
use blake3::Hasher;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use tracing::{debug, warn};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Which code-generation strategy to apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CodeTrainingStrategy {
    /// `signature + docstring + imports → function body`
    SignatureToBody,
    /// `surrounding N lines + placeholder → complete function`
    ContextToFunction,
    /// `docstring → full function including signature`
    DocstringToImplementation,
    /// `<|fim_prefix|>...<|fim_suffix|>...<|fim_middle|> → middle`
    FillInTheMiddle,
}

/// A single code-generation training pair.
#[derive(Debug, Clone)]
pub struct CodeTrainingPair {
    /// The prompt (input) for the model.
    pub prompt: String,
    /// The completion (target) the model should produce.
    pub completion: String,
    /// Strategy that generated this pair.
    pub strategy: CodeTrainingStrategy,
    /// Structured metadata for provenance tracking.
    pub metadata: BTreeMap<String, String>,
    /// Training weight (higher = more emphasis during training).
    pub weight: f32,
}

/// Configuration for the code training pair generator.
#[derive(Debug, Clone)]
pub struct CodeTrainingGenConfig {
    /// Which strategies to apply. Empty = all applicable.
    pub strategies: Vec<CodeTrainingStrategy>,
    /// Include private symbols (default: false).
    pub include_private: bool,
    /// Minimum function body lines to be considered non-trivial.
    pub min_body_lines: usize,
    /// Lines of context before/after for ContextToFunction and FIM.
    pub context_lines: usize,
    /// FIM special tokens (prefix, suffix, middle).
    pub fim_prefix_token: String,
    pub fim_suffix_token: String,
    pub fim_middle_token: String,
}

impl Default for CodeTrainingGenConfig {
    fn default() -> Self {
        Self {
            strategies: vec![
                CodeTrainingStrategy::SignatureToBody,
                CodeTrainingStrategy::ContextToFunction,
                CodeTrainingStrategy::DocstringToImplementation,
                CodeTrainingStrategy::FillInTheMiddle,
            ],
            include_private: false,
            min_body_lines: 3,
            context_lines: 20,
            fim_prefix_token: "<|fim_prefix|>".to_string(),
            fim_suffix_token: "<|fim_suffix|>".to_string(),
            fim_middle_token: "<|fim_middle|>".to_string(),
        }
    }
}

/// Summary statistics from a generation run.
#[derive(Debug, Clone, Default)]
pub struct CodeTrainingGenStats {
    pub symbols_considered: usize,
    pub symbols_skipped_trivial: usize,
    pub symbols_skipped_test: usize,
    pub symbols_skipped_generated: usize,
    pub symbols_skipped_todo: usize,
    pub symbols_skipped_read_error: usize,
    pub pairs_signature_to_body: usize,
    pub pairs_context_to_function: usize,
    pub pairs_docstring_to_impl: usize,
    pub pairs_fim: usize,
}

impl CodeTrainingGenStats {
    pub fn total_pairs(&self) -> usize {
        self.pairs_signature_to_body
            + self.pairs_context_to_function
            + self.pairs_docstring_to_impl
            + self.pairs_fim
    }
}

// ---------------------------------------------------------------------------
// Generator
// ---------------------------------------------------------------------------

/// Generates code-generation training pairs from a [`CodeGraph`].
pub struct CodeTrainingGenerator {
    config: CodeTrainingGenConfig,
}

impl CodeTrainingGenerator {
    pub fn new(config: CodeTrainingGenConfig) -> Self {
        Self { config }
    }

    /// Generate all applicable training pairs from the code graph.
    ///
    /// Reads source files on disk using span information from each symbol.
    /// Returns pairs in deterministic order (sorted by qualified name, then
    /// file path, then span).
    pub fn generate(
        &self,
        graph: &CodeGraph,
        repo_root: &Path,
    ) -> Result<(Vec<CodeTrainingPair>, CodeTrainingGenStats)> {
        let mut pairs = Vec::new();
        let mut stats = CodeTrainingGenStats::default();

        // Select function-like symbols, deterministically sorted.
        let mut symbols: Vec<&SymbolNode> = graph
            .symbols
            .values()
            .filter(|s| self.is_candidate(s))
            .collect();

        // DETERMINISM: full tie-breaker chain matching code_ingestion.rs
        symbols.sort_by(|a, b| {
            a.qualified_name()
                .cmp(&b.qualified_name())
                .then_with(|| a.file_path.cmp(&b.file_path))
                .then_with(|| a.span.start_line.cmp(&b.span.start_line))
                .then_with(|| a.span.start_column.cmp(&b.span.start_column))
                .then_with(|| a.span.end_line.cmp(&b.span.end_line))
                .then_with(|| a.span.end_column.cmp(&b.span.end_column))
                .then_with(|| a.id.cmp(&b.id))
        });

        // File content cache to avoid re-reading the same file for every symbol.
        let mut file_cache: BTreeMap<String, String> = BTreeMap::new();

        for symbol in &symbols {
            stats.symbols_considered += 1;

            // Read source file (cached)
            let file_content = match self.read_file_cached(symbol, repo_root, &mut file_cache) {
                Some(content) => content,
                None => {
                    stats.symbols_skipped_read_error += 1;
                    continue;
                }
            };

            let body = extract_function_body(file_content, &symbol.span);
            let rel_path = relative_path(repo_root, &symbol.file_path);

            // Quality filters
            if is_test_function(symbol, &rel_path) {
                stats.symbols_skipped_test += 1;
                continue;
            }
            if is_generated_code(&rel_path) {
                stats.symbols_skipped_generated += 1;
                continue;
            }
            if is_trivial_body(&body, self.config.min_body_lines) {
                stats.symbols_skipped_trivial += 1;
                continue;
            }
            if is_todo_body(&body) {
                stats.symbols_skipped_todo += 1;
                continue;
            }

            let imports = extract_relevant_imports(file_content, &body);
            let (prefix_ctx, suffix_ctx) =
                extract_context(file_content, symbol, self.config.context_lines);

            let base_meta = self.base_metadata(symbol, &rel_path);

            // Generate pairs for each enabled strategy
            for strategy in &self.config.strategies {
                match strategy {
                    CodeTrainingStrategy::SignatureToBody => {
                        if let Some(pair) = self
                            .gen_signature_to_body(symbol, &body, &imports, &rel_path, &base_meta)
                        {
                            pairs.push(pair);
                            stats.pairs_signature_to_body += 1;
                        }
                    }
                    CodeTrainingStrategy::ContextToFunction => {
                        if let Some(pair) = self.gen_context_to_function(
                            symbol,
                            &body,
                            &prefix_ctx,
                            &suffix_ctx,
                            &rel_path,
                            &base_meta,
                        ) {
                            pairs.push(pair);
                            stats.pairs_context_to_function += 1;
                        }
                    }
                    CodeTrainingStrategy::DocstringToImplementation => {
                        if let Some(pair) =
                            self.gen_docstring_to_impl(symbol, &body, &rel_path, &base_meta)
                        {
                            pairs.push(pair);
                            stats.pairs_docstring_to_impl += 1;
                        }
                    }
                    CodeTrainingStrategy::FillInTheMiddle => {
                        if let Some(pair) =
                            self.gen_fim(symbol, &body, &prefix_ctx, &suffix_ctx, &base_meta)
                        {
                            pairs.push(pair);
                            stats.pairs_fim += 1;
                        }
                    }
                }
            }
        }

        debug!(
            considered = stats.symbols_considered,
            total_pairs = stats.total_pairs(),
            "Code training generation complete"
        );

        Ok((pairs, stats))
    }

    /// Compute a deterministic BLAKE3 hash over all generated pairs.
    pub fn hash_pairs(pairs: &[CodeTrainingPair]) -> String {
        let mut hasher = Hasher::new();
        for pair in pairs {
            hasher.update(pair.prompt.as_bytes());
            hasher.update(b"\0");
            hasher.update(pair.completion.as_bytes());
            hasher.update(b"\0");
            hasher.update(&pair.weight.to_le_bytes());
            for (k, v) in &pair.metadata {
                hasher.update(k.as_bytes());
                hasher.update(v.as_bytes());
            }
        }
        hasher.finalize().to_hex().to_string()
    }

    // -----------------------------------------------------------------------
    // Candidate selection
    // -----------------------------------------------------------------------

    fn is_candidate(&self, symbol: &SymbolNode) -> bool {
        let kind_ok = matches!(symbol.kind, SymbolKind::Function | SymbolKind::Method);
        let vis_ok = self.config.include_private || matches!(symbol.visibility, Visibility::Public);
        kind_ok && vis_ok
    }

    // -----------------------------------------------------------------------
    // File I/O (cached)
    // -----------------------------------------------------------------------

    fn read_file_cached<'a>(
        &self,
        symbol: &SymbolNode,
        repo_root: &Path,
        cache: &'a mut BTreeMap<String, String>,
    ) -> Option<&'a String> {
        let file_path = symbol.file_path.clone();
        if !cache.contains_key(&file_path) {
            let abs_path = resolve_symbol_path(repo_root, &file_path);
            match std::fs::read_to_string(&abs_path) {
                Ok(content) => {
                    cache.insert(file_path.clone(), content);
                }
                Err(e) => {
                    warn!(path = %abs_path.display(), error = %e, "Failed to read source file");
                    return None;
                }
            }
        }
        cache.get(&file_path)
    }

    // -----------------------------------------------------------------------
    // Base metadata
    // -----------------------------------------------------------------------

    fn base_metadata(&self, symbol: &SymbolNode, rel_path: &str) -> BTreeMap<String, String> {
        let mut m = BTreeMap::new();
        m.insert("generator".to_string(), "code_training_gen".to_string());
        m.insert("symbol_name".to_string(), symbol.qualified_name());
        m.insert("symbol_kind".to_string(), format!("{}", symbol.kind));
        m.insert("language".to_string(), symbol.language.to_string());
        m.insert("file_path".to_string(), rel_path.to_string());
        m.insert("start_line".to_string(), symbol.span.start_line.to_string());
        m.insert("end_line".to_string(), symbol.span.end_line.to_string());
        m.insert("visibility".to_string(), format!("{}", symbol.visibility));
        if symbol.is_async {
            m.insert("is_async".to_string(), "true".to_string());
        }
        if symbol.is_unsafe {
            m.insert("is_unsafe".to_string(), "true".to_string());
        }
        m
    }

    // -----------------------------------------------------------------------
    // Strategy 1: Signature → Body
    // -----------------------------------------------------------------------

    fn gen_signature_to_body(
        &self,
        symbol: &SymbolNode,
        body: &str,
        imports: &[String],
        rel_path: &str,
        base_meta: &BTreeMap<String, String>,
    ) -> Option<CodeTrainingPair> {
        let sig = symbol.signature.as_ref()?;

        let mut prompt = String::new();
        prompt.push_str("Implement the following Rust function:\n\n```rust\n");
        prompt.push_str(sig.trim());
        prompt.push_str("\n```\n\n");
        prompt.push_str(&format!(
            "Context: This function is in `{}`, module `{}`.\n",
            rel_path,
            symbol.module_path.join("::")
        ));

        if let Some(doc) = symbol.docstring.as_ref().filter(|d| !d.trim().is_empty()) {
            prompt.push_str(&format!("Documentation: {}\n", sanitize_whitespace(doc)));
        }

        if !imports.is_empty() {
            prompt.push_str(&format!("Dependencies:\n{}\n", imports.join("\n")));
        }

        let mut meta = base_meta.clone();
        meta.insert("strategy".to_string(), "signature_to_body".to_string());

        // Weight: documented functions are higher value
        let weight = if symbol.docstring.is_some() { 1.2 } else { 1.0 };

        Some(CodeTrainingPair {
            prompt,
            completion: body.to_string(),
            strategy: CodeTrainingStrategy::SignatureToBody,
            metadata: meta,
            weight,
        })
    }

    // -----------------------------------------------------------------------
    // Strategy 2: Context → Function
    // -----------------------------------------------------------------------

    fn gen_context_to_function(
        &self,
        symbol: &SymbolNode,
        body: &str,
        prefix_ctx: &str,
        suffix_ctx: &str,
        rel_path: &str,
        base_meta: &BTreeMap<String, String>,
    ) -> Option<CodeTrainingPair> {
        // Need both prefix and suffix context to make this strategy useful
        if prefix_ctx.is_empty() && suffix_ctx.is_empty() {
            return None;
        }

        // Full function = signature + body
        let full_function = if let Some(sig) = &symbol.signature {
            format!("{} {{\n{}\n}}", sig.trim(), body)
        } else {
            body.to_string()
        };

        let mut prompt = format!(
            "Given the following Rust code context from `{}`:\n\n```rust\n",
            rel_path
        );
        if !prefix_ctx.is_empty() {
            prompt.push_str(prefix_ctx);
            prompt.push('\n');
        }
        prompt.push_str(&format!("// TODO: implement {}\n", symbol.name));
        if !suffix_ctx.is_empty() {
            prompt.push_str(suffix_ctx);
            prompt.push('\n');
        }
        prompt.push_str("```\n\n");
        prompt.push_str(&format!("Implement `{}`", symbol.name));
        if let Some(doc) = symbol.docstring.as_ref().filter(|d| !d.trim().is_empty()) {
            prompt.push_str(&format!(" that {}", sanitize_whitespace(doc)));
        }
        prompt.push('.');

        let mut meta = base_meta.clone();
        meta.insert("strategy".to_string(), "context_to_function".to_string());

        Some(CodeTrainingPair {
            prompt,
            completion: full_function,
            strategy: CodeTrainingStrategy::ContextToFunction,
            metadata: meta,
            weight: 1.0,
        })
    }

    // -----------------------------------------------------------------------
    // Strategy 3: Docstring → Implementation
    // -----------------------------------------------------------------------

    fn gen_docstring_to_impl(
        &self,
        symbol: &SymbolNode,
        body: &str,
        rel_path: &str,
        base_meta: &BTreeMap<String, String>,
    ) -> Option<CodeTrainingPair> {
        let doc = symbol.docstring.as_ref().filter(|d| !d.trim().is_empty())?;

        let prompt = format!(
            "{}\n\nImplement this in Rust (file: `{}`):",
            sanitize_whitespace(doc),
            rel_path,
        );

        // Completion is the full function including signature
        let full_fn = if let Some(sig) = &symbol.signature {
            format!("{} {{\n{}\n}}", sig.trim(), body)
        } else {
            body.to_string()
        };

        let mut meta = base_meta.clone();
        meta.insert("strategy".to_string(), "docstring_to_impl".to_string());

        Some(CodeTrainingPair {
            prompt,
            completion: full_fn,
            strategy: CodeTrainingStrategy::DocstringToImplementation,
            metadata: meta,
            weight: 1.5, // Documented functions are highest value
        })
    }

    // -----------------------------------------------------------------------
    // Strategy 4: Fill-in-the-Middle (FIM)
    // -----------------------------------------------------------------------

    fn gen_fim(
        &self,
        symbol: &SymbolNode,
        body: &str,
        prefix_ctx: &str,
        suffix_ctx: &str,
        base_meta: &BTreeMap<String, String>,
    ) -> Option<CodeTrainingPair> {
        // FIM needs surrounding context to be meaningful
        if prefix_ctx.is_empty() && suffix_ctx.is_empty() {
            return None;
        }

        // Build the FIM-style prefix: everything up to (and including) the
        // opening brace of the function
        let sig = symbol.signature.as_ref()?;
        let fim_prefix = format!("{}{} {{\n", prefix_ctx, sig.trim());
        let fim_suffix = format!("\n}}\n{}", suffix_ctx);
        let fim_middle = body.to_string();

        let prompt = format!(
            "{}{}{}{}{}",
            self.config.fim_prefix_token,
            fim_prefix,
            self.config.fim_suffix_token,
            fim_suffix,
            self.config.fim_middle_token,
        );

        let mut meta = base_meta.clone();
        meta.insert("strategy".to_string(), "fim".to_string());

        Some(CodeTrainingPair {
            prompt,
            completion: fim_middle,
            strategy: CodeTrainingStrategy::FillInTheMiddle,
            metadata: meta,
            weight: 1.0,
        })
    }
}

// ---------------------------------------------------------------------------
// Source extraction helpers
// ---------------------------------------------------------------------------

/// Extract the body of a function using span line information.
///
/// The span covers the entire function (signature + body). We extract just the
/// inner body by skipping the first line (signature) and last line (closing brace).
/// For single-expression functions the whole span is returned.
fn extract_function_body(file_content: &str, span: &Span) -> String {
    let lines: Vec<&str> = file_content.lines().collect();
    let start = (span.start_line as usize).saturating_sub(1); // 1-indexed → 0-indexed
    let end = (span.end_line as usize).min(lines.len());
    if start >= end {
        return String::new();
    }

    let full_lines = &lines[start..end];

    // Try to extract just the body (between `{` and `}`)
    // Find the first line containing `{` after the signature
    let mut brace_line = 0;
    for (i, line) in full_lines.iter().enumerate() {
        let s: &str = line;
        if s.contains('{') {
            brace_line = i;
            break;
        }
    }

    // If the closing brace is on the last line, extract between
    let last = full_lines.len() - 1;
    if last > brace_line && full_lines[last].trim() == "}" {
        let body_start = brace_line + 1;
        if body_start <= last {
            return full_lines[body_start..last].join("\n");
        }
    }

    // Fallback: return everything
    full_lines.join("\n")
}

/// Extract `use` statements from a file that are referenced in the function body.
fn extract_relevant_imports(file_content: &str, function_body: &str) -> Vec<String> {
    let mut imports = Vec::new();
    for line in file_content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("use ") && trimmed.ends_with(';') {
            // Extract the last segment of the import path (the name being imported)
            let path_part = trimmed
                .trim_start_matches("use ")
                .trim_end_matches(';')
                .trim();

            // Check if any imported name appears in the body
            let names = extract_import_names(path_part);
            if names.iter().any(|name| function_body.contains(name)) {
                imports.push(trimmed.to_string());
            }
        }
    }
    imports
}

/// Extract the leaf names from a use path.
///
/// Examples:
/// - `std::collections::HashMap` → `["HashMap"]`
/// - `std::io::{Read, Write}` → `["Read", "Write"]`
/// - `super::types::*` → `[]` (glob imports are not matchable)
fn extract_import_names(path: &str) -> Vec<&str> {
    // Handle grouped imports: `foo::{A, B}`
    if let Some(brace_start) = path.find('{') {
        if let Some(brace_end) = path.find('}') {
            let inner = &path[brace_start + 1..brace_end];
            return inner
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty() && *s != "*")
                .collect();
        }
    }

    // Handle `as` aliases: `Foo as Bar` → use "Bar"
    if let Some(as_idx) = path.find(" as ") {
        let alias = path[as_idx + 4..].trim();
        if !alias.is_empty() {
            return vec![alias];
        }
    }

    // Simple path: last segment
    if let Some(last) = path.rsplit("::").next() {
        let last = last.trim();
        if last != "*" && !last.is_empty() {
            return vec![last];
        }
    }

    Vec::new()
}

/// Extract N lines of context before and after a symbol's span.
fn extract_context(
    file_content: &str,
    symbol: &SymbolNode,
    context_lines: usize,
) -> (String, String) {
    let lines: Vec<&str> = file_content.lines().collect();
    let start = (symbol.span.start_line as usize).saturating_sub(1);
    let end = (symbol.span.end_line as usize).min(lines.len());

    // Prefix: up to `context_lines` before the function
    let prefix_start = start.saturating_sub(context_lines);
    let prefix = if prefix_start < start {
        lines[prefix_start..start].join("\n")
    } else {
        String::new()
    };

    // Suffix: up to `context_lines` after the function
    let suffix_end = (end + context_lines).min(lines.len());
    let suffix = if end < suffix_end {
        lines[end..suffix_end].join("\n")
    } else {
        String::new()
    };

    (prefix, suffix)
}

// ---------------------------------------------------------------------------
// Quality filters
// ---------------------------------------------------------------------------

/// Skip test functions and test modules.
fn is_test_function(symbol: &SymbolNode, rel_path: &str) -> bool {
    // Test file paths
    if rel_path.contains("/tests/") || rel_path.ends_with("_test.rs") {
        return true;
    }

    // Test function names
    if symbol.name.starts_with("test_") {
        return true;
    }

    // Module path contains "tests"
    if symbol.module_path.iter().any(|m| m == "tests") {
        return true;
    }

    false
}

/// Skip generated code (build artifacts, macro outputs, etc.)
fn is_generated_code(rel_path: &str) -> bool {
    rel_path.starts_with("target/")
        || rel_path.contains("/target/")
        || rel_path.starts_with("generated/")
        || rel_path.contains("/generated/")
        || rel_path.contains(".generated.")
        || rel_path.ends_with(".pb.rs")
        || rel_path.ends_with(".g.rs")
}

/// Skip trivial function bodies (getters, single-line returns, etc.)
fn is_trivial_body(body: &str, min_lines: usize) -> bool {
    let meaningful_lines = body
        .lines()
        .filter(|l| {
            let t = l.trim();
            !t.is_empty() && !t.starts_with("//")
        })
        .count();
    meaningful_lines < min_lines
}

/// Skip functions whose body is just `todo!()`, `unimplemented!()`, or `panic!()`.
fn is_todo_body(body: &str) -> bool {
    let trimmed: String = body
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with("//"))
        .collect::<Vec<_>>()
        .join(" ");

    trimmed == "todo!()"
        || trimmed == "unimplemented!()"
        || trimmed.starts_with("todo!(\"")
        || trimmed.starts_with("unimplemented!(\"")
        || trimmed == "panic!()"
}

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

fn resolve_symbol_path(repo_root: &Path, file_path: &str) -> std::path::PathBuf {
    let p = std::path::PathBuf::from(file_path);
    if p.is_absolute() {
        p
    } else {
        repo_root.join(file_path)
    }
}

fn relative_path(root: &Path, file_path: &str) -> String {
    let input = std::path::PathBuf::from(file_path);
    if input.is_absolute() {
        if let Ok(stripped) = input.strip_prefix(root) {
            return stripped.to_string_lossy().to_string();
        }
    }
    input.to_string_lossy().to_string()
}

fn sanitize_whitespace(input: &str) -> String {
    input
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_retrieval::codegraph::types::Span;
    use adapteros_retrieval::codegraph::{Language, SymbolId};

    fn make_test_symbol(
        name: &str,
        sig: &str,
        doc: Option<&str>,
        start_line: u32,
        end_line: u32,
        file_path: &str,
    ) -> SymbolNode {
        let id = SymbolId::new(file_path, &format!("{}:{}", start_line, end_line), name);
        let span = Span::new(start_line, 1, end_line, 1, 0, 0);
        let mut node = SymbolNode::new(
            id,
            name.to_string(),
            SymbolKind::Function,
            Language::Rust,
            span,
            file_path.to_string(),
        )
        .with_visibility(Visibility::Public)
        .with_signature(sig.to_string());

        if let Some(d) = doc {
            node = node.with_docstring(d.to_string());
        }

        node
    }

    #[test]
    fn test_extract_function_body() {
        let source = "\
fn foo(x: i32) -> i32 {
    let y = x * 2;
    let z = y + 1;
    z
}";
        let span = Span::new(1, 1, 5, 2, 0, source.len());
        let body = extract_function_body(source, &span);
        assert!(body.contains("let y = x * 2;"));
        assert!(body.contains("let z = y + 1;"));
        assert!(body.contains("z"));
        // Should not contain the signature or closing brace
        assert!(!body.contains("fn foo"));
        assert!(!body.trim().ends_with('}'));
    }

    #[test]
    fn test_extract_relevant_imports() {
        let file = "\
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;
use crate::foo::Bar;
";
        let body = "let m: HashMap<String, Bar> = HashMap::new();";
        let imports = extract_relevant_imports(file, body);
        assert!(imports.iter().any(|i| i.contains("HashMap")));
        assert!(imports.iter().any(|i| i.contains("Bar")));
        assert!(!imports.iter().any(|i| i.contains("Read")));
        assert!(!imports.iter().any(|i| i.contains("Path")));
    }

    #[test]
    fn test_extract_import_names() {
        assert_eq!(
            extract_import_names("std::collections::HashMap"),
            vec!["HashMap"]
        );
        assert_eq!(
            extract_import_names("std::io::{Read, Write}"),
            vec!["Read", "Write"]
        );
        assert!(extract_import_names("std::io::*").is_empty());
        assert_eq!(extract_import_names("foo::Bar as Baz"), vec!["Baz"]);
    }

    #[test]
    fn test_quality_filters() {
        // Trivial body
        assert!(is_trivial_body("self.x", 3));
        assert!(!is_trivial_body("let a = 1;\nlet b = 2;\nlet c = 3;", 3));

        // Todo body
        assert!(is_todo_body("    todo!()   "));
        assert!(is_todo_body("unimplemented!()"));
        assert!(!is_todo_body("let x = 1;\nreturn x;"));

        // Test function
        let sym = make_test_symbol("test_foo", "fn test_foo()", None, 1, 5, "src/lib.rs");
        assert!(is_test_function(&sym, "src/lib.rs"));

        let sym2 = make_test_symbol("process", "fn process()", None, 1, 5, "src/lib.rs");
        assert!(!is_test_function(&sym2, "src/lib.rs"));

        // Generated code
        assert!(is_generated_code("target/debug/build/foo.rs"));
        assert!(is_generated_code("src/generated/proto.pb.rs"));
        assert!(!is_generated_code("src/lib.rs"));
    }

    #[test]
    fn test_extract_context() {
        let source = "line1\nline2\nline3\nfn foo() {\n    body\n}\nline7\nline8\nline9\n";
        let sym = make_test_symbol("foo", "fn foo()", None, 4, 6, "test.rs");
        let (prefix, suffix) = extract_context(source, &sym, 2);
        assert!(prefix.contains("line2"));
        assert!(prefix.contains("line3"));
        assert!(!prefix.contains("fn foo"));
        assert!(suffix.contains("line7"));
        assert!(suffix.contains("line8"));
    }

    #[test]
    fn test_gen_signature_to_body_requires_signature() {
        let cfg = CodeTrainingGenConfig::default();
        let gen = CodeTrainingGenerator::new(cfg);

        let mut sym = make_test_symbol("foo", "fn foo()", None, 1, 5, "src/lib.rs");
        let meta = gen.base_metadata(&sym, "src/lib.rs");
        let body = "let x = 1;\nlet y = 2;\nreturn x + y;";
        let imports = vec![];

        // With signature
        let pair = gen.gen_signature_to_body(&sym, body, &imports, "src/lib.rs", &meta);
        assert!(pair.is_some());

        // Without signature
        sym.signature = None;
        let pair = gen.gen_signature_to_body(&sym, body, &imports, "src/lib.rs", &meta);
        assert!(pair.is_none());
    }

    #[test]
    fn test_gen_docstring_to_impl_requires_docstring() {
        let cfg = CodeTrainingGenConfig::default();
        let gen = CodeTrainingGenerator::new(cfg);

        let sym_with_doc = make_test_symbol(
            "foo",
            "fn foo() -> i32",
            Some("Returns the answer to life"),
            1,
            5,
            "src/lib.rs",
        );
        let meta = gen.base_metadata(&sym_with_doc, "src/lib.rs");
        let body = "42";

        let pair = gen.gen_docstring_to_impl(&sym_with_doc, body, "src/lib.rs", &meta);
        assert!(pair.is_some());
        let p = pair.unwrap();
        assert!(p.prompt.contains("Returns the answer to life"));
        assert_eq!(p.strategy, CodeTrainingStrategy::DocstringToImplementation);

        // Without docstring
        let sym_no_doc = make_test_symbol("foo", "fn foo() -> i32", None, 1, 5, "src/lib.rs");
        let meta2 = gen.base_metadata(&sym_no_doc, "src/lib.rs");
        let pair2 = gen.gen_docstring_to_impl(&sym_no_doc, body, "src/lib.rs", &meta2);
        assert!(pair2.is_none());
    }

    #[test]
    fn test_hash_pairs_deterministic() {
        let pair = CodeTrainingPair {
            prompt: "implement foo".to_string(),
            completion: "fn foo() { 42 }".to_string(),
            strategy: CodeTrainingStrategy::SignatureToBody,
            metadata: BTreeMap::new(),
            weight: 1.0,
        };
        let h1 = CodeTrainingGenerator::hash_pairs(std::slice::from_ref(&pair));
        let h2 = CodeTrainingGenerator::hash_pairs(&[pair]);
        assert_eq!(h1, h2);
    }
}
