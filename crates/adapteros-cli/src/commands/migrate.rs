//! Adapter migration commands
//!
//! Migrate existing adapters to .aos format

use crate::output::OutputWriter;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser, Clone)]
#[command(name = "migrate")]
pub struct MigrateArgs {
    #[command(subcommand)]
    pub cmd: MigrateCmd,
}

#[derive(Debug, Subcommand, Clone)]
pub enum MigrateCmd {
    /// Migrate adapter directory to .aos file
    Adapter(AdapterMigrateArgs),
}

#[derive(Debug, Parser, Clone)]
pub struct AdapterMigrateArgs {
    /// Source adapter directory
    #[arg(long)]
    pub source: PathBuf,

    /// Output .aos file path
    #[arg(long)]
    pub output: PathBuf,

    /// Adapter ID for .aos file
    #[arg(long)]
    pub adapter_id: Option<String>,

    /// Version for .aos file
    #[arg(long, default_value = "1.0.0")]
    pub version: String,
}

pub async fn run(args: MigrateArgs, output: &OutputWriter) -> Result<()> {
    match args.cmd {
        MigrateCmd::Adapter(mig) => migrate_adapter(mig, output).await,
    }
}

async fn migrate_adapter(args: AdapterMigrateArgs, output: &OutputWriter) -> Result<()> {
    output.info(&format!(
        "Migrating adapter from {} to {}",
        args.source.display(),
        args.output.display()
    ));

    // Convert Option<String> to Option<&str> for adapter_id
    let adapter_id = args.adapter_id.as_deref();

    // Call the migration logic from adapteros_aos
    match adapteros_aos::single_file::migrate_adapter(
        adapter_id,
        &args.source,
        &args.output,
        &args.version,
    )
    .await
    {
        Ok(result) => {
            output.success(&format!(
                "Successfully migrated adapter. Duration: {}ms",
                result.duration_ms
            ));
            if !result.warnings.is_empty() {
                output.warning("Migration completed with warnings:");
                for warning in result.warnings {
                    output.warning(&format!("  - {}", warning));
                }
            }
            Ok(())
        }
        Err(e) => {
            output.error(&format!("Failed to migrate adapter: {}", e));
            Err(e.into())
        }
    }
}
