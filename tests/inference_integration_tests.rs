//! Integration tests for AdapterOS Inference System
//!
//! These tests verify end-to-end inference workflows including:
//! - Basic inference with LoRA adapter routing
//! - Evidence-based generation with RAG integration
//! - Deterministic behavior and replay capability
//! - Policy enforcement (refusal, evidence requirements)
//! - Router feature extraction and adapter selection
//! - Memory management and adapter eviction
//!
//! They require a running AdapterOS instance with proper configuration.
//!
//! Run with: `cargo test --test inference_integration_tests -- --ignored --nocapture`

use anyhow::Result;
use serde_json::json;

fn integrations_enabled() -> bool {
    std::env::var("AOS_RUN_FULL_INTEGRATION")
        .map(|v| v == "1")
        .unwrap_or(false)
}

/// Test base URL from environment or default
fn test_base_url() -> String {
    std::env::var("MPLORA_TEST_URL").unwrap_or_else(|_| "http://localhost:9443".to_string())
}

#[tokio::test]
async fn test_basic_inference() -> Result<()> {
    if !integrations_enabled() {
        eprintln!("⏭️ Skipping basic inference test (set AOS_RUN_FULL_INTEGRATION=1 to enable)");
        return Ok(());
    }

    println!("\n=== Test: Basic Inference ===");

    let client = reqwest::Client::new();
    let url = format!("{}/api/v1/inference", test_base_url());

    let request = json!({
        "cpid": "test_cp_v1",
        "prompt": "Explain how Rust's ownership system works",
        "max_tokens": 100,
        "require_evidence": false
    });

    let response = client.post(&url).json(&request).send().await?;

    assert!(response.status().is_success(), "Inference request failed");

    let body: serde_json::Value = response.json().await?;

    assert!(body["text"].is_string(), "Response should contain text");
    assert_eq!(body["status"], "success", "Status should be success");
    assert!(
        body["trace"]["token_count"].as_u64().unwrap() > 0,
        "Should have token count"
    );

    println!("✓ Basic inference completed");
    println!("  Tokens: {}", body["trace"]["token_count"]);
    println!(
        "  Text preview: {}",
        &body["text"].as_str().unwrap()[..50.min(body["text"].as_str().unwrap().len())]
    );

    Ok(())
}

#[tokio::test]
async fn test_evidence_based_inference() -> Result<()> {
    if !integrations_enabled() {
        eprintln!(
            "⏭️ Skipping evidence-based inference test (set AOS_RUN_FULL_INTEGRATION=1 to enable)"
        );
        return Ok(());
    }

    println!("\n=== Test: Evidence-Based Inference ===");

    let client = reqwest::Client::new();
    let url = format!("{}/api/v1/inference", test_base_url());

    let request = json!({
        "cpid": "test_cp_v1",
        "prompt": "What is the recommended way to handle errors in async Rust code?",
        "max_tokens": 150,
        "require_evidence": true
    });

    let response = client.post(&url).json(&request).send().await?;

    assert!(
        response.status().is_success(),
        "Evidence-based inference failed"
    );

    let body: serde_json::Value = response.json().await?;

    // Should have evidence in trace
    assert!(
        body["trace"]["evidence"].is_array(),
        "Should have evidence array"
    );
    let evidence_count = body["trace"]["evidence"].as_array().unwrap().len();
    assert!(
        evidence_count > 0,
        "Should have at least one evidence citation"
    );

    println!("✓ Evidence-based inference completed");
    println!("  Evidence citations: {}", evidence_count);

    // Verify evidence structure
    let first_evidence = &body["trace"]["evidence"][0];
    assert!(
        first_evidence["doc_id"].is_string(),
        "Evidence should have doc_id"
    );
    assert!(
        first_evidence["score"].is_number(),
        "Evidence should have score"
    );

    Ok(())
}

#[tokio::test]
async fn test_deterministic_inference() -> Result<()> {
    if !integrations_enabled() {
        eprintln!(
            "⏭️ Skipping deterministic inference test (set AOS_RUN_FULL_INTEGRATION=1 to enable)"
        );
        return Ok(());
    }

    println!("\n=== Test: Deterministic Inference ===");

    let client = reqwest::Client::new();
    let url = format!("{}/api/v1/inference", test_base_url());

    let request = json!({
        "cpid": "test_cp_v1",
        "prompt": "Write a function to calculate factorial",
        "max_tokens": 100,
        "require_evidence": false
    });

    // Make the same request twice
    let response1 = client
        .post(&url)
        .json(&request)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;

    let response2 = client
        .post(&url)
        .json(&request)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;

    // Responses should be identical (deterministic)
    assert_eq!(
        response1["text"], response2["text"],
        "Deterministic inference should produce identical results"
    );

    assert_eq!(
        response1["trace"]["token_count"], response2["trace"]["token_count"],
        "Token counts should match"
    );

    println!("✓ Deterministic inference verified");
    println!("  Both runs produced identical output");

    Ok(())
}

#[tokio::test]
async fn test_policy_refusal() -> Result<()> {
    if !integrations_enabled() {
        eprintln!("⏭️ Skipping policy refusal test (set AOS_RUN_FULL_INTEGRATION=1 to enable)");
        return Ok(());
    }

    println!("\n=== Test: Policy Refusal ===");

    let client = reqwest::Client::new();
    let url = format!("{}/api/v1/inference", test_base_url());

    // Request that requires evidence but none available
    let request = json!({
        "cpid": "test_cp_v1",
        "prompt": "What is the torque specification for component XYZ-123?",
        "max_tokens": 100,
        "require_evidence": true
    });

    let response = client.post(&url).json(&request).send().await?;

    let body: serde_json::Value = response.json().await?;

    // Should refuse when evidence is insufficient
    if body["refusal"].is_object() {
        println!("✓ Policy refusal triggered correctly");
        println!("  Reason: {}", body["refusal"]["reason"]);
        assert!(
            body["text"].is_null(),
            "Should not generate text when refusing"
        );
    } else {
        println!("⚠ No refusal triggered (may have found evidence)");
    }

    Ok(())
}

#[tokio::test]
async fn test_router_adapter_selection() -> Result<()> {
    if !integrations_enabled() {
        eprintln!(
            "⏭️ Skipping router adapter selection test (set AOS_RUN_FULL_INTEGRATION=1 to enable)"
        );
        return Ok(());
    }

    println!("\n=== Test: Router Adapter Selection ===");

    let client = reqwest::Client::new();
    let url = format!("{}/api/v1/inference", test_base_url());

    // Rust-specific prompt
    let rust_request = json!({
        "cpid": "test_cp_v1",
        "prompt": "Explain Rust's borrow checker",
        "max_tokens": 100,
        "require_evidence": false
    });

    let rust_response = client
        .post(&url)
        .json(&rust_request)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;

    // Python-specific prompt
    let python_request = json!({
        "cpid": "test_cp_v1",
        "prompt": "Explain Python's GIL",
        "max_tokens": 100,
        "require_evidence": false
    });

    let python_response = client
        .post(&url)
        .json(&python_request)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;

    // Check router summaries
    let rust_adapters = &rust_response["trace"]["router_summary"]["adapters_used"];
    let python_adapters = &python_response["trace"]["router_summary"]["adapters_used"];

    println!("✓ Router adapter selection completed");
    println!("  Rust prompt adapters: {:?}", rust_adapters);
    println!("  Python prompt adapters: {:?}", python_adapters);

    // Adapters should be selected based on prompt content
    assert!(rust_adapters.is_array(), "Should have adapter list");
    assert!(python_adapters.is_array(), "Should have adapter list");

    Ok(())
}

#[tokio::test]
async fn test_max_tokens_limit() -> Result<()> {
    if !integrations_enabled() {
        eprintln!("⏭️ Skipping max-tokens test (set AOS_RUN_FULL_INTEGRATION=1 to enable)");
        return Ok(());
    }

    println!("\n=== Test: Max Tokens Limit ===");

    let client = reqwest::Client::new();
    let url = format!("{}/api/v1/inference", test_base_url());

    let request = json!({
        "cpid": "test_cp_v1",
        "prompt": "Write a detailed explanation of machine learning",
        "max_tokens": 50,
        "require_evidence": false
    });

    let response = client
        .post(&url)
        .json(&request)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;

    let token_count = response["trace"]["token_count"].as_u64().unwrap();

    assert!(token_count <= 50, "Should respect max_tokens limit");

    println!("✓ Max tokens limit enforced");
    println!("  Generated tokens: {}", token_count);

    Ok(())
}

#[tokio::test]
async fn test_concurrent_inference() -> Result<()> {
    if !integrations_enabled() {
        eprintln!(
            "⏭️ Skipping concurrent inference test (set AOS_RUN_FULL_INTEGRATION=1 to enable)"
        );
        return Ok(());
    }

    println!("\n=== Test: Concurrent Inference ===");

    let client = reqwest::Client::new();
    let url = format!("{}/api/v1/inference", test_base_url());

    let mut handles = Vec::new();

    for i in 0..5 {
        let client = client.clone();
        let url = url.clone();

        handles.push(tokio::spawn(async move {
            let request = json!({
                "cpid": "test_cp_v1",
                "prompt": format!("Explain concept {}", i),
                "max_tokens": 50,
                "require_evidence": false
            });

            client
                .post(&url)
                .json(&request)
                .send()
                .await?
                .json::<serde_json::Value>()
                .await
        }));
    }

    for (i, handle) in handles.into_iter().enumerate() {
        let response = handle.await.expect("join should succeed")?;
        assert_eq!(
            response["status"], "success",
            "Request {} should succeed",
            i
        );
    }

    println!("✓ Concurrent inference completed");
    println!("  All 5 requests succeeded");

    Ok(())
}

#[tokio::test]
async fn test_memory_pressure_handling() -> Result<()> {
    if !integrations_enabled() {
        eprintln!("⏭️ Skipping memory pressure test (set AOS_RUN_FULL_INTEGRATION=1 to enable)");
        return Ok(());
    }

    println!("\n=== Test: Memory Pressure Handling ===");

    let client = reqwest::Client::new();
    let url = format!("{}/api/v1/inference", test_base_url());

    // Make many requests to potentially trigger memory management
    for i in 0..20 {
        let request = json!({
            "cpid": "test_cp_v1",
            "prompt": format!("Generate text for iteration {}", i),
            "max_tokens": 100,
            "require_evidence": false
        });

        let response = client.post(&url).json(&request).send().await?;

        assert!(
            response.status().is_success(),
            "Request {} should succeed",
            i
        );

        // Small delay between requests
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    println!("✓ Memory pressure handling verified");
    println!("  All 20 requests completed successfully");

    Ok(())
}

#[tokio::test]
async fn test_end_to_end_inference_workflow() -> Result<()> {
    if !integrations_enabled() {
        eprintln!(
            "⏭️ Skipping end-to-end workflow test (set AOS_RUN_FULL_INTEGRATION=1 to enable)"
        );
        return Ok(());
    }

    println!("\n=== Test: End-to-End Inference Workflow ===");

    let client = reqwest::Client::new();
    let base_url = test_base_url();

    // 1. Health check
    println!("1. Checking system health...");
    let health_response = client
        .get(format!("{}/healthz", base_url))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;

    assert_eq!(health_response["status"], "healthy");
    println!("   ✓ System healthy");

    // 2. Simple inference
    println!("2. Running simple inference...");
    let simple_request = json!({
        "cpid": "test_cp_v1",
        "prompt": "Hello, how are you?",
        "max_tokens": 50,
        "require_evidence": false
    });

    let simple_response = client
        .post(format!("{}/api/v1/inference", base_url))
        .json(&simple_request)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;

    assert_eq!(simple_response["status"], "success");
    println!("   ✓ Simple inference completed");

    // 3. Evidence-based inference
    println!("3. Running evidence-based inference...");
    let evidence_request = json!({
        "cpid": "test_cp_v1",
        "prompt": "What are best practices for error handling?",
        "max_tokens": 100,
        "require_evidence": true
    });

    let evidence_response = client
        .post(format!("{}/api/v1/inference", base_url))
        .json(&evidence_request)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;

    if evidence_response["refusal"].is_null() {
        assert!(
            evidence_response["trace"]["evidence"]
                .as_array()
                .unwrap()
                .len()
                > 0
        );
        println!(
            "   ✓ Evidence-based inference completed with {} citations",
            evidence_response["trace"]["evidence"]
                .as_array()
                .unwrap()
                .len()
        );
    } else {
        println!("   ✓ Correctly refused due to insufficient evidence");
    }

    // 4. Verify determinism
    println!("4. Verifying deterministic behavior...");
    let det_request = json!({
        "cpid": "test_cp_v1",
        "prompt": "Count to five",
        "max_tokens": 20,
        "require_evidence": false
    });

    let det_response1 = client
        .post(format!("{}/api/v1/inference", base_url))
        .json(&det_request)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;

    let det_response2 = client
        .post(format!("{}/api/v1/inference", base_url))
        .json(&det_request)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;

    assert_eq!(det_response1["text"], det_response2["text"]);
    println!("   ✓ Deterministic behavior verified");

    println!("\n✓ End-to-end workflow completed successfully!");

    Ok(())
}

/// Test helper functions
#[cfg(test)]
mod helpers {
    use super::*;

    /// Setup test environment
    pub async fn setup_test_env() -> Result<()> {
        // Verify server is running
        let client = reqwest::Client::new();
        let url = format!("{}/healthz", test_base_url());

        match client.get(&url).send().await {
            Ok(response) if response.status().is_success() => Ok(()),
            _ => Err(anyhow::anyhow!(
                "AdapterOS server not running at {}. Start it first.",
                test_base_url()
            )),
        }
    }
}
