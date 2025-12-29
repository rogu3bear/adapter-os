//! Log digest command
//!
//! Runs the log digest script to summarize WARN/ERROR entries.

use crate::output::OutputWriter;
use adapteros_platform::common::PlatformUtils;
use anyhow::{Context, Result};
use clap::Args;
use serde::Serialize;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Args, Clone)]
pub struct LogDigestCommand {
    /// Log directory to scan (defaults to $AOS_VAR_DIR/logs or ./var/logs)
    #[arg(long)]
    pub log_dir: Option<PathBuf>,

    /// Output directory for digest files (defaults to $AOS_VAR_DIR/analysis or ./var/analysis)
    #[arg(long)]
    pub out_dir: Option<PathBuf>,

    /// Lookback window in minutes
    #[arg(long, default_value = "60")]
    pub minutes: u64,

    /// Maximum entries to include in the digest
    #[arg(long, default_value = "500")]
    pub max_entries: usize,

    /// Top message groups to include
    #[arg(long, default_value = "20")]
    pub top: usize,

    /// Python executable to use
    #[arg(long, default_value = "python3")]
    pub python: String,
}

#[derive(Debug, Serialize)]
struct LogDigestResult {
    log_dir: String,
    out_dir: String,
    digest_json: String,
    digest_txt: String,
}

pub async fn run(cmd: LogDigestCommand, output: &OutputWriter) -> Result<()> {
    let var_dir = PlatformUtils::aos_var_dir();
    let log_dir = cmd.log_dir.unwrap_or_else(|| var_dir.join("logs"));
    let out_dir = cmd.out_dir.unwrap_or_else(|| var_dir.join("analysis"));

    std::fs::create_dir_all(&out_dir)
        .with_context(|| format!("Failed to create output directory: {}", out_dir.display()))?;

    let script_path = PathBuf::from("scripts/log_digest.py");
    if !script_path.exists() {
        return Err(anyhow::anyhow!(
            "Log digest script not found at {} (run from repo root)",
            script_path.display()
        ));
    }

    let status = Command::new(&cmd.python)
        .arg(&script_path)
        .arg("--log-dir")
        .arg(&log_dir)
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--minutes")
        .arg(cmd.minutes.to_string())
        .arg("--max-entries")
        .arg(cmd.max_entries.to_string())
        .arg("--top")
        .arg(cmd.top.to_string())
        .status()
        .with_context(|| format!("Failed to run {}", cmd.python))?;

    if !status.success() {
        return Err(anyhow::anyhow!(
            "Log digest script exited with status: {}",
            status
        ));
    }

    let digest_json = out_dir.join("digest.json");
    let digest_txt = out_dir.join("digest.txt");

    let payload = LogDigestResult {
        log_dir: log_dir.display().to_string(),
        out_dir: out_dir.display().to_string(),
        digest_json: digest_json.display().to_string(),
        digest_txt: digest_txt.display().to_string(),
    };

    output.json(&payload)?;
    if !output.mode().is_json() {
        output.success("Log digest completed");
        output.info(format!("log_dir: {}", payload.log_dir));
        output.info(format!("out_dir: {}", payload.out_dir));
        output.info(format!("digest.json: {}", payload.digest_json));
        output.info(format!("digest.txt: {}", payload.digest_txt));
    }

    Ok(())
}
