//! Federation management commands
//!
//! Provides subcommands for federation operations including cross-host
//! signature verification.

use crate::commands::verify_federation;
use crate::output::OutputWriter;
use anyhow::Result;
use clap::Subcommand;
use std::path::PathBuf;
use tracing::info;

/// Federation management subcommands
#[derive(Debug, Subcommand, Clone)]
pub enum FederationCommand {
    /// Verify cross-host federation signatures
    #[command(
        after_help = r#"Examples:
  aosctl federation verify --bundle-dir ./var/telemetry
  aosctl federation verify --bundle-dir ./var/telemetry --database ./var/cp.db
  aosctl federation verify --bundle-dir ./var/telemetry --json > federation.json
"#
    )]
    Verify {
        /// Telemetry bundle directory
        #[arg(short, long)]
        bundle_dir: PathBuf,

        /// Database path
        #[arg(long, default_value = "./var/cp.db")]
        database: PathBuf,
    },
}

/// Get federation command name for telemetry
fn get_federation_command_name(cmd: &FederationCommand) -> &'static str {
    match cmd {
        FederationCommand::Verify { .. } => "federation_verify",
    }
}

/// Handle federation management commands
///
/// Routes federation commands to appropriate handlers
pub async fn handle_federation_command(cmd: FederationCommand, output: &OutputWriter) -> Result<()> {
    let command_name = get_federation_command_name(&cmd);

    info!(command = ?cmd, "Handling federation command");

    // Emit telemetry
    let _ = crate::cli_telemetry::emit_cli_command(command_name, None, true).await;

    match cmd {
        FederationCommand::Verify {
            bundle_dir,
            database,
        } => {
            verify_federation::run(&bundle_dir, &database, output).await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_federation_command_name() {
        assert_eq!(
            get_federation_command_name(&FederationCommand::Verify {
                bundle_dir: PathBuf::from("./var/telemetry"),
                database: PathBuf::from("./var/cp.db"),
            }),
            "federation_verify"
        );
    }
}
