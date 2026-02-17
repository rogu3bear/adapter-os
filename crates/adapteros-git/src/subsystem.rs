//! Git subsystem implementation
//!
//! Provides lightweight helpers for querying commit history and diffs for
//! repositories registered in the control plane database.

use adapteros_core::{AosError, Result};
use adapteros_db::{git_repositories::GitRepository, Db};
use chrono::Duration as ChronoDuration;
use chrono::{DateTime, Utc};
use git2::{BranchType, DiffFormat, Oid, Status, StatusOptions};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::RwLock;
use tokio::task;
use tokio::task::JoinHandle;
use tokio::time::Duration;
use tracing::{debug, error, info, warn};

use crate::branch_manager::{BranchManager, BranchManagerConfig};
use crate::config::WatcherConfig;

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

/// Working tree status payload used by API handlers.
#[derive(Debug, Clone)]
pub struct WorkingTreeStatus {
    pub branch: String,
    pub modified_files: Vec<String>,
    pub untracked_files: Vec<String>,
    pub staged_files: Vec<String>,
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

/// File system watcher for Git repository changes
pub struct GitWatcher {
    config: WatcherConfig,
    db: Db,
    tx: mpsc::Sender<()>,
    watch_handle: Option<JoinHandle<()>>,
    is_running: bool,
}

impl GitWatcher {
    /// Create a new GitWatcher instance
    pub async fn new(config: WatcherConfig, db: Db, tx: mpsc::Sender<()>) -> Result<Self> {
        info!(debounce_ms = config.debounce_ms, "Initializing Git watcher");

        Ok(Self {
            config,
            db,
            tx,
            watch_handle: None,
            is_running: false,
        })
    }

    /// Start watching for Git repository changes
    pub async fn start(&mut self) -> Result<()> {
        if self.is_running {
            debug!(component = "git_watcher", "Git watcher already running");
            return Ok(());
        }

        info!(component = "git_watcher", "Starting Git watcher");

        // Get repositories to watch
        let repos = self.db.list_git_repositories().await.map_err(|e| {
            AosError::Git(format!("Failed to list repositories for watching: {}", e))
        })?;

        if repos.is_empty() {
            warn!(
                component = "git_watcher",
                "No repositories configured for Git watcher"
            );
            self.is_running = true;
            return Ok(());
        }

        let debounce_duration = Duration::from_millis(self.config.debounce_ms);
        let tx = self.tx.clone();
        let repo_paths: Vec<PathBuf> = repos.iter().map(|r| PathBuf::from(&r.path)).collect();

        // Spawn watcher task
        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(debounce_duration);

            loop {
                interval.tick().await;

                // Check each repository for changes using spawn_blocking for git2
                let paths = repo_paths.clone();
                let changes = task::spawn_blocking(move || {
                    let mut changed_repos = Vec::new();
                    for repo_path in &paths {
                        if let Ok(repo) = git2::Repository::open(repo_path) {
                            if let Ok(statuses) = repo.statuses(None) {
                                if !statuses.is_empty() {
                                    changed_repos.push((repo_path.clone(), statuses.len()));
                                }
                            }
                        }
                    }
                    changed_repos
                })
                .await;

                if let Ok(changed_repos) = changes {
                    for (path, count) in changed_repos {
                        debug!(path = %path.display(), changes = count, "Detected changes in repository");
                        if let Err(e) = tx.send(()).await {
                            error!(error = %e, "Failed to send change notification");
                            return;
                        }
                    }
                }
            }
        });

        self.watch_handle = Some(handle);
        self.is_running = true;

        info!(repo_count = repos.len(), "Git watcher started successfully");
        Ok(())
    }

    /// Stop watching for changes and cleanup resources
    pub async fn stop(&mut self) -> Result<()> {
        if !self.is_running {
            debug!(component = "git_watcher", "Git watcher not running");
            return Ok(());
        }

        info!(component = "git_watcher", "Stopping Git watcher");

        // Abort the watch task
        if let Some(handle) = self.watch_handle.take() {
            handle.abort();
            // Wait for task to complete
            match handle.await {
                Ok(()) => debug!(component = "git_watcher", "Git watcher task completed"),
                Err(e) if e.is_cancelled() => {
                    debug!(component = "git_watcher", "Git watcher task cancelled")
                }
                Err(e) => warn!(error = %e, "Git watcher task failed"),
            }
        }

        self.is_running = false;
        info!(component = "git_watcher", "Git watcher stopped");
        Ok(())
    }

    /// Check if the watcher is currently running
    pub fn is_running(&self) -> bool {
        self.is_running
    }
}

/// Mutable state for GitSubsystem (wrapped in Arc<RwLock<>>)
struct GitSubsystemState {
    watcher: Option<GitWatcher>,
    daemon_handle: Option<JoinHandle<()>>,
    is_polling: bool,
}

/// Git subsystem manager
pub struct GitSubsystem {
    pub enabled: bool,
    pub db: Db,
    branch_manager: Arc<RwLock<BranchManager>>,
    pub enabled_tenants: Arc<RwLock<HashSet<String>>>,
    state: Arc<RwLock<GitSubsystemState>>,
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
            branch_manager: Arc::new(RwLock::new(branch_manager)),
            enabled_tenants: Arc::new(RwLock::new(HashSet::new())),
            state: Arc::new(RwLock::new(GitSubsystemState {
                watcher: None,
                daemon_handle: None,
                is_polling: false,
            })),
        })
    }

    /// Start background tasks. Currently a no-op.
    pub async fn start(&self) -> Result<()> {
        if self.enabled {
            self.start_polling().await?;
        }
        if self.enabled {
            info!(component = "git_subsystem", "Git subsystem started");
        } else {
            info!(component = "git_subsystem", "Git subsystem disabled");
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
        let branch_manager = self.branch_manager.read().await;
        let active_sessions = branch_manager.list_active_sessions().await.len() as u32;
        let repositories = self.db.list_git_repositories().await.unwrap_or_default();
        let repositories_tracked = repositories.len() as u32;

        // Find the most recent last_scan timestamp across all repositories
        let last_scan = repositories
            .iter()
            .filter_map(|repo| repo.last_scan.as_ref())
            .max()
            .cloned();

        Ok(GitStatusResponse {
            enabled: self.enabled,
            active_sessions,
            repositories_tracked,
            last_scan,
        })
    }

    pub async fn get_working_tree_status(
        &self,
        repo_id: Option<&str>,
    ) -> Result<WorkingTreeStatus> {
        let repo = self.resolve_repository(repo_id).await?;
        let repo_path = PathBuf::from(&repo.path);

        task::spawn_blocking(move || -> Result<WorkingTreeStatus> {
            let repo = git2::Repository::open(&repo_path).map_err(|e| {
                AosError::Git(format!(
                    "Failed to open repository {}: {}",
                    repo_path.display(),
                    e
                ))
            })?;

            let branch = repo
                .head()
                .ok()
                .and_then(|head| head.shorthand().map(|name| name.to_string()))
                .unwrap_or_else(|| "HEAD".to_string());

            let mut options = StatusOptions::new();
            options.include_untracked(true).recurse_untracked_dirs(true);
            let statuses = repo.statuses(Some(&mut options)).map_err(|e| {
                AosError::Git(format!("Failed to compute repository status: {}", e))
            })?;

            let mut modified_files = Vec::new();
            let mut untracked_files = Vec::new();
            let mut staged_files = Vec::new();

            for entry in statuses.iter() {
                let status = entry.status();
                let Some(path) = entry.path() else {
                    continue;
                };
                let path = path.to_string();

                if is_staged_status(status) {
                    staged_files.push(path.clone());
                }
                if is_untracked_status(status) {
                    untracked_files.push(path.clone());
                } else if is_modified_status(status) {
                    modified_files.push(path.clone());
                }
            }

            modified_files.sort();
            modified_files.dedup();
            untracked_files.sort();
            untracked_files.dedup();
            staged_files.sort();
            staged_files.dedup();

            Ok(WorkingTreeStatus {
                branch,
                modified_files,
                untracked_files,
                staged_files,
            })
        })
        .await
        .map_err(|e| AosError::Git(format!("Git worker join error: {}", e)))?
    }

    pub async fn stage_file(&self, repo_id: Option<&str>, file_path: &str) -> Result<()> {
        let repo = self.resolve_repository(repo_id).await?;
        let repo_path = PathBuf::from(&repo.path);
        let relative = normalize_repo_relative_path(file_path)?;

        task::spawn_blocking(move || -> Result<()> {
            let repo = git2::Repository::open(&repo_path).map_err(|e| {
                AosError::Git(format!(
                    "Failed to open repository {}: {}",
                    repo_path.display(),
                    e
                ))
            })?;
            let mut index = repo
                .index()
                .map_err(|e| AosError::Git(format!("Failed to open repository index: {}", e)))?;
            index
                .add_path(&relative)
                .map_err(|e| AosError::Git(format!("Failed to stage file: {}", e)))?;
            index
                .write()
                .map_err(|e| AosError::Git(format!("Failed to write index: {}", e)))?;
            Ok(())
        })
        .await
        .map_err(|e| AosError::Git(format!("Git worker join error: {}", e)))?
    }

    pub async fn get_working_diff(
        &self,
        repo_id: Option<&str>,
        file_path: Option<&str>,
    ) -> Result<String> {
        let repo = self.resolve_repository(repo_id).await?;
        let repo_path = PathBuf::from(&repo.path);
        let relative = if let Some(path) = file_path {
            Some(normalize_repo_relative_path(path)?)
        } else {
            None
        };
        let relative_string = relative.map(|p| p.to_string_lossy().to_string());

        task::spawn_blocking(move || -> Result<String> {
            let repo = git2::Repository::open(&repo_path).map_err(|e| {
                AosError::Git(format!(
                    "Failed to open repository {}: {}",
                    repo_path.display(),
                    e
                ))
            })?;

            let mut options = git2::DiffOptions::new();
            if let Some(ref path) = relative_string {
                options.pathspec(path);
            }

            let head_tree = repo.head().ok().and_then(|head| head.peel_to_tree().ok());
            let diff = repo
                .diff_tree_to_workdir_with_index(head_tree.as_ref(), Some(&mut options))
                .map_err(|e| AosError::Git(format!("Failed to compute working diff: {}", e)))?;

            let mut diff_text = String::new();
            diff.print(DiffFormat::Patch, |_delta, _hunk, line| {
                if let Ok(text) = std::str::from_utf8(line.content()) {
                    diff_text.push_str(text);
                }
                true
            })
            .map_err(|e| AosError::Git(format!("Failed to render working diff: {}", e)))?;

            Ok(diff_text)
        })
        .await
        .map_err(|e| AosError::Git(format!("Git worker join error: {}", e)))?
    }

    pub async fn unstage_file(&self, repo_id: Option<&str>, file_path: &str) -> Result<()> {
        let repo = self.resolve_repository(repo_id).await?;
        let repo_path = PathBuf::from(&repo.path);
        let relative = normalize_repo_relative_path(file_path)?;
        let relative_string = relative.to_string_lossy().to_string();

        task::spawn_blocking(move || -> Result<()> {
            let repo = git2::Repository::open(&repo_path).map_err(|e| {
                AosError::Git(format!(
                    "Failed to open repository {}: {}",
                    repo_path.display(),
                    e
                ))
            })?;

            repo.reset_default(None, [relative_string.as_str()])
                .map_err(|e| AosError::Git(format!("Failed to unstage file: {}", e)))?;
            Ok(())
        })
        .await
        .map_err(|e| AosError::Git(format!("Git worker join error: {}", e)))?
    }

    pub async fn discard_file(&self, repo_id: Option<&str>, file_path: &str) -> Result<()> {
        let repo = self.resolve_repository(repo_id).await?;
        let repo_path = PathBuf::from(&repo.path);
        let relative = normalize_repo_relative_path(file_path)?;
        let relative_string = relative.to_string_lossy().to_string();

        task::spawn_blocking(move || -> Result<()> {
            let repo = git2::Repository::open(&repo_path).map_err(|e| {
                AosError::Git(format!(
                    "Failed to open repository {}: {}",
                    repo_path.display(),
                    e
                ))
            })?;

            let full_path = repo_path.join(&relative);
            if full_path.exists() {
                let mut checkout = git2::build::CheckoutBuilder::new();
                checkout.force().path(&relative_string);
                let checkout_result = repo.checkout_head(Some(&mut checkout));
                if checkout_result.is_err() {
                    let metadata = std::fs::metadata(&full_path).map_err(|e| {
                        AosError::Git(format!("Failed to inspect file for discard: {}", e))
                    })?;
                    if metadata.is_dir() {
                        std::fs::remove_dir_all(&full_path).map_err(|e| {
                            AosError::Git(format!("Failed to remove directory: {}", e))
                        })?;
                    } else {
                        std::fs::remove_file(&full_path)
                            .map_err(|e| AosError::Git(format!("Failed to remove file: {}", e)))?;
                    }
                }
            }
            Ok(())
        })
        .await
        .map_err(|e| AosError::Git(format!("Git worker join error: {}", e)))?
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

    pub async fn create_commit(&self, repo_id: Option<&str>, message: &str) -> Result<String> {
        let repo = self.resolve_repository(repo_id).await?;
        let repo_path = PathBuf::from(&repo.path);
        let message = message.trim().to_string();
        if message.is_empty() {
            return Err(AosError::Git("commit message cannot be empty".to_string()));
        }

        task::spawn_blocking(move || -> Result<String> {
            let repo = git2::Repository::open(&repo_path).map_err(|e| {
                AosError::Git(format!(
                    "Failed to open repository {}: {}",
                    repo_path.display(),
                    e
                ))
            })?;

            let mut index = repo
                .index()
                .map_err(|e| AosError::Git(format!("Failed to open repository index: {}", e)))?;
            let tree_oid = index
                .write_tree()
                .map_err(|e| AosError::Git(format!("Failed to write index tree: {}", e)))?;
            let tree = repo
                .find_tree(tree_oid)
                .map_err(|e| AosError::Git(format!("Failed to lookup commit tree: {}", e)))?;

            let signature = repo
                .signature()
                .or_else(|_| git2::Signature::now("AdapterOS", "adapteros@localhost"))
                .map_err(|e| AosError::Git(format!("Failed to build commit signature: {}", e)))?;

            let commit_oid = if let Ok(head) = repo.head() {
                let parent = head
                    .peel_to_commit()
                    .map_err(|e| AosError::Git(format!("Failed to resolve HEAD commit: {}", e)))?;
                repo.commit(
                    Some("HEAD"),
                    &signature,
                    &signature,
                    &message,
                    &tree,
                    &[&parent],
                )
                .map_err(|e| AosError::Git(format!("Failed to create commit: {}", e)))?
            } else {
                repo.commit(Some("HEAD"), &signature, &signature, &message, &tree, &[])
                    .map_err(|e| AosError::Git(format!("Failed to create initial commit: {}", e)))?
            };

            Ok(commit_oid.to_string())
        })
        .await
        .map_err(|e| AosError::Git(format!("Git worker join error: {}", e)))?
    }

    pub async fn checkout_branch(&self, repo_id: Option<&str>, branch: &str) -> Result<()> {
        let repo = self.resolve_repository(repo_id).await?;
        let repo_path = PathBuf::from(&repo.path);
        let branch = branch.trim().to_string();
        if branch.is_empty() {
            return Err(AosError::Git("branch cannot be empty".to_string()));
        }

        task::spawn_blocking(move || -> Result<()> {
            let repo = git2::Repository::open(&repo_path).map_err(|e| {
                AosError::Git(format!(
                    "Failed to open repository {}: {}",
                    repo_path.display(),
                    e
                ))
            })?;

            let branch_ref = if branch.starts_with("refs/heads/") {
                branch.clone()
            } else {
                format!("refs/heads/{}", branch)
            };

            repo.find_reference(&branch_ref)
                .map_err(|e| AosError::Git(format!("Branch '{}' not found: {}", branch, e)))?;

            repo.set_head(&branch_ref)
                .map_err(|e| AosError::Git(format!("Failed to set HEAD to '{}': {}", branch, e)))?;

            let mut checkout = git2::build::CheckoutBuilder::new();
            checkout.safe();
            repo.checkout_head(Some(&mut checkout))
                .map_err(|e| AosError::Git(format!("Failed to checkout '{}': {}", branch, e)))?;

            Ok(())
        })
        .await
        .map_err(|e| AosError::Git(format!("Git worker join error: {}", e)))?
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
    pub fn branch_manager(&self) -> Arc<RwLock<BranchManager>> {
        self.branch_manager.clone()
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

    pub async fn start_polling(&self) -> Result<()> {
        let mut state = self.state.write().await;

        if state.is_polling {
            debug!(component = "git_polling", "Git polling already active");
            return Ok(());
        }

        if self.enabled_tenants.read().await.is_empty() {
            debug!(
                component = "git_subsystem",
                "No tenants enabled for Git polling"
            );
            return Ok(());
        }

        info!(component = "git_polling", "Starting Git polling");

        // Start watcher with default configuration from config module
        let config = WatcherConfig::default();
        let (tx, mut rx) = mpsc::channel::<()>(1024);
        let mut watcher = GitWatcher::new(config, self.db.clone(), tx).await?;
        watcher.start().await?;
        state.watcher = Some(watcher);

        // Start daemon to process change notifications
        let enabled_tenants = self.enabled_tenants.clone();
        let db = self.db.clone();
        let daemon_handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        // Periodic check
                        let tenants: Vec<_> = enabled_tenants.read().await.iter().cloned().collect();
                        if tenants.is_empty() {
                            continue;
                        }

                        debug!(tenant_count = tenants.len(), "Periodic Git repository check");

                        // List repositories for enabled tenants
                        match db.list_git_repositories().await {
                            Ok(repos) => {
                                for repo in repos {
                                    debug!(repo_id = %repo.repo_id, "Checking repository for updates");
                                }
                            }
                            Err(e) => {
                                error!(error = %e, "Failed to list Git repositories");
                            }
                        }
                    }
                    Some(()) = rx.recv() => {
                        // Change notification received
                        debug!(component = "git_polling", "Received Git change notification");
                    }
                }
            }
        });
        state.daemon_handle = Some(daemon_handle);

        state.is_polling = true;
        info!(component = "git_polling", "Git polling started");
        Ok(())
    }

    pub async fn stop_polling(&self) -> Result<()> {
        let mut state = self.state.write().await;

        if !state.is_polling {
            debug!(component = "git_polling", "Git polling not active");
            return Ok(());
        }

        info!(component = "git_polling", "Stopping Git polling");

        // Stop watcher
        if let Some(mut watcher) = state.watcher.take() {
            watcher.stop().await?;
        }

        // Abort daemon
        if let Some(handle) = state.daemon_handle.take() {
            handle.abort();
            match handle.await {
                Ok(()) => debug!(component = "git_polling", "Git daemon task completed"),
                Err(e) if e.is_cancelled() => {
                    debug!(component = "git_polling", "Git daemon task cancelled")
                }
                Err(e) => warn!(error = %e, "Git daemon task failed"),
            }
        }

        state.is_polling = false;
        info!(component = "git_polling", "Git polling stopped");
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

fn normalize_repo_relative_path(file_path: &str) -> Result<PathBuf> {
    let path = Path::new(file_path);
    if path.is_absolute() {
        return Err(AosError::Git(
            "file_path must be repository-relative".to_string(),
        ));
    }
    if file_path.is_empty() {
        return Err(AosError::Git("file_path cannot be empty".to_string()));
    }

    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => normalized.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(AosError::Git(
                    "file_path cannot contain traversal segments".to_string(),
                ));
            }
        }
    }

    if normalized.as_os_str().is_empty() {
        return Err(AosError::Git("file_path cannot be empty".to_string()));
    }

    Ok(normalized)
}

fn is_staged_status(status: Status) -> bool {
    status.intersects(
        Status::INDEX_NEW
            | Status::INDEX_MODIFIED
            | Status::INDEX_DELETED
            | Status::INDEX_RENAMED
            | Status::INDEX_TYPECHANGE,
    )
}

fn is_untracked_status(status: Status) -> bool {
    status.contains(Status::WT_NEW)
}

fn is_modified_status(status: Status) -> bool {
    status.intersects(
        Status::WT_MODIFIED | Status::WT_DELETED | Status::WT_RENAMED | Status::WT_TYPECHANGE,
    )
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
