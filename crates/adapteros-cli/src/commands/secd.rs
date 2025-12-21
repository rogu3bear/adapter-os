//! Security daemon (secd) management commands
//!
//! Provides a git-style subcommand interface for aos-secd operations:
//! - `aosctl secd status` - Show aos-secd daemon status
//! - `aosctl secd audit` - Show aos-secd operation audit trail

use clap::Subcommand;
use std::path::PathBuf;

#[cfg(feature = "secd-support")]
use crate::commands::{secd_audit, secd_status};

/// Security daemon (secd) subcommands
#[derive(Debug, Subcommand, Clone)]
pub enum SecdCommand {
    /// Show aos-secd daemon status
    #[command(after_help = r#"Examples:
  aosctl secd status
  aosctl secd status --database ./var/custom.db
  aosctl secd status --pid-file /custom/path/aos-secd.pid
"#)]
    Status {
        /// PID file path
        #[arg(long, default_value = "/var/run/aos-secd.pid")]
        pid_file: PathBuf,

        /// Heartbeat file path
        #[arg(long, default_value = "/var/run/aos-secd.heartbeat")]
        heartbeat_file: PathBuf,

        /// Socket path
        #[arg(long, default_value = "/var/run/aos-secd.sock")]
        socket: PathBuf,

        /// Database path
        #[arg(long, default_value = "./var/aos-cp.sqlite3")]
        database: PathBuf,
    },

    /// Show aos-secd operation audit trail
    #[command(after_help = r#"Examples:
  aosctl secd audit
  aosctl secd audit --limit 100
  aosctl secd audit --operation sign
  aosctl secd audit --database ./var/custom.db
"#)]
    Audit {
        /// Database path
        #[arg(long, default_value = "./var/aos-cp.sqlite3")]
        database: PathBuf,

        /// Number of operations to show
        #[arg(short, long, default_value = "50")]
        limit: i64,

        /// Filter by operation type (sign, seal, unseal, get_public_key)
        #[arg(short, long)]
        operation: Option<String>,
    },
}

/// Handle secd subcommands
///
/// Routes secd commands to appropriate handlers. Requires the `secd-support` feature.
///
/// # Errors
///
/// Returns error if:
/// - The `secd-support` feature is not enabled
/// - The underlying command handler fails
#[cfg(feature = "secd-support")]
pub async fn handle_secd_command(cmd: SecdCommand) -> anyhow::Result<()> {
    use tracing::info;

    info!(command = ?cmd, "Handling secd command");

    match cmd {
        SecdCommand::Status {
            pid_file,
            heartbeat_file,
            socket,
            database,
        } => {
            secd_status::run(&pid_file, &heartbeat_file, &socket, Some(&database)).await?;
        }
        SecdCommand::Audit {
            database,
            limit,
            operation,
        } => {
            secd_audit::run(&database, limit, operation.as_deref()).await?;
        }
    }

    Ok(())
}

/// Handle secd subcommands (feature disabled stub)
///
/// Returns an error indicating the secd-support feature is required.
#[cfg(not(feature = "secd-support"))]
pub async fn handle_secd_command(cmd: SecdCommand) -> anyhow::Result<()> {
    let _ = cmd; // Suppress unused warning
    anyhow::bail!("secd commands require the 'secd-support' feature to be enabled")
}

/// Get secd command name for telemetry
pub fn get_secd_command_name(cmd: &SecdCommand) -> &'static str {
    match cmd {
        SecdCommand::Status { .. } => "secd_status",
        SecdCommand::Audit { .. } => "secd_audit",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_secd_command_name() {
        let status_cmd = SecdCommand::Status {
            pid_file: PathBuf::from("/var/run/aos-secd.pid"),
            heartbeat_file: PathBuf::from("/var/run/aos-secd.heartbeat"),
            socket: PathBuf::from("/var/run/aos-secd.sock"),
            database: PathBuf::from("./var/aos-cp.sqlite3"),
        };
        assert_eq!(get_secd_command_name(&status_cmd), "secd_status");

        let audit_cmd = SecdCommand::Audit {
            database: PathBuf::from("./var/aos-cp.sqlite3"),
            limit: 50,
            operation: None,
        };
        assert_eq!(get_secd_command_name(&audit_cmd), "secd_audit");
    }

    #[test]
    fn test_secd_command_variants() {
        // Ensure command variants exist and can be constructed
        let _status = SecdCommand::Status {
            pid_file: PathBuf::from("/var/run/aos-secd.pid"),
            heartbeat_file: PathBuf::from("/var/run/aos-secd.heartbeat"),
            socket: PathBuf::from("/var/run/aos-secd.sock"),
            database: PathBuf::from("./var/aos-cp.sqlite3"),
        };

        let _audit = SecdCommand::Audit {
            database: PathBuf::from("./var/aos-cp.sqlite3"),
            limit: 100,
            operation: Some("sign".to_string()),
        };
    }
}
