//! Patch generation pipeline with LLM integration
//!
//! Implements patch generation with evidence citations and structured output.
//! Aligns with Code Policy requirements and evidence-first philosophy.

use crate::evidence::{EvidenceCitation, EvidenceSpan};
use adapteros_core::{AosError, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::info;

/// Patch generation request
#[derive(Debug, Clone)]
pub struct PatchGenerationRequest {
    pub repo_id: String,
    pub commit_sha: Option<String>,
    pub target_files: Vec<String>,
    pub description: String,
    pub evidence: Vec<EvidenceSpan>,
    pub context: HashMap<String, String>,
}

/// Generated patch proposal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchProposal {
    pub proposal_id: String,
    pub rationale: String,
    pub patches: Vec<FilePatch>,
    pub citations: Vec<EvidenceCitation>,
    pub confidence: f32,
    pub metadata: HashMap<String, String>,
}

/// File-level patch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilePatch {
    pub file_path: String,
    pub hunks: Vec<PatchHunk>,
    pub total_lines: usize,
    pub metadata: HashMap<String, String>,
}

/// Patch hunk (unified diff format)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchHunk {
    pub start_line: usize,
    pub end_line: usize,
    pub context_lines: Vec<String>,
    pub modified_lines: Vec<String>,
    pub hunk_type: HunkType,
}

/// Type of patch hunk
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HunkType {
    Addition,
    Deletion,
    Modification,
    Context,
}

/// LLM backend trait for patch generation
#[async_trait]
pub trait LlmBackend: Send + Sync {
    async fn generate_patch(&self, context: &PatchContext) -> Result<String>;

    async fn extract_rationale(&self, patch_text: &str) -> Result<String>;
}

/// Patch generation context
#[derive(Debug, Clone)]
pub struct PatchContext {
    pub request: PatchGenerationRequest,
    pub evidence_summary: String,
    pub file_contexts: HashMap<String, String>,
    pub constraints: Vec<String>,
}

/// Patch generator with LLM integration
pub struct PatchGenerator {
    llm_backend: Box<dyn LlmBackend>,
    patch_parser: PatchParser,
    citation_extractor: CitationExtractor,
}

impl PatchGenerator {
    pub fn new(
        llm_backend: Box<dyn LlmBackend>,
        patch_parser: PatchParser,
        citation_extractor: CitationExtractor,
    ) -> Self {
        Self {
            llm_backend,
            patch_parser,
            citation_extractor,
        }
    }

    /// Generate patch proposal with evidence citations
    pub async fn generate_patch(&self, request: PatchGenerationRequest) -> Result<PatchProposal> {
        info!("Generating patch for: {}", request.description);

        // 1. Build context from evidence and request
        let context = self.build_context(&request)?;

        // 2. Generate patch using LLM with evidence citations
        let patch_text = self.llm_backend.generate_patch(&context).await?;

        // 3. Extract rationale from generated text
        let rationale = self.llm_backend.extract_rationale(&patch_text).await?;

        // 4. Parse patch into structured format
        let patches = self.patch_parser.parse_patch_text(&patch_text)?;

        // 5. Extract citations from evidence
        let citations = self
            .citation_extractor
            .extract_citations(&request.evidence)?;

        // 6. Compute confidence score
        let confidence = self.compute_confidence(&patches, &request.evidence)?;

        // 7. Generate proposal ID
        let proposal_id = self.generate_proposal_id(&request);

        info!(
            "Patch generation complete: {} files, {} citations, confidence: {:.3}",
            patches.len(),
            citations.len(),
            confidence
        );

        Ok(PatchProposal {
            proposal_id,
            rationale,
            patches,
            citations,
            confidence,
            metadata: HashMap::new(),
        })
    }

    /// Build context for LLM generation
    fn build_context(&self, request: &PatchGenerationRequest) -> Result<PatchContext> {
        // Summarize evidence
        let evidence_summary = self.summarize_evidence(&request.evidence);

        // Build file contexts
        let mut file_contexts = HashMap::new();
        for file_path in &request.target_files {
            if let Some(context) = request.context.get(file_path) {
                file_contexts.insert(file_path.clone(), context.clone());
            }
        }

        // Define constraints
        let constraints = vec![
            "Follow existing code style and patterns".to_string(),
            "Include proper error handling".to_string(),
            "Add appropriate tests if needed".to_string(),
            "Maintain backward compatibility".to_string(),
            "Cite evidence for all changes".to_string(),
        ];

        Ok(PatchContext {
            request: request.clone(),
            evidence_summary,
            file_contexts,
            constraints,
        })
    }

    /// Summarize evidence for context
    fn summarize_evidence(&self, evidence: &[EvidenceSpan]) -> String {
        let mut summary = String::new();
        summary.push_str("Evidence Summary:\n");

        for (i, span) in evidence.iter().enumerate() {
            summary.push_str(&format!(
                "{}. {:?} ({}): {} (score: {:.3})\n",
                i + 1,
                span.evidence_type,
                span.file_path,
                span.content,
                span.score
            ));
        }

        summary
    }

    /// Compute confidence score based on evidence quality and patch complexity
    fn compute_confidence(&self, patches: &[FilePatch], evidence: &[EvidenceSpan]) -> Result<f32> {
        // Base confidence from evidence quality
        let evidence_score = if evidence.is_empty() {
            0.0
        } else {
            evidence.iter().map(|e| e.score).sum::<f32>() / evidence.len() as f32
        };

        // Adjust for patch complexity
        let total_lines: usize = patches.iter().map(|p| p.total_lines).sum();
        let complexity_factor = if total_lines > 100 {
            0.8
        } else if total_lines > 50 {
            0.9
        } else {
            1.0
        };

        // Adjust for number of files
        let file_factor = if patches.len() > 5 {
            0.8
        } else if patches.len() > 3 {
            0.9
        } else {
            1.0
        };

        let confidence = evidence_score * complexity_factor * file_factor;
        Ok(confidence.clamp(0.0, 1.0))
    }

    /// Generate unique proposal ID
    fn generate_proposal_id(&self, request: &PatchGenerationRequest) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        request.repo_id.hash(&mut hasher);
        request.description.hash(&mut hasher);
        request.target_files.hash(&mut hasher);

        format!("patch_{:x}", hasher.finish())
    }
}

/// Patch parser for converting text to structured format
pub struct PatchParser {
    hunk_parser: HunkParser,
}

impl Default for PatchParser {
    fn default() -> Self {
        Self::new()
    }
}

impl PatchParser {
    pub fn new() -> Self {
        Self {
            hunk_parser: HunkParser::new(),
        }
    }

    /// Parse patch text into structured format
    pub fn parse_patch_text(&self, patch_text: &str) -> Result<Vec<FilePatch>> {
        let mut patches = Vec::new();
        let mut current_file: Option<String> = None;
        let mut current_hunks = Vec::new();
        let mut current_lines = 0;

        for line in patch_text.lines() {
            if line.starts_with("--- ") || line.starts_with("+++ ") {
                // Save previous file if exists
                if let Some(file_path) = current_file.take() {
                    patches.push(FilePatch {
                        file_path,
                        hunks: current_hunks.clone(),
                        total_lines: current_lines,
                        metadata: HashMap::new(),
                    });
                }

                // Start new file
                if let Some(stripped) = line.strip_prefix("+++ ") {
                    current_file = Some(stripped.to_string());
                    current_hunks.clear();
                    current_lines = 0;
                }
            } else if line.starts_with("@@ ") {
                // Parse hunk header
                if let Ok(hunk) = self.hunk_parser.parse_hunk_header(line) {
                    current_hunks.push(hunk);
                }
            } else if !current_hunks.is_empty() {
                // Add line to current hunk
                if let Some(last_hunk) = current_hunks.last_mut() {
                    if let Some(stripped) = line.strip_prefix('+') {
                        last_hunk.modified_lines.push(stripped.to_string());
                        last_hunk.hunk_type = HunkType::Addition;
                    } else if let Some(stripped) = line.strip_prefix('-') {
                        last_hunk.modified_lines.push(stripped.to_string());
                        last_hunk.hunk_type = HunkType::Deletion;
                    } else {
                        last_hunk.context_lines.push(line.to_string());
                        last_hunk.hunk_type = HunkType::Context;
                    }
                    current_lines += 1;
                }
            }
        }

        // Save last file
        if let Some(file_path) = current_file {
            patches.push(FilePatch {
                file_path,
                hunks: current_hunks,
                total_lines: current_lines,
                metadata: HashMap::new(),
            });
        }

        Ok(patches)
    }
}

/// Hunk parser for unified diff format
pub struct HunkParser;

impl Default for HunkParser {
    fn default() -> Self {
        Self::new()
    }
}

impl HunkParser {
    pub fn new() -> Self {
        Self
    }

    /// Parse hunk header (e.g., "@@ -10,5 +15,6 @@"")
    pub fn parse_hunk_header(&self, header: &str) -> Result<PatchHunk> {
        // Simplified parser - in real implementation would parse the full header
        let parts: Vec<&str> = header.split_whitespace().collect();
        if parts.len() < 3 {
            return Err(AosError::Worker("Invalid hunk header".to_string()));
        }

        // Parse line numbers (simplified)
        let start_line = parts[1].parse::<usize>().unwrap_or(1);
        let end_line = parts[2].parse::<usize>().unwrap_or(start_line);

        Ok(PatchHunk {
            start_line,
            end_line,
            context_lines: Vec::new(),
            modified_lines: Vec::new(),
            hunk_type: HunkType::Modification,
        })
    }
}

/// Citation extractor for evidence
pub struct CitationExtractor;

impl Default for CitationExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl CitationExtractor {
    pub fn new() -> Self {
        Self
    }

    /// Extract citations from evidence spans
    pub fn extract_citations(&self, evidence: &[EvidenceSpan]) -> Result<Vec<EvidenceCitation>> {
        let mut citations = Vec::new();

        for span in evidence {
            let citation = EvidenceCitation {
                doc_id: span.doc_id.clone(),
                rev: span.rev.clone(),
                span_hash: span.span_hash.clone(),
                span_id: span.span_hash.clone(),
                evidence_type: span.evidence_type,
                score: span.score,
                file_path: span.file_path.clone(),
                line_range: (span.start_line, span.end_line),
                relevance_score: span.score,
                rationale: format!(
                    "Evidence from {:?}: {} (score: {:.3})",
                    span.evidence_type, span.file_path, span.score
                ),
            };
            citations.push(citation);
        }

        // Sort by relevance score
        citations.sort_by(|a, b| {
            b.relevance_score
                .partial_cmp(&a.relevance_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(citations)
    }
}

/// Mock LLM backend for testing
pub struct MockLlmBackend;

#[async_trait]
impl LlmBackend for MockLlmBackend {
    async fn generate_patch(&self, context: &PatchContext) -> Result<String> {
        Ok(format!(
            "--- a/{}\n+++ b/{}\n@@ -1,1 +1,2 @@\n-{}\n+{}\n+// Generated patch",
            context
                .request
                .target_files
                .first()
                .unwrap_or(&"test.rs".to_string()),
            context
                .request
                .target_files
                .first()
                .unwrap_or(&"test.rs".to_string()),
            "old code",
            "new code"
        ))
    }

    async fn extract_rationale(&self, _patch_text: &str) -> Result<String> {
        Ok("Mock rationale for testing".to_string())
    }
}

/// Simple rule-based backend that generates diffs using request description
/// and target file names, without external model calls.
pub struct RuleBasedLlmBackend;

impl RuleBasedLlmBackend {
    /// Parse transformation type from description
    fn parse_transformation(&self, description: &str) -> TransformationType {
        let desc_lower = description.to_lowercase();
        if desc_lower.contains("add function") || desc_lower.contains("implement") {
            TransformationType::AddFunction
        } else if desc_lower.contains("rename") {
            TransformationType::Rename
        } else if desc_lower.contains("add import") || desc_lower.contains("use ") {
            TransformationType::AddImport
        } else if desc_lower.contains("add error") || desc_lower.contains("error handling") {
            TransformationType::AddErrorHandling
        } else if desc_lower.contains("add test") {
            TransformationType::AddTest
        } else if desc_lower.contains("remove") || desc_lower.contains("delete") {
            TransformationType::Remove
        } else {
            TransformationType::Modify
        }
    }

    /// Extract function name from description
    fn extract_function_name(&self, description: &str) -> String {
        // Look for patterns like "add function foo" or "implement bar"
        let words: Vec<&str> = description.split_whitespace().collect();
        for (i, word) in words.iter().enumerate() {
            if (*word == "function" || *word == "implement" || *word == "method")
                && i + 1 < words.len()
            {
                return words[i + 1]
                    .trim_matches(|c: char| !c.is_alphanumeric() && c != '_')
                    .to_string();
            }
        }
        "new_function".to_string()
    }

    /// Extract identifier to rename from description
    fn extract_rename_target(&self, description: &str) -> (String, String) {
        // Look for patterns like "rename foo to bar" or "rename foo -> bar"
        let desc_lower = description.to_lowercase();
        if let Some(pos) = desc_lower.find("rename ") {
            let rest = &description[pos + 7..];
            let parts: Vec<&str> = rest
                .split(|c| c == ' ' || c == '-' || c == '>')
                .filter(|s| !s.is_empty() && *s != "to")
                .collect();
            if parts.len() >= 2 {
                return (parts[0].to_string(), parts[1].to_string());
            }
        }
        ("old_name".to_string(), "new_name".to_string())
    }

    /// Generate actual code transformation based on type
    fn generate_transformation(
        &self,
        transform_type: &TransformationType,
        description: &str,
        file_content: Option<&str>,
        file_path: &str,
    ) -> (String, String, usize, usize) {
        match transform_type {
            TransformationType::AddFunction => {
                let func_name = self.extract_function_name(description);
                let is_rust = file_path.ends_with(".rs");
                let is_python = file_path.ends_with(".py");

                let (old_lines, new_lines) = if is_rust {
                    (
                        String::new(),
                        format!(
                            "/// {}\npub fn {}() -> Result<()> {{\n    // Implementation based on: {}\n    Ok(())\n}}\n",
                            description, func_name, description
                        )
                    )
                } else if is_python {
                    (
                        String::new(),
                        format!(
                            "def {}():\n    \"\"\"{}.\"\"\"\n    # Implementation based on: {}\n    pass\n",
                            func_name, description, description
                        )
                    )
                } else {
                    (
                        String::new(),
                        format!("function {}() {{\n    // {}\n}}\n", func_name, description),
                    )
                };

                let line_num = file_content.map(|c| c.lines().count()).unwrap_or(1);
                (old_lines, new_lines, line_num, line_num)
            }
            TransformationType::Rename => {
                let (old_name, new_name) = self.extract_rename_target(description);
                if let Some(content) = file_content {
                    // Find the first occurrence of old_name and replace it
                    for (i, line) in content.lines().enumerate() {
                        if line.contains(&old_name) {
                            let new_line = line.replace(&old_name, &new_name);
                            return (line.to_string(), new_line, i + 1, i + 1);
                        }
                    }
                }
                (old_name.clone(), new_name.clone(), 1, 1)
            }
            TransformationType::AddImport => {
                let is_rust = file_path.ends_with(".rs");
                let new_import = if is_rust {
                    format!(
                        "use {};",
                        description.split_whitespace().last().unwrap_or("std::io")
                    )
                } else {
                    format!(
                        "import {}",
                        description.split_whitespace().last().unwrap_or("module")
                    )
                };
                (String::new(), new_import, 1, 1)
            }
            TransformationType::AddErrorHandling => {
                let is_rust = file_path.ends_with(".rs");
                if is_rust {
                    if let Some(content) = file_content {
                        // Find unwrap() or expect() calls to wrap with proper error handling
                        for (i, line) in content.lines().enumerate() {
                            if line.contains(".unwrap()") {
                                let new_line = line.replace(
                                    ".unwrap()",
                                    ".map_err(|e| AosError::Validation(e.to_string()))?",
                                );
                                return (line.to_string(), new_line, i + 1, i + 1);
                            }
                        }
                    }
                }
                (String::new(), "// Error handling added".to_string(), 1, 1)
            }
            TransformationType::AddTest => {
                let func_name = self.extract_function_name(description);
                let test_code = format!(
                    "#[test]\nfn test_{}() {{\n    // Test for: {}\n    assert!(true);\n}}\n",
                    func_name, description
                );
                let line_num = file_content.map(|c| c.lines().count()).unwrap_or(1);
                (String::new(), test_code, line_num, line_num)
            }
            TransformationType::Remove => {
                if let Some(content) = file_content {
                    // Find line containing the target to remove
                    let target = description.split_whitespace().last().unwrap_or("");
                    for (i, line) in content.lines().enumerate() {
                        if line.contains(target) {
                            return (line.to_string(), String::new(), i + 1, i + 1);
                        }
                    }
                }
                ("// removed".to_string(), String::new(), 1, 1)
            }
            TransformationType::Modify => {
                // Generic modification - add a documented change
                let comment = if file_path.ends_with(".rs") {
                    format!("// Modified: {}", description)
                } else if file_path.ends_with(".py") {
                    format!("# Modified: {}", description)
                } else {
                    format!("/* Modified: {} */", description)
                };
                (String::new(), comment, 1, 1)
            }
        }
    }

    /// Validate generated code syntax (basic validation)
    fn validate_syntax(&self, code: &str, file_path: &str) -> Result<()> {
        let is_rust = file_path.ends_with(".rs");

        if is_rust {
            // Basic Rust syntax validation
            let open_braces = code.matches('{').count();
            let close_braces = code.matches('}').count();
            if open_braces != close_braces {
                return Err(AosError::Validation(format!(
                    "Unbalanced braces: {} open, {} close",
                    open_braces, close_braces
                )));
            }

            let open_parens = code.matches('(').count();
            let close_parens = code.matches(')').count();
            if open_parens != close_parens {
                return Err(AosError::Validation(format!(
                    "Unbalanced parentheses: {} open, {} close",
                    open_parens, close_parens
                )));
            }
        }

        Ok(())
    }
}

/// Types of code transformations
#[derive(Debug, Clone)]
enum TransformationType {
    AddFunction,
    Rename,
    AddImport,
    AddErrorHandling,
    AddTest,
    Remove,
    Modify,
}

#[async_trait]
impl LlmBackend for RuleBasedLlmBackend {
    async fn generate_patch(&self, context: &PatchContext) -> Result<String> {
        let file = context
            .request
            .target_files
            .first()
            .cloned()
            .unwrap_or_else(|| "unknown.txt".to_string());

        // Parse transformation type from description
        let transform_type = self.parse_transformation(&context.request.description);

        // Get file content if available
        let file_content = context.file_contexts.get(&file).map(|s| s.as_str());

        // Generate actual code transformation
        let (old_code, new_code, start_line, end_line) = self.generate_transformation(
            &transform_type,
            &context.request.description,
            file_content,
            &file,
        );

        // Validate generated code syntax
        if !new_code.is_empty() {
            self.validate_syntax(&new_code, &file)?;
        }

        // Build unified diff format
        let header = format!("--- a/{}\n+++ b/{}\n", &file, &file);
        let hunk_header = format!(
            "@@ -{},{} +{},{} @@\n",
            start_line,
            if old_code.is_empty() {
                0
            } else {
                old_code.lines().count()
            },
            start_line,
            if new_code.is_empty() {
                0
            } else {
                new_code.lines().count()
            }
        );

        let mut body = String::new();

        // Add context lines if available
        if let Some(content) = file_content {
            // Add 3 lines of context before the change
            let lines: Vec<&str> = content.lines().collect();
            let context_start = start_line.saturating_sub(3);
            for i in context_start..start_line.saturating_sub(1) {
                if i < lines.len() {
                    body.push_str(&format!(" {}\n", lines[i]));
                }
            }
        }

        // Add old lines (deletions)
        for line in old_code.lines() {
            body.push_str(&format!("-{}\n", line));
        }

        // Add new lines (additions)
        for line in new_code.lines() {
            body.push_str(&format!("+{}\n", line));
        }

        Ok(format!("{}{}{}", header, hunk_header, body))
    }

    async fn extract_rationale(&self, patch_text: &str) -> Result<String> {
        // Extract meaningful rationale from the patch
        let mut rationale_parts = Vec::new();

        // Count additions and deletions
        let additions = patch_text
            .lines()
            .filter(|l| l.starts_with('+') && !l.starts_with("+++"))
            .count();
        let deletions = patch_text
            .lines()
            .filter(|l| l.starts_with('-') && !l.starts_with("---"))
            .count();

        if additions > 0 && deletions > 0 {
            rationale_parts.push(format!(
                "Modified {} lines, added {} lines",
                deletions, additions
            ));
        } else if additions > 0 {
            rationale_parts.push(format!("Added {} lines of new code", additions));
        } else if deletions > 0 {
            rationale_parts.push(format!("Removed {} lines", deletions));
        }

        // Extract function names or key identifiers from additions
        for line in patch_text.lines() {
            if line.starts_with('+') && !line.starts_with("+++") {
                let content = line.trim_start_matches('+');
                if content.contains("fn ") {
                    if let Some(func_start) = content.find("fn ") {
                        let func_part = &content[func_start + 3..];
                        if let Some(paren) = func_part.find('(') {
                            let func_name = &func_part[..paren].trim();
                            rationale_parts.push(format!("Implemented function '{}'", func_name));
                        }
                    }
                } else if content.contains("def ") {
                    if let Some(func_start) = content.find("def ") {
                        let func_part = &content[func_start + 4..];
                        if let Some(paren) = func_part.find('(') {
                            let func_name = &func_part[..paren].trim();
                            rationale_parts.push(format!("Implemented function '{}'", func_name));
                        }
                    }
                }
            }
        }

        if rationale_parts.is_empty() {
            Ok("Applied code transformation based on request description.".to_string())
        } else {
            Ok(rationale_parts.join(". "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_patch_generation() {
        let generator = PatchGenerator::new(
            Box::new(MockLlmBackend),
            PatchParser::new(),
            CitationExtractor::new(),
        );

        let request = PatchGenerationRequest {
            repo_id: "test_repo".to_string(),
            commit_sha: None,
            target_files: vec!["src/test.rs".to_string()],
            description: "Test patch".to_string(),
            evidence: vec![EvidenceSpan {
                doc_id: "test_doc".to_string(),
                rev: "v1".to_string(),
                span_hash: "hash1".to_string(),
                score: 0.9,
                evidence_type: crate::evidence::EvidenceType::Symbol,
                file_path: "src/test.rs".to_string(),
                start_line: 10,
                end_line: 15,
                content: "test symbol".to_string(),
                metadata: HashMap::new(),
            }],
            context: HashMap::new(),
        };

        let proposal = generator
            .generate_patch(request)
            .await
            .expect("Test patch generation should succeed");

        assert!(!proposal.proposal_id.is_empty());
        assert!(!proposal.rationale.is_empty());
        assert!(!proposal.patches.is_empty());
        assert!(!proposal.citations.is_empty());
        assert!(proposal.confidence > 0.0);
    }

    #[test]
    fn test_patch_parser() {
        let parser = PatchParser::new();
        let patch_text = "--- a/src/test.rs\n+++ b/src/test.rs\n@@ -1,1 +1,2 @@\n-old code\n+new code\n+// Generated patch";

        let patches = parser
            .parse_patch_text(patch_text)
            .expect("Test patch parsing should succeed");

        assert_eq!(patches.len(), 1);
        assert_eq!(patches[0].file_path, "src/test.rs");
        assert_eq!(patches[0].hunks.len(), 1);
    }

    #[test]
    fn test_citation_extractor() {
        let extractor = CitationExtractor::new();
        let evidence = vec![EvidenceSpan {
            doc_id: "test_doc".to_string(),
            rev: "v1".to_string(),
            span_hash: "hash1".to_string(),
            score: 0.9,
            evidence_type: crate::evidence::EvidenceType::Symbol,
            file_path: "src/test.rs".to_string(),
            start_line: 10,
            end_line: 15,
            content: "test symbol".to_string(),
            metadata: HashMap::new(),
        }];

        let citations = extractor
            .extract_citations(&evidence)
            .expect("Test citation extraction should succeed");

        assert_eq!(citations.len(), 1);
        assert_eq!(citations[0].span_id, "hash1");
        assert_eq!(citations[0].relevance_score, 0.9);
    }
}
