//! List Commit Delta Packs for a repository

use super::NOT_IMPLEMENTED_MESSAGE;
use clap::Parser;
use std::path::PathBuf;

/// [NOT IMPLEMENTED] List Commit Delta Packs for a repository
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
        "{}",
        NOT_IMPLEMENTED_MESSAGE
    );
    println!("{}", NOT_IMPLEMENTED_MESSAGE);
    Err(anyhow::anyhow!(NOT_IMPLEMENTED_MESSAGE))
}
