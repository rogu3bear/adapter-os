//! Evidence index manager - coordinates all evidence indices
//!
//! Manages symbol, test, doc, and vector indices with incremental updates
//! and multi-index search capabilities.

use crate::{
    chunking::CodeChunker,
    fts_index::{DocIndexImpl, IndexedDoc, IndexedTest, SymbolIndexImpl, TestIndexImpl},
    retrieval::{EvidenceSpan, EvidenceType},
    DocMetadata, TenantIndex,
};
use adapteros_codegraph::types::{Language, SymbolNode};
use adapteros_core::{B3Hash, Result};
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Statistics from indexing operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexStats {
    pub symbols_indexed: usize,
    pub tests_indexed: usize,
    pub docs_indexed: usize,
    pub chunks_indexed: usize,
    pub files_processed: usize,
    pub errors: Vec<String>,
}

impl IndexStats {
    pub fn new() -> Self {
        Self {
            symbols_indexed: 0,
            tests_indexed: 0,
            docs_indexed: 0,
            chunks_indexed: 0,
            files_processed: 0,
            errors: Vec::new(),
        }
    }

    pub fn merge(&mut self, other: IndexStats) {
        self.symbols_indexed += other.symbols_indexed;
        self.tests_indexed += other.tests_indexed;
        self.docs_indexed += other.docs_indexed;
        self.chunks_indexed += other.chunks_indexed;
        self.files_processed += other.files_processed;
        self.errors.extend(other.errors);
    }
}

impl Default for IndexStats {
    fn default() -> Self {
        Self::new()
    }
}

/// File change type
#[derive(Debug, Clone)]
pub enum ChangeType {
    Added,
    Modified,
    Deleted,
    Renamed,
}

/// File change information
#[derive(Debug, Clone)]
pub struct FileChange {
    pub path: PathBuf,
    pub change_type: ChangeType,
    pub old_path: Option<PathBuf>,
}

/// Embedding model trait (simplified for now)
pub trait EmbeddingModel: Send + Sync {
    fn encode_text(&self, text: &str) -> Result<Vec<f32>>;
    fn model_hash(&self) -> B3Hash;
}

/// Evidence index manager
pub struct EvidenceIndexManager {
    tenant_id: String,
    symbol_index: Arc<RwLock<SymbolIndexImpl>>,
    test_index: Arc<RwLock<TestIndexImpl>>,
    doc_index: Arc<RwLock<DocIndexImpl>>,
    vector_index: Arc<RwLock<TenantIndex>>,
    embedding_model: Option<Arc<dyn EmbeddingModel>>,
    chunker: CodeChunker,
}

impl EvidenceIndexManager {
    /// Create a new evidence index manager for a tenant
    pub async fn new(
        indices_root: PathBuf,
        tenant_id: String,
        embedding_model: Option<Arc<dyn EmbeddingModel>>,
    ) -> Result<Self> {
        let tenant_path = indices_root.join(&tenant_id);
        tokio::fs::create_dir_all(&tenant_path).await?;

        // Create individual indices
        let symbol_index = SymbolIndexImpl::new(tenant_path.clone(), tenant_id.clone())
            .await
            .context("Failed to create symbol index")?;

        let test_index = TestIndexImpl::new(tenant_path.clone(), tenant_id.clone())
            .await
            .context("Failed to create test index")?;

        let doc_index = DocIndexImpl::new(tenant_path.clone(), tenant_id.clone())
            .await
            .context("Failed to create doc index")?;

        // Create vector index
        let embedding_hash = embedding_model
            .as_ref()
            .map(|m| m.model_hash())
            .unwrap_or_else(|| B3Hash::hash(b"mock_embedding"));

        let vector_index = TenantIndex::new(tenant_path.join("vectors"), embedding_hash)?;

        Ok(Self {
            tenant_id,
            symbol_index: Arc::new(RwLock::new(symbol_index)),
            test_index: Arc::new(RwLock::new(test_index)),
            doc_index: Arc::new(RwLock::new(doc_index)),
            vector_index: Arc::new(RwLock::new(vector_index)),
            embedding_model,
            chunker: CodeChunker::default(),
        })
    }

    /// Search across all evidence indices
    pub async fn search_evidence(
        &self,
        query: &str,
        evidence_types: &[EvidenceType],
        repo_id: Option<&str>,
        max_results: usize,
    ) -> Result<Vec<EvidenceSpan>> {
        let mut all_spans = Vec::new();

        // Search sequentially across different index types
        // TODO: Optimize with parallel searches in the future
        for evidence_type in evidence_types {
            match evidence_type {
                EvidenceType::Symbol
                | EvidenceType::Test
                | EvidenceType::Doc
                | EvidenceType::Framework => {
                    // For now, skip FTS5 searches
                    // TODO: Implement conversion from IndexedSymbol/Test/Doc to EvidenceSpan
                }
                EvidenceType::Code => {
                    // Vector search for code chunks
                    if let Some(ref embedding_model) = self.embedding_model {
                        let embedding = embedding_model.encode_text(query)?;
                        let vector_index = self.vector_index.read().await;
                        let results = vector_index.retrieve(&embedding, max_results)?;
                        all_spans.extend(results);
                    }
                }
            }
        }

        // Apply deterministic ordering: (score desc, doc_id asc)
        all_spans.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.doc_id.cmp(&b.doc_id))
        });

        // Take top results
        all_spans.truncate(max_results);

        Ok(all_spans)
    }

    /// Update indices for a single file
    pub async fn update_file_indices(
        &mut self,
        file_path: &Path,
        repo_id: &str,
        commit_sha: &str,
    ) -> Result<IndexStats> {
        let mut stats = IndexStats::new();

        // Read file content
        let content = tokio::fs::read_to_string(file_path)
            .await
            .context("Failed to read file")?;

        // Compute file hash
        let file_hash = B3Hash::hash(content.as_bytes()).to_hex();

        // Parse file with CodeGraph
        let language = Language::from_path(file_path);
        if language.is_none() {
            // Skip unsupported file types
            return Ok(stats);
        }

        // For now, we'll use a simplified parsing approach
        // In production, integrate with adapteros-codegraph parser
        let symbols = self.extract_symbols_from_file(file_path, &content).await?;

        // Index symbols
        if !symbols.is_empty() {
            let symbol_index = self.symbol_index.write().await;
            let count = symbol_index
                .index_symbols(symbols.clone(), repo_id, commit_sha, &file_hash)
                .await?;
            stats.symbols_indexed = count;
        }

        // Extract and index tests
        let tests = self
            .extract_tests_from_file(file_path, &content, repo_id, commit_sha)
            .await?;
        if !tests.is_empty() {
            let test_index = self.test_index.write().await;
            let count = test_index.index_tests(tests, repo_id, commit_sha).await?;
            stats.tests_indexed = count;
        }

        // Extract and index documentation
        let docs = self
            .extract_docs_from_file(file_path, &content, repo_id, commit_sha)
            .await?;
        if !docs.is_empty() {
            let doc_index = self.doc_index.write().await;
            let count = doc_index.index_docs(docs, repo_id, commit_sha).await?;
            stats.docs_indexed = count;
        }

        // Chunk and index code
        if let Some(ref embedding_model) = self.embedding_model {
            let chunks = self
                .chunker
                .chunk_file(file_path, &content, &symbols, repo_id, commit_sha)?;

            let mut vector_index = self.vector_index.write().await;
            for chunk in chunks {
                let embedding = embedding_model.encode_text(&chunk.content)?;
                let metadata = DocMetadata {
                    doc_id: chunk.chunk_id.clone(),
                    rev: commit_sha.to_string(),
                    effectivity: "current".to_string(),
                    source_type: "code_chunk".to_string(),
                    superseded_by: None,
                };
                vector_index.add_document(
                    chunk.chunk_id.clone(),
                    chunk.content,
                    embedding,
                    metadata,
                )?;
                stats.chunks_indexed += 1;
            }
        }

        stats.files_processed = 1;
        Ok(stats)
    }

    /// Handle multiple file changes (incremental update)
    pub async fn handle_file_changes(
        &mut self,
        changes: &[FileChange],
        repo_id: &str,
        commit_sha: &str,
    ) -> Result<IndexStats> {
        let mut total_stats = IndexStats::new();

        for change in changes {
            match change.change_type {
                ChangeType::Added | ChangeType::Modified => {
                    match self
                        .update_file_indices(&change.path, repo_id, commit_sha)
                        .await
                    {
                        Ok(stats) => total_stats.merge(stats),
                        Err(e) => {
                            total_stats.errors.push(format!(
                                "Failed to index {}: {}",
                                change.path.display(),
                                e
                            ));
                        }
                    }
                }
                ChangeType::Deleted => {
                    if let Err(e) = self.remove_file_indices(&change.path, repo_id).await {
                        total_stats.errors.push(format!(
                            "Failed to remove indices for {}: {}",
                            change.path.display(),
                            e
                        ));
                    }
                }
                ChangeType::Renamed => {
                    if let Some(ref old_path) = change.old_path {
                        if let Err(e) = self.remove_file_indices(old_path, repo_id).await {
                            total_stats.errors.push(format!(
                                "Failed to remove old indices for {}: {}",
                                old_path.display(),
                                e
                            ));
                        }
                    }
                    match self
                        .update_file_indices(&change.path, repo_id, commit_sha)
                        .await
                    {
                        Ok(stats) => total_stats.merge(stats),
                        Err(e) => {
                            total_stats.errors.push(format!(
                                "Failed to index renamed file {}: {}",
                                change.path.display(),
                                e
                            ));
                        }
                    }
                }
            }
        }

        Ok(total_stats)
    }

    /// Remove indices for a specific file
    async fn remove_file_indices(&self, file_path: &Path, repo_id: &str) -> Result<()> {
        let file_path_str = file_path.display().to_string();

        let symbol_index = self.symbol_index.write().await;
        symbol_index
            .remove_file_symbols(&file_path_str, repo_id)
            .await?;

        let test_index = self.test_index.write().await;
        test_index
            .remove_file_tests(&file_path_str, repo_id)
            .await?;

        let doc_index = self.doc_index.write().await;
        doc_index.remove_file_docs(&file_path_str, repo_id).await?;

        Ok(())
    }

    /// Extract symbols from file (simplified)
    async fn extract_symbols_from_file(
        &self,
        _file_path: &Path,
        _content: &str,
    ) -> Result<Vec<SymbolNode>> {
        // In production, this would use adapteros-codegraph parser
        // For now, return empty vector
        Ok(Vec::new())
    }

    /// Extract tests from file
    async fn extract_tests_from_file(
        &self,
        file_path: &Path,
        content: &str,
        repo_id: &str,
        commit_sha: &str,
    ) -> Result<Vec<IndexedTest>> {
        let mut tests = Vec::new();

        // Simple heuristic: look for test functions
        let test_patterns = ["#[test]", "fn test_", "def test_", "it(", "test("];

        for (line_num, line) in content.lines().enumerate() {
            for pattern in &test_patterns {
                if line.contains(pattern) {
                    let test_name = self.extract_test_name(line);
                    if let Some(name) = test_name {
                        let test_id = format!("{}:{}:{}", repo_id, file_path.display(), line_num);
                        tests.push(IndexedTest {
                            test_id,
                            test_name: name,
                            file_path: file_path.display().to_string(),
                            start_line: line_num as i32,
                            end_line: line_num as i32, // Simplified
                            target_symbol_id: None,
                            target_function: None,
                            repo_id: repo_id.to_string(),
                            commit_sha: commit_sha.to_string(),
                        });
                    }
                }
            }
        }

        Ok(tests)
    }

    /// Extract documentation from file
    async fn extract_docs_from_file(
        &self,
        file_path: &Path,
        content: &str,
        repo_id: &str,
        commit_sha: &str,
    ) -> Result<Vec<IndexedDoc>> {
        let mut docs = Vec::new();

        // Extract README or markdown files
        if file_path.extension().and_then(|e| e.to_str()) == Some("md") {
            let doc_id = format!("{}:{}", repo_id, file_path.display());
            let title = file_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string();

            docs.push(IndexedDoc {
                doc_id,
                doc_type: "markdown".to_string(),
                file_path: file_path.display().to_string(),
                title,
                content: content.to_string(),
                repo_id: repo_id.to_string(),
                commit_sha: commit_sha.to_string(),
                start_line: None,
                end_line: None,
            });
        }

        // Extract doc comments (simplified)
        // In production, use tree-sitter to extract structured doc comments

        Ok(docs)
    }

    /// Extract test name from line
    fn extract_test_name(&self, line: &str) -> Option<String> {
        // Very simplified extraction
        if let Some(idx) = line.find("fn ") {
            let rest = &line[idx + 3..];
            if let Some(end) = rest.find('(') {
                return Some(rest[..end].trim().to_string());
            }
        }
        None
    }

    /// Index an entire repository
    pub async fn index_repository(
        &mut self,
        repo_path: &Path,
        repo_id: &str,
    ) -> Result<IndexStats> {
        let mut total_stats = IndexStats::new();

        // Walk repository and index all files
        let walker = walkdir::WalkDir::new(repo_path)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file());

        for entry in walker {
            let path = entry.path();

            // Skip hidden files and common ignore patterns
            if path.components().any(|c| {
                c.as_os_str()
                    .to_str()
                    .map(|s| s.starts_with('.'))
                    .unwrap_or(false)
            }) {
                continue;
            }

            // Only index supported file types
            if Language::from_path(path).is_none()
                && !path.extension().map(|e| e == "md").unwrap_or(false)
            {
                continue;
            }

            match self.update_file_indices(path, repo_id, "HEAD").await {
                Ok(stats) => total_stats.merge(stats),
                Err(e) => {
                    total_stats
                        .errors
                        .push(format!("Failed to index {}: {}", path.display(), e));
                }
            }
        }

        Ok(total_stats)
    }

    /// Get index statistics
    pub async fn get_stats(&self) -> Result<HashMap<String, usize>> {
        let mut stats = HashMap::new();

        let symbol_index = self.symbol_index.read().await;
        stats.insert("symbols".to_string(), symbol_index.count().await?);

        let test_index = self.test_index.read().await;
        stats.insert("tests".to_string(), test_index.count().await?);

        let doc_index = self.doc_index.read().await;
        stats.insert("docs".to_string(), doc_index.count().await?);

        Ok(stats)
    }
}
