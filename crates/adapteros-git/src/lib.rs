//! Git integration for AdapterOS
//!
//! Provides Git watcher, auto-commit daemon, and adapter branch management
//! for seamless version control integration with agentic code editing workflows.

mod branch_manager;
mod commit_daemon;
mod config;
mod types;
mod watcher;

pub use branch_manager::{BranchManager, BranchOperation};
pub use commit_daemon::CommitDaemon;
pub use config::{BranchManagerConfig, CommitConfig, GitConfig, WatcherConfig};
pub use types::{ChangeType, FileChangeEvent, GitSession};
pub use watcher::GitWatcher;

use adapteros_core::Result;
use tracing::{error, info};

/// Initialize Git subsystem with all components
pub struct GitSubsystem {
    watcher: GitWatcher,
    commit_daemon: CommitDaemon,
    branch_manager: BranchManager,
}

impl GitSubsystem {
    /// Create a new Git subsystem
    pub async fn new(config: GitConfig, db: adapteros_db::Db) -> Result<Self> {
        let branch_manager = BranchManager::new(db.clone(), config.branch_manager.clone()).await?;
        let commit_daemon = CommitDaemon::new(
            config.commit_daemon.clone(),
            db.clone(),
            branch_manager.clone(),
        )
        .await?;
        let watcher = GitWatcher::new(
            config.watcher.clone(),
            db.clone(),
            commit_daemon.event_sender(),
        )
        .await?;

        Ok(Self {
            watcher,
            commit_daemon,
            branch_manager,
        })
    }

    /// Start all Git subsystem components
    pub async fn start(&mut self) -> Result<()> {
        self.commit_daemon.start().await?;
        self.watcher.start().await?;
        Ok(())
    }

    /// Stop all Git subsystem components
    pub async fn stop(&mut self) -> Result<()> {
        info!("Stopping Git subsystem components...");

        // Stop watcher first to prevent new events
        if let Err(e) = self.watcher.stop().await {
            error!("Failed to stop watcher: {}", e);
        }

        // Stop commit daemon and flush pending commits
        if let Err(e) = self.commit_daemon.stop().await {
            error!("Failed to stop commit daemon: {}", e);
        }

        info!("Git subsystem stopped gracefully.");
        Ok(())
    }

    /// Get reference to watcher for adding repositories
    pub fn watcher(&self) -> &GitWatcher {
        &self.watcher
    }

    /// Get reference to branch manager
    pub fn branch_manager(&self) -> &BranchManager {
        &self.branch_manager
    }

    /// Get reference to commit daemon for event streaming
    pub fn commit_daemon(&self) -> &CommitDaemon {
        &self.commit_daemon
    }
}
