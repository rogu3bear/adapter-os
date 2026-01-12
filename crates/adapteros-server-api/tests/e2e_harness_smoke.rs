mod common;
mod support;

use anyhow::Result;
use std::time::Duration;
use support::e2e_harness::{E2eHarness, HarnessSetup};

#[tokio::test]
async fn e2e_harness_waits_for_worker_or_skips() -> Result<()> {
    let setup = E2eHarness::from_env().await?;
    let harness = match setup {
        HarnessSetup::Skip { reason } => {
            eprintln!("skipping: {}", reason);
            return Ok(());
        }
        HarnessSetup::Ready(h) => h,
    };

    harness
        .wait_for_worker_ready(Duration::from_secs(5))
        .await?;

    Ok(())
}
