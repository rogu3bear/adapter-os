//! Log prompt command
//!
//! Builds an LLM prompt from triage output.

use crate::output::OutputWriter;
use adapteros_platform::common::PlatformUtils;
use anyhow::{Context, Result};
use clap::Args;
use serde::Serialize;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Args, Clone)]
pub struct LogPromptCommand {
    /// Triage JSON path (defaults to $AOS_VAR_DIR/analysis/triage.json)
    #[arg(long)]
    pub triage: Option<PathBuf>,

    /// Output directory for prompts (defaults to $AOS_VAR_DIR/analysis/proposals)
    #[arg(long)]
    pub out_dir: Option<PathBuf>,

    /// Maximum findings to include
    #[arg(long, default_value = "20")]
    pub max_findings: usize,

    /// Maximum unmatched groups to include
    #[arg(long, default_value = "10")]
    pub max_unmatched: usize,

    /// Python executable to use
    #[arg(long, default_value = "python3")]
    pub python: String,
}

#[derive(Debug, Serialize)]
struct LogPromptResult {
    triage: String,
    out_dir: String,
    prompt_latest: String,
}

pub async fn run(cmd: LogPromptCommand, output: &OutputWriter) -> Result<()> {
    let var_dir = PlatformUtils::aos_var_dir();
    let triage = cmd
        .triage
        .unwrap_or_else(|| var_dir.join("analysis").join("triage.json"));
    let out_dir = cmd
        .out_dir
        .unwrap_or_else(|| var_dir.join("analysis").join("proposals"));

    std::fs::create_dir_all(&out_dir)
        .with_context(|| format!("Failed to create output directory: {}", out_dir.display()))?;

    let script_path = PathBuf::from("scripts/log_prompt.py");
    if !script_path.exists() {
        return Err(anyhow::anyhow!(
            "Log prompt script not found at {} (run from repo root)",
            script_path.display()
        ));
    }

    let status = Command::new(&cmd.python)
        .arg(&script_path)
        .arg("--triage")
        .arg(&triage)
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
            "Log prompt script exited with status: {}",
            status
        ));
    }

    let prompt_latest = out_dir.join("prompt-latest.md");
    let payload = LogPromptResult {
        triage: triage.display().to_string(),
        out_dir: out_dir.display().to_string(),
        prompt_latest: prompt_latest.display().to_string(),
    };

    output.json(&payload)?;
    if !output.mode().is_json() {
        output.success("Log prompt generated");
        output.info(format!("triage: {}", payload.triage));
        output.info(format!("prompt: {}", payload.prompt_latest));
    }

    Ok(())
}
