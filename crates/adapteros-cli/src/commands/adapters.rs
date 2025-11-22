//! Adapters group commands
//!
//! This module provides a wrapper around adapter::AdapterCommand::Register
//! for backwards compatibility with existing CLI patterns (app.rs, deploy.rs).
//!
//! The canonical implementation is in adapter.rs.

use crate::commands::adapter::{handle_adapter_command, AdapterCommand};
use crate::output::OutputWriter;
use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser, Clone)]
#[command(name = "adapters")]
pub struct AdaptersArgs {
    #[command(subcommand)]
    pub cmd: AdaptersCmd,
}

#[derive(Debug, Subcommand, Clone)]
pub enum AdaptersCmd {
    /// Register a packaged adapter by path (dir or weights file)
    Register(RegisterArgs),
}

#[derive(Debug, Parser, Clone)]
pub struct RegisterArgs {
    /// Path to packaged adapter dir or weights.safetensors
    #[arg(long)]
    pub path: PathBuf,

    /// Adapter ID (defaults to directory name)
    #[arg(long)]
    pub adapter_id: Option<String>,

    /// Name to display (defaults to adapter_id)
    #[arg(long)]
    pub name: Option<String>,

    /// Rank (defaults from manifest if present; else 8)
    #[arg(long)]
    pub rank: Option<i32>,

    /// Tier (ephemeral=0, persistent=1) default ephemeral
    #[arg(long)]
    pub tier: Option<i32>,

    /// Control plane base URL
    #[arg(long, default_value = "http://127.0.0.1:8080/api")]
    pub base_url: String,
}

pub async fn run(args: AdaptersArgs, output: &OutputWriter) -> Result<()> {
    match args.cmd {
        AdaptersCmd::Register(reg) => {
            // Delegate to the canonical adapter::AdapterCommand::Register
            let adapter_cmd = AdapterCommand::Register {
                path: reg.path,
                adapter_id: reg.adapter_id,
                name: reg.name,
                rank: reg.rank,
                tier: reg.tier,
                base_url: reg.base_url,
            };
            handle_adapter_command(adapter_cmd, output)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))
        }
    }
}
