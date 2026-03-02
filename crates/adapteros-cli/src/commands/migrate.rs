//! Adapter migration commands
//!
//! Migrate existing adapters to .aos format

use crate::commands::NOT_IMPLEMENTED_MESSAGE;
use crate::output::OutputWriter;
use adapteros_core::AosError;
// Removed: use adapteros_core::B3Hash;
// Removed: use adapteros_lora_worker::training::{TrainingConfig, TrainingExample};
// Removed: use adapteros_aos::single_file::{...};

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
    /// Migrate adapter directory to .aos file [NOT IMPLEMENTED]
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

async fn migrate_adapter(_args: AdapterMigrateArgs, output: &OutputWriter) -> Result<()> {
    output.warning("migrate adapter command is temporarily disabled");
    output.info(NOT_IMPLEMENTED_MESSAGE);

    Err(anyhow::anyhow!(AosError::Config(
        NOT_IMPLEMENTED_MESSAGE.to_string()
    )))
}
