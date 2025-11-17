use std::env;
use std::process::Command;

#[tokio::test]
async fn test_infer_backpressure() {
    // Set mock env for high pressure (assume UmaPressureMonitor checks env)
    env::set_var("AOS_MOCK_HIGH_PRESSURE", "1");

    let output = Command::new("cargo")
        .args([
            "run",
            "--",
            "infer",
            "--prompt",
            "test",
            "--no-backpressure",
        ]) // Assume flag to skip for normal test
        .env("AOS_MOCK_HIGH_PRESSURE", "1")
        .output()
        .await
        .expect("Failed to run aosctl");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("System under pressure"));
}

