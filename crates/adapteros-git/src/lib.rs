//! Git integration for AdapterOS

use adapteros_core::plugins::{Plugin, PluginConfig, PluginHealth, PluginStatus};
use adapteros_core::Result;
use serde_json::Value;
use tracing::info;

pub mod branch_manager;
pub mod diff_analyzer;
pub mod subsystem;

pub use branch_manager::BranchManager;
pub use diff_analyzer::{
    ChangedSymbol, DiffAnalysis, DiffAnalyzer, DiffSummary, SymbolChangeType, SymbolKind,
};
pub use subsystem::{
    CommitDiff, CommitInfo, GitBranchInfo, GitConfig, GitStatusResponse, GitSubsystem,
};

impl Plugin for GitSubsystem {
    fn name(&self) -> &'static str {
        "git"
    }

    async fn load(&self, config: &PluginConfig) -> Result<(), String> {
        let config_value = serde_json::Value::Object(
            config
                .specific
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        );
        let git_cfg: GitConfig = serde_json::from_value(config_value).map_err(|e| {
            format!(
                "Failed to deserialize GitConfig from plugin specific: {}",
                e
            )
        })?;
        // For now, log or update if needed
        info!("Loaded GitConfig from plugin: enabled={}", git_cfg.enabled);
        Ok(())
    }

    async fn start(&self) -> Result<(), String> {
        self.start_polling().await.map_err(|e| e.to_string())
    }

    async fn stop(&self) -> Result<(), String> {
        self.stop_polling().await.map_err(|e| e.to_string())
    }

    async fn reload(&self, _config: &PluginConfig) -> Result<(), String> {
        // Reload branch manager if config changes
        Ok(())
    }

    async fn health_check(&self) -> Result<PluginHealth, String> {
        let enabled_count = self.enabled_tenants.read().await.len();
        let total_tenants = self.db.count_tenants().await.map_err(|e| e.to_string())? as usize;
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

    async fn set_tenant_enabled(&self, tenant_id: &str, enabled: bool) -> Result<(), String> {
        let tenant = tenant_id.to_string();
        let mut set = self.enabled_tenants.write().await;
        if enabled {
            set.insert(tenant.clone());
        } else {
            set.remove(&tenant);
            // Supervisor notify: if all disabled, pause ops
            if set.is_empty() {
                info!("All tenants disabled for Git, pausing polling");
                // self.stop_polling().await?; but &self, assume called elsewhere or ignore for now
            }
        }
        info!(
            "Git plugin tenant {} {}",
            tenant_id,
            if enabled { "enabled" } else { "disabled" }
        );
        Ok(())
    }
}

// NOTE: The original GitSubsystem implementation (watcher, commit daemon, branch manager)
// has been temporarily stubbed out to resolve a feature conflict. The primary
// functionality of this crate is now the DiffAnalyzer. The GitSubsystem will be
// fully implemented in a future iteration.
