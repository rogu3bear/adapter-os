//! Adapter migration commands
//!
//! TODO: Migrate to adapteros-aos v3.0 types
//! This module is temporarily stubbed pending migration from the deleted
//! adapteros-single-file-adapter crate.
//!
//! Migrate existing adapters to .aos format

use crate::output::OutputWriter;
use adapteros_core::AosError;
// Removed: use adapteros_core::B3Hash;
// Removed: use adapteros_lora_worker::training::{TrainingConfig, TrainingExample};
// Removed: use adapteros_single_file_adapter::{...};

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

async fn migrate_adapter(_args: AdapterMigrateArgs, output: &OutputWriter) -> Result<()> {
    output.warning("migrate adapter command is temporarily disabled pending migration to v3.0 types");

    // TODO: Migrate to adapteros-aos v3.0 types
    // The original implementation used:
    // - adapteros_single_file_adapter::format::{AdapterManifest, LineageInfo, SingleFileAdapter}
    // - adapteros_single_file_adapter::SingleFileAdapterPackager
    // - adapteros_lora_worker::training::{TrainingConfig, TrainingExample}
    //
    // These need to be replaced with types from adapteros-aos v3.0

    Err(anyhow::anyhow!(AosError::Config(
        "migrate adapter: pending migration to adapteros-aos v3.0 types".to_string()
    )))
}
