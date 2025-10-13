//! Core types for Git integration

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Type of file change
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeType {
    Create,
    Modify,
    Delete,
}

impl std::fmt::Display for ChangeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChangeType::Create => write!(f, "create"),
            ChangeType::Modify => write!(f, "modify"),
            ChangeType::Delete => write!(f, "delete"),
        }
    }
}

/// File change event for streaming to Cursor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChangeEvent {
    pub id: String,
    pub adapter_id: Option<String>,
    pub repo_id: String,
    pub file_path: PathBuf,
    pub change_type: ChangeType,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl FileChangeEvent {
    pub fn new(
        repo_id: String,
        file_path: PathBuf,
        change_type: ChangeType,
        adapter_id: Option<String>,
    ) -> Self {
        Self {
            id: uuid::Uuid::now_v7().to_string(),
            adapter_id,
            repo_id,
            file_path,
            change_type,
            timestamp: chrono::Utc::now(),
        }
    }
}

/// Git session for an adapter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitSession {
    pub id: String,
    pub adapter_id: String,
    pub repo_id: String,
    pub branch_name: String,
    pub base_commit_sha: String,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub ended_at: Option<chrono::DateTime<chrono::Utc>>,
    pub status: SessionStatus,
    pub merge_commit_sha: Option<String>,
}

/// Status of a Git session
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    Active,
    Merged,
    Abandoned,
}

impl std::fmt::Display for SessionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionStatus::Active => write!(f, "active"),
            SessionStatus::Merged => write!(f, "merged"),
            SessionStatus::Abandoned => write!(f, "abandoned"),
        }
    }
}

/// Batch of file changes for a single commit
#[derive(Debug, Clone)]
pub struct ChangeBatch {
    pub adapter_id: String,
    pub repo_id: String,
    pub changes: Vec<FileChangeEvent>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl ChangeBatch {
    pub fn new(adapter_id: String, repo_id: String) -> Self {
        Self {
            adapter_id,
            repo_id,
            changes: Vec::new(),
            created_at: chrono::Utc::now(),
        }
    }

    pub fn add_change(&mut self, change: FileChangeEvent) {
        self.changes.push(change);
    }

    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    pub fn len(&self) -> usize {
        self.changes.len()
    }
}
