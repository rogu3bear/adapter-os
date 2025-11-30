#![cfg(all(test, feature = "extended-tests"))]

//! End-to-End Test for .aos Adapter Workflow
//!
//! Tests the complete workflow: train -> package .aos -> deploy -> infer

use std::process::Command;
use tokio::time::{sleep, Duration};
use anyhow::Result;
use tempfile::TempDir;

#[tokio::test]
async fn test_aos_complete_workflow() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let manifest_path = "training/datasets/base/code/adapteros/manifest.json";
    let tokenizer_path = std::env::var("AOS_TOKENIZER_PATH")
        .unwrap_or_else(|_| "var/model-cache/models/qwen2.5-7b-instruct-bf16/tokenizer.json".to_string());
    let output_dir = temp_dir.path().join("adapters");
    
    // 1. Train and package as .aos
    let status = Command::new("cargo")
        .args(&["xtask", "train-base-adapter", "--output-format", "aos", "--output-dir", output_dir.to_str().unwrap(), "--adapter-id", "e2e_test_adapter"])
        .status()
        .await?;
    assert!(status.success());
    
    let aos_path = output_dir.join("e2e_test_adapter.aos");
    assert!(aos_path.exists());
    
    // 2. Verify .aos file
    let output = Command::new("aosctl")
        .args(&["aos", "verify", "--path", aos_path.to_str().unwrap()])
        .output()
        .await?;
    assert!(output.status.success());
    
    // 3. Deploy .aos adapter (assume server running on localhost:8080)
    // Start a test server if needed, but for e2e assume it's running
    let deploy_output = Command::new("aosctl")
        .args(&["aos", "load", "--path", aos_path.to_str().unwrap()])
        .output()
        .await?;
    assert!(deploy_output.status.success());
    
    // Wait for loading
    sleep(Duration::from_secs(2)).await;
    
    // 4. Run inference
    let inference_prompt = "Explain how AdapterOS works in one sentence.";
    let inference_output = Command::new("curl")
        .args(&[
            "-X", "POST", "http://localhost:8080/v1/infer",
            "-H", "Content-Type: application/json",
            "-d", &format!(r#"{{"prompt": "{}", "adapter_id": "e2e_test_adapter", "max_tokens": 50}}"#, inference_prompt),
        ])
        .output()
        .await?;
    assert!(inference_output.status.success());
    let response = String::from_utf8(inference_output.stdout)?;
    assert!(response.contains("\"response\":"));
    
    // 5. Verify response is non-empty
    let response_str = response.clone();
    assert!(!response_str.is_empty());
    
    // Cleanup
    temp_dir.close()?;
    
    Ok(())
}
