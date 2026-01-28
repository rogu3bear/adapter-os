//! Bootstrap command for initial setup

use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use tracing::{error, info};

#[derive(Serialize, Deserialize)]
struct ProgressUpdate {
    step: String,
    progress: f64,
    message: String,
    status: String,
}

pub async fn run(
    mode: &str,
    air_gapped: bool,
    json_output: bool,
    checkpoint_file: Option<PathBuf>,
) -> Result<()> {
    if !json_output {
        info!("Starting adapterOS bootstrap...");
        info!("Mode: {}", mode);
        info!("Air-gapped: {}", air_gapped);
    }

    // Determine workspace root (assuming CLI is in crates/mplora-cli)
    let workspace_root = std::env::current_dir()
        .map_err(|e| AosError::Io(format!("Failed to get current directory: {}", e)))?;

    let script_path = workspace_root.join("scripts/bootstrap_with_checkpoints.sh");

    if !script_path.exists() {
        return Err(AosError::NotFound(format!(
            "Bootstrap script not found at: {}",
            script_path.display()
        )));
    }

    // Prepare checkpoint file argument
    let checkpoint_arg = checkpoint_file
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "var/adapteros_install.state".to_string());

    // Build command arguments
    let air_gapped_str = if air_gapped { "true" } else { "false" };
    let json_str = if json_output { "true" } else { "false" };

    // Execute bootstrap script
    let mut child = Command::new("bash")
        .arg(&script_path)
        .arg(&checkpoint_arg)
        .arg(mode)
        .arg(air_gapped_str)
        .arg(json_str)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| AosError::System(format!("Failed to spawn bootstrap process: {}", e)))?;

    // Stream stdout
    if let Some(stdout) = child.stdout.take() {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            match line {
                Ok(line) => {
                    // If JSON mode, try to parse and validate
                    if json_output && line.trim().starts_with('{') {
                        if let Ok(progress) = serde_json::from_str::<ProgressUpdate>(&line) {
                            // Re-emit the JSON
                            info!("Bootstrap progress: {}", serde_json::to_string(&progress)?);
                        } else {
                            // Not valid progress JSON, just log
                            info!("Bootstrap output: {}", line);
                        }
                    } else {
                        // Regular output
                        info!("Bootstrap output: {}", line);
                    }
                }
                Err(e) => {
                    error!("Error reading output: {}", e);
                    break;
                }
            }
        }
    }

    // Wait for completion
    let status = child
        .wait()
        .map_err(|e| AosError::System(format!("Failed to wait for bootstrap process: {}", e)))?;

    if !status.success() {
        return Err(AosError::System(format!(
            "Bootstrap failed with exit code: {:?}",
            status.code()
        )));
    }

    if json_output {
        let completion = ProgressUpdate {
            step: "complete".to_string(),
            progress: 1.0,
            message: "Bootstrap completed successfully".to_string(),
            status: "completed".to_string(),
        };
        info!(
            "Bootstrap completion: {}",
            serde_json::to_string(&completion)?
        );
    } else {
        info!("✓ Bootstrap completed successfully!");
    }

    Ok(())
}
