//! Recovery command for error codes
//!
//! Use `aosctl recover <ERROR_CODE>` to get recovery instructions and optionally
//! execute the recovery action for an error code.

use adapteros_core::errors::{ECode, RecoveryAction};
use adapteros_error_registry::{FixSafety, IntoEnumIterator};
use anyhow::{Context, Result};
use dialoguer::Confirm;
use std::process::Command;

/// Execute recovery action for an error code
///
/// This command looks up the recovery action for an error code and either:
/// - Executes it automatically (if Safe)
/// - Asks for confirmation (if RequiresConfirm)
/// - Shows instructions only (if Unsafe)
pub async fn recover(code: &str, force: bool, dry_run: bool) -> Result<()> {
    // Parse the error code
    let ecode = ECode::parse(code).ok_or_else(|| {
        anyhow::anyhow!(
            "Invalid error code: {}\n\n\
             Use `aosctl error-codes` to list all valid error codes.",
            code
        )
    })?;

    let recovery = ecode.recovery_action();
    let safety = recovery.safety();

    println!("Error Code: {} - {}", ecode.as_str(), ecode.category());
    println!("Recovery Action: {}", recovery.description());
    println!("Safety Level: {:?}", safety);
    println!();

    if dry_run {
        println!("🔍 Dry run mode - no changes will be made");
        if let Some(cmd) = recovery.to_cli_command() {
            println!("Would execute: {}", cmd);
        }
        return Ok(());
    }

    match safety {
        FixSafety::Safe => {
            // Safe actions can be executed automatically
            execute_recovery(&recovery, force).await
        }
        FixSafety::RequiresConfirm => {
            // Require user confirmation
            if force {
                println!("⚠️  Force mode enabled - executing without confirmation");
                execute_recovery(&recovery, true).await
            } else {
                let confirmed = Confirm::new()
                    .with_prompt("Execute this recovery action?")
                    .default(false)
                    .interact()
                    .context("Failed to get user confirmation")?;

                if confirmed {
                    execute_recovery(&recovery, false).await
                } else {
                    println!("Recovery cancelled.");
                    Ok(())
                }
            }
        }
        FixSafety::Unsafe => {
            // Unsafe actions require manual intervention
            println!("⛔ This recovery action requires manual intervention.");
            println!();
            show_manual_instructions(&recovery, &ecode);
            Ok(())
        }
    }
}

/// Execute a recovery action
async fn execute_recovery(recovery: &RecoveryAction, _force: bool) -> Result<()> {
    match recovery {
        RecoveryAction::Retry {
            max_attempts,
            base_backoff_ms,
        } => {
            println!(
                "ℹ️  Retry strategy: {} attempts with {}ms base backoff",
                max_attempts, base_backoff_ms
            );
            println!("   Re-run your previous command to retry the operation.");
            Ok(())
        }

        RecoveryAction::EvictCache { target_mb } => {
            println!("🗑️  Evicting cache...");
            if let Some(mb) = target_mb {
                println!("   Target: {} MB", mb);
            }
            // Execute cache eviction command
            let output = Command::new("aosctl")
                .args(["storage", "cache", "evict"])
                .output()
                .context("Failed to execute cache eviction")?;

            if output.status.success() {
                println!("✅ Cache evicted successfully");
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                println!("❌ Cache eviction failed: {}", stderr);
            }
            Ok(())
        }

        RecoveryAction::WaitForResource {
            resource,
            timeout_ms,
        } => {
            println!(
                "⏳ Waiting for resource '{}' (timeout: {}ms)...",
                resource, timeout_ms
            );
            tokio::time::sleep(std::time::Duration::from_millis(*timeout_ms)).await;
            println!("   Ready to retry. Re-run your previous command.");
            Ok(())
        }

        RecoveryAction::RunMigrations => {
            println!("🔄 Running database migrations...");
            let output = Command::new("aosctl")
                .args(["db", "migrate"])
                .output()
                .context("Failed to run migrations")?;

            if output.status.success() {
                println!("✅ Migrations completed successfully");
                let stdout = String::from_utf8_lossy(&output.stdout);
                if !stdout.is_empty() {
                    println!("{}", stdout);
                }
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                println!("❌ Migration failed: {}", stderr);
            }
            Ok(())
        }

        RecoveryAction::RepairHashes { entity_type } => {
            println!("🔧 Repairing hashes for {}...", entity_type);
            let output = Command::new("aosctl")
                .args(["adapter", "repair-hashes", "--all"])
                .output()
                .context("Failed to repair hashes")?;

            if output.status.success() {
                println!("✅ Hash repair completed successfully");
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                println!("❌ Hash repair failed: {}", stderr);
            }
            Ok(())
        }

        RecoveryAction::RebuildIndex { index_name } => {
            println!("🔧 Rebuilding index: {}", index_name);
            let output = Command::new("aosctl")
                .args(["storage", "index", "rebuild", index_name])
                .output()
                .context("Failed to rebuild index")?;

            if output.status.success() {
                println!("✅ Index rebuilt successfully");
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                println!("❌ Index rebuild failed: {}", stderr);
            }
            Ok(())
        }

        RecoveryAction::CliCommand { command, args } => {
            println!("🔧 Executing: {} {}", command, args.join(" "));
            let output = Command::new(command)
                .args(*args)
                .output()
                .context(format!("Failed to execute {}", command))?;

            if output.status.success() {
                println!("✅ Command completed successfully");
                let stdout = String::from_utf8_lossy(&output.stdout);
                if !stdout.is_empty() {
                    println!("{}", stdout);
                }
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                println!("❌ Command failed: {}", stderr);
            }
            Ok(())
        }

        RecoveryAction::RestartComponent { component } => {
            println!("🔄 Restarting component: {}", component);
            println!("   Run: aosctl service restart {}", component);
            Ok(())
        }

        RecoveryAction::Reinitialize { component } => {
            println!("🔄 Reinitializing component: {}", component);
            println!("   Run: aosctl {} init --force", component);
            Ok(())
        }

        // These require manual intervention - shouldn't reach here
        RecoveryAction::PolicyAdjust
        | RecoveryAction::Resign
        | RecoveryAction::ValidationFix
        | RecoveryAction::ConfigChange { .. }
        | RecoveryAction::InstallDependency { .. }
        | RecoveryAction::Manual
        | RecoveryAction::Quarantine => {
            println!("⛔ This action requires manual intervention.");
            Ok(())
        }
    }
}

/// Show manual instructions for unsafe recovery actions
fn show_manual_instructions(recovery: &RecoveryAction, ecode: &ECode) {
    match recovery {
        RecoveryAction::PolicyAdjust => {
            println!("📋 Manual Steps:");
            println!("   1. Review the policy violation in the logs");
            println!("   2. Edit the policy pack: aosctl policy edit <pack>");
            println!("   3. Re-run preflight: aosctl preflight");
        }

        RecoveryAction::Resign => {
            println!("📋 Manual Steps:");
            println!("   1. Review the signature issue in the logs");
            println!("   2. Re-sign the artifact: aosctl adapter sign <adapter>");
            println!("   3. Verify the signature: aosctl adapter verify <adapter>");
        }

        RecoveryAction::ValidationFix => {
            println!("📋 Manual Steps:");
            println!("   1. Review the validation error message");
            println!("   2. Fix the invalid input or configuration");
            println!("   3. Re-run the command");
        }

        RecoveryAction::ConfigChange {
            setting,
            suggested_value,
        } => {
            println!("📋 Configuration Change Required:");
            println!("   Setting: {}", setting);
            if let Some(value) = suggested_value {
                println!("   Suggested value: {}", value);
            }
            println!();
            println!(
                "   Edit your config file or use: aosctl config set {} <value>",
                setting
            );
        }

        RecoveryAction::InstallDependency { name, version } => {
            println!("📋 Dependency Installation Required:");
            println!("   Package: {}", name);
            if let Some(ver) = version {
                println!("   Version: {}", ver);
            }
            println!();
            println!("   Install the required dependency and retry.");
        }

        RecoveryAction::Quarantine => {
            println!("📋 Resource Quarantined:");
            println!("   The resource has been quarantined due to integrity issues.");
            println!();
            println!("   Steps:");
            println!("   1. Review quarantine log: aosctl storage quarantine list");
            println!("   2. Investigate the issue");
            println!("   3. Release if safe: aosctl storage quarantine release <id>");
        }

        RecoveryAction::Manual => {
            println!("📋 This error requires manual investigation.");
            println!(
                "   Use `aosctl explain {}` for more details.",
                ecode.as_str()
            );
        }

        _ => {
            println!(
                "📋 See `aosctl explain {}` for recovery guidance.",
                ecode.as_str()
            );
        }
    }
}

/// List all recovery actions by safety level
pub async fn list_recovery_actions(safety_filter: Option<&str>) -> Result<()> {
    let filter = safety_filter.map(|s| match s.to_lowercase().as_str() {
        "safe" => Some(FixSafety::Safe),
        "confirm" | "requires_confirm" => Some(FixSafety::RequiresConfirm),
        "unsafe" | "manual" => Some(FixSafety::Unsafe),
        _ => None,
    });

    if let Some(Some(safety)) = filter.as_ref() {
        println!("Recovery Actions (Safety: {:?})", safety);
    } else {
        println!("All Recovery Actions by Safety Level");
    }
    println!("════════════════════════════════════════════════════════════════\n");

    for safety in [
        FixSafety::Safe,
        FixSafety::RequiresConfirm,
        FixSafety::Unsafe,
    ] {
        if let Some(Some(filter_safety)) = filter.as_ref() {
            if *filter_safety != safety {
                continue;
            }
        }

        let icon = match safety {
            FixSafety::Safe => "✅",
            FixSafety::RequiresConfirm => "⚠️",
            FixSafety::Unsafe => "⛔",
        };

        println!("{} {:?}", icon, safety);
        println!("────────────────────────────────────────────────────────────────");

        for ecode in ECode::iter() {
            let recovery = ecode.recovery_action();
            if recovery.safety() == safety {
                println!(
                    "  {} - {} → {}",
                    ecode.as_str(),
                    ecode.category(),
                    recovery.description()
                );
            }
        }
        println!();
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_ecode() {
        assert!(ECode::parse("E1001").is_some());
        assert!(ECode::parse("E9001").is_some());
    }

    #[test]
    fn test_parse_invalid_ecode() {
        assert!(ECode::parse("INVALID").is_none());
        assert!(ECode::parse("E0000").is_none());
    }

    #[test]
    fn test_recovery_action_safety() {
        let retry = RecoveryAction::Retry {
            max_attempts: 3,
            base_backoff_ms: 100,
        };
        assert_eq!(retry.safety(), FixSafety::Safe);

        let migrate = RecoveryAction::RunMigrations;
        assert_eq!(migrate.safety(), FixSafety::RequiresConfirm);

        let policy = RecoveryAction::PolicyAdjust;
        assert_eq!(policy.safety(), FixSafety::Unsafe);
    }
}
