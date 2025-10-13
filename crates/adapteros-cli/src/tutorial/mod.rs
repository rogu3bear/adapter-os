//! Interactive tutorial module

use anyhow::Result;

pub mod quickstart;
pub mod advanced;

pub async fn run_tutorial(advanced: bool, ci_mode: bool) -> Result<()> {
    if advanced {
        advanced::run(ci_mode).await
    } else {
        quickstart::run(ci_mode).await
    }
}

