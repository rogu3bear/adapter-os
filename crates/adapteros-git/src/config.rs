//! Configuration for Git integration

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Complete Git subsystem configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitConfig {
    pub enabled: bool,
    pub version: String,
    pub watcher: WatcherConfig,
    pub commit_daemon: CommitConfig,
    pub branch_manager: BranchManagerConfig,
}

impl Default for GitConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            version: "v1".to_string(),
            watcher: WatcherConfig::default(),
            commit_daemon: CommitConfig::default(),
            branch_manager: BranchManagerConfig::default(),
        }
    }
}

/// Configuration for Git watcher
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatcherConfig {
    pub debounce_ms: u64,
    pub exclude_patterns: Vec<String>,
    pub include_extensions: Vec<String>,
}

impl Default for WatcherConfig {
    fn default() -> Self {
        Self {
            debounce_ms: 300,
            exclude_patterns: vec![
                ".git".to_string(),
                "target".to_string(),
                "node_modules".to_string(),
                "*.log".to_string(),
                ".DS_Store".to_string(),
            ],
            include_extensions: vec![
                "rs".to_string(),
                "toml".to_string(),
                "md".to_string(),
                "json".to_string(),
                "yaml".to_string(),
                "yml".to_string(),
                "py".to_string(),
                "js".to_string(),
                "ts".to_string(),
                "tsx".to_string(),
                "jsx".to_string(),
            ],
        }
    }
}

/// Configuration for commit daemon
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitConfig {
    pub auto_commit_interval_secs: u64,
    pub max_changes_per_commit: usize,
    pub commit_author_name: String,
    pub commit_author_email: String,
}

impl Default for CommitConfig {
    fn default() -> Self {
        Self {
            auto_commit_interval_secs: 30,
            max_changes_per_commit: 50,
            commit_author_name: "AdapterOS".to_string(),
            commit_author_email: "aos@localhost".to_string(),
        }
    }
}

impl CommitConfig {
    pub fn interval(&self) -> Duration {
        Duration::from_secs(self.auto_commit_interval_secs)
    }
}

/// Configuration for branch manager
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchManagerConfig {
    pub branch_prefix: String,
    pub auto_merge_on_close: bool,
    pub preserve_abandoned_branches: bool,
}

impl Default for BranchManagerConfig {
    fn default() -> Self {
        Self {
            branch_prefix: "adapter".to_string(),
            auto_merge_on_close: false,
            preserve_abandoned_branches: true,
        }
    }
}
