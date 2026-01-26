//! Evidence retrieval module with real FTS5 and vector indices
//!
//! Provides evidence-grounded retrieval across symbols, tests, docs, and code chunks.

// use adapteros_core::B3Hash; // unused
use adapteros_retrieval::rag::{EvidenceIndexManager, EvidenceType as RagEvidenceType};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Evidence request
#[derive(Debug, Clone)]
pub struct EvidenceRequest {
    pub query: String,
    pub target_files: Vec<String>,
    pub repo_id: String,
    pub commit_sha: Option<String>,
    pub max_results: usize,
    pub min_score: f32,
}

/// Evidence result
#[derive(Debug, Clone)]
pub struct EvidenceResult {
    pub spans: Vec<EvidenceSpan>,
    pub total_found: usize,
    pub retrieval_time_ms: u64,
    pub sources_used: Vec<EvidenceType>,
}

/// Evidence span
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceSpan {
    pub doc_id: String,
    pub rev: String,
    pub span_hash: String,
    pub score: f32,
    pub evidence_type: EvidenceType,
    pub file_path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub content: String,
    pub metadata: HashMap<String, String>,
}

/// Evidence type
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum EvidenceType {
    Symbol,
    Test,
    Doc,
    Code,
    Framework,
}

/// Evidence citation for patches
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceCitation {
    pub doc_id: String,
    pub rev: String,
    pub span_hash: String,
    pub span_id: String,
    pub evidence_type: EvidenceType,
    pub score: f32,
    pub file_path: String,
    pub line_range: (usize, usize),
    pub relevance_score: f32,
    pub rationale: String,
}

/// Evidence retriever with real indices
pub struct EvidenceRetriever {
    evidence_manager: Arc<Mutex<EvidenceIndexManager>>,
}

impl EvidenceRetriever {
    /// Create new evidence retriever with real indices
    pub fn new(evidence_manager: Arc<Mutex<EvidenceIndexManager>>) -> Self {
        Self { evidence_manager }
    }

    /// Classify query to determine which evidence types to search
    fn classify_query(&self, query: &str) -> Vec<RagEvidenceType> {
        let query_lower = query.to_lowercase();
        let mut types = Vec::new();

        // Check for symbol-related queries
        if query_lower.contains("function")
            || query_lower.contains("struct")
            || query_lower.contains("class")
            || query_lower.contains("method")
            || query_lower.contains("trait")
        {
            types.push(RagEvidenceType::Symbol);
        }

        // Check for test-related queries
        if query_lower.contains("test") || query_lower.contains("spec") {
            types.push(RagEvidenceType::Test);
        }

        // Check for documentation queries
        if query_lower.contains("document")
            || query_lower.contains("readme")
            || query_lower.contains("doc")
        {
            types.push(RagEvidenceType::Doc);
        }

        // Always include code search
        types.push(RagEvidenceType::Code);

        // If no specific type detected, search all types
        if types.len() == 1 {
            types = vec![
                RagEvidenceType::Symbol,
                RagEvidenceType::Test,
                RagEvidenceType::Doc,
                RagEvidenceType::Code,
            ];
        }

        types
    }

    /// Convert RAG EvidenceSpan to Worker EvidenceSpan
    fn convert_evidence_span(&self, rag_span: adapteros_lora_rag::EvidenceSpan) -> EvidenceSpan {
        let evidence_type = match rag_span.evidence_type {
            Some(RagEvidenceType::Symbol) => EvidenceType::Symbol,
            Some(RagEvidenceType::Test) => EvidenceType::Test,
            Some(RagEvidenceType::Doc) => EvidenceType::Doc,
            Some(RagEvidenceType::Code) => EvidenceType::Code,
            Some(RagEvidenceType::Framework) => EvidenceType::Framework,
            None => EvidenceType::Code, // Default
        };

        EvidenceSpan {
            doc_id: rag_span.doc_id,
            rev: rag_span.rev,
            span_hash: rag_span.span_hash.to_hex(),
            score: rag_span.score,
            evidence_type,
            file_path: rag_span.file_path.unwrap_or_default(),
            start_line: rag_span.start_line.unwrap_or(0),
            end_line: rag_span.end_line.unwrap_or(0),
            content: rag_span.text,
            metadata: rag_span.metadata,
        }
    }

    /// Retrieve patch evidence using real indices
    pub async fn retrieve_patch_evidence(
        &self,
        request: &EvidenceRequest,
        _tenant_id: &str,
    ) -> Result<EvidenceResult> {
        let start = std::time::Instant::now();

        // Classify query to determine evidence types to search
        let evidence_types = self.classify_query(&request.query);

        // Search across all relevant indices
        let manager = self.evidence_manager.lock().await;
        let rag_spans = manager
            .search_evidence(
                &request.query,
                &evidence_types,
                Some(&request.repo_id),
                request.max_results,
            )
            .await?;

        // Convert RAG spans to worker spans
        let mut spans: Vec<EvidenceSpan> = rag_spans
            .into_iter()
            .map(|s| self.convert_evidence_span(s))
            .collect();

        // Filter by score threshold
        spans.retain(|span| span.score >= request.min_score);

        // Apply deterministic ordering: (score desc, doc_id asc)
        // Already done by evidence_manager, but ensure it
        spans.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.doc_id.cmp(&b.doc_id))
        });

        let retrieval_time = start.elapsed();
        let total_found = spans.len();

        // Determine which sources were actually used
        let mut sources_used: Vec<EvidenceType> = spans
            .iter()
            .map(|s| s.evidence_type)
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        sources_used.sort_by_key(|t| format!("{:?}", t));

        Ok(EvidenceResult {
            spans,
            total_found,
            retrieval_time_ms: retrieval_time.as_millis() as u64,
            sources_used,
        })
    }
}
