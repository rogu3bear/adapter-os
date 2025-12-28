//! RAG Staleness Checker Service
//!
//! Background service that proactively detects when RAG snapshots become stale.
//! A snapshot is considered stale when referenced documents have been deleted or superseded.
//!
//! This prevents the catastrophic failure where replay silently proceeds with missing
//! RAG documents, producing different outputs than the original run while evidence
//! exports falsely claim comparability.

use crate::state::AppState;
use adapteros_db::Db;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

/// Configuration for the staleness checker
pub struct RagStalenessCheckerConfig {
    /// How often to check for stale snapshots (default: 15 minutes)
    pub check_interval: Duration,
    /// Only check snapshots not validated in this many minutes
    pub older_than_minutes: i64,
    /// Maximum number of snapshots to check per run
    pub batch_size: i64,
}

impl Default for RagStalenessCheckerConfig {
    fn default() -> Self {
        Self {
            check_interval: Duration::from_secs(15 * 60), // 15 minutes
            older_than_minutes: 15,
            batch_size: 100,
        }
    }
}

/// RAG Staleness Checker background service
pub struct RagStalenessChecker {
    db: Arc<Db>,
    config: RagStalenessCheckerConfig,
}

impl RagStalenessChecker {
    /// Create a new staleness checker
    pub fn new(db: Arc<Db>, config: RagStalenessCheckerConfig) -> Self {
        Self { db, config }
    }

    /// Create with default configuration
    pub fn with_defaults(db: Arc<Db>) -> Self {
        Self::new(db, RagStalenessCheckerConfig::default())
    }

    /// Run the staleness checker as a background task
    ///
    /// This function runs indefinitely, checking for stale snapshots
    /// at the configured interval.
    pub async fn run(&self) {
        info!(
            interval_secs = self.config.check_interval.as_secs(),
            older_than_minutes = self.config.older_than_minutes,
            batch_size = self.config.batch_size,
            "Starting RAG staleness checker background service"
        );

        let mut ticker = interval(self.config.check_interval);

        loop {
            ticker.tick().await;

            match self.check_staleness_batch().await {
                Ok((checked, marked_stale)) => {
                    if marked_stale > 0 {
                        info!(
                            checked = checked,
                            marked_stale = marked_stale,
                            "RAG staleness check completed - marked stale snapshots"
                        );
                    } else {
                        debug!(
                            checked = checked,
                            "RAG staleness check completed - no stale snapshots found"
                        );
                    }
                }
                Err(e) => {
                    error!(error = %e, "RAG staleness check failed");
                }
            }
        }
    }

    /// Check a batch of snapshots for staleness
    ///
    /// Returns (total_checked, marked_stale)
    async fn check_staleness_batch(&self) -> Result<(usize, u64), Box<dyn std::error::Error + Send + Sync>> {
        // Get candidates that need checking
        let candidates = self
            .db
            .get_stale_check_candidates(self.config.older_than_minutes, self.config.batch_size)
            .await?;

        if candidates.is_empty() {
            return Ok((0, 0));
        }

        let total = candidates.len();
        let mut stale_ids = Vec::new();
        let mut checked_ids = Vec::new();

        for (inference_id, tenant_id, rag_doc_ids_json) in candidates {
            checked_ids.push(inference_id.clone());

            // Parse doc IDs from JSON
            let doc_ids: Vec<String> = match serde_json::from_str(&rag_doc_ids_json) {
                Ok(ids) => ids,
                Err(e) => {
                    warn!(
                        inference_id = %inference_id,
                        error = %e,
                        "Failed to parse rag_doc_ids_json, skipping"
                    );
                    continue;
                }
            };

            if doc_ids.is_empty() {
                continue;
            }

            // Check if all documents still exist
            let any_missing = self.check_documents_missing(&tenant_id, &doc_ids).await?;

            if any_missing {
                stale_ids.push(inference_id);
            }
        }

        // Update timestamps for all checked IDs
        if !checked_ids.is_empty() {
            self.db.update_staleness_checked(&checked_ids).await?;
        }

        // Mark stale ones
        let marked = if !stale_ids.is_empty() {
            self.db.mark_rag_stale(&stale_ids).await?
        } else {
            0
        };

        Ok((total, marked))
    }

    /// Check if any of the given document IDs are missing from the database
    async fn check_documents_missing(
        &self,
        tenant_id: &str,
        doc_ids: &[String],
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        // Extract base document IDs (strip chunk suffix if present)
        let base_doc_ids: Vec<String> = doc_ids
            .iter()
            .map(|id| {
                // Doc IDs may be in format "{document_id}__chunk_{index}"
                if let Some(idx) = id.find("__chunk_") {
                    id[..idx].to_string()
                } else {
                    id.clone()
                }
            })
            .collect();

        // Deduplicate
        let unique_ids: std::collections::HashSet<_> = base_doc_ids.iter().cloned().collect();

        // Check each document
        for doc_id in unique_ids {
            let exists = self.db.rag_document_exists(tenant_id, &doc_id).await?;
            if !exists {
                return Ok(true);
            }

            // Also check for supersession
            let superseded = self.db.rag_document_superseded(tenant_id, &doc_id).await?;
            if superseded {
                return Ok(true);
            }
        }

        Ok(false)
    }
}

/// Spawn the staleness checker as a background task
///
/// Returns a handle that can be used to abort the task if needed.
pub fn spawn_staleness_checker(db: Arc<Db>) -> tokio::task::JoinHandle<()> {
    let checker = RagStalenessChecker::with_defaults(db);
    tokio::spawn(async move {
        checker.run().await;
    })
}

/// Spawn with custom configuration
pub fn spawn_staleness_checker_with_config(
    db: Arc<Db>,
    config: RagStalenessCheckerConfig,
) -> tokio::task::JoinHandle<()> {
    let checker = RagStalenessChecker::new(db, config);
    tokio::spawn(async move {
        checker.run().await;
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = RagStalenessCheckerConfig::default();
        assert_eq!(config.check_interval, Duration::from_secs(15 * 60));
        assert_eq!(config.older_than_minutes, 15);
        assert_eq!(config.batch_size, 100);
    }
}
