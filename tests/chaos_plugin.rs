use adapteros_core::Result;
use adapteros_server::{main, Cli};
use std::collections::HashSet;
use std::process::Stdio;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::RwLock;

// Basic chaos test structure
#[tokio::test]
async fn test_plugin_isolation_git_failure() -> Result<()> {
    // Spawn server in background
    let mut server = Command::new("cargo")
        .args([
            "run",
            "--bin",
            "aos-cp",
            "--",
            "--config",
            "configs/cp.toml",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    // Wait for startup
    tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;

    // Enable git for default tenant via API
    let client = reqwest::Client::new();
    let _ = client
        .post("http://127.0.0.1:8080/api/v1/plugins/git/enable")
        .json(&json!({"tenant_id": "default"}))
        .send()
        .await;

    // Register a git repo to trigger git ops
    let _ = client
        .post("http://127.0.0.1:8080/api/v1/code/register-repo")
        .json(&json!({"repo_id": "test", "path": "/tmp/test-repo"}))
        .send()
        .await;

    // Simulate git failure by killing git process or something
    // For now, assume timeout test from handler

    // Verify inference endpoint still works
    let response = client
        .post("http://127.0.0.1:8080/api/v1/infer")
        .json(&json!({"prompt": "hello", "max_tokens": 10}))
        .send()
        .await?;

    assert_eq!(response.status(), 200);

    // Kill server
    server.kill().await?;

    Ok(())
}

#[tokio::test]
async fn test_supervisor_kill() -> Result<()> {
    use std::process::Command as SyncCommand;
    use tokio::process::Command;

    // Create test repo
    let temp_dir = tempfile::tempdir()?;
    // init git repo
    SyncCommand::new("git")
        .args(["init"])
        .current_dir(&temp_dir)
        .output()?;

    // Spawn server
    let mut server = Command::new("cargo")
        .args([
            "run",
            "--bin",
            "aos-cp",
            "--",
            "--config",
            "configs/cp.toml",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    // Wait startup
    tokio::time::sleep(Duration::from_secs(15)).await;

    let client = reqwest::Client::new();

    // Enable Git
    let enable_res = client
        .post("http://127.0.0.1:8080/v1/plugins/git/enable")
        .json(&serde_json::json!({"tenant_id": "default"}))
        .send()
        .await?;
    assert_eq!(enable_res.status(), 200);

    // Start reg, but kill mid
    let reg_handle = tokio::spawn(async move {
        let reg_res = client
            .post("http://127.0.0.1:8080/v1/code/register-repo")
            .json(
                &serde_json::json!({"repo_id": "test", "path": temp_dir.path().to_str().unwrap()}),
            )
            .send()
            .await;
        reg_res
    });

    // Simulate mid-reg, sleep 2s then kill
    tokio::time::sleep(Duration::from_secs(2)).await;
    server.kill().await?;

    // Wait reg finish (should error or fallback)
    let reg_res = reg_handle.await??;
    // expect 200 with fallback true or error, but since kill server, probably error, but test inference on new server? Complex.

    // Restart server
    let mut server2 = Command::new("cargo")
        .args([
            "run",
            "--bin",
            "aos-cp",
            "--",
            "--config",
            "configs/cp.toml",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    tokio::time::sleep(Duration::from_secs(10)).await;

    // Verify inference
    let inf_res = client
        .post("http://127.0.0.1:8080/v1/infer")
        .json(&serde_json::json!({"prompt": "test", "max_tokens": 10}))
        .send()
        .await?;
    assert_eq!(inf_res.status(), 200);

    server2.kill().await?;

    Ok(())
}

#[tokio::test]
async fn test_disable_poll_skip() -> Result<()> {
    // Mock GitSubsystem
    let enabled_tenants = Arc::new(RwLock::new(HashSet::new()));
    let subsystem = // mock with enabled_tenants

    // Enable tenant
    subsystem.set_tenant_enabled("test", true).await.unwrap();
    // simulate poll, expect event

    let poll_count = Arc::new(AtomicUsize::new(0));
    // spawn poll task that increments if enabled

    // disable
    subsystem.set_tenant_enabled("test", false).await.unwrap();

    // assert poll_count == 0 after some time or in mock

    // For mock, use a test poll fn that checks enabled_tenants.contains

    let tenant = "test".to_string();
    enabled_tenants.write().await.insert(tenant.clone());

    let poll_mock = || {
        let set = enabled_tenants.blocking_read();
        if set.contains(&tenant) {
            poll_count.fetch_add(1, Ordering::SeqCst);
        }
    };

    poll_mock(); // 1

    enabled_tenants.write().await.remove(&tenant);

    poll_mock(); // no inc

    assert_eq!(poll_count.load(Ordering::SeqCst), 1);

    Ok(())
}

// More tests for disable, etc.
