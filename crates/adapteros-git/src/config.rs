//! Configuration types for Git subsystem

use serde::{Deserialize, Serialize};

/// Configuration for automatic commit behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitConfig {
    /// Interval in seconds between automatic commits
    #[serde(default = "default_commit_interval")]
    pub commit_interval_secs: u64,

    /// Maximum number of changes to batch into a single commit
    #[serde(default = "default_max_changes")]
    pub max_changes_per_commit: usize,

    /// Git author name for automated commits
    #[serde(default = "default_author_name")]
    pub author_name: String,

    /// Git author email for automated commits
    #[serde(default = "default_author_email")]
    pub author_email: String,
}

impl Default for CommitConfig {
    fn default() -> Self {
        Self {
            commit_interval_secs: default_commit_interval(),
            max_changes_per_commit: default_max_changes(),
            author_name: default_author_name(),
            author_email: default_author_email(),
        }
    }
}

fn default_commit_interval() -> u64 {
    300 // 5 minutes
}

fn default_max_changes() -> usize {
    100
}

fn default_author_name() -> String {
    "adapterOS".to_string()
}

fn default_author_email() -> String {
    "adapteros@localhost".to_string()
}

/// Configuration for file system watcher
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatcherConfig {
    /// Debounce interval in milliseconds for change detection
    #[serde(default = "default_debounce_ms")]
    pub debounce_ms: u64,

    /// Patterns to exclude from watching (e.g., "*.tmp", "node_modules/**")
    #[serde(default)]
    pub exclude_patterns: Vec<String>,

    /// File extensions to include (e.g., ".rs", ".toml"). Empty = all files
    #[serde(default)]
    pub include_extensions: Vec<String>,
}

impl Default for WatcherConfig {
    fn default() -> Self {
        Self {
            debounce_ms: default_debounce_ms(),
            exclude_patterns: default_exclude_patterns(),
            include_extensions: Vec::new(),
        }
    }
}

fn default_debounce_ms() -> u64 {
    500
}

fn default_exclude_patterns() -> Vec<String> {
    vec![
        "*.tmp".to_string(),
        "*.swp".to_string(),
        ".git/**".to_string(),
        "target/**".to_string(),
        "node_modules/**".to_string(),
    ]
}
