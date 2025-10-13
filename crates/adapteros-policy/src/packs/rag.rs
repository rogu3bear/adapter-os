//! RAG Policy Pack
//!
//! Per-tenant isolation, deterministic ordering, and supersession logic.
//! Enforces strict per-tenant data boundaries and stable retrieval.

use crate::{Audit, Policy, PolicyContext, PolicyId, Severity};
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};

/// RAG policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagConfig {
    /// Index scope configuration
    pub index_scope: IndexScope,
    /// Required document tags
    pub doc_tags_required: Vec<String>,
    /// Embedding model hash
    pub embedding_model_hash: String,
    /// Top-K retrieval count
    pub topk: usize,
    /// Retrieval ordering rules
    pub order: Vec<OrderingRule>,
    /// Tenant isolation settings
    pub tenant_isolation: TenantIsolation,
    /// Supersession logic
    pub supersession: SupersessionConfig,
}

/// Index scope configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IndexScope {
    /// Per-tenant isolation
    PerTenant,
    /// Global shared index
    Global,
    /// Hybrid approach
    Hybrid,
}

/// Ordering rules for retrieval
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OrderingRule {
    /// Sort by score descending
    ScoreDesc,
    /// Sort by document ID ascending
    DocIdAsc,
    /// Sort by revision descending
    RevisionDesc,
    /// Sort by timestamp ascending
    TimestampAsc,
}

/// Tenant isolation settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantIsolation {
    /// Enforce strict isolation
    pub strict_isolation: bool,
    /// Allow cross-tenant queries
    pub allow_cross_tenant: bool,
    /// Isolation validation rules
    pub validation_rules: Vec<IsolationRule>,
}

/// Isolation validation rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IsolationRule {
    /// Check tenant ID match
    TenantIdMatch,
    /// Check namespace isolation
    NamespaceIsolation,
    /// Check data boundary
    DataBoundary,
}

/// Supersession configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupersessionConfig {
    /// Enable supersession detection
    pub enable_supersession: bool,
    /// Supersession warning threshold
    pub warning_threshold: f32,
    /// Supersession action
    pub action: SupersessionAction,
}

/// Supersession action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SupersessionAction {
    /// Warn but continue
    Warn,
    /// Block superseded content
    Block,
    /// Auto-update to latest
    AutoUpdate,
}

impl Default for RagConfig {
    fn default() -> Self {
        Self {
            index_scope: IndexScope::PerTenant,
            doc_tags_required: vec![
                "doc_id".to_string(),
                "rev".to_string(),
                "effectivity".to_string(),
                "source_type".to_string(),
            ],
            embedding_model_hash: "b3:default".to_string(),
            topk: 5,
            order: vec![OrderingRule::ScoreDesc, OrderingRule::DocIdAsc],
            tenant_isolation: TenantIsolation {
                strict_isolation: true,
                allow_cross_tenant: false,
                validation_rules: vec![
                    IsolationRule::TenantIdMatch,
                    IsolationRule::NamespaceIsolation,
                ],
            },
            supersession: SupersessionConfig {
                enable_supersession: true,
                warning_threshold: 0.8,
                action: SupersessionAction::Warn,
            },
        }
    }
}

/// Document metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentMetadata {
    /// Document ID
    pub doc_id: String,
    /// Revision number
    pub rev: u32,
    /// Effectivity information
    pub effectivity: String,
    /// Source type
    pub source_type: String,
    /// Tenant ID
    pub tenant_id: String,
    /// Timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Embedding hash
    pub embedding_hash: String,
}

/// Retrieval result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalResult {
    /// Document metadata
    pub metadata: DocumentMetadata,
    /// Relevance score
    pub score: f32,
    /// Supersession status
    pub supersession_status: SupersessionStatus,
}

/// Supersession status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SupersessionStatus {
    /// Current version
    Current,
    /// Superseded by newer version
    Superseded { newer_rev: u32 },
    /// Superseding newer version
    Superseding { older_rev: u32 },
}

/// RAG policy enforcement
pub struct RagPolicy {
    config: RagConfig,
}

impl RagPolicy {
    /// Create a new RAG policy
    pub fn new(config: RagConfig) -> Self {
        Self { config }
    }

    /// Validate document metadata
    pub fn validate_document_metadata(&self, metadata: &DocumentMetadata) -> Result<()> {
        // Check required tags
        for required_tag in &self.config.doc_tags_required {
            match required_tag.as_str() {
                "doc_id" => {
                    if metadata.doc_id.is_empty() {
                        return Err(AosError::PolicyViolation(
                            "Document ID is required".to_string(),
                        ));
                    }
                }
                "rev" => {
                    if metadata.rev == 0 {
                        return Err(AosError::PolicyViolation(
                            "Revision number is required".to_string(),
                        ));
                    }
                }
                "effectivity" => {
                    if metadata.effectivity.is_empty() {
                        return Err(AosError::PolicyViolation(
                            "Effectivity information is required".to_string(),
                        ));
                    }
                }
                "source_type" => {
                    if metadata.source_type.is_empty() {
                        return Err(AosError::PolicyViolation(
                            "Source type is required".to_string(),
                        ));
                    }
                }
                _ => {
                    return Err(AosError::PolicyViolation(format!(
                        "Unknown required tag: {}",
                        required_tag
                    )));
                }
            }
        }

        Ok(())
    }

    /// Validate tenant isolation
    pub fn validate_tenant_isolation(
        &self,
        query_tenant_id: &str,
        document_tenant_id: &str,
    ) -> Result<()> {
        if self.config.tenant_isolation.strict_isolation && query_tenant_id != document_tenant_id {
            return Err(AosError::PolicyViolation(format!(
                "Tenant isolation violation: query tenant {} != document tenant {}",
                query_tenant_id, document_tenant_id
            )));
        }

        Ok(())
    }

    /// Validate embedding model hash
    pub fn validate_embedding_model_hash(&self, hash: &str) -> Result<()> {
        if hash != self.config.embedding_model_hash {
            Err(AosError::PolicyViolation(format!(
                "Embedding model hash mismatch: expected {}, got {}",
                self.config.embedding_model_hash, hash
            )))
        } else {
            Ok(())
        }
    }

    /// Validate retrieval ordering
    pub fn validate_retrieval_ordering(&self, ordering: &[OrderingRule]) -> Result<()> {
        if ordering != self.config.order {
            Err(AosError::PolicyViolation(
                "Retrieval ordering does not match policy requirements".to_string(),
            ))
        } else {
            Ok(())
        }
    }

    /// Check supersession status
    pub fn check_supersession_status(
        &self,
        current_doc: &DocumentMetadata,
        other_docs: &[DocumentMetadata],
    ) -> SupersessionStatus {
        if !self.config.supersession.enable_supersession {
            return SupersessionStatus::Current;
        }

        let same_doc_id: Vec<&DocumentMetadata> = other_docs
            .iter()
            .filter(|doc| doc.doc_id == current_doc.doc_id)
            .collect();

        if same_doc_id.is_empty() {
            return SupersessionStatus::Current;
        }

        let max_rev = same_doc_id.iter().map(|doc| doc.rev).max().unwrap();
        let min_rev = same_doc_id.iter().map(|doc| doc.rev).min().unwrap();

        if current_doc.rev == max_rev {
            SupersessionStatus::Current
        } else if current_doc.rev < max_rev {
            SupersessionStatus::Superseded { newer_rev: max_rev }
        } else {
            SupersessionStatus::Superseding { older_rev: min_rev }
        }
    }

    /// Apply supersession action
    pub fn apply_supersession_action(
        &self,
        status: &SupersessionStatus,
        result: &mut RetrievalResult,
    ) -> Result<()> {
        match (&self.config.supersession.action, status) {
            (SupersessionAction::Warn, SupersessionStatus::Superseded { newer_rev }) => {
                tracing::warn!(
                    "Document {} is superseded by revision {}",
                    result.metadata.doc_id,
                    newer_rev
                );
                result.supersession_status = status.clone();
            }
            (SupersessionAction::Block, SupersessionStatus::Superseded { .. }) => {
                return Err(AosError::PolicyViolation(
                    "Superseded document is blocked by policy".to_string(),
                ));
            }
            (SupersessionAction::AutoUpdate, SupersessionStatus::Superseded { newer_rev }) => {
                // In a real implementation, this would update the document
                tracing::info!(
                    "Auto-updating document {} to revision {}",
                    result.metadata.doc_id,
                    newer_rev
                );
                result.metadata.rev = *newer_rev;
                result.supersession_status = SupersessionStatus::Current;
            }
            _ => {
                result.supersession_status = status.clone();
            }
        }

        Ok(())
    }

    /// Validate top-K parameter
    pub fn validate_topk(&self, topk: usize) -> Result<()> {
        if topk > self.config.topk {
            Err(AosError::PolicyViolation(format!(
                "Top-K parameter {} exceeds maximum {}",
                topk, self.config.topk
            )))
        } else {
            Ok(())
        }
    }

    /// Sort retrieval results according to policy
    pub fn sort_retrieval_results(
        &self,
        mut results: Vec<RetrievalResult>,
    ) -> Vec<RetrievalResult> {
        results.sort_by(|a, b| {
            for rule in &self.config.order {
                match rule {
                    OrderingRule::ScoreDesc => {
                        let score_cmp = b
                            .score
                            .partial_cmp(&a.score)
                            .unwrap_or(std::cmp::Ordering::Equal);
                        if score_cmp != std::cmp::Ordering::Equal {
                            return score_cmp;
                        }
                    }
                    OrderingRule::DocIdAsc => {
                        let doc_id_cmp = a.metadata.doc_id.cmp(&b.metadata.doc_id);
                        if doc_id_cmp != std::cmp::Ordering::Equal {
                            return doc_id_cmp;
                        }
                    }
                    OrderingRule::RevisionDesc => {
                        let rev_cmp = b.metadata.rev.cmp(&a.metadata.rev);
                        if rev_cmp != std::cmp::Ordering::Equal {
                            return rev_cmp;
                        }
                    }
                    OrderingRule::TimestampAsc => {
                        let timestamp_cmp = a.metadata.timestamp.cmp(&b.metadata.timestamp);
                        if timestamp_cmp != std::cmp::Ordering::Equal {
                            return timestamp_cmp;
                        }
                    }
                }
            }
            std::cmp::Ordering::Equal
        });

        results
    }
}

impl Policy for RagPolicy {
    fn id(&self) -> PolicyId {
        PolicyId::Rag
    }

    fn name(&self) -> &'static str {
        "RAG"
    }

    fn severity(&self) -> Severity {
        Severity::High
    }

    fn enforce(&self, _ctx: &dyn PolicyContext) -> Result<Audit> {
        let violations = Vec::new();

        // Basic validation - in a real implementation, this would check
        // specific policy requirements

        if violations.is_empty() {
            Ok(Audit::passed(self.id()))
        } else {
            Ok(Audit::failed(self.id(), violations))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_rag_policy_creation() {
        let config = RagConfig::default();
        let policy = RagPolicy::new(config);
        assert_eq!(policy.id(), PolicyId::Rag);
        assert_eq!(policy.name(), "RAG");
        assert_eq!(policy.severity(), Severity::High);
    }

    #[test]
    fn test_rag_config_default() {
        let config = RagConfig::default();
        assert_eq!(config.topk, 5);
        assert!(config.tenant_isolation.strict_isolation);
        assert!(!config.tenant_isolation.allow_cross_tenant);
        assert!(config.supersession.enable_supersession);
    }

    #[test]
    fn test_validate_document_metadata() {
        let config = RagConfig::default();
        let policy = RagPolicy::new(config);

        let valid_metadata = DocumentMetadata {
            doc_id: "doc1".to_string(),
            rev: 1,
            effectivity: "all".to_string(),
            source_type: "manual".to_string(),
            tenant_id: "tenant1".to_string(),
            timestamp: Utc::now(),
            embedding_hash: "b3:default".to_string(),
        };

        assert!(policy.validate_document_metadata(&valid_metadata).is_ok());

        let invalid_metadata = DocumentMetadata {
            doc_id: "".to_string(), // Empty doc_id
            rev: 1,
            effectivity: "all".to_string(),
            source_type: "manual".to_string(),
            tenant_id: "tenant1".to_string(),
            timestamp: Utc::now(),
            embedding_hash: "b3:default".to_string(),
        };

        assert!(policy
            .validate_document_metadata(&invalid_metadata)
            .is_err());
    }

    #[test]
    fn test_validate_tenant_isolation() {
        let config = RagConfig::default();
        let policy = RagPolicy::new(config);

        // Valid case - same tenant
        assert!(policy
            .validate_tenant_isolation("tenant1", "tenant1")
            .is_ok());

        // Invalid case - different tenants
        assert!(policy
            .validate_tenant_isolation("tenant1", "tenant2")
            .is_err());
    }

    #[test]
    fn test_validate_embedding_model_hash() {
        let config = RagConfig::default();
        let policy = RagPolicy::new(config);

        // Valid case
        assert!(policy.validate_embedding_model_hash("b3:default").is_ok());

        // Invalid case
        assert!(policy
            .validate_embedding_model_hash("b3:different")
            .is_err());
    }

    #[test]
    fn test_check_supersession_status() {
        let config = RagConfig::default();
        let policy = RagPolicy::new(config);

        let current_doc = DocumentMetadata {
            doc_id: "doc1".to_string(),
            rev: 1,
            effectivity: "all".to_string(),
            source_type: "manual".to_string(),
            tenant_id: "tenant1".to_string(),
            timestamp: Utc::now(),
            embedding_hash: "b3:default".to_string(),
        };

        let other_docs = vec![DocumentMetadata {
            doc_id: "doc1".to_string(),
            rev: 2, // Newer revision
            effectivity: "all".to_string(),
            source_type: "manual".to_string(),
            tenant_id: "tenant1".to_string(),
            timestamp: Utc::now(),
            embedding_hash: "b3:default".to_string(),
        }];

        let status = policy.check_supersession_status(&current_doc, &other_docs);
        match status {
            SupersessionStatus::Superseded { newer_rev } => {
                assert_eq!(newer_rev, 2);
            }
            _ => panic!("Expected superseded status"),
        }
    }

    #[test]
    fn test_validate_topk() {
        let config = RagConfig::default();
        let policy = RagPolicy::new(config);

        // Valid case
        assert!(policy.validate_topk(5).is_ok());
        assert!(policy.validate_topk(3).is_ok());

        // Invalid case
        assert!(policy.validate_topk(10).is_err());
    }

    #[test]
    fn test_sort_retrieval_results() {
        let config = RagConfig::default();
        let policy = RagPolicy::new(config);

        let results = vec![
            RetrievalResult {
                metadata: DocumentMetadata {
                    doc_id: "doc2".to_string(),
                    rev: 1,
                    effectivity: "all".to_string(),
                    source_type: "manual".to_string(),
                    tenant_id: "tenant1".to_string(),
                    timestamp: Utc::now(),
                    embedding_hash: "b3:default".to_string(),
                },
                score: 0.8,
                supersession_status: SupersessionStatus::Current,
            },
            RetrievalResult {
                metadata: DocumentMetadata {
                    doc_id: "doc1".to_string(),
                    rev: 1,
                    effectivity: "all".to_string(),
                    source_type: "manual".to_string(),
                    tenant_id: "tenant1".to_string(),
                    timestamp: Utc::now(),
                    embedding_hash: "b3:default".to_string(),
                },
                score: 0.9,
                supersession_status: SupersessionStatus::Current,
            },
        ];

        let sorted = policy.sort_retrieval_results(results);
        assert_eq!(sorted[0].score, 0.9); // Higher score first
        assert_eq!(sorted[0].metadata.doc_id, "doc1");
    }
}
