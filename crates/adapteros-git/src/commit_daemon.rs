//! Auto-commit daemon that batches file changes into Git commits

use crate::branch_manager::BranchManager;
use crate::config::CommitConfig;
use crate::types::{ChangeBatch, ChangeType, FileChangeEvent};
use crossbeam_channel::{bounded, Receiver, Sender};
use adapteros_core::{AosError, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::interval;
use tracing::{debug, error, info};
use adapteros_deterministic_exec::spawn_deterministic;

/// Auto-commit daemon that batches changes per adapter
pub struct CommitDaemon {
    config: CommitConfig,
    db: adapteros_db::Db,
    branch_manager: BranchManager,
    event_receiver: Option<Receiver<FileChangeEvent>>,
    event_sender: Sender<FileChangeEvent>,
    batches: Arc<RwLock<HashMap<String, ChangeBatch>>>,
    running: Arc<RwLock<bool>>,
}

impl CommitDaemon {
    /// Create a new commit daemon
    pub async fn new(
        config: CommitConfig,
        db: adapteros_db::Db,
        branch_manager: BranchManager,
    ) -> Result<Self> {
        let (tx, rx) = bounded(10000);

        Ok(Self {
            config,
            db,
            branch_manager,
            event_receiver: Some(rx),
            event_sender: tx,
            batches: Arc::new(RwLock::new(HashMap::new())),
            running: Arc::new(RwLock::new(false)),
        })
    }

    /// Get event sender for watcher
    pub fn event_sender(&self) -> Sender<FileChangeEvent> {
        self.event_sender.clone()
    }

    /// Start the commit daemon
    pub async fn start(&mut self) -> Result<()> {
        let mut running = self.running.write().await;
        if *running {
            return Ok(());
        }

        info!("Starting commit daemon");

        let receiver = self
            .event_receiver
            .take()
            .ok_or_else(|| AosError::Git("Receiver already taken".to_string()))?;

        let config = self.config.clone();
        let db = self.db.clone();
        let branch_manager = self.branch_manager.clone();
        let batches = self.batches.clone();
        let running_flag = self.running.clone();

        *running = true;

        // Spawn task to process events and commit periodically
        let _ = spawn_deterministic("Git commit daemon".to_string(), async move {
            let mut ticker = interval(config.interval());

            loop {
                tokio::select! {
                    // Process incoming file change events
                    result = tokio::task::spawn_blocking({
                        let receiver = receiver.clone();
                        move || receiver.recv()
                    }) => {
                        match result {
                            Ok(Ok(event)) => {
                                if let Err(e) = Self::process_event(event, &batches, config.max_changes_per_commit).await {
                                    error!("Failed to process file change event: {}", e);
                                }
                            }
                            Ok(Err(e)) => {
                                error!("Failed to receive event: {}", e);
                            }
                            Err(e) => {
                                error!("Task join error: {}", e);
                            }
                        }
                    }

                    // Periodic commit tick
                    _ = ticker.tick() => {
                        if let Err(e) = Self::commit_batches(
                            &batches,
                            &db,
                            &branch_manager,
                            &config,
                        ).await {
                            error!("Failed to commit batches: {}", e);
                        }
                    }
                }

                // Check if we should stop
                if !*running_flag.read().await {
                    info!("Commit daemon stopping");
                    break;
                }
            }
        });

        Ok(())
    }

    /// Stop the commit daemon
    pub async fn stop(&mut self) -> Result<()> {
        let mut running = self.running.write().await;
        if !*running {
            return Ok(());
        }

        info!("Stopping commit daemon");
        *running = false;

        // Commit any pending batches before stopping
        Self::commit_batches(&self.batches, &self.db, &self.branch_manager, &self.config).await?;

        Ok(())
    }

    /// Process a file change event
    async fn process_event(
        event: FileChangeEvent,
        batches: &Arc<RwLock<HashMap<String, ChangeBatch>>>,
        max_changes_per_commit: usize,
    ) -> Result<()> {
        let mut batches = batches.write().await;

        // Skip events without an adapter ID (manual changes)
        let adapter_id = match &event.adapter_id {
            Some(id) => id.clone(),
            None => return Ok(()),
        };

        // Get or create batch for this adapter
        let batch = batches
            .entry(adapter_id.clone())
            .or_insert_with(|| ChangeBatch::new(adapter_id, event.repo_id.clone()));

        batch.add_change(event);

        // If batch is full, we should commit it immediately
        if batch.len() >= max_changes_per_commit {
            debug!(
                "Batch for adapter {} is full, will commit on next tick",
                batch.adapter_id
            );
        }

        Ok(())
    }

    /// Commit all pending batches
    async fn commit_batches(
        batches: &Arc<RwLock<HashMap<String, ChangeBatch>>>,
        db: &adapteros_db::Db,
        branch_manager: &BranchManager,
        config: &CommitConfig,
    ) -> Result<()> {
        let mut batches = batches.write().await;

        if batches.is_empty() {
            return Ok(());
        }

        debug!("Committing {} batches", batches.len());

        for (adapter_id, batch) in batches.drain() {
            if batch.is_empty() {
                continue;
            }

            if let Err(e) = Self::commit_batch(batch, db, branch_manager, config).await {
                error!("Failed to commit batch for adapter {}: {}", adapter_id, e);
                // Continue with other batches even if one fails
            }
        }

        Ok(())
    }

    /// Commit a single batch
    async fn commit_batch(
        batch: ChangeBatch,
        db: &adapteros_db::Db,
        branch_manager: &BranchManager,
        config: &CommitConfig,
    ) -> Result<()> {
        let repo_path = branch_manager
            .get_repository_path(&batch.repo_id)
            .await
            .ok_or_else(|| AosError::Git(format!("Repository {} not found", batch.repo_id)))?;

        let adapter_id = batch.adapter_id.clone();
        let adapter_id_log = adapter_id.clone();
        let change_count = batch.len();
        let config = config.clone();
        let batch_for_commit = batch.clone();
        let batch_for_db = batch.clone();

        // Run all git2 operations in spawn_blocking
        let commit_id = tokio::task::spawn_blocking(move || {
            // Open repository
            let repo = git2::Repository::open(&repo_path)
                .map_err(|e| AosError::Git(format!("Failed to open repository: {}", e)))?;

            // Get current branch
            let head = repo
                .head()
                .map_err(|e| AosError::Git(format!("Failed to get HEAD: {}", e)))?;
            let branch_name = head
                .shorthand()
                .ok_or_else(|| AosError::Git("Failed to get branch name".to_string()))?
                .to_string();

            info!(
                "Committing {} changes for adapter {} on branch {}",
                change_count, adapter_id, branch_name
            );

            // Stage all changed files
            let mut index = repo
                .index()
                .map_err(|e| AosError::Git(format!("Failed to get index: {}", e)))?;

            for change in &batch_for_commit.changes {
                let relative_path = change
                    .file_path
                    .strip_prefix(&repo_path)
                    .unwrap_or(&change.file_path);

                match change.change_type {
                    ChangeType::Create | ChangeType::Modify => {
                        index
                            .add_path(relative_path)
                            .map_err(|e| AosError::Git(format!("Failed to add file: {}", e)))?;
                    }
                    ChangeType::Delete => {
                        index
                            .remove_path(relative_path)
                            .map_err(|e| AosError::Git(format!("Failed to remove file: {}", e)))?;
                    }
                }
            }

            index
                .write()
                .map_err(|e| AosError::Git(format!("Failed to write index: {}", e)))?;

            // Create tree from index
            let tree_id = index
                .write_tree()
                .map_err(|e| AosError::Git(format!("Failed to write tree: {}", e)))?;
            let tree = repo
                .find_tree(tree_id)
                .map_err(|e| AosError::Git(format!("Failed to find tree: {}", e)))?;

            // Get parent commit
            let parent_commit = repo
                .head()
                .and_then(|h| h.peel_to_commit())
                .map_err(|e| AosError::Git(format!("Failed to get parent commit: {}", e)))?;

            // Generate commit message
            let message = Self::generate_commit_message(&batch_for_commit);

            // Create signature
            let signature =
                git2::Signature::now(&config.commit_author_name, &config.commit_author_email)
                    .map_err(|e| AosError::Git(format!("Failed to create signature: {}", e)))?;

            // Create commit
            let commit_id = repo
                .commit(
                    Some("HEAD"),
                    &signature,
                    &signature,
                    &message,
                    &tree,
                    &[&parent_commit],
                )
                .map_err(|e| AosError::Git(format!("Failed to create commit: {}", e)))?;

            Ok::<_, AosError>(commit_id.to_string())
        })
        .await
        .map_err(|e| AosError::Git(format!("Task join error: {}", e)))??;

        info!(
            "Created commit {} for adapter {} with {} changes",
            commit_id, adapter_id_log, change_count
        );

        // Store commit hash in database transaction
        if let Err(e) = Self::store_commit_record(db, &batch_for_db, &commit_id).await {
            error!("Failed to store commit record: {}", e);
            // Don't fail the commit for DB errors - Git operation succeeded
        }

        Ok(())
    }

    /// Generate semantic commit message from batch
    fn generate_commit_message(batch: &ChangeBatch) -> String {
        let mut creates = 0;
        let mut modifies = 0;
        let mut deletes = 0;

        for change in &batch.changes {
            match change.change_type {
                ChangeType::Create => creates += 1,
                ChangeType::Modify => modifies += 1,
                ChangeType::Delete => deletes += 1,
            }
        }

        // Determine primary action
        let action = if creates > modifies && creates > deletes {
            "feat"
        } else if deletes > creates && deletes > modifies {
            "remove"
        } else {
            "chore"
        };

        // Generate message
        let mut parts = Vec::new();
        if creates > 0 {
            parts.push(format!("add {} file(s)", creates));
        }
        if modifies > 0 {
            parts.push(format!("modify {} file(s)", modifies));
        }
        if deletes > 0 {
            parts.push(format!("delete {} file(s)", deletes));
        }

        let summary = parts.join(", ");

        format!(
            "{}(adapter:{}): auto commit\n\n{}\n\nRepo: {}\nSession: {}\nTimestamp: {}",
            action,
            batch.adapter_id,
            summary,
            batch.repo_id,
            batch.adapter_id, // Using adapter_id as session identifier for now
            batch.created_at.to_rfc3339()
        )
    }

    /// Store commit record in database
    async fn store_commit_record(
        _db: &adapteros_db::Db,
        batch: &ChangeBatch,
        commit_id: &str,
    ) -> Result<()> {
        // TODO: Implement database storage for commit records
        // This will be implemented when the database schema is ready
        debug!(
            "Would store commit {} for adapter {} in database",
            commit_id, batch.adapter_id
        );
        Ok(())
    }
}
