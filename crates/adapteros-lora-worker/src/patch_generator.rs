//! Patch generation pipeline with LLM integration
//!
//! Implements patch generation with evidence citations and structured output.
//! Aligns with Code Policy requirements and evidence-first philosophy.

use crate::evidence::{EvidenceCitation, EvidenceSpan, EvidenceType};
use async_trait::async_trait;
use adapteros_core::{AosError, Result};
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
        Ok(confidence.min(1.0).max(0.0))
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
                if line.starts_with("+++ ") {
                    current_file = Some(line[4..].to_string());
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
                    if line.starts_with('+') {
                        last_hunk.modified_lines.push(line[1..].to_string());
                        last_hunk.hunk_type = HunkType::Addition;
                    } else if line.starts_with('-') {
                        last_hunk.modified_lines.push(line[1..].to_string());
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
                evidence_type: EvidenceType::Symbol,
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
            evidence_type: EvidenceType::Symbol,
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
