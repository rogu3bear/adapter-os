//! Golden Determinism Tests
//!
//! Validates that identical inference requests with the same seed produce
//! identical outputs and receipt digests.
//!
//! Usage:
//!   cargo test --test golden_determinism_test -- --ignored
//!
//! Requires AOS_TEST_URL environment variable to point to a running server.

use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Golden test case from JSON
#[derive(Debug, Clone, Deserialize)]
struct GoldenCase {
    id: String,
    description: String,
    prompt: String,
    seed: u64,
    max_tokens: usize,
    temperature: f32,
    reasoning_mode: bool,
    expected_contains: Option<String>,
}

/// Golden test suite from JSON
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct GoldenSuite {
    description: String,
    version: String,
    cases: Vec<GoldenCase>,
}

/// Inference request
#[derive(Debug, Serialize)]
struct InferRequest {
    prompt: String,
    seed: u64,
    max_tokens: usize,
    temperature: f32,
    reasoning_mode: bool,
}

/// Inference response (subset of fields we need)
#[derive(Debug, Deserialize)]
struct InferResponse {
    text: Option<String>,
    #[serde(default)]
    tokens: Vec<u32>,
    run_receipt: Option<RunReceipt>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RunReceipt {
    receipt_digest: Option<String>,
    output_digest: Option<String>,
}

/// Run a single inference request
async fn run_inference(base_url: &str, case: &GoldenCase) -> Result<InferResponse, String> {
    let client = reqwest::Client::new();

    let req = InferRequest {
        prompt: case.prompt.clone(),
        seed: case.seed,
        max_tokens: case.max_tokens,
        temperature: case.temperature,
        reasoning_mode: case.reasoning_mode,
    };

    let resp = client
        .post(format!("{}/v1/infer", base_url))
        .json(&req)
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("HTTP {}: {}", status, body));
    }

    resp.json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))
}

/// Compute hash of output for comparison
fn hash_output(text: &str, tokens: &[u32]) -> u64 {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    tokens.hash(&mut hasher);
    hasher.finish()
}

/// Run a golden case twice and compare results
async fn run_golden_case(base_url: &str, case: &GoldenCase) -> Result<(), String> {
    println!("  Running case '{}': {}", case.id, case.description);

    // First run
    let resp1 = run_inference(base_url, case).await?;
    if let Some(ref err) = resp1.error {
        return Err(format!("Run 1 error: {}", err));
    }

    let text1 = resp1.text.clone().unwrap_or_default();
    let tokens1 = resp1.tokens.clone();
    let receipt1 = resp1
        .run_receipt
        .as_ref()
        .and_then(|r| r.receipt_digest.clone());
    let output_digest1 = resp1
        .run_receipt
        .as_ref()
        .and_then(|r| r.output_digest.clone());

    // Second run
    let resp2 = run_inference(base_url, case).await?;
    if let Some(ref err) = resp2.error {
        return Err(format!("Run 2 error: {}", err));
    }

    let text2 = resp2.text.clone().unwrap_or_default();
    let tokens2 = resp2.tokens.clone();
    let receipt2 = resp2
        .run_receipt
        .as_ref()
        .and_then(|r| r.receipt_digest.clone());
    let output_digest2 = resp2
        .run_receipt
        .as_ref()
        .and_then(|r| r.output_digest.clone());

    // Compare output hashes
    let hash1 = hash_output(&text1, &tokens1);
    let hash2 = hash_output(&text2, &tokens2);

    if hash1 != hash2 {
        return Err(format!(
            "Output hash mismatch!\n  Run 1: '{}'\n  Run 2: '{}'\n  Hash 1: {}\n  Hash 2: {}",
            text1, text2, hash1, hash2
        ));
    }

    // Compare receipt digests
    match (receipt1.as_ref(), receipt2.as_ref()) {
        (Some(r1), Some(r2)) if r1 != r2 => {
            return Err(format!(
                "Receipt digest mismatch!\n  Run 1: {}\n  Run 2: {}",
                r1, r2
            ));
        }
        (None, Some(_)) | (Some(_), None) => {
            return Err("Receipt presence mismatch between runs".to_string());
        }
        _ => {}
    }

    // Compare output digests
    match (output_digest1.as_ref(), output_digest2.as_ref()) {
        (Some(d1), Some(d2)) if d1 != d2 => {
            return Err(format!(
                "Output digest mismatch!\n  Run 1: {}\n  Run 2: {}",
                d1, d2
            ));
        }
        _ => {}
    }

    // Check expected content if specified
    if let Some(ref expected) = case.expected_contains {
        let text_lower = text1.to_lowercase();
        let expected_lower = expected.to_lowercase();
        if !text_lower.contains(&expected_lower) {
            return Err(format!(
                "Output does not contain expected '{}'\n  Got: '{}'",
                expected, text1
            ));
        }
    }

    println!("    ✓ Output hash: {}", hash1);
    if let Some(ref r) = receipt1 {
        println!("    ✓ Receipt digest: {}...", &r[..16.min(r.len())]);
    }

    Ok(())
}

/// Load golden suite from JSON file
fn load_golden_suite() -> Result<GoldenSuite, String> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden_runs.json");
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read golden_runs.json: {}", e))?;
    serde_json::from_str(&content).map_err(|e| format!("Failed to parse golden_runs.json: {}", e))
}

#[tokio::test]
#[ignore] // Requires running server
async fn test_golden_determinism() {
    let base_url =
        std::env::var("AOS_TEST_URL").unwrap_or_else(|_| "http://localhost:18080".to_string());

    println!("\n═══════════════════════════════════════════════════════");
    println!(" GOLDEN DETERMINISM TESTS");
    println!("═══════════════════════════════════════════════════════");
    println!(" Server: {}", base_url);
    println!("═══════════════════════════════════════════════════════\n");

    let suite = load_golden_suite().expect("Failed to load golden suite");
    println!("Loaded {} test cases\n", suite.cases.len());

    let mut passed = 0;
    let mut failed = 0;
    let mut errors = Vec::new();

    for case in &suite.cases {
        match run_golden_case(&base_url, case).await {
            Ok(()) => {
                passed += 1;
            }
            Err(e) => {
                failed += 1;
                errors.push((case.id.clone(), e));
                println!("    ✗ FAILED");
            }
        }
    }

    println!("\n───────────────────────────────────────────────────────");
    println!(" Results: {} passed, {} failed", passed, failed);
    println!("───────────────────────────────────────────────────────\n");

    if !errors.is_empty() {
        println!("Failures:");
        for (id, err) in &errors {
            println!("\n  [{}]", id);
            for line in err.lines() {
                println!("    {}", line);
            }
        }
        panic!("Golden determinism tests failed!");
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn test_hash_output_deterministic() {
        let text = "test output";
        let tokens = vec![1, 2, 3, 4];

        let hash1 = hash_output(text, &tokens);
        let hash2 = hash_output(text, &tokens);

        assert_eq!(hash1, hash2, "Hash should be deterministic");
    }

    #[test]
    fn test_hash_output_different() {
        let text1 = "test output 1";
        let text2 = "test output 2";
        let tokens = vec![1, 2, 3, 4];

        let hash1 = hash_output(text1, &tokens);
        let hash2 = hash_output(text2, &tokens);

        assert_ne!(
            hash1, hash2,
            "Different outputs should have different hashes"
        );
    }

    #[test]
    fn test_load_golden_suite() {
        let suite = load_golden_suite();
        assert!(suite.is_ok(), "Should be able to load golden_runs.json");

        let suite = suite.unwrap();
        assert!(!suite.cases.is_empty(), "Suite should have test cases");
    }
}
