//! Git file watcher implementation

use crate::config::WatcherConfig;
use crate::types::{ChangeType, FileChangeEvent};
use adapteros_core::{validate_path_characters, AosError, Result};
use adapteros_deterministic_exec::spawn_deterministic;
use crossbeam_channel::{bounded, Sender};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Threshold for triggering automatic rescan: number of dropped events
const RESCAN_DROP_THRESHOLD: u64 = 100;

/// Time window in seconds for counting dropped events
const RESCAN_WINDOW_SECS: u64 = 60;

/// Metrics for the file watcher
///
/// These can be exposed via Prometheus or other monitoring systems.
#[derive(Debug, Clone, Copy)]
pub struct WatcherMetrics {
    /// Number of events dropped due to channel overflow in current window
    pub dropped_events: u64,
    /// Whether a rescan is currently pending
    pub rescan_pending: bool,
}

/// Git file watcher that monitors repositories for changes
pub struct GitWatcher {
    config: WatcherConfig,
    db: adapteros_db::Db,
    watcher: Option<RecommendedWatcher>,
    watched_repos: Arc<RwLock<HashMap<String, PathBuf>>>,
    event_sender: Sender<FileChangeEvent>,
    running: Arc<RwLock<bool>>,
    /// Counter for dropped events in the current window
    dropped_events: Arc<AtomicU64>,
    /// Start time of the current drop counting window
    dropped_window_start: Arc<RwLock<Instant>>,
    /// Flag indicating a rescan should be triggered
    rescan_pending: Arc<AtomicBool>,
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
            dropped_events: Arc::new(AtomicU64::new(0)),
            dropped_window_start: Arc::new(RwLock::new(Instant::now())),
            rescan_pending: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Get the count of dropped events in the current window
    pub fn dropped_event_count(&self) -> u64 {
        self.dropped_events.load(Ordering::Relaxed)
    }

    /// Check if a rescan is pending
    pub fn is_rescan_pending(&self) -> bool {
        self.rescan_pending.load(Ordering::Relaxed)
    }

    /// Get watcher metrics for monitoring
    ///
    /// Returns a struct with current metrics that can be exposed via
    /// Prometheus or other monitoring systems.
    pub fn metrics(&self) -> WatcherMetrics {
        WatcherMetrics {
            dropped_events: self.dropped_events.load(Ordering::Relaxed),
            rescan_pending: self.rescan_pending.load(Ordering::Relaxed),
        }
    }

    /// Log current watcher metrics (for observability)
    pub fn log_metrics(&self) {
        let metrics = self.metrics();
        info!(
            component = "git_file_watcher",
            dropped_events = metrics.dropped_events,
            rescan_pending = metrics.rescan_pending,
            "File watcher metrics"
        );
    }

    /// Start watching repositories
    pub async fn start(&mut self) -> Result<()> {
        let mut running = self.running.write().await;
        if *running {
            return Ok(());
        }

        info!(component = "git_file_watcher", "Starting Git file watcher");

        // Create channel for file system events
        let (tx, rx) = bounded(1000);

        // Clone necessary data for the event handler
        let config = self.config.clone();
        let db = self.db.clone();
        let watched_repos = self.watched_repos.clone();
        let event_sender = self.event_sender.clone();

        // Clone drop tracking state for the watcher callback
        let dropped_events = self.dropped_events.clone();
        let dropped_window_start = self.dropped_window_start.clone();
        let rescan_pending = self.rescan_pending.clone();

        // Create the watcher with drop counting on channel overflow
        let mut watcher = RecommendedWatcher::new(
            move |res: notify::Result<Event>| {
                match tx.try_send(res) {
                    Ok(_) => {}
                    Err(crossbeam_channel::TrySendError::Full(_)) => {
                        let count = dropped_events.fetch_add(1, Ordering::Relaxed) + 1;

                        // Check if we've exceeded threshold in the current window
                        // Note: Using blocking_read is safe here as this is a sync callback
                        // and the lock is held briefly
                        if let Ok(guard) = dropped_window_start.try_read() {
                            if guard.elapsed().as_secs() > RESCAN_WINDOW_SECS {
                                // Reset window on next opportunity
                                drop(guard);
                                if let Ok(mut write_guard) = dropped_window_start.try_write() {
                                    *write_guard = Instant::now();
                                    dropped_events.store(1, Ordering::Relaxed);
                                }
                            } else if count >= RESCAN_DROP_THRESHOLD {
                                // Trigger rescan: 100+ drops in 1 minute
                                rescan_pending.store(true, Ordering::Release);
                                warn!(
                                    dropped_count = count,
                                    "Threshold exceeded ({} drops in {}s), triggering rescan",
                                    RESCAN_DROP_THRESHOLD,
                                    RESCAN_WINDOW_SECS
                                );
                            }
                        }

                        warn!(
                            component = "git_file_watcher",
                            dropped_count = count,
                            "File watcher event dropped: channel full"
                        );
                    }
                    Err(crossbeam_channel::TrySendError::Disconnected(_)) => {
                        error!(
                            component = "git_file_watcher",
                            "File watcher channel disconnected"
                        );
                    }
                }
            },
            notify::Config::default().with_poll_interval(Duration::from_millis(config.debounce_ms)),
        )
        .map_err(|e| AosError::Git(format!("Failed to create file watcher: {}", e)))?;

        // Load and watch all registered repositories
        let repos = self.load_repositories().await?;
        for (repo_id, repo_path) in repos {
            if let Err(e) = watcher.watch(&repo_path, RecursiveMode::Recursive) {
                warn!(
                    repo_id = %repo_id,
                    path = %repo_path.display(),
                    error = %e,
                    "Failed to watch repository"
                );
                continue;
            }
            self.watched_repos
                .write()
                .await
                .insert(repo_id.clone(), repo_path.clone());
            info!(
                repo_id = %repo_id,
                path = %repo_path.display(),
                "Watching repository"
            );
        }

        self.watcher = Some(watcher);
        *running = true;

        // Spawn task to process file system events
        let _ = spawn_deterministic("Git file watcher".to_string(), async move {
            Self::process_events(rx, config, db, watched_repos, event_sender).await;
        });

        // Spawn background task to monitor for rescan trigger
        let rescan_pending = self.rescan_pending.clone();
        let watched_repos_rescan = self.watched_repos.clone();
        let event_sender_rescan = self.event_sender.clone();
        let config_rescan = self.config.clone();
        let _ = spawn_deterministic(
            "Git file watcher rescan monitor".to_string(),
            async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    if rescan_pending.swap(false, Ordering::AcqRel) {
                        info!(
                            component = "git_file_watcher",
                            "Auto-rescan triggered after event drops"
                        );
                        Self::perform_rescan(
                            &watched_repos_rescan,
                            &event_sender_rescan,
                            &config_rescan,
                        )
                        .await;
                    }
                }
            },
        );

        Ok(())
    }

    /// Stop watching repositories
    pub async fn stop(&mut self) -> Result<()> {
        let mut running = self.running.write().await;
        if !*running {
            return Ok(());
        }

        info!(component = "git_file_watcher", "Stopping Git file watcher");
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
                repo_id = %repo_id,
                path = %repo_path.display(),
                "Added watch for repository"
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
                info!(
                    repo_id = %repo_id,
                    path = %repo_path.display(),
                    "Removed watch for repository"
                );
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
                        error!(error = %e, "Failed to handle file system event");
                    }
                }
                Err(e) => {
                    error!(error = %e, "File system event error");
                }
            }
        }
        info!(
            component = "git_file_watcher",
            "File watcher event loop terminated"
        );
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
            // Validate path has no OS-specific invalid characters
            if let Err(e) = validate_path_characters(&path) {
                warn!(
                    path = %path.display(),
                    error = %e,
                    "Skipping path with invalid characters"
                );
                continue;
            }

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
                    repo_id = %repo_id,
                    path = %path.display(),
                    change_type = ?change_type,
                    "File change detected"
                );

                // Get active adapter for this repository (if any)
                // For now, we'll set it to None - will be implemented later
                let adapter_id = None;

                // Create file change event with full tagging
                let file_event =
                    FileChangeEvent::new(repo_id.clone(), path.clone(), change_type, adapter_id);

                // Send event to commit daemon and SSE broadcaster
                if let Err(e) = event_sender.send(file_event) {
                    error!(
                        repo_id = %repo_id,
                        path = %path.display(),
                        error = %e,
                        "Failed to send file change event"
                    );
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

    /// Perform a full rescan of all watched repositories
    ///
    /// This function walks all watched repository directories and emits
    /// synthetic Create events for all tracked files. This is used to
    /// recover state after event drops.
    async fn perform_rescan(
        watched_repos: &Arc<RwLock<HashMap<String, PathBuf>>>,
        event_sender: &Sender<FileChangeEvent>,
        config: &WatcherConfig,
    ) {
        let repos = watched_repos.read().await;
        let mut total_files = 0u64;

        for (repo_id, repo_path) in repos.iter() {
            info!(
                repo_id = %repo_id,
                path = %repo_path.display(),
                "Rescanning repository after event drops"
            );

            // Walk the repository directory
            if let Err(e) = Self::walk_and_emit(repo_id, repo_path, event_sender, config, &mut total_files).await {
                warn!(
                    repo_id = %repo_id,
                    path = %repo_path.display(),
                    error = %e,
                    "Error during rescan"
                );
            }
        }

        info!(
            component = "git_file_watcher",
            total_files = total_files,
            "Rescan complete"
        );
    }

    /// Walk a directory tree and emit events for matching files
    async fn walk_and_emit(
        repo_id: &str,
        root: &Path,
        event_sender: &Sender<FileChangeEvent>,
        config: &WatcherConfig,
        total_files: &mut u64,
    ) -> Result<()> {
        let mut stack = vec![root.to_path_buf()];

        while let Some(current) = stack.pop() {
            let entries = match std::fs::read_dir(&current) {
                Ok(entries) => entries,
                Err(e) => {
                    debug!(
                        path = %current.display(),
                        error = %e,
                        "Cannot read directory during rescan"
                    );
                    continue;
                }
            };

            for entry in entries.flatten() {
                let path = entry.path();

                // Skip excluded paths
                if Self::should_exclude(&path, config) {
                    continue;
                }

                if path.is_dir() {
                    stack.push(path);
                } else if path.is_file() && Self::should_include(&path, config) {
                    // Validate path before emitting
                    if let Err(e) = validate_path_characters(&path) {
                        debug!(
                            path = %path.display(),
                            error = %e,
                            "Skipping invalid path during rescan"
                        );
                        continue;
                    }

                    // Emit synthetic Create event
                    let file_event = FileChangeEvent::new(
                        repo_id.to_string(),
                        path.clone(),
                        ChangeType::Create,
                        None,
                    );

                    if let Err(e) = event_sender.try_send(file_event) {
                        debug!(
                            path = %path.display(),
                            error = %e,
                            "Failed to emit rescan event"
                        );
                    } else {
                        *total_files += 1;
                    }
                }
            }
        }

        Ok(())
    }
}
