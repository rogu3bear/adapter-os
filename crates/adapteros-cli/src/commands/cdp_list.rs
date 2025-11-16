//! List Commit Delta Packs for a repository

use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
pub struct CdpListArgs {
    /// Repository ID to list CDPs for
    #[arg(long)]
    pub repo_id: String,

    /// CDP storage directory
    #[arg(long, default_value = "var/cdps")]
    pub storage: PathBuf,
}

pub async fn execute(args: CdpListArgs) -> anyhow::Result<()> {
    // CdpStore implementation will be added when code intelligence is integrated
    println!(
        "CDP listing not yet implemented for repository: {}",
        args.repo_id
    );
    Ok(())
}
