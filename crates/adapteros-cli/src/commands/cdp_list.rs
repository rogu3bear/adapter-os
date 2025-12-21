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

/// Execute the CDP list command
///
/// Reserved: Awaiting code intelligence integration (CDP store implementation)
pub async fn execute(args: CdpListArgs) -> anyhow::Result<()> {
    tracing::warn!(
        repo_id = %args.repo_id,
        "CDP listing not yet implemented - awaiting code intelligence integration"
    );
    Ok(())
}
