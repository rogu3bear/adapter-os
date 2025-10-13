//! Git file watcher implementation

use crate::config::WatcherConfig;
use crate::types::{ChangeType, FileChangeEvent};
use adapteros_core::{AosError, Result};
use adapteros_deterministic_exec::spawn_deterministic;
use crossbeam_channel::{bounded, Sender};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Git file watcher that monitors repositories for changes
pub struct GitWatcher {
    config: WatcherConfig,
    db: adapteros_db::Db,
    watcher: Option<RecommendedWatcher>,
    watched_repos: Arc<RwLock<HashMap<String, PathBuf>>>,
    event_sender: Sender<FileChangeEvent>,
    running: Arc<RwLock<bool>>,
}

impl GitWatcher {
    /// Create a new Git watcher
    pub async fn new(
        config: WatcherConfig,
        db: adapteros_db::Db,
        event_sender: Sender<FileChangeEvent>,
    ) -> Result<Self> {
        Ok(Self {
            config,
            db,
            watcher: None,
            watched_repos: Arc::new(RwLock::new(HashMap::new())),
            event_sender,
            running: Arc::new(RwLock::new(false)),
        })
    }

    /// Start watching repositories
    pub async fn start(&mut self) -> Result<()> {
        let mut running = self.running.write().await;
        if *running {
            return Ok(());
        }

        info!("Starting Git file watcher");

        // Create channel for file system events
        let (tx, rx) = bounded(1000);

        // Clone necessary data for the event handler
        let config = self.config.clone();
        let db = self.db.clone();
        let watched_repos = self.watched_repos.clone();
        let event_sender = self.event_sender.clone();

        // Create the watcher
        let mut watcher = RecommendedWatcher::new(
            move |res: notify::Result<Event>| {
                if let Err(e) = tx.send(res) {
                    error!("Failed to send file system event: {}", e);
                }
            },
            notify::Config::default().with_poll_interval(Duration::from_millis(config.debounce_ms)),
        )
        .map_err(|e| AosError::Git(format!("Failed to create file watcher: {}", e)))?;

        // Load and watch all registered repositories
        let repos = self.load_repositories().await?;
        for (repo_id, repo_path) in repos {
            if let Err(e) = watcher.watch(&repo_path, RecursiveMode::Recursive) {
                warn!("Failed to watch repository {}: {}", repo_id, e);
                continue;
            }
            self.watched_repos
                .write()
                .await
                .insert(repo_id.clone(), repo_path.clone());
            info!(
                "Watching repository: {} at {}",
                repo_id,
                repo_path.display()
            );
        }

        self.watcher = Some(watcher);
        *running = true;

        // Spawn task to process file system events
        let _ = spawn_deterministic("Git file watcher".to_string(), async move {
            Self::process_events(rx, config, db, watched_repos, event_sender).await;
        });

        Ok(())
    }

    /// Stop watching repositories
    pub async fn stop(&mut self) -> Result<()> {
        let mut running = self.running.write().await;
        if !*running {
            return Ok(());
        }

        info!("Stopping Git file watcher");
        self.watcher = None;
        *running = false;
        Ok(())
    }

    /// Add a repository to watch
    pub async fn watch_repository(&mut self, repo_id: String, repo_path: PathBuf) -> Result<()> {
        if let Some(ref mut watcher) = self.watcher {
            watcher
                .watch(&repo_path, RecursiveMode::Recursive)
                .map_err(|e| AosError::Git(format!("Failed to watch repository: {}", e)))?;

            self.watched_repos
                .write()
                .await
                .insert(repo_id.clone(), repo_path.clone());
            info!(
                "Added watch for repository: {} at {}",
                repo_id,
                repo_path.display()
            );
            Ok(())
        } else {
            Err(AosError::Git("Watcher not started".to_string()))
        }
    }

    /// Remove a repository from watch list
    pub async fn unwatch_repository(&mut self, repo_id: &str) -> Result<()> {
        let mut repos = self.watched_repos.write().await;
        if let Some(repo_path) = repos.remove(repo_id) {
            if let Some(ref mut watcher) = self.watcher {
                watcher
                    .unwatch(&repo_path)
                    .map_err(|e| AosError::Git(format!("Failed to unwatch repository: {}", e)))?;
                info!("Removed watch for repository: {}", repo_id);
            }
        }
        Ok(())
    }

    /// Load repositories from database
    async fn load_repositories(&self) -> Result<Vec<(String, PathBuf)>> {
        // Query repositories from database
        // For now, return empty list - will be implemented when DB methods are added
        Ok(Vec::new())
    }

    /// Process file system events
    async fn process_events(
        rx: crossbeam_channel::Receiver<notify::Result<Event>>,
        config: WatcherConfig,
        db: adapteros_db::Db,
        watched_repos: Arc<RwLock<HashMap<String, PathBuf>>>,
        event_sender: Sender<FileChangeEvent>,
    ) {
        while let Ok(result) = rx.recv() {
            match result {
                Ok(event) => {
                    if let Err(e) =
                        Self::handle_event(event, &config, &db, &watched_repos, &event_sender).await
                    {
                        error!("Failed to handle file system event: {}", e);
                    }
                }
                Err(e) => {
                    error!("File system event error: {}", e);
                }
            }
        }
        info!("File watcher event loop terminated");
    }

    /// Handle a single file system event
    async fn handle_event(
        event: Event,
        config: &WatcherConfig,
        _db: &adapteros_db::Db,
        watched_repos: &Arc<RwLock<HashMap<String, PathBuf>>>,
        event_sender: &Sender<FileChangeEvent>,
    ) -> Result<()> {
        // Determine change type
        let change_type = match event.kind {
            EventKind::Create(_) => ChangeType::Create,
            EventKind::Modify(_) => ChangeType::Modify,
            EventKind::Remove(_) => ChangeType::Delete,
            _ => return Ok(()), // Ignore other event types
        };

        // Process each path in the event
        for path in event.paths {
            // Check if path should be excluded
            if Self::should_exclude(&path, config) {
                continue;
            }

            // Check if file extension is included
            if !Self::should_include(&path, config) {
                continue;
            }

            // Find which repository this file belongs to
            let repos = watched_repos.read().await;
            let repo_id = Self::find_repository(&path, &repos);

            if let Some(repo_id) = repo_id {
                debug!(
                    "File change detected: {} ({}) in repo {}",
                    path.display(),
                    change_type,
                    repo_id
                );

                // Get active adapter for this repository (if any)
                // For now, we'll set it to None - will be implemented later
                let adapter_id = None;

                // Create file change event with full tagging
                let file_event =
                    FileChangeEvent::new(repo_id.clone(), path.clone(), change_type, adapter_id);

                // Send event to commit daemon and SSE broadcaster
                if let Err(e) = event_sender.send(file_event) {
                    error!("Failed to send file change event: {}", e);
                }
            }
        }

        Ok(())
    }

    /// Check if path should be excluded
    fn should_exclude(path: &Path, config: &WatcherConfig) -> bool {
        let path_str = path.to_string_lossy();

        // Guard against recursive updates
        if path_str.contains(".git/")
            || path_str.contains("target/")
            || path_str.contains("node_modules/")
        {
            return true;
        }

        // Check configured exclude patterns
        for pattern in &config.exclude_patterns {
            if path_str.contains(pattern) {
                return true;
            }
        }

        false
    }

    /// Check if file should be included based on extension
    fn should_include(path: &Path, config: &WatcherConfig) -> bool {
        if let Some(ext) = path.extension() {
            let ext_str = ext.to_string_lossy().to_lowercase();
            config.include_extensions.iter().any(|e| e == &ext_str)
        } else {
            false
        }
    }

    /// Find which repository a path belongs to
    fn find_repository(path: &Path, repos: &HashMap<String, PathBuf>) -> Option<String> {
        for (repo_id, repo_path) in repos {
            if path.starts_with(repo_path) {
                return Some(repo_id.clone());
            }
        }
        None
    }
}
