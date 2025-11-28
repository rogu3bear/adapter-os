//! TUI dashboard command for aosctl
//!
//! Launches an interactive terminal dashboard for monitoring and controlling AdapterOS.

use anyhow::Result;
use clap::Args;

/// Arguments for the TUI command
#[derive(Debug, Args, Clone)]
pub struct TuiArgs {
    /// Server URL for API connections (default: http://localhost:8080)
    #[arg(long, env = "AOS_SERVER_URL")]
    pub server_url: Option<String>,
}

/// Run the TUI dashboard
pub async fn run(args: TuiArgs) -> Result<()> {
    adapteros_tui::run_tui(args.server_url).await
}
