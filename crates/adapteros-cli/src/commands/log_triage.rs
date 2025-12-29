//! Log triage command
//!
//! Applies rules to a log digest to produce actionable findings.

use crate::output::OutputWriter;
use adapteros_platform::common::PlatformUtils;
use anyhow::{Context, Result};
use clap::Args;
use serde::Serialize;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Args, Clone)]
pub struct LogTriageCommand {
    /// Digest JSON path (defaults to $AOS_VAR_DIR/analysis/digest.json)
    #[arg(long)]
    pub digest: Option<PathBuf>,

    /// Rules file path (defaults to configs/log_triage_rules.json)
    #[arg(long, env = "AOS_LOG_TRIAGE_RULES")]
    pub rules: Option<PathBuf>,

    /// Output directory for triage files (defaults to $AOS_VAR_DIR/analysis)
    #[arg(long)]
    pub out_dir: Option<PathBuf>,

    /// Maximum findings to include
    #[arg(long, default_value = "50")]
    pub max_findings: usize,

    /// Maximum unmatched groups to include
    #[arg(long, default_value = "20")]
    pub max_unmatched: usize,

    /// Python executable to use
    #[arg(long, default_value = "python3")]
    pub python: String,
}

#[derive(Debug, Serialize)]
struct LogTriageResult {
    digest: String,
    rules: String,
    out_dir: String,
    triage_json: String,
    triage_txt: String,
}

pub async fn run(cmd: LogTriageCommand, output: &OutputWriter) -> Result<()> {
    let var_dir = PlatformUtils::aos_var_dir();
    let digest = cmd
        .digest
        .unwrap_or_else(|| var_dir.join("analysis").join("digest.json"));
    let rules = cmd
        .rules
        .unwrap_or_else(|| PathBuf::from("configs/log_triage_rules.json"));
    let out_dir = cmd.out_dir.unwrap_or_else(|| var_dir.join("analysis"));

    std::fs::create_dir_all(&out_dir)
        .with_context(|| format!("Failed to create output directory: {}", out_dir.display()))?;

    let script_path = PathBuf::from("scripts/log_triage.py");
    if !script_path.exists() {
        return Err(anyhow::anyhow!(
            "Log triage script not found at {} (run from repo root)",
            script_path.display()
        ));
    }

    let status = Command::new(&cmd.python)
        .arg(&script_path)
        .arg("--digest")
        .arg(&digest)
        .arg("--rules")
        .arg(&rules)
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--max-findings")
        .arg(cmd.max_findings.to_string())
        .arg("--max-unmatched")
        .arg(cmd.max_unmatched.to_string())
        .status()
        .with_context(|| format!("Failed to run {}", cmd.python))?;

    if !status.success() {
        return Err(anyhow::anyhow!(
            "Log triage script exited with status: {}",
            status
        ));
    }

    let triage_json = out_dir.join("triage.json");
    let triage_txt = out_dir.join("triage.txt");

    let payload = LogTriageResult {
        digest: digest.display().to_string(),
        rules: rules.display().to_string(),
        out_dir: out_dir.display().to_string(),
        triage_json: triage_json.display().to_string(),
        triage_txt: triage_txt.display().to_string(),
    };

    output.json(&payload)?;
    if !output.mode().is_json() {
        output.success("Log triage completed");
        output.info(format!("digest: {}", payload.digest));
        output.info(format!("rules: {}", payload.rules));
        output.info(format!("triage.json: {}", payload.triage_json));
        output.info(format!("triage.txt: {}", payload.triage_txt));
    }

    Ok(())
}
