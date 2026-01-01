//! Recovery actions for error handling
//!
//! Extends the FixSafety pattern from preflight to provide machine-readable
//! recovery actions that can be executed automatically or with user confirmation.

use serde::{Deserialize, Serialize};

/// Safety classification for recovery actions.
///
/// This mirrors the FixSafety enum from preflight to ensure consistent behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FixSafety {
    /// Safe to execute automatically without user confirmation
    Safe,
    /// Requires explicit user confirmation before execution
    RequiresConfirm,
    /// Unsafe - requires manual intervention, cannot be automated
    Unsafe,
}

/// Machine-readable recovery actions for error remediation.
///
/// Each action has an associated safety level that determines whether it can
/// be executed automatically, requires confirmation, or needs manual intervention.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum RecoveryAction {
    // =========================================================================
    // Safe actions - can be executed automatically
    // =========================================================================
    /// Retry the operation with exponential backoff
    Retry {
        max_attempts: u8,
        base_backoff_ms: u64,
    },

    /// Evict cache entries to free memory
    EvictCache {
        /// Target memory to free in MB, None means evict all non-pinned
        target_mb: Option<u64>,
    },

    /// Wait for a resource to become available
    WaitForResource {
        #[serde(skip)]
        resource: &'static str,
        timeout_ms: u64,
    },

    // =========================================================================
    // RequiresConfirm actions - need user approval
    // =========================================================================
    /// Run database migrations
    RunMigrations,

    /// Repair hash integrity for entities
    RepairHashes {
        #[serde(skip)]
        entity_type: &'static str,
    },

    /// Rebuild an index or cache
    RebuildIndex {
        #[serde(skip)]
        index_name: &'static str,
    },

    /// Execute a CLI command
    CliCommand {
        #[serde(skip)]
        command: &'static str,
        #[serde(skip)]
        args: &'static [&'static str],
    },

    /// Restart a component
    RestartComponent {
        #[serde(skip)]
        component: &'static str,
    },

    /// Clear and reinitialize state
    Reinitialize {
        #[serde(skip)]
        component: &'static str,
    },

    // =========================================================================
    // Unsafe actions - require manual intervention
    // =========================================================================
    /// Policy adjustment needed - cannot be automated
    PolicyAdjust,

    /// Re-sign artifacts with valid key
    Resign,

    /// Fix validation error in input
    ValidationFix,

    /// Configuration change required
    ConfigChange {
        #[serde(skip)]
        setting: &'static str,
        #[serde(skip)]
        suggested_value: Option<&'static str>,
    },

    /// Upgrade or install required dependency
    InstallDependency {
        #[serde(skip)]
        name: &'static str,
        #[serde(skip)]
        version: Option<&'static str>,
    },

    /// Contact support or investigate manually
    Manual,

    /// Quarantine the resource and fail fast
    Quarantine,
}

impl RecoveryAction {
    /// Get the safety level for this recovery action
    pub const fn safety(&self) -> FixSafety {
        match self {
            // Safe - auto-execute
            Self::Retry { .. } | Self::EvictCache { .. } | Self::WaitForResource { .. } => {
                FixSafety::Safe
            }

            // RequiresConfirm - ask user
            Self::RunMigrations
            | Self::RepairHashes { .. }
            | Self::RebuildIndex { .. }
            | Self::CliCommand { .. }
            | Self::RestartComponent { .. }
            | Self::Reinitialize { .. } => FixSafety::RequiresConfirm,

            // Unsafe - manual only
            Self::PolicyAdjust
            | Self::Resign
            | Self::ValidationFix
            | Self::ConfigChange { .. }
            | Self::InstallDependency { .. }
            | Self::Manual
            | Self::Quarantine => FixSafety::Unsafe,
        }
    }

    /// Check if this action is safe to execute automatically
    pub const fn is_safe(&self) -> bool {
        matches!(self.safety(), FixSafety::Safe)
    }

    /// Check if this action requires user confirmation
    pub const fn requires_confirm(&self) -> bool {
        matches!(self.safety(), FixSafety::RequiresConfirm)
    }

    /// Check if this action requires manual intervention
    pub const fn is_manual(&self) -> bool {
        matches!(self.safety(), FixSafety::Unsafe)
    }

    /// Convert to a CLI command string, if applicable
    pub fn to_cli_command(&self) -> Option<String> {
        match self {
            Self::RunMigrations => Some("aosctl db migrate".to_string()),

            Self::RepairHashes { entity_type } => Some(format!(
                "aosctl adapter repair-hashes --{}-id <id>",
                entity_type
            )),

            Self::RebuildIndex { index_name } => {
                Some(format!("aosctl index rebuild {}", index_name))
            }

            Self::CliCommand { command, args } => {
                if args.is_empty() {
                    Some(format!("aosctl {}", command))
                } else {
                    Some(format!("aosctl {} {}", command, args.join(" ")))
                }
            }

            Self::RestartComponent { component } => {
                Some(format!("aosctl service restart {}", component))
            }

            Self::Reinitialize { component } => Some(format!("aosctl {} init --force", component)),

            Self::InstallDependency { name, version } => {
                if let Some(v) = version {
                    Some(format!("# Install {} version {}", name, v))
                } else {
                    Some(format!("# Install {}", name))
                }
            }

            _ => None,
        }
    }

    /// Get a human-readable description of the action
    pub fn description(&self) -> &'static str {
        match self {
            Self::Retry { .. } => "Retry the operation with exponential backoff",
            Self::EvictCache { .. } => "Evict cache entries to free memory",
            Self::WaitForResource { .. } => "Wait for resource availability",
            Self::RunMigrations => "Run pending database migrations",
            Self::RepairHashes { .. } => "Repair hash integrity for entities",
            Self::RebuildIndex { .. } => "Rebuild index or cache",
            Self::CliCommand { .. } => "Execute CLI command",
            Self::RestartComponent { .. } => "Restart component",
            Self::Reinitialize { .. } => "Clear and reinitialize state",
            Self::PolicyAdjust => "Adjust policy configuration",
            Self::Resign => "Re-sign artifacts with valid key",
            Self::ValidationFix => "Fix validation error in input",
            Self::ConfigChange { .. } => "Change configuration setting",
            Self::InstallDependency { .. } => "Install or upgrade dependency",
            Self::Manual => "Manual investigation required",
            Self::Quarantine => "Quarantine resource and fail fast",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safety_levels() {
        assert_eq!(
            RecoveryAction::Retry {
                max_attempts: 3,
                base_backoff_ms: 100
            }
            .safety(),
            FixSafety::Safe
        );

        assert_eq!(
            RecoveryAction::RunMigrations.safety(),
            FixSafety::RequiresConfirm
        );

        assert_eq!(RecoveryAction::PolicyAdjust.safety(), FixSafety::Unsafe);
    }

    #[test]
    fn test_cli_command_generation() {
        assert_eq!(
            RecoveryAction::RunMigrations.to_cli_command(),
            Some("aosctl db migrate".to_string())
        );

        assert_eq!(
            RecoveryAction::RepairHashes {
                entity_type: "adapter"
            }
            .to_cli_command(),
            Some("aosctl adapter repair-hashes --adapter-id <id>".to_string())
        );

        assert_eq!(RecoveryAction::PolicyAdjust.to_cli_command(), None);
    }

    #[test]
    fn test_serialization() {
        let action = RecoveryAction::Retry {
            max_attempts: 3,
            base_backoff_ms: 100,
        };
        let json = serde_json::to_string(&action).unwrap();
        let parsed: RecoveryAction = serde_json::from_str(&json).unwrap();
        assert_eq!(action, parsed);
    }
}
