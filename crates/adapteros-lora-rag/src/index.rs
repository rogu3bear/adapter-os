//! Per-tenant HNSW index with metadata filtering

use crate::{DocMetadata, EvidenceSpan};
use adapteros_core::{AosError, B3Hash, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Per-tenant index
#[derive(Clone)]
pub struct TenantIndex {
    _path: PathBuf,
    _embedding_hash: B3Hash,
    documents: HashMap<String, (String, DocMetadata, Vec<f32>)>,
}

impl TenantIndex {
    /// Create new tenant index
    pub fn new<P: AsRef<Path>>(path: P, embedding_hash: B3Hash) -> Result<Self> {
        std::fs::create_dir_all(path.as_ref())
            .map_err(|e| AosError::Other(format!("Failed to create index directory: {}", e)))?;

        Ok(Self {
            _path: path.as_ref().to_path_buf(),
            _embedding_hash: embedding_hash,
            documents: HashMap::new(),
        })
    }

    /// Add document
    pub fn add_document(
        &mut self,
        doc_id: String,
        text: String,
        embedding: Vec<f32>,
        metadata: DocMetadata,
    ) -> Result<()> {
        self.documents
            .insert(doc_id.clone(), (text, metadata, embedding));
        Ok(())
    }

    /// Retrieve with deterministic ordering
    pub fn retrieve(&self, query: &[f32], top_k: usize) -> Result<Vec<EvidenceSpan>> {
        // Compute cosine similarity
        let mut scores: Vec<(String, f32)> = self
            .documents
            .iter()
            .map(|(doc_id, (_, _, emb))| {
                let score = cosine_similarity(query, emb);
                (doc_id.clone(), score)
            })
            .collect();

        // Deterministic tie-breaking: (score desc, doc_id asc)
        scores.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
        });

        // Take top K
        let results: Vec<EvidenceSpan> = scores
            .into_iter()
            .take(top_k)
            .filter_map(|(doc_id, score)| {
                self.documents.get(&doc_id).map(|(text, metadata, _)| {
                    let span_hash = compute_span_hash(&doc_id, text, &metadata.rev);
                    EvidenceSpan {
                        doc_id: doc_id.clone(),
                        rev: metadata.rev.clone(),
                        text: text.clone(),
                        score,
                        span_hash,
                        superseded: metadata.superseded_by.clone(),
                        evidence_type: None,
                        file_path: None,
                        start_line: None,
                        end_line: None,
                        metadata: std::collections::HashMap::new(),
                    }
                })
            })
            .collect();

        Ok(results)
    }
}

/// Compute cosine similarity
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len());
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    dot / (norm_a * norm_b)
}

/// Compute span hash
fn compute_span_hash(doc_id: &str, text: &str, rev: &str) -> B3Hash {
    let combined = format!("{}||{}||{}", doc_id, rev, text);
    B3Hash::hash(combined.as_bytes())
}
