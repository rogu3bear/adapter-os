//! Gated end-to-end startup test for aos-worker model loading and clean shutdown.
//!
//! This test is skipped unless the following environment variables are set:
//! - `AOS_E2E_MODEL_PATH`: path to a tiny model directory containing `model.safetensors`
//!   (or shard) and `config.json`.
//! - `AOS_E2E_UDS`: UDS socket path to bind (e.g., `var/run/aos-e2e.sock`).
//! Optional:
//! - `AOS_E2E_BACKEND`: backend choice (`auto`, `coreml`, `metal`, `mlx`). Defaults to `auto`.
//!
//! Usage:
//! ```bash
//! AOS_E2E_MODEL_PATH=/path/to/model \
//! AOS_E2E_UDS=var/run/aos-e2e.sock \
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
    let uds_path =
        std::env::var("AOS_E2E_UDS").unwrap_or_else(|_| "var/run/aos-e2e.sock".to_string());
    // Prefer explicit e2e backend, fall back to model backend env, otherwise auto
    let backend = std::env::var("AOS_E2E_BACKEND")
        .ok()
        .or_else(|| std::env::var("AOS_MODEL_BACKEND").ok())
        .unwrap_or_else(|| "auto".to_string());

    if let Some(parent) = Path::new(&uds_path).parent() {
        fs::create_dir_all(parent)?;
    }

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

/// Test that worker fails fast at startup if cache budget is not configured.
/// This verifies the fail-fast behavior required by PRD-RECT-005.
#[test]
fn test_worker_fails_fast_on_missing_cache_budget() -> anyhow::Result<()> {
    // Use a minimal test setup that doesn't require a real model
    let uds_path = "var/run/test-no-budget.sock";

    if let Some(parent) = Path::new(uds_path).parent() {
        fs::create_dir_all(parent)?;
    }
    let _ = fs::remove_file(uds_path);

    // Create a temporary manifest file (even though we won't reach model loading)
    let temp_dir = tempfile::tempdir()?;
    let manifest_path = temp_dir.path().join("test-manifest.yaml");
    fs::write(
        &manifest_path,
        r#"
schema: adapteros.manifest.v3
base:
  model_id: test-model
  model_hash: "0000000000000000000000000000000000000000000000000000000000000000"
  arch: llama
  vocab_size: 32000
  hidden_dim: 4096
  n_layers: 32
  n_heads: 32
  config_hash: "0000000000000000000000000000000000000000000000000000000000000000"
  tokenizer_hash: "0000000000000000000000000000000000000000000000000000000000000000"
  tokenizer_cfg_hash: "0000000000000000000000000000000000000000000000000000000000000000"
"#,
    )?;

    // Spawn worker WITHOUT setting AOS_MODEL_CACHE_MAX_MB
    // This should fail fast at startup, NOT when trying to load a model
    let mut command = Command::new("cargo");
    let start = Instant::now();

    let mut child = command
        .arg("run")
        .arg("-p")
        .arg("adapteros-lora-worker")
        .arg("--bin")
        .arg("aos-worker")
        .arg("--")
        .arg("--uds-path")
        .arg(uds_path)
        .arg("--manifest")
        .arg(manifest_path.to_str().unwrap())
        .env_remove("AOS_MODEL_CACHE_MAX_MB") // Ensure it's not set
        .env("AOS_DEV_NO_AUTH", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    // Wait for process to exit
    let status = wait_with_timeout(&mut child, Duration::from_secs(30))?;
    let elapsed = start.elapsed();

    // Read stderr to verify error message
    let stderr = child.stderr.take();
    let stderr_output = if let Some(stderr) = stderr {
        let mut buf = String::new();
        std::io::Read::read_to_string(&mut stderr, &mut buf).ok();
        buf
    } else {
        String::new()
    };

    // Cleanup
    let _ = fs::remove_file(uds_path);

    // Verify exit code is 1 (EXIT_CONFIG_ERROR)
    assert!(
        !status.success(),
        "Worker should fail when cache budget not configured"
    );
    assert_eq!(
        status.code(),
        Some(1),
        "Worker should exit with code 1 (EXIT_CONFIG_ERROR)"
    );

    // Verify the worker failed FAST (within a few seconds, not after expensive operations)
    // The comment in aos_worker.rs says this should save "100-200ms of wasted work"
    // Let's be generous and say it should fail within 10 seconds
    assert!(
        elapsed < Duration::from_secs(10),
        "Worker should fail fast, but took {:?}",
        elapsed
    );

    // Verify error message mentions cache budget
    assert!(
        stderr_output.contains("Model cache budget not configured")
            || stderr_output.contains("AOS_MODEL_CACHE_MAX_MB"),
        "Error output should mention cache budget configuration: {}",
        stderr_output
    );

    Ok(())
}
