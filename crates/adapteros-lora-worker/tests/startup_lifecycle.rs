//! Gated end-to-end startup test for aos-worker model loading and clean shutdown.
//!
//! This test is skipped unless the following environment variables are set:
//! - `AOS_E2E_MODEL_PATH`: path to a tiny model directory containing `model.safetensors`
//!   (or shard) and `config.json`.
//! - `AOS_E2E_UDS`: UDS socket path to bind (e.g., `/tmp/aos-e2e.sock`).
//! Optional:
//! - `AOS_E2E_BACKEND`: backend choice (`auto`, `coreml`, `metal`, `mlx`). Defaults to `auto`.
//!
//! Usage:
//! ```bash
//! AOS_E2E_MODEL_PATH=/path/to/model \
//! AOS_E2E_UDS=/tmp/aos-e2e.sock \
//! cargo test -p adapteros-lora-worker --test startup_lifecycle -- --nocapture
//! ```

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const STARTUP_TIMEOUT: Duration = Duration::from_secs(60);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);

#[test]
fn startup_and_shutdown_with_model() -> anyhow::Result<()> {
    let model_path = match std::env::var("AOS_E2E_MODEL_PATH") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("skipping: set AOS_E2E_MODEL_PATH to run this test");
            return Ok(());
        }
    };
    let uds_path = std::env::var("AOS_E2E_UDS").unwrap_or_else(|_| "/tmp/aos-e2e.sock".to_string());
    // Prefer explicit e2e backend, fall back to model backend env, otherwise auto
    let backend = std::env::var("AOS_E2E_BACKEND")
        .ok()
        .or_else(|| std::env::var("AOS_MODEL_BACKEND").ok())
        .unwrap_or_else(|| "auto".to_string());

    // Pre-clean socket if present
    let _ = fs::remove_file(&uds_path);

    // Spawn worker via cargo run to exercise real binary
    let mut command = Command::new("cargo");
    command.arg("run").arg("-p").arg("adapteros-lora-worker");

    // Enable MLX backend when requested; requires the feature to be built in
    if backend == "mlx" {
        command.arg("--features").arg("multi-backend");
    }

    let mut child = command
        .arg("--bin")
        .arg("aos_worker")
        .arg("--")
        .arg("--uds-path")
        .arg(&uds_path)
        .arg("--model-path")
        .arg(&model_path)
        .arg("--backend")
        .arg(&backend)
        .env("AOS_DEV_NO_AUTH", "1")
        .env("AOS_DETERMINISTIC", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("failed to capture stdout"))?;
    let mut reader = BufReader::new(stdout).lines();

    let mut saw_backend = false;
    let mut saw_listen = false;
    let deadline = Instant::now() + STARTUP_TIMEOUT;

    while Instant::now() < deadline {
        if let Some(line) = reader
            .by_ref()
            .next()
            .transpose()
            .map_err(|e| anyhow::anyhow!(e))?
        {
            let line = line.to_lowercase();
            if line.contains("loaded model")
                || line.contains("creating coreml kernel backend")
                || line.contains("creating metal kernel backend")
                || line.contains("creating mlx ffi kernel backend")
            {
                saw_backend = true;
            }
            if line.contains("uds") && line.contains("listening") {
                saw_listen = true;
                break;
            }
        } else {
            // EOF before ready; break and fail
            break;
        }
    }

    // Ensure we stop the worker either way
    let _ = child.kill();
    let status = wait_with_timeout(&mut child, SHUTDOWN_TIMEOUT)?;

    assert!(
        saw_backend,
        "model/backend did not initialize before timeout"
    );
    assert!(saw_listen, "UDS server did not report listening");
    assert!(status.success(), "worker did not exit cleanly");

    // Cleanup socket
    let _ = fs::remove_file(&uds_path);

    // Basic sanity check: model path existed
    assert!(Path::new(&model_path).exists(), "model path must exist");

    Ok(())
}

fn wait_with_timeout(
    child: &mut std::process::Child,
    timeout: Duration,
) -> anyhow::Result<std::process::ExitStatus> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            child.kill().ok();
            child.wait()?;
            return Err(anyhow::anyhow!("process did not exit within {:?}", timeout));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
