//! Interactive tutorial command

use crate::output::OutputWriter;
use anyhow::Result;

#[derive(clap::Args)]
pub struct TutorialArgs {
    /// Run advanced tutorial
    #[arg(long)]
    pub advanced: bool,

    /// Non-interactive mode for CI
    #[arg(long)]
    pub ci: bool,
}

pub async fn run_tutorial(mut out: OutputWriter, args: TutorialArgs) -> Result<()> {
    out.section("AdapterOS Tutorial");

    if !args.advanced {
        out.info("Quickstart: init → verify → diag");

        step(&mut out, "Initialize tenant", || {
            run_command("aosctl", &["tenant-init", "--dry-run"])
        })?;
        step(&mut out, "Verify telemetry", || {
            run_command("aosctl", &["telemetry-verify"])
        })?;
        step(&mut out, "System diagnostics", || {
            run_command("aosctl", &["diag", "--system"])
        })?;

        out.success("Quickstart complete");
        return Ok(());
    }

    out.info("Advanced: serve → import → audit");

    step(&mut out, "Serve worker (dry-run)", || {
        run_command("aosctl", &["serve", "--dry-run"])
    })?;
    step(&mut out, "Import adapter (example)", || {
        run_command(
            "aosctl",
            &[
                "adapter-register",
                "--path",
                "./examples/adapters/demo-a.lora",
            ],
        )
    })?;
    step(&mut out, "Audit chain", || {
        run_command("aosctl", &["telemetry-verify"])
    })?;

    out.success("Advanced tutorial complete");
    Ok(())
}

fn step<F: FnOnce() -> Result<()>>(out: &mut OutputWriter, title: &str, f: F) -> Result<()> {
    out.section(title);
    f().map(|_| out.success("ok")).map_err(|e| {
        out.error(&e.to_string());
        e
    })
}

// Tiny runner to keep air-gapped, no shell tricks
fn run_command(bin: &str, args: &[&str]) -> Result<()> {
    use std::process::Command;
    let status = Command::new(bin).args(args).status()?;
    if status.success() {
        Ok(())
    } else {
        anyhow::bail!("command failed")
    }
}
