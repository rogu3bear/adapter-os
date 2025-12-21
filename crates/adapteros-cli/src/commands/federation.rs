//! Federation management commands
//!
//! Provides subcommands for federation operations including cross-host
//! signature verification.

use crate::output::OutputWriter;
use adapteros_db::Db;
use adapteros_verify::verify_cross_host;
use anyhow::{Context, Result};
use clap::Subcommand;
use serde::Serialize;
use std::path::{Path, PathBuf};
use tracing::info;

/// Federation management subcommands
#[derive(Debug, Subcommand, Clone)]
pub enum FederationCommand {
    /// Verify cross-host federation signatures
    #[command(after_help = r#"Examples:
  aosctl federation verify --bundle-dir ./var/telemetry
  aosctl federation verify --bundle-dir ./var/telemetry --database ./var/cp.db
  aosctl federation verify --bundle-dir ./var/telemetry --json > federation.json
"#)]
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
pub async fn handle_federation_command(
    cmd: FederationCommand,
    output: &OutputWriter,
) -> Result<()> {
    let command_name = get_federation_command_name(&cmd);

    info!(command = ?cmd, "Handling federation command");

    // Emit telemetry
    let _ = crate::cli_telemetry::emit_cli_command(command_name, None, true).await;

    match cmd {
        FederationCommand::Verify {
            bundle_dir,
            database,
        } => verify_federation(&bundle_dir, &database, output).await,
    }
}

// ============================================================
// Federation Verify Implementation
// (consolidated from verify_federation.rs)
// ============================================================

#[derive(Serialize)]
struct FederationVerificationResult {
    pub total_bundles: usize,
    pub total_signatures: usize,
    pub verified: bool,
    pub errors: Vec<String>,
}

/// Verify cross-host federation signatures
pub async fn verify_federation(
    bundle_dir: &Path,
    database: &Path,
    output: &OutputWriter,
) -> Result<()> {
    output.info(format!(
        "Verifying federation chain: {}",
        bundle_dir.display()
    ));

    // Connect to database
    output.progress("Connecting to database");
    let db_path_str = database.to_str().ok_or_else(|| {
        anyhow::anyhow!(
            "Database path contains invalid UTF-8: {}",
            database.display()
        )
    })?;
    let db = Db::connect(db_path_str)
        .await
        .context("Failed to connect to database")?;

    // Run migrations to ensure federation tables exist
    db.migrate()
        .await
        .context("Failed to run database migrations")?;

    output.progress_done(true);

    // Verify cross-host chain
    output.progress("Verifying cross-host signatures");

    match verify_cross_host(bundle_dir, &db).await {
        Ok(_) => {
            output.progress_done(true);
            output.success("Federation chain verification successful");

            if output.is_json() {
                let result = FederationVerificationResult {
                    total_bundles: 0, // Would be populated from actual verification
                    total_signatures: 0,
                    verified: true,
                    errors: vec![],
                };
                output.json(&result)?;
            }
        }
        Err(e) => {
            output.progress_done(false);
            output.error(format!("Federation chain verification failed: {}", e));

            if output.is_json() {
                let result = FederationVerificationResult {
                    total_bundles: 0,
                    total_signatures: 0,
                    verified: false,
                    errors: vec![e.to_string()],
                };
                output.json(&result)?;
            }

            return Err(anyhow::anyhow!("Federation verification failed"));
        }
    }

    Ok(())
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
