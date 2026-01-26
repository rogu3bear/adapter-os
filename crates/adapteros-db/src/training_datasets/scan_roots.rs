//! Dataset scan root operations
//!
//! This module handles the persistence and retrieval of scan root metadata,
//! which represents the file system locations that were scanned to produce
//! a training dataset's content.

use crate::query_helpers::db_err;
use crate::Db;
use adapteros_core::Result;
use adapteros_core::{normalize_repo_slug, sanitize_repo_slug};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use crate::constants::DATASET_SCAN_ROOT_COLUMNS;

// Import normalization helper from parent
use super::normalize_optional_value;

// ============================================================================
// Scan Root Types
// ============================================================================

/// A single scan root entry representing a directory scanned for training data.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DatasetScanRoot {
    pub id: String,
    pub dataset_id: String,
    #[sqlx(default)]
    pub dataset_version_id: Option<String>,
    #[sqlx(default)]
    pub session_id: Option<String>,
    pub path: String,
    #[sqlx(default)]
    pub label: Option<String>,
    #[sqlx(default)]
    pub file_count: Option<i64>,
    #[sqlx(default)]
    pub byte_count: Option<i64>,
    #[sqlx(default)]
    pub content_hash_b3: Option<String>,
    #[sqlx(default)]
    pub scanned_at: Option<String>,
    pub ordinal: i32,
    #[sqlx(default)]
    pub repo_name: Option<String>,
    #[sqlx(default)]
    pub repo_slug: Option<String>,
    #[sqlx(default)]
    pub commit_sha: Option<String>,
    #[sqlx(default)]
    pub branch: Option<String>,
    #[sqlx(default)]
    pub remote_url: Option<String>,
    #[sqlx(default)]
    pub tenant_id: Option<String>,
    #[sqlx(default)]
    pub created_by: Option<String>,
    #[sqlx(default)]
    pub metadata_json: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Parameters for creating a dataset scan root entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateDatasetScanRootParams {
    /// Dataset ID this scan root belongs to
    pub dataset_id: String,
    /// Dataset version ID for versioned tracking
    pub dataset_version_id: Option<String>,
    /// Session ID for atomic operations
    pub session_id: Option<String>,
    /// Absolute path to the scan root directory
    pub path: String,
    /// Label describing this scan root's role (e.g., "primary", "reference")
    pub label: Option<String>,
    /// Number of files in this scan root
    pub file_count: Option<u64>,
    /// Total bytes of files in this scan root
    pub byte_count: Option<u64>,
    /// Content hash of the scan root
    pub content_hash_b3: Option<String>,
    /// Timestamp when the scan was performed
    pub scanned_at: Option<String>,
    /// Order of this scan root relative to others
    pub ordinal: i32,
    /// Repository name
    pub repo_name: Option<String>,
    /// Repository slug (org/repo format)
    pub repo_slug: Option<String>,
    /// Git commit SHA at scan time
    pub commit_sha: Option<String>,
    /// Git branch at scan time
    pub branch: Option<String>,
    /// Git remote URL
    pub remote_url: Option<String>,
    /// Tenant ID for isolation
    pub tenant_id: Option<String>,
    /// User who created this entry
    pub created_by: Option<String>,
    /// Additional metadata as JSON
    pub metadata_json: Option<String>,
}

impl CreateDatasetScanRootParams {
    /// Create a new builder for scan root creation parameters
    pub fn builder(
        dataset_id: impl Into<String>,
        path: impl Into<String>,
    ) -> CreateDatasetScanRootParamsBuilder {
        CreateDatasetScanRootParamsBuilder::new(dataset_id, path)
    }

    /// Create a minimal scan root params with just required fields
    pub fn new(dataset_id: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            dataset_id: dataset_id.into(),
            dataset_version_id: None,
            session_id: None,
            path: path.into(),
            label: None,
            file_count: None,
            byte_count: None,
            content_hash_b3: None,
            scanned_at: None,
            ordinal: 0,
            repo_name: None,
            repo_slug: None,
            commit_sha: None,
            branch: None,
            remote_url: None,
            tenant_id: None,
            created_by: None,
            metadata_json: None,
        }
    }
}

/// Builder for creating `CreateDatasetScanRootParams`
#[derive(Debug, Default)]
pub struct CreateDatasetScanRootParamsBuilder {
    dataset_id: String,
    dataset_version_id: Option<String>,
    session_id: Option<String>,
    path: String,
    label: Option<String>,
    file_count: Option<u64>,
    byte_count: Option<u64>,
    content_hash_b3: Option<String>,
    scanned_at: Option<String>,
    ordinal: i32,
    repo_name: Option<String>,
    repo_slug: Option<String>,
    commit_sha: Option<String>,
    branch: Option<String>,
    remote_url: Option<String>,
    tenant_id: Option<String>,
    created_by: Option<String>,
    metadata_json: Option<String>,
}

impl CreateDatasetScanRootParamsBuilder {
    /// Create a new builder with required fields
    pub fn new(dataset_id: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            dataset_id: dataset_id.into(),
            path: path.into(),
            ordinal: 0,
            ..Default::default()
        }
    }

    /// Set the dataset version ID
    pub fn dataset_version_id(mut self, id: impl Into<String>) -> Self {
        self.dataset_version_id = Some(id.into());
        self
    }

    /// Set the session ID for atomic operations
    pub fn session_id(mut self, id: impl Into<String>) -> Self {
        self.session_id = Some(id.into());
        self
    }

    /// Set the label describing this scan root's role
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Set the file count
    pub fn file_count(mut self, count: u64) -> Self {
        self.file_count = Some(count);
        self
    }

    /// Set the byte count
    pub fn byte_count(mut self, count: u64) -> Self {
        self.byte_count = Some(count);
        self
    }

    /// Set the content hash
    pub fn content_hash_b3(mut self, hash: impl Into<String>) -> Self {
        self.content_hash_b3 = Some(hash.into());
        self
    }

    /// Set the scanned_at timestamp
    pub fn scanned_at(mut self, timestamp: impl Into<String>) -> Self {
        self.scanned_at = Some(timestamp.into());
        self
    }

    /// Set the ordinal for ordering
    pub fn ordinal(mut self, ordinal: i32) -> Self {
        self.ordinal = ordinal;
        self
    }

    /// Set the repository name
    pub fn repo_name(mut self, name: impl Into<String>) -> Self {
        self.repo_name = Some(name.into());
        self
    }

    /// Set the repository slug
    pub fn repo_slug(mut self, slug: impl Into<String>) -> Self {
        self.repo_slug = Some(slug.into());
        self
    }

    /// Set the commit SHA
    pub fn commit_sha(mut self, sha: impl Into<String>) -> Self {
        self.commit_sha = Some(sha.into());
        self
    }

    /// Set the branch
    pub fn branch(mut self, branch: impl Into<String>) -> Self {
        self.branch = Some(branch.into());
        self
    }

    /// Set the remote URL
    pub fn remote_url(mut self, url: impl Into<String>) -> Self {
        self.remote_url = Some(url.into());
        self
    }

    /// Set the tenant ID
    pub fn tenant_id(mut self, id: impl Into<String>) -> Self {
        self.tenant_id = Some(id.into());
        self
    }

    /// Set the created_by user
    pub fn created_by(mut self, user: impl Into<String>) -> Self {
        self.created_by = Some(user.into());
        self
    }

    /// Set additional metadata as JSON
    pub fn metadata_json(mut self, json: impl Into<String>) -> Self {
        self.metadata_json = Some(json.into());
        self
    }

    /// Build the params
    pub fn build(self) -> CreateDatasetScanRootParams {
        let repo_slug =
            sanitize_repo_slug(self.repo_slug.as_deref()).map(|slug| normalize_repo_slug(&slug));
        let branch = normalize_optional_value(self.branch.as_deref());
        let commit_sha = normalize_optional_value(self.commit_sha.as_deref());
        CreateDatasetScanRootParams {
            dataset_id: self.dataset_id,
            dataset_version_id: self.dataset_version_id,
            session_id: self.session_id,
            path: self.path,
            label: self.label,
            file_count: self.file_count,
            byte_count: self.byte_count,
            content_hash_b3: self.content_hash_b3,
            scanned_at: self.scanned_at,
            ordinal: self.ordinal,
            repo_name: self.repo_name,
            repo_slug,
            commit_sha,
            branch,
            remote_url: self.remote_url,
            tenant_id: self.tenant_id,
            created_by: self.created_by,
            metadata_json: self.metadata_json,
        }
    }
}

/// Aggregate statistics for scan roots of a dataset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetScanRootStats {
    /// Number of scan roots
    pub root_count: i64,
    /// Total files across all roots
    pub total_files: i64,
    /// Total bytes across all roots
    pub total_bytes: i64,
}

// ============================================================================
// Database Operations
// ============================================================================

impl Db {
    /// Insert a single dataset scan root entry.
    pub async fn insert_dataset_scan_root(
        &self,
        params: &CreateDatasetScanRootParams,
    ) -> Result<String> {
        let id = Uuid::now_v7().to_string();
        let file_count = params.file_count.map(|v| v as i64);
        let byte_count = params.byte_count.map(|v| v as i64);

        sqlx::query(
            "INSERT INTO dataset_scan_roots (
                id, dataset_id, dataset_version_id, session_id, path, label,
                file_count, byte_count, content_hash_b3, scanned_at, ordinal,
                repo_name, repo_slug, commit_sha, branch, remote_url,
                tenant_id, created_by, metadata_json
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&params.dataset_id)
        .bind(&params.dataset_version_id)
        .bind(&params.session_id)
        .bind(&params.path)
        .bind(&params.label)
        .bind(file_count)
        .bind(byte_count)
        .bind(&params.content_hash_b3)
        .bind(&params.scanned_at)
        .bind(params.ordinal)
        .bind(&params.repo_name)
        .bind(&params.repo_slug)
        .bind(&params.commit_sha)
        .bind(&params.branch)
        .bind(&params.remote_url)
        .bind(&params.tenant_id)
        .bind(&params.created_by)
        .bind(&params.metadata_json)
        .execute(self.pool())
        .await
        .map_err(db_err("insert dataset scan root"))?;

        Ok(id)
    }

    /// Bulk insert dataset scan roots in a single transaction.
    pub async fn bulk_insert_dataset_scan_roots(
        &self,
        roots: &[CreateDatasetScanRootParams],
    ) -> Result<usize> {
        if roots.is_empty() {
            return Ok(0);
        }

        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(db_err("begin scan_roots transaction"))?;

        let mut count = 0;
        for params in roots {
            let id = Uuid::now_v7().to_string();
            let file_count = params.file_count.map(|v| v as i64);
            let byte_count = params.byte_count.map(|v| v as i64);

            sqlx::query(
                "INSERT INTO dataset_scan_roots (
                    id, dataset_id, dataset_version_id, session_id, path, label,
                    file_count, byte_count, content_hash_b3, scanned_at, ordinal,
                    repo_name, repo_slug, commit_sha, branch, remote_url,
                    tenant_id, created_by, metadata_json
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(&params.dataset_id)
            .bind(&params.dataset_version_id)
            .bind(&params.session_id)
            .bind(&params.path)
            .bind(&params.label)
            .bind(file_count)
            .bind(byte_count)
            .bind(&params.content_hash_b3)
            .bind(&params.scanned_at)
            .bind(params.ordinal)
            .bind(&params.repo_name)
            .bind(&params.repo_slug)
            .bind(&params.commit_sha)
            .bind(&params.branch)
            .bind(&params.remote_url)
            .bind(&params.tenant_id)
            .bind(&params.created_by)
            .bind(&params.metadata_json)
            .execute(&mut *tx)
            .await
            .map_err(db_err("insert scan root in transaction"))?;

            count += 1;
        }

        tx.commit()
            .await
            .map_err(db_err("commit scan_roots transaction"))?;

        Ok(count)
    }

    /// List all scan roots for a dataset.
    pub async fn list_dataset_scan_roots(&self, dataset_id: &str) -> Result<Vec<DatasetScanRoot>> {
        let query = format!(
            "SELECT {} FROM dataset_scan_roots WHERE dataset_id = ? ORDER BY ordinal, created_at",
            DATASET_SCAN_ROOT_COLUMNS
        );

        let roots = sqlx::query_as::<_, DatasetScanRoot>(&query)
            .bind(dataset_id)
            .fetch_all(self.pool())
            .await
            .map_err(db_err("list dataset scan roots"))?;

        Ok(roots)
    }

    /// List scan roots for a specific dataset version.
    pub async fn list_dataset_scan_roots_for_version(
        &self,
        dataset_version_id: &str,
    ) -> Result<Vec<DatasetScanRoot>> {
        let query = format!(
            "SELECT {} FROM dataset_scan_roots WHERE dataset_version_id = ? ORDER BY ordinal, created_at",
            DATASET_SCAN_ROOT_COLUMNS
        );

        let roots = sqlx::query_as::<_, DatasetScanRoot>(&query)
            .bind(dataset_version_id)
            .fetch_all(self.pool())
            .await
            .map_err(db_err("list dataset scan roots for version"))?;

        Ok(roots)
    }

    /// List scan roots for a specific session.
    pub async fn list_dataset_scan_roots_by_session(
        &self,
        session_id: &str,
    ) -> Result<Vec<DatasetScanRoot>> {
        let query = format!(
            "SELECT {} FROM dataset_scan_roots WHERE session_id = ? ORDER BY ordinal, created_at",
            DATASET_SCAN_ROOT_COLUMNS
        );

        let roots = sqlx::query_as::<_, DatasetScanRoot>(&query)
            .bind(session_id)
            .fetch_all(self.pool())
            .await
            .map_err(db_err("list dataset scan roots by session"))?;

        Ok(roots)
    }

    /// Update scan root entries with a new session version.
    pub async fn update_dataset_scan_root_session_version(
        &self,
        session_id: &str,
        dataset_version_id: &str,
    ) -> Result<u64> {
        let result = sqlx::query(
            "UPDATE dataset_scan_roots SET dataset_version_id = ? WHERE session_id = ?",
        )
        .bind(dataset_version_id)
        .bind(session_id)
        .execute(self.pool())
        .await
        .map_err(db_err("update scan root session version"))?;

        Ok(result.rows_affected())
    }

    /// Delete all scan roots for a session.
    pub async fn delete_dataset_scan_roots_by_session(&self, session_id: &str) -> Result<u64> {
        let result = sqlx::query("DELETE FROM dataset_scan_roots WHERE session_id = ?")
            .bind(session_id)
            .execute(self.pool())
            .await
            .map_err(db_err("delete scan roots by session"))?;

        Ok(result.rows_affected())
    }

    /// Get aggregate statistics for all scan roots of a dataset.
    pub async fn get_dataset_scan_root_stats(
        &self,
        dataset_id: &str,
    ) -> Result<DatasetScanRootStats> {
        let (root_count, total_files, total_bytes): (i64, i64, i64) = sqlx::query_as(
            "SELECT COUNT(*),
                COALESCE(SUM(file_count), 0),
                COALESCE(SUM(byte_count), 0)
             FROM dataset_scan_roots
             WHERE dataset_id = ?",
        )
        .bind(dataset_id)
        .fetch_one(self.pool())
        .await
        .map_err(db_err("get dataset scan root stats"))?;

        Ok(DatasetScanRootStats {
            root_count,
            total_files,
            total_bytes,
        })
    }
}
