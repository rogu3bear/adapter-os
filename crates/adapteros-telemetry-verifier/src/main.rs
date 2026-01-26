use adapteros_telemetry::verifier::{run_verifier, workspace_root};
use anyhow::Result;
use std::process;

fn main() -> Result<()> {
    let root = workspace_root();
    let report = run_verifier(&root)?;
    let json = serde_json::to_string_pretty(&report)?;
    println!("{}", json);

    if report.failed {
        process::exit(1);
    }

    Ok(())
}
