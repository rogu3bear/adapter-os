//! Branch manager for adapter lifecycle

use adapteros_core::{AosError, Result};
use chrono;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Configuration for the branch manager
#[derive(Debug, Clone)]
pub struct BranchManagerConfig {
    pub branch_prefix: String,
    pub preserve_abandoned_branches: bool,
}

impl Default for BranchManagerConfig {
    fn default() -> Self {
        Self {
            branch_prefix: "adapteros".to_string(),
            preserve_abandoned_branches: false,
        }
    }
}

/// Git session status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionStatus {
    Active,
    Merged,
    Abandoned,
}

/// Git session information
#[derive(Debug, Clone)]
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

/// Branch operation types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchOperation {
    Create,
    Switch,
    Merge,
    Delete,
}

/// Branch manager for adapter sessions
#[derive(Clone)]
pub struct BranchManager {
    config: BranchManagerConfig,
    db: adapteros_db::Db,
    active_sessions: Arc<RwLock<HashMap<String, GitSession>>>,
    repositories: Arc<RwLock<HashMap<String, PathBuf>>>,
}

impl BranchManager {
    /// Create a new branch manager
    pub async fn new(db: adapteros_db::Db, config: BranchManagerConfig) -> Result<Self> {
        let manager = Self {
            config,
            db,
            active_sessions: Arc::new(RwLock::new(HashMap::new())),
            repositories: Arc::new(RwLock::new(HashMap::new())),
        };

        // Load active sessions from database
        manager.load_active_sessions().await?;

        Ok(manager)
    }

    /// Register a repository with the branch manager
    pub async fn register_repository(&self, repo_id: String, repo_path: PathBuf) -> Result<()> {
        self.repositories
            .write()
            .await
            .insert(repo_id.clone(), repo_path.clone());
        info!(
            "Registered repository {} at {}",
            repo_id,
            repo_path.display()
        );
        Ok(())
    }

    /// Get repository path
    pub async fn get_repository_path(&self, repo_id: &str) -> Option<PathBuf> {
        self.repositories.read().await.get(repo_id).cloned()
    }

    /// Start a new Git session for an adapter
    pub async fn start_session(
        &self,
        adapter_id: String,
        repo_id: String,
        base_branch: Option<String>,
    ) -> Result<GitSession> {
        let repo_path = self
            .get_repository_path(&repo_id)
            .await
            .ok_or_else(|| AosError::Git(format!("Repository {} not found", repo_id)))?;

        let branch_prefix = self.config.branch_prefix.clone();
        let adapter_id_clone = adapter_id.clone();

        // Run all git2 operations in spawn_blocking
        let (base_commit_sha, branch_name) = tokio::task::spawn_blocking(move || {
            // Open repository
            let repo = git2::Repository::open(&repo_path)
                .map_err(|e| AosError::Git(format!("Failed to open repository: {}", e)))?;

            // Get base commit SHA
            let base_commit_sha = Self::get_commit_sha_sync(&repo, base_branch.as_deref())?;

            // Generate versioned branch name
            let branch_name = format!("{}/v1/{}", branch_prefix, adapter_id_clone);

            // Create branch
            let commit = repo
                .find_commit(
                    git2::Oid::from_str(&base_commit_sha)
                        .map_err(|e| AosError::Git(format!("Invalid commit SHA: {}", e)))?,
                )
                .map_err(|e| AosError::Git(format!("Failed to find commit: {}", e)))?;

            repo.branch(&branch_name, &commit, false)
                .map_err(|e| AosError::Git(format!("Failed to create branch: {}", e)))?;

            // Switch to the new branch
            Self::switch_branch_sync(&repo, &branch_name)?;

            Ok::<_, AosError>((base_commit_sha, branch_name))
        })
        .await
        .map_err(|e| AosError::Git(format!("Task join error: {}", e)))??;

        info!(
            "Created branch {} for adapter {} on repo {} (base: {})",
            branch_name, adapter_id, repo_id, base_commit_sha
        );

        // Create session
        let session = GitSession {
            id: uuid::Uuid::now_v7().to_string(),
            adapter_id: adapter_id.clone(),
            repo_id: repo_id.clone(),
            branch_name: branch_name.clone(),
            base_commit_sha: base_commit_sha.clone(),
            started_at: chrono::Utc::now(),
            ended_at: None,
            status: SessionStatus::Active,
            merge_commit_sha: None,
        };

        // Store session in memory
        self.active_sessions
            .write()
            .await
            .insert(session.id.clone(), session.clone());

        // Store session in database
        self.db.create_git_session(
            &session.id,
            &session.adapter_id,
            &session.repo_id,
            &session.branch_name,
            &session.base_commit_sha,
        ).await.map_err(|e| {
            AosError::Database(format!("Failed to store git session: {}", e))
        })?;

        Ok(session)
    }

    /// End a Git session (merge or abandon)
    pub async fn end_session(&self, session_id: &str, merge: bool) -> Result<Option<String>> {
        let mut sessions = self.active_sessions.write().await;
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| AosError::Git(format!("Session {} not found", session_id)))?;

        let repo_path = self
            .get_repository_path(&session.repo_id)
            .await
            .ok_or_else(|| AosError::Git(format!("Repository {} not found", session.repo_id)))?;

        let branch_name = session.branch_name.clone();
        let adapter_id = session.adapter_id.clone();
        let preserve_branches = self.config.preserve_abandoned_branches;

        // Run all git2 operations in spawn_blocking
        let merge_commit_sha = tokio::task::spawn_blocking(move || {
            let repo = git2::Repository::open(&repo_path)
                .map_err(|e| AosError::Git(format!("Failed to open repository: {}", e)))?;

            let result = if merge {
                // Merge the branch
                let commit_sha = Self::merge_branch_sync(&repo, &branch_name)?;
                info!(
                    "Merged branch {} for adapter {} (commit: {})",
                    branch_name, adapter_id, commit_sha
                );

                // Delete branch if configured
                if !preserve_branches {
                    Self::delete_branch_sync(&repo, &branch_name)?;
                    debug!("Deleted merged branch {}", branch_name);
                }

                Some(commit_sha)
            } else {
                // Abandon the branch
                info!(
                    "Abandoning branch {} for adapter {} (preserve: {})",
                    branch_name, adapter_id, preserve_branches
                );

                if !preserve_branches {
                    Self::delete_branch_sync(&repo, &branch_name)?;
                    debug!("Deleted abandoned branch {}", branch_name);
                }

                None
            };

            Ok::<_, AosError>(result)
        })
        .await
        .map_err(|e| AosError::Git(format!("Task join error: {}", e)))??;

        if merge {
            session.status = SessionStatus::Merged;
            session.merge_commit_sha = merge_commit_sha.clone();
        } else {
            session.status = SessionStatus::Abandoned;
        }

        session.ended_at = Some(chrono::Utc::now());

        // Update session in database
        let status = if merge_commit_sha.is_some() { "merged" } else { "abandoned" };
        self.db.update_git_session_status(
            session_id,
            status,
            merge_commit_sha.as_deref(),
        ).await.map_err(|e| {
            AosError::Database(format!("Failed to update git session: {}", e))
        })?;

        Ok(merge_commit_sha)
    }

    /// List all active sessions
    pub async fn list_active_sessions(&self) -> Vec<GitSession> {
        self.active_sessions
            .read()
            .await
            .values()
            .cloned()
            .collect()
    }

    /// Get session by ID
    pub async fn get_session(&self, session_id: &str) -> Option<GitSession> {
        self.active_sessions.read().await.get(session_id).cloned()
    }

    /// Switch to a branch (synchronous, call from spawn_blocking)
    fn switch_branch_sync(repo: &git2::Repository, branch_name: &str) -> Result<()> {
        let obj = repo
            .revparse_single(&format!("refs/heads/{}", branch_name))
            .map_err(|e| AosError::Git(format!("Failed to find branch: {}", e)))?;

        repo.checkout_tree(&obj, None)
            .map_err(|e| AosError::Git(format!("Failed to checkout tree: {}", e)))?;

        repo.set_head(&format!("refs/heads/{}", branch_name))
            .map_err(|e| AosError::Git(format!("Failed to set HEAD: {}", e)))?;

        debug!("Switched to branch {}", branch_name);
        Ok(())
    }

    /// Merge a branch into main (synchronous, call from spawn_blocking)
    fn merge_branch_sync(repo: &git2::Repository, branch_name: &str) -> Result<String> {
        // Switch to main branch
        Self::switch_branch_sync(repo, "main")
            .or_else(|_| Self::switch_branch_sync(repo, "master"))?;

        // Get the branch commit
        let branch_ref = repo
            .find_reference(&format!("refs/heads/{}", branch_name))
            .map_err(|e| AosError::Git(format!("Failed to find branch: {}", e)))?;

        let branch_commit = branch_ref
            .peel_to_commit()
            .map_err(|e| AosError::Git(format!("Failed to get commit: {}", e)))?;

        // Get the current HEAD commit
        let head_commit = repo
            .head()
            .and_then(|h| h.peel_to_commit())
            .map_err(|e| AosError::Git(format!("Failed to get HEAD: {}", e)))?;

        // Perform merge
        let mut index = repo
            .merge_commits(&head_commit, &branch_commit, None)
            .map_err(|e| AosError::Git(format!("Failed to merge commits: {}", e)))?;

        if index.has_conflicts() {
            return Err(AosError::Git(format!(
                "Merge conflicts detected in branch {}",
                branch_name
            )));
        }

        // Write merged tree
        let tree_id = index
            .write_tree_to(repo)
            .map_err(|e| AosError::Git(format!("Failed to write tree: {}", e)))?;

        let tree = repo
            .find_tree(tree_id)
            .map_err(|e| AosError::Git(format!("Failed to find tree: {}", e)))?;

        // Create merge commit
        let signature = git2::Signature::now("AdapterOS", "aos@localhost")
            .map_err(|e| AosError::Git(format!("Failed to create signature: {}", e)))?;

        let message = format!("Merge branch '{}' (adapter session)", branch_name);

        let commit_id = repo
            .commit(
                Some("HEAD"),
                &signature,
                &signature,
                &message,
                &tree,
                &[&head_commit, &branch_commit],
            )
            .map_err(|e| AosError::Git(format!("Failed to create commit: {}", e)))?;

        Ok(commit_id.to_string())
    }

    /// Delete a branch (synchronous, call from spawn_blocking)
    fn delete_branch_sync(repo: &git2::Repository, branch_name: &str) -> Result<()> {
        let mut branch = repo
            .find_branch(branch_name, git2::BranchType::Local)
            .map_err(|e| AosError::Git(format!("Failed to find branch: {}", e)))?;

        branch
            .delete()
            .map_err(|e| AosError::Git(format!("Failed to delete branch: {}", e)))?;

        debug!("Deleted branch {}", branch_name);
        Ok(())
    }

    /// Get commit SHA for a branch (synchronous, call from spawn_blocking)
    fn get_commit_sha_sync(repo: &git2::Repository, branch: Option<&str>) -> Result<String> {
        let reference = if let Some(branch_name) = branch {
            repo.find_reference(&format!("refs/heads/{}", branch_name))
                .map_err(|e| {
                    AosError::Git(format!("Failed to find branch {}: {}", branch_name, e))
                })?
        } else {
            repo.head()
                .map_err(|e| AosError::Git(format!("Failed to get HEAD: {}", e)))?
        };

        let commit = reference
            .peel_to_commit()
            .map_err(|e| AosError::Git(format!("Failed to get commit: {}", e)))?;

        Ok(commit.id().to_string())
    }

    /// Load active sessions from database
    async fn load_active_sessions(&self) -> Result<()> {
        let active_sessions = self.db.list_active_git_sessions().await
            .map_err(|e| AosError::Database(format!("Failed to load active sessions: {}", e)))?;

        let mut sessions = self.active_sessions.write().await;
        for db_session in active_sessions {
            let session = GitSession {
                id: db_session.id,
                adapter_id: db_session.adapter_id,
                repo_id: db_session.repo_id,
                branch_name: db_session.branch_name,
                base_commit_sha: db_session.base_commit_sha,
                started_at: db_session.started_at.parse()
                    .map_err(|e| AosError::Database(format!("Failed to parse started_at: {}", e)))?,
                ended_at: db_session.ended_at.and_then(|s| s.parse().ok()),
                status: match db_session.status.as_str() {
                    "active" => SessionStatus::Active,
                    "merged" => SessionStatus::Merged,
                    "abandoned" => SessionStatus::Abandoned,
                    _ => SessionStatus::Active,
                },
                merge_commit_sha: db_session.merge_commit_sha,
            };
            sessions.insert(session.id.clone(), session);
        }

        Ok(())
    }
}
