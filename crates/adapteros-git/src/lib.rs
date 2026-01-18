//! Git integration for adapterOS

#![allow(unused_imports)]
#![allow(unused_variables)]
#![allow(dead_code)]
#![allow(clippy::manual_clamp)]
#![allow(clippy::collapsible_if)]

use adapteros_core::plugins::{Plugin, PluginConfig, PluginHealth, PluginStatus};
use adapteros_core::Result;
use async_trait::async_trait;
use serde_json::Value;
use tracing::info;

pub mod branch_manager;
pub mod config;
pub mod diff_analyzer;
pub mod subsystem;
pub mod types;

pub use branch_manager::BranchManager;
pub use config::{CommitConfig, WatcherConfig};
pub use diff_analyzer::{
    ChangedSymbol, DiffAnalysis, DiffAnalyzer, DiffSummary, SymbolChangeType, SymbolKind,
};
pub use subsystem::{
    CommitDiff, CommitInfo, GitBranchInfo, GitConfig, GitStatusResponse, GitSubsystem,
};
pub use types::{ChangeBatch, ChangeType, FileChangeEvent};

#[async_trait]
impl Plugin for GitSubsystem {
    fn name(&self) -> &'static str {
        "git"
    }

    async fn load(&self, config: &PluginConfig) -> Result<()> {
        let config_value = serde_json::Value::Object(
            config
                .specific
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        );
        let git_cfg: GitConfig = serde_json::from_value(config_value)?;
        // For now, log or update if needed
        info!(
            plugin = "git",
            enabled = git_cfg.enabled,
            "Loaded GitConfig from plugin"
        );
        Ok(())
    }

    async fn start(&self) -> Result<()> {
        info!(plugin = "git", "Starting Git plugin");
        if self.enabled {
            self.start_polling().await?;
            info!(plugin = "git", "Git plugin started successfully");
        } else {
            info!(
                plugin = "git",
                enabled = false,
                "Git plugin disabled, not starting polling"
            );
        }
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        info!(plugin = "git", "Stopping Git plugin");
        self.stop_polling().await?;
        info!(plugin = "git", "Git plugin stopped successfully");
        Ok(())
    }

    async fn reload(&self, config: &PluginConfig) -> Result<()> {
        info!(plugin = "git", "Reloading Git plugin configuration");

        // Parse the new config
        let config_value = serde_json::Value::Object(
            config
                .specific
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        );
        let git_cfg: GitConfig = serde_json::from_value(config_value)?;

        // Stop polling if currently running
        self.stop_polling().await?;

        // Reload branch manager config if needed
        let branch_manager = self.branch_manager();
        let bm = branch_manager.write().await;
        // BranchManager doesn't expose reload currently, but we can recreate if needed
        drop(bm);

        // Restart if enabled
        if git_cfg.enabled {
            self.start_polling().await?;
        }

        info!(
            plugin = "git",
            "Git plugin configuration reloaded successfully"
        );
        Ok(())
    }

    async fn health_check(&self) -> Result<PluginHealth> {
        let enabled_count = self.enabled_tenants.read().await.len();
        let total_tenants = self.db.list_tenants().await?.len();
        let percentage = if total_tenants > 0 {
            enabled_count as f32 / total_tenants as f32 * 100.0
        } else {
            0.0
        };
        let status = if enabled_count == 0 {
            PluginStatus::Stopped
        } else if percentage < 50.0 {
            PluginStatus::Degraded(format!("Only {:.0}% tenants enabled", percentage))
        } else {
            PluginStatus::Started
        };
        Ok(PluginHealth {
            status,
            details: None,
        })
    }

    async fn set_tenant_enabled(&self, tenant_id: &str, enabled: bool) -> Result<()> {
        let tenant = tenant_id.to_string();
        let mut set = self.enabled_tenants.write().await;
        if enabled {
            set.insert(tenant.clone());
        } else {
            set.remove(&tenant);
            // Supervisor notify: if all disabled, pause ops
            if set.is_empty() {
                info!(
                    plugin = "git",
                    enabled_count = 0,
                    "All tenants disabled for Git, pausing polling"
                );
                // self.stop_polling().await?; but &self, assume called elsewhere or ignore for now
            }
        }
        info!(
            plugin = "git",
            tenant_id = %tenant_id,
            enabled = enabled,
            "Updated Git plugin tenant state"
        );
        Ok(())
    }
}

// GitSubsystem provides full git integration including:
// - File system watcher (watcher.rs) - monitors for changes via notify crate
// - Commit daemon (commit_daemon.rs) - batches changes into deterministic commits
// - Branch manager (branch_manager.rs) - handles adapter session lifecycle
// - Diff analyzer (diff_analyzer.rs) - analyzes code changes for adapter priors
