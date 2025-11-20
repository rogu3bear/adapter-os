//! Git subsystem implementation
//!
//! Provides lightweight helpers for querying commit history and diffs for
//! repositories registered in the control plane database.

use adapteros_core::{AosError, Result};
use adapteros_db::{git_repositories::GitRepository, Db};
use chrono::Duration as ChronoDuration;
use chrono::{DateTime, Utc};
use git2::{BranchType, DiffFormat, Oid};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::RwLock;
use tokio::task;
use tokio::task::JoinHandle;
use tokio::time::Duration;
use tracing::info;

use crate::branch_manager::{BranchManager, BranchManagerConfig};

/// Configuration for the Git subsystem.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GitConfig {
    /// Enable or disable the Git subsystem.
    #[serde(default)]
    pub enabled: bool,
}

/// Lightweight commit summary used by the API layer.
#[derive(Debug, Clone)]
pub struct CommitInfo {
    pub repo_id: String,
    pub sha: String,
    pub message: String,
    pub author: String,
    pub date: DateTime<Utc>,
    pub branch: Option<String>,
    pub changed_files: Vec<String>,
    pub impacted_symbols: Vec<String>,
    pub ephemeral_adapter_id: Option<String>,
}

/// Commit diff payload used by the API layer.
#[derive(Debug, Clone)]
pub struct CommitDiff {
    pub sha: String,
    pub diff: String,
    pub files_changed: i32,
    pub insertions: i32,
    pub deletions: i32,
}

/// Git status response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitStatusResponse {
    pub enabled: bool,
    pub active_sessions: u32,
    pub repositories_tracked: u32,
    pub last_scan: Option<String>,
}

/// Git branch information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitBranchInfo {
    pub name: String,
    pub is_current: bool,
    pub last_commit: String,
    pub ahead: u32,
    pub behind: u32,
}

pub struct GitWatcher;

impl GitWatcher {
    pub async fn new(_config: WatcherConfig, _db: Db, _tx: mpsc::Sender<()>) -> Result<Self> {
        Ok(Self)
    }

    pub async fn start(&mut self) -> Result<()> {
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        Ok(())
    }
}

#[derive(Debug)]
pub struct WatcherConfig {
    pub debounce_ms: u64,
}

/// Git subsystem manager
pub struct GitSubsystem {
    pub enabled: bool,
    pub db: Db,
    branch_manager: BranchManager,
    pub enabled_tenants: Arc<RwLock<HashSet<String>>>,
    watcher: Option<()>,
    daemon_handle: Option<()>,
    pub is_polling: bool,
}

impl std::fmt::Debug for GitSubsystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GitSubsystem")
            .field("enabled", &self.enabled)
            .finish()
    }
}

impl GitSubsystem {
    /// Construct a new Git subsystem from config and a database handle.
    pub async fn new(cfg: GitConfig, db: Db) -> Result<Self> {
        let branch_manager = BranchManager::new(db.clone(), BranchManagerConfig::default()).await?;

        Ok(Self {
            enabled: cfg.enabled,
            db,
            branch_manager,
            enabled_tenants: Arc::new(RwLock::new(HashSet::new())),
            watcher: None,
            daemon_handle: None,
            is_polling: false,
        })
    }

    /// Start background tasks. Currently a no-op.
    pub async fn start(&mut self) -> Result<()> {
        if self.enabled {
            self.start_polling().await?;
        }
        if self.enabled {
            tracing::info!("Git subsystem started");
        } else {
            tracing::info!("Git subsystem disabled");
        }
        Ok(())
    }

    /// List commits for a repository/branch (newest first).
    pub async fn list_commits(
        &self,
        repo_id: Option<&str>,
        branch: Option<&str>,
        limit: usize,
    ) -> Result<Vec<CommitInfo>> {
        let repo = self.resolve_repository(repo_id).await?;
        let repo_path = PathBuf::from(&repo.path);
        let repo_id_string = repo.repo_id.clone();
        let branch_name = branch
            .map(|s| s.to_string())
            .or_else(|| Some(repo.branch.clone()));
        let limit = limit.clamp(1, 200);

        task::spawn_blocking(move || -> Result<Vec<CommitInfo>> {
            let repo = git2::Repository::open(&repo_path).map_err(|e| {
                AosError::Git(format!(
                    "Failed to open repository {}: {}",
                    repo_path.display(),
                    e
                ))
            })?;

            let head_oid = resolve_branch_head(&repo, branch_name.as_deref())?;
            let mut revwalk = repo
                .revwalk()
                .map_err(|e| AosError::Git(format!("Failed to create revwalk iterator: {}", e)))?;
            revwalk
                .set_sorting(git2::Sort::TIME | git2::Sort::TOPOLOGICAL)
                .map_err(|e| AosError::Git(format!("Failed to set revwalk sorting: {}", e)))?;
            revwalk
                .push(head_oid)
                .map_err(|e| AosError::Git(format!("Failed to push revwalk: {}", e)))?;

            let mut commits = Vec::new();
            for oid_result in revwalk.take(limit) {
                let oid = oid_result.map_err(|e| AosError::Git(format!("Revwalk error: {}", e)))?;
                let commit = repo
                    .find_commit(oid)
                    .map_err(|e| AosError::Git(format!("Failed to lookup commit: {}", e)))?;
                commits.push(build_commit_info(
                    &repo,
                    &commit,
                    repo_id_string.clone(),
                    branch_name.clone(),
                )?);
            }

            Ok(commits)
        })
        .await
        .map_err(|e| AosError::Git(format!("Git worker join error: {}", e)))?
    }

    /// Fetch a single commit by SHA.
    pub async fn get_commit(&self, repo_id: Option<&str>, sha: &str) -> Result<CommitInfo> {
        let repo = self.resolve_repository(repo_id).await?;
        let repo_path = PathBuf::from(&repo.path);
        let repo_id_string = repo.repo_id.clone();
        let branch_name = Some(repo.branch.clone());
        let sha = sha.to_string();

        task::spawn_blocking(move || -> Result<CommitInfo> {
            let repo = git2::Repository::open(&repo_path).map_err(|e| {
                AosError::Git(format!(
                    "Failed to open repository {}: {}",
                    repo_path.display(),
                    e
                ))
            })?;
            let oid = Oid::from_str(&sha)
                .map_err(|e| AosError::Git(format!("Invalid commit SHA {}: {}", sha, e)))?;
            let commit = repo
                .find_commit(oid)
                .map_err(|e| AosError::Git(format!("Failed to lookup commit: {}", e)))?;

            build_commit_info(&repo, &commit, repo_id_string, branch_name)
        })
        .await
        .map_err(|e| AosError::Git(format!("Git worker join error: {}", e)))?
    }

    /// Compute a commit diff for the supplied SHA.
    pub async fn get_commit_diff(&self, repo_id: Option<&str>, sha: &str) -> Result<CommitDiff> {
        let repo = self.resolve_repository(repo_id).await?;
        let repo_path = PathBuf::from(&repo.path);
        let sha = sha.to_string();

        task::spawn_blocking(move || -> Result<CommitDiff> {
            let repo = git2::Repository::open(&repo_path).map_err(|e| {
                AosError::Git(format!(
                    "Failed to open repository {}: {}",
                    repo_path.display(),
                    e
                ))
            })?;
            let oid = Oid::from_str(&sha)
                .map_err(|e| AosError::Git(format!("Invalid commit SHA {}: {}", sha, e)))?;
            let commit = repo
                .find_commit(oid)
                .map_err(|e| AosError::Git(format!("Failed to lookup commit: {}", e)))?;

            let current_tree = commit
                .tree()
                .map_err(|e| AosError::Git(format!("Failed to get commit tree: {}", e)))?;
            let parent_tree = if commit.parent_count() > 0 {
                Some(
                    commit
                        .parent(0)
                        .map_err(|e| AosError::Git(format!("Failed to get parent commit: {}", e)))?
                        .tree()
                        .map_err(|e| AosError::Git(format!("Failed to get parent tree: {}", e)))?,
                )
            } else {
                None
            };

            let diff = repo
                .diff_tree_to_tree(parent_tree.as_ref(), Some(&current_tree), None)
                .map_err(|e| AosError::Git(format!("Failed to compute diff: {}", e)))?;

            let mut diff_text = String::new();
            diff.print(DiffFormat::Patch, |_delta, _hunk, line| {
                if let Ok(content) = std::str::from_utf8(line.content()) {
                    diff_text.push_str(content);
                }
                true
            })
            .map_err(|e| AosError::Git(format!("Failed to render diff: {}", e)))?;

            let stats = diff
                .stats()
                .map_err(|e| AosError::Git(format!("Failed to compute diff stats: {}", e)))?;

            Ok(CommitDiff {
                sha,
                diff: diff_text,
                files_changed: stats.files_changed().try_into().unwrap_or(0),
                insertions: stats.insertions().try_into().unwrap_or(0),
                deletions: stats.deletions().try_into().unwrap_or(0),
            })
        })
        .await
        .map_err(|e| AosError::Git(format!("Git worker join error: {}", e)))?
    }

    pub async fn get_status(&self) -> Result<GitStatusResponse> {
        let active_sessions = self.branch_manager.list_active_sessions().await.len() as u32;
        let repositories = self.db.list_git_repositories().await.unwrap_or_default();
        let repositories_tracked = repositories.len() as u32;

        // Find the most recent created_at timestamp across all repositories
        // TODO: Add last_scan field to GitRepository schema if needed
        let last_scan = repositories
            .iter()
            .map(|repo| &repo.created_at)
            .max()
            .cloned();

        Ok(GitStatusResponse {
            enabled: self.enabled,
            active_sessions,
            repositories_tracked,
            last_scan,
        })
    }

    pub async fn list_branches(&self, repo_id: Option<&str>) -> Result<Vec<crate::GitBranchInfo>> {
        let repo = self.resolve_repository(repo_id).await?;
        let repo_path = PathBuf::from(&repo.path);

        task::spawn_blocking(move || -> Result<Vec<crate::GitBranchInfo>> {
            let repo = git2::Repository::open(&repo_path).map_err(|e| {
                AosError::Git(format!(
                    "Failed to open repository {}: {}",
                    repo_path.display(),
                    e
                ))
            })?;

            let mut branches = Vec::new();

            // Local branches
            let local_branches = repo
                .branches(Some(BranchType::Local))
                .map_err(|e| AosError::Git(format!("Failed to list local branches: {}", e)))?;

            for branch_result in local_branches {
                let (branch, _) = branch_result
                    .map_err(|e| AosError::Git(format!("Failed to get branch: {}", e)))?;

                let name = branch
                    .name()
                    .map_err(|e| AosError::Git(format!("Failed to get branch name: {}", e)))?
                    .unwrap_or("unknown")
                    .to_string();

                let is_current = branch.is_head();
                let branch_ref = branch.into_reference();
                let branch_oid = branch_ref.target();
                let last_commit = if let Some(ref_) = branch_oid {
                    if let Ok(commit) = repo.find_commit(ref_) {
                        commit.summary().unwrap_or("No message").to_string()
                    } else {
                        "unknown".to_string()
                    }
                } else {
                    "unknown".to_string()
                };

                // Calculate ahead/behind counts
                let (ahead, behind) = if let Some(oid) = branch_oid {
                    Self::calculate_ahead_behind(&repo, oid)?
                } else {
                    (0, 0)
                };

                branches.push(crate::GitBranchInfo {
                    name,
                    is_current,
                    last_commit,
                    ahead,
                    behind,
                });
            }

            Ok(branches)
        })
        .await
        .map_err(|e| AosError::Git(format!("Branch listing task join error: {}", e)))?
    }

    /// Calculate ahead/behind counts for a branch compared to its upstream or default branch
    fn calculate_ahead_behind(
        repo: &git2::Repository,
        branch_oid: git2::Oid,
    ) -> Result<(u32, u32)> {
        // Try to find an upstream branch to compare against
        // First, look for origin/main, then origin/master
        let upstream_refs = ["refs/remotes/origin/main", "refs/remotes/origin/master"];

        for upstream_ref in &upstream_refs {
            if let Ok(upstream_ref_obj) = repo.find_reference(upstream_ref) {
                if let Some(upstream_oid) = upstream_ref_obj.target() {
                    if let Ok(upstream_commit) = repo.find_commit(upstream_oid) {
                        // Calculate ahead/behind
                        let (ahead, behind) = repo
                            .graph_ahead_behind(branch_oid, upstream_commit.id())
                            .map_err(|e| {
                                AosError::Git(format!("Failed to calculate ahead/behind: {}", e))
                            })?;

                        return Ok((ahead as u32, behind as u32));
                    }
                }
            }
        }

        // If no upstream found, compare against HEAD if it's different
        if let Ok(head_ref) = repo.head() {
            if let Some(head_oid) = head_ref.target() {
                if head_oid != branch_oid {
                    let (ahead, behind) =
                        repo.graph_ahead_behind(branch_oid, head_oid).map_err(|e| {
                            AosError::Git(format!(
                                "Failed to calculate ahead/behind vs HEAD: {}",
                                e
                            ))
                        })?;
                    return Ok((ahead as u32, behind as u32));
                }
            }
        }

        // No comparison possible, return 0,0
        Ok((0, 0))
    }

    /// Get reference to the branch manager
    pub fn branch_manager(&self) -> &BranchManager {
        &self.branch_manager
    }

    async fn resolve_repository(&self, repo_id: Option<&str>) -> Result<GitRepository> {
        if let Some(id) = repo_id {
            self.db
                .get_git_repository(id)
                .await?
                .ok_or_else(|| AosError::Git(format!("Repository '{}' not found", id)))
        } else {
            let repos = self.db.list_git_repositories().await?;
            repos
                .into_iter()
                .next()
                .ok_or_else(|| AosError::Git("No Git repositories registered".to_string()))
        }
    }

    pub async fn start_polling(&mut self) -> Result<()> {
        if self.is_polling {
            return Ok(());
        }

        if self.enabled_tenants.read().await.is_empty() {
            return Ok(());
        }

        // Start watcher
        let config = WatcherConfig { debounce_ms: 500 };
        let (tx, _) = mpsc::channel::<()>(1024);
        // let mut watcher = GitWatcher::new(config, self.db.clone(), tx).await?;
        // watcher.start().await?;
        // self.watcher = Some(watcher);

        self.watcher = Some(());

        // Start daemon
        // let enabled_tenants = self.enabled_tenants.clone();
        // let db = self.db.clone();
        // let daemon_handle = tokio::spawn(async move {
        //     let mut interval = tokio::time::interval(Duration::from_secs(60));
        //     loop {
        //         interval.tick().await;
        //         let tenants: Vec<_> = enabled_tenants.read().await.iter().cloned().collect();
        //         if tenants.is_empty() { continue; }
        //         if let Ok(repos) = db.list_git_repositories_for_tenants(&tenants).await {
        //             for repo in repos {
        //                 // Poll for changes
        //                 if let Some(new_commit) = poll_new_commits(&repo).await.ok() {
        //                     // Process event
        //                     info!("New commit in repo {}: {}", repo.repo_id, new_commit.sha);
        //                 }
        //             }
        //         }
        //     }
        // });
        // self.daemon_handle = Some(daemon_handle);

        self.daemon_handle = Some(());

        self.is_polling = true;
        Ok(())
    }

    pub async fn stop_polling(&mut self) -> Result<()> {
        // Stop watcher
        if let Some(w) = &self.watcher {
            // w.stop().await?;
        }
        self.watcher = None;

        // Abort daemon
        if let Some(h) = self.daemon_handle.take() {
            // h.abort();
        }

        self.is_polling = false;
        Ok(())
    }
}

fn resolve_branch_head(repo: &git2::Repository, branch: Option<&str>) -> Result<Oid> {
    if let Some(branch_name) = branch {
        if let Ok(branch) = repo.find_branch(branch_name, BranchType::Local) {
            return branch
                .into_reference()
                .target()
                .ok_or_else(|| AosError::Git(format!("Branch '{}' has no target", branch_name)));
        }

        if let Ok(branch) = repo.find_branch(branch_name, BranchType::Remote) {
            return branch
                .into_reference()
                .target()
                .ok_or_else(|| AosError::Git(format!("Branch '{}' has no target", branch_name)));
        }

        return Err(AosError::Git(format!("Branch '{}' not found", branch_name)));
    }

    repo.head()
        .map_err(|e| AosError::Git(format!("Failed to read HEAD: {}", e)))?
        .target()
        .ok_or_else(|| AosError::Git("HEAD reference has no target".to_string()))
}

fn build_commit_info(
    repo: &git2::Repository,
    commit: &git2::Commit<'_>,
    repo_id: String,
    branch: Option<String>,
) -> Result<CommitInfo> {
    let changed_files = collect_changed_files(repo, commit)?;
    let author = commit.author();
    let author_name = author.name().unwrap_or("unknown").to_string();
    let message = commit.summary().unwrap_or("no commit message").to_string();

    Ok(CommitInfo {
        repo_id,
        sha: commit.id().to_string(),
        message,
        author: author_name,
        date: commit_time_to_datetime(commit.time()),
        branch,
        changed_files,
        impacted_symbols: Vec::new(),
        ephemeral_adapter_id: None,
    })
}

fn collect_changed_files(
    repo: &git2::Repository,
    commit: &git2::Commit<'_>,
) -> Result<Vec<String>> {
    let current_tree = commit
        .tree()
        .map_err(|e| AosError::Git(format!("Failed to get commit tree: {}", e)))?;
    let parent_tree = if commit.parent_count() > 0 {
        Some(
            commit
                .parent(0)
                .map_err(|e| AosError::Git(format!("Failed to get parent commit: {}", e)))?
                .tree()
                .map_err(|e| AosError::Git(format!("Failed to get parent tree: {}", e)))?,
        )
    } else {
        None
    };

    let diff = repo
        .diff_tree_to_tree(parent_tree.as_ref(), Some(&current_tree), None)
        .map_err(|e| AosError::Git(format!("Failed to compute diff: {}", e)))?;

    let mut files = Vec::new();
    diff.foreach(
        &mut |delta, _| {
            if let Some(path) = delta.new_file().path().or_else(|| delta.old_file().path()) {
                files.push(path.display().to_string());
            }
            true
        },
        None,
        None,
        None,
    )
    .map_err(|e| AosError::Git(format!("Failed to iterate diff: {}", e)))?;

    files.sort();
    files.dedup();
    Ok(files)
}

fn commit_time_to_datetime(time: git2::Time) -> DateTime<Utc> {
    let naive = DateTime::from_timestamp(time.seconds(), 0)
        .unwrap_or_else(|| DateTime::from_timestamp(0, 0).unwrap())
        .naive_utc();
    let offset = ChronoDuration::minutes(time.offset_minutes() as i64);
    DateTime::from_naive_utc_and_offset(naive, Utc) + offset
}
