//! Tests for verify-receipt CLI command
//!
//! These tests verify the offline receipt verification functionality including:
//! - Valid receipt verification
//! - Tampered receipt detection
//! - Output format handling (JSON vs human-readable)
//! - Error handling for missing/invalid files
//! - Signature verification

#![allow(unused_variables)]
#![allow(clippy::or_fun_call)]
#![allow(clippy::ptr_arg)]
#![allow(clippy::unwrap_or_default)]

use adapteros_core::B3Hash;
use adapteros_crypto::signature::Keypair;
use anyhow::Result;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

fn new_test_tempdir() -> TempDir {
    TempDir::with_prefix("aos-test-").expect("create temp dir")
}

fn run_aosctl(args: &[&str]) -> std::process::Output {
    Command::new("cargo")
        .args(["run", "-p", "adapteros-cli", "--bin", "aosctl", "--"])
        .args(args)
        .env("CARGO_INCREMENTAL", "0")
        .env("CARGO_TERM_PROGRESS_WHEN", "never")
        .output()
        .expect("Failed to execute aosctl")
}

// Re-export types from verify_receipt module for testing
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReceiptBundle {
    #[serde(default)]
    version: Option<String>,
    trace_id: String,
    tenant_id: String,
    #[serde(default)]
    request_id: Option<String>,
    #[serde(default)]
    context_digest_hex: Option<String>,
    context: ReceiptContext,
    tokens: Vec<ReceiptToken>,
    output_tokens: Vec<u32>,
    receipt: ReceiptDigests,
    #[serde(default)]
    expected_backend: Option<String>,
    #[serde(default)]
    expected_kernel_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReceiptContext {
    tenant_namespace: String,
    stack_hash_hex: String,
    prompt_tokens: Vec<u32>,
    #[serde(default)]
    policy_mask_digest_hex: Option<String>,
    #[serde(default)]
    context_digest_hex: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReceiptToken {
    token_index: u32,
    adapter_ids: Vec<String>,
    gates_q15: Vec<i16>,
    #[serde(default)]
    policy_mask_digest_hex: Option<String>,
    #[serde(default)]
    backend_id: Option<String>,
    #[serde(default)]
    kernel_version_id: Option<String>,
    #[serde(default)]
    decision_hash_hex: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReceiptDigests {
    run_head_hash_hex: String,
    output_digest_hex: String,
    receipt_digest_hex: String,
    #[serde(default)]
    signature_b64: Option<String>,
    #[serde(default)]
    public_key_hex: Option<String>,
    #[serde(default)]
    logical_prompt_tokens: u32,
    #[serde(default)]
    prefix_cached_token_count: u32,
    #[serde(default)]
    billed_input_tokens: u32,
    #[serde(default)]
    logical_output_tokens: u32,
    #[serde(default)]
    billed_output_tokens: u32,
}

// Helper functions to create valid receipt bundles
fn encode_adapter_ids(ids: &[String]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + ids.iter().map(|s| s.len() + 4).sum::<usize>());
    out.extend_from_slice(&(ids.len() as u32).to_le_bytes());
    for id in ids {
        let bytes = id.as_bytes();
        out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(bytes);
    }
    out
}

fn encode_gates_q15(gates: &[i16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + gates.len() * 2);
    out.extend_from_slice(&(gates.len() as u32).to_le_bytes());
    for g in gates {
        out.extend_from_slice(&g.to_le_bytes());
    }
    out
}

fn hash_decision(
    context_digest: &[u8; 32],
    token_index: u32,
    adapter_blob: &[u8],
    gates_blob: &[u8],
    policy_mask_digest: Option<[u8; 32]>,
    backend_id: Option<&str>,
    kernel_version_id: Option<&str>,
) -> B3Hash {
    let policy_bytes = policy_mask_digest
        .map(|d| d.to_vec())
        .unwrap_or_else(Vec::new);
    let backend_bytes = backend_id.unwrap_or("").as_bytes().to_vec();
    let kernel_bytes = kernel_version_id.unwrap_or("").as_bytes().to_vec();

    B3Hash::hash_multi(&[
        &context_digest[..],
        &token_index.to_le_bytes(),
        &(adapter_blob.len() as u32).to_le_bytes(),
        adapter_blob,
        &(gates_blob.len() as u32).to_le_bytes(),
        gates_blob,
        &(policy_bytes.len() as u32).to_le_bytes(),
        &policy_bytes,
        &(backend_bytes.len() as u32).to_le_bytes(),
        &backend_bytes,
        &(kernel_bytes.len() as u32).to_le_bytes(),
        &kernel_bytes,
    ])
}

fn update_head(prev: &B3Hash, token_index: u32, decision_hash: &B3Hash) -> B3Hash {
    B3Hash::hash_multi(&[
        prev.as_bytes(),
        decision_hash.as_bytes(),
        &token_index.to_le_bytes(),
    ])
}

fn compute_output_digest(output_tokens: &[u32]) -> B3Hash {
    let mut buf = Vec::with_capacity(4 + output_tokens.len() * 4);
    buf.extend_from_slice(&(output_tokens.len() as u32).to_le_bytes());
    for t in output_tokens {
        buf.extend_from_slice(&t.to_le_bytes());
    }
    B3Hash::hash(&buf)
}

fn compute_context_digest(ctx: &ReceiptContext) -> Result<B3Hash> {
    let stack_bytes = hex::decode(&ctx.stack_hash_hex)?;
    let mut buf = Vec::with_capacity(
        ctx.tenant_namespace.len() + stack_bytes.len() + 4 + (ctx.prompt_tokens.len() * 4),
    );
    buf.extend_from_slice(ctx.tenant_namespace.as_bytes());
    buf.extend_from_slice(&stack_bytes);
    buf.extend_from_slice(&(ctx.prompt_tokens.len() as u32).to_le_bytes());
    for t in &ctx.prompt_tokens {
        buf.extend_from_slice(&t.to_le_bytes());
    }
    Ok(B3Hash::hash(&buf))
}

fn make_keypair() -> Keypair {
    let mut seed = [0u8; 32];
    use rand::RngCore;
    rand::thread_rng().fill_bytes(&mut seed);
    Keypair::from_bytes(&seed)
}

/// Create a valid receipt bundle with proper digests and signatures
fn create_valid_bundle(dir: &PathBuf) -> PathBuf {
    let prompt_tokens = vec![11u32, 22u32, 33u32];
    let stack_hash = B3Hash::hash(b"stack-xyz").to_hex();
    let context = ReceiptContext {
        tenant_namespace: "test-tenant".to_string(),
        stack_hash_hex: stack_hash,
        prompt_tokens: prompt_tokens.clone(),
        policy_mask_digest_hex: None,
        context_digest_hex: None,
    };

    let context_digest = compute_context_digest(&context).expect("context digest");

    let token0 = ReceiptToken {
        token_index: 0,
        adapter_ids: vec!["adapter-alpha".to_string()],
        gates_q15: vec![100],
        policy_mask_digest_hex: None,
        backend_id: Some("coreml".to_string()),
        kernel_version_id: Some("v1".to_string()),
        decision_hash_hex: None,
    };

    let token1 = ReceiptToken {
        token_index: 1,
        adapter_ids: vec!["adapter-beta".to_string(), "adapter-gamma".to_string()],
        gates_q15: vec![200, 150],
        policy_mask_digest_hex: None,
        backend_id: Some("coreml".to_string()),
        kernel_version_id: Some("v1".to_string()),
        decision_hash_hex: None,
    };

    let mut tokens = vec![token0, token1];
    let mut run_head = B3Hash::zero();

    for t in tokens.iter_mut() {
        let adapter_blob = encode_adapter_ids(&t.adapter_ids);
        let gates_blob = encode_gates_q15(&t.gates_q15);
        let decision = hash_decision(
            context_digest.as_bytes(),
            t.token_index,
            &adapter_blob,
            &gates_blob,
            None,
            t.backend_id.as_deref(),
            t.kernel_version_id.as_deref(),
        );
        t.decision_hash_hex = Some(decision.to_hex());
        run_head = update_head(&run_head, t.token_index, &decision);
    }

    let output_tokens = vec![201u32, 202u32, 203u32];
    let output_digest = compute_output_digest(&output_tokens);
    let logical_prompt_tokens = prompt_tokens.len() as u32;
    let prefix_cached_token_count = 0;
    let billed_input_tokens = logical_prompt_tokens.saturating_sub(prefix_cached_token_count);
    let logical_output_tokens = output_tokens.len() as u32;
    let billed_output_tokens = logical_output_tokens;

    let receipt_digest = B3Hash::hash_multi(&[
        context_digest.as_bytes(),
        run_head.as_bytes(),
        output_digest.as_bytes(),
        &logical_prompt_tokens.to_le_bytes(),
        &prefix_cached_token_count.to_le_bytes(),
        &billed_input_tokens.to_le_bytes(),
        &logical_output_tokens.to_le_bytes(),
        &billed_output_tokens.to_le_bytes(),
    ]);

    let keypair = make_keypair();
    let signature = keypair.sign(receipt_digest.as_bytes());
    let signature_b64 = STANDARD.encode(signature.to_bytes());
    let public_key_hex = hex::encode(keypair.public_key().to_bytes());

    let bundle = ReceiptBundle {
        version: Some("aos-receipt-v1".to_string()),
        trace_id: "trace-test-001".to_string(),
        tenant_id: "test-tenant".to_string(),
        request_id: Some("req-test-001".to_string()),
        context_digest_hex: Some(context_digest.to_hex()),
        context,
        tokens,
        output_tokens,
        receipt: ReceiptDigests {
            run_head_hash_hex: run_head.to_hex(),
            output_digest_hex: output_digest.to_hex(),
            receipt_digest_hex: receipt_digest.to_hex(),
            signature_b64: Some(signature_b64),
            public_key_hex: Some(public_key_hex),
            logical_prompt_tokens,
            prefix_cached_token_count,
            billed_input_tokens,
            logical_output_tokens,
            billed_output_tokens,
        },
        expected_backend: Some("coreml".to_string()),
        expected_kernel_version: Some("v1".to_string()),
    };

    let bundle_path = dir.join("receipt_bundle.json");
    let json = serde_json::to_string_pretty(&bundle).expect("serialize bundle");
    fs::write(&bundle_path, json).expect("write bundle");
    bundle_path
}

#[test]
fn test_valid_receipt_verification() {
    let temp_dir = new_test_tempdir();
    let bundle_path = create_valid_bundle(&temp_dir.path().to_path_buf());

    // Run verification using the command module
    let output = run_aosctl(&["verify-receipt", "--bundle", bundle_path.to_str().unwrap()]);

    assert!(
        output.status.success(),
        "Verification should succeed for valid receipt. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Verification passed") || stdout.contains("match"),
        "Output should indicate success"
    );
}

#[test]
fn test_tampered_receipt_detection() {
    let temp_dir = new_test_tempdir();
    let bundle_path = create_valid_bundle(&temp_dir.path().to_path_buf());

    // Read and tamper with the bundle
    let bundle_json = fs::read_to_string(&bundle_path).expect("read bundle");
    let mut bundle: ReceiptBundle = serde_json::from_str(&bundle_json).expect("parse bundle");

    // Tamper: flip a gate value
    bundle.tokens[0].gates_q15[0] = 999;

    // Write tampered bundle
    let tampered_json = serde_json::to_string_pretty(&bundle).expect("serialize tampered");
    fs::write(&bundle_path, tampered_json).expect("write tampered bundle");

    // Run verification
    let output = run_aosctl(&["verify-receipt", "--bundle", bundle_path.to_str().unwrap()]);

    assert!(
        !output.status.success(),
        "Verification should fail for tampered receipt"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("TRACE_TAMPER") || stderr.contains("mismatch"),
        "Error should indicate tampering: {}",
        stderr
    );
}

#[test]
fn test_adapter_id_history_tamper_is_rejected() {
    let temp_dir = new_test_tempdir();
    let bundle_path = create_valid_bundle(&temp_dir.path().to_path_buf());

    let bundle_json = fs::read_to_string(&bundle_path).expect("read bundle");
    let mut bundle: ReceiptBundle = serde_json::from_str(&bundle_json).expect("parse bundle");

    let mut adapter_bytes = bundle.tokens[0].adapter_ids[0].as_bytes().to_vec();
    // Flip one byte of the adapter ID to simulate corruption.
    adapter_bytes[0] = adapter_bytes[0].wrapping_add(1);
    bundle.tokens[0].adapter_ids[0] =
        String::from_utf8(adapter_bytes).expect("adapter id remains utf8 after flip");

    let tampered_json = serde_json::to_string_pretty(&bundle).expect("serialize tampered");
    fs::write(&bundle_path, tampered_json).expect("write tampered bundle");

    let output = run_aosctl(&["verify-receipt", "--bundle", bundle_path.to_str().unwrap()]);

    assert!(
        !output.status.success(),
        "Verification should fail when adapter history is corrupted"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("TRACE_TAMPER")
            || stderr.contains("mismatch")
            || stderr.contains("adapter"),
        "Error should indicate adapter history mismatch: {}",
        stderr
    );
}

#[test]
fn test_json_output_format() {
    let temp_dir = new_test_tempdir();
    let bundle_path = create_valid_bundle(&temp_dir.path().to_path_buf());

    // Run verification with --json flag
    let output = run_aosctl(&[
        "--json",
        "verify-receipt",
        "--bundle",
        bundle_path.to_str().unwrap(),
    ]);

    assert!(
        output.status.success(),
        "Verification should succeed. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Output should be valid JSON
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("Output should be valid JSON");

    // Check for expected fields in JSON output
    assert!(
        parsed.get("trace_id").is_some(),
        "JSON should contain trace_id"
    );
    assert!(
        parsed.get("reasons").is_some(),
        "JSON should contain reasons"
    );
    assert!(
        parsed.get("context_digest").is_some(),
        "JSON should contain context_digest"
    );
}

#[test]
fn test_missing_file_error() {
    // Try to verify a non-existent file
    let output = run_aosctl(&["verify-receipt", "--bundle", "var/nonexistent-receipt.json"]);

    assert!(
        !output.status.success(),
        "Verification should fail for missing file"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Bundle not found") || stderr.contains("Failed to read"),
        "Error should indicate missing file: {}",
        stderr
    );
}

#[test]
fn test_directory_with_default_bundle_filename() {
    let temp_dir = TempDir::with_prefix("aos-test-").expect("create temp dir");
    let bundle_path = create_valid_bundle(&temp_dir.path().to_path_buf());

    // The bundle was created at temp_dir/receipt_bundle.json
    // Now verify by passing just the directory
    let output = run_aosctl(&[
        "verify-receipt",
        "--bundle",
        temp_dir.path().to_str().unwrap(),
    ]);

    assert!(
        output.status.success(),
        "Verification should work with directory path. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_invalid_json_error() {
    let temp_dir = new_test_tempdir();
    let bundle_path = temp_dir.path().join("invalid.json");

    // Write invalid JSON
    fs::write(&bundle_path, "{ this is not valid json }").expect("write invalid json");

    let output = run_aosctl(&["verify-receipt", "--bundle", bundle_path.to_str().unwrap()]);

    assert!(
        !output.status.success(),
        "Verification should fail for invalid JSON"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Failed to parse") || stderr.contains("JSON"),
        "Error should indicate JSON parsing failure: {}",
        stderr
    );
}

#[test]
fn test_backend_mismatch_detection() {
    let temp_dir = new_test_tempdir();
    let bundle_path = create_valid_bundle(&temp_dir.path().to_path_buf());

    // Read and modify backend expectation
    let bundle_json = fs::read_to_string(&bundle_path).expect("read bundle");
    let mut bundle: ReceiptBundle = serde_json::from_str(&bundle_json).expect("parse bundle");

    // Change expected backend to trigger mismatch
    bundle.expected_backend = Some("metal".to_string());

    // Write modified bundle
    let modified_json = serde_json::to_string_pretty(&bundle).expect("serialize modified");
    fs::write(&bundle_path, modified_json).expect("write modified bundle");

    // Run verification
    let output = run_aosctl(&["verify-receipt", "--bundle", bundle_path.to_str().unwrap()]);

    assert!(
        !output.status.success(),
        "Verification should fail for backend mismatch"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("BACKEND_MISMATCH"),
        "Error should indicate backend mismatch: {}",
        stderr
    );
}

#[test]
fn test_output_digest_mismatch() {
    let temp_dir = new_test_tempdir();
    let bundle_path = create_valid_bundle(&temp_dir.path().to_path_buf());

    // Read and tamper with output tokens
    let bundle_json = fs::read_to_string(&bundle_path).expect("read bundle");
    let mut bundle: ReceiptBundle = serde_json::from_str(&bundle_json).expect("parse bundle");

    // Tamper: change output tokens
    bundle.output_tokens.push(999);

    // Write tampered bundle
    let tampered_json = serde_json::to_string_pretty(&bundle).expect("serialize tampered");
    fs::write(&bundle_path, tampered_json).expect("write tampered bundle");

    // Run verification
    let output = run_aosctl(&["verify-receipt", "--bundle", bundle_path.to_str().unwrap()]);

    assert!(
        !output.status.success(),
        "Verification should fail for output mismatch"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("OUTPUT_MISMATCH") || stderr.contains("TRACE_TAMPER"),
        "Error should indicate output mismatch: {}",
        stderr
    );
}

#[test]
fn test_context_digest_mismatch() {
    let temp_dir = new_test_tempdir();
    let bundle_path = create_valid_bundle(&temp_dir.path().to_path_buf());

    // Read and tamper with context
    let bundle_json = fs::read_to_string(&bundle_path).expect("read bundle");
    let mut bundle: ReceiptBundle = serde_json::from_str(&bundle_json).expect("parse bundle");

    // Tamper: change tenant namespace
    bundle.context.tenant_namespace = "different-tenant".to_string();

    // Write tampered bundle
    let tampered_json = serde_json::to_string_pretty(&bundle).expect("serialize tampered");
    fs::write(&bundle_path, tampered_json).expect("write tampered bundle");

    // Run verification
    let output = run_aosctl(&["verify-receipt", "--bundle", bundle_path.to_str().unwrap()]);

    assert!(
        !output.status.success(),
        "Verification should fail for context mismatch"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("CONTEXT_MISMATCH"),
        "Error should indicate context mismatch: {}",
        stderr
    );
}

#[test]
fn test_verify_receipt_help() {
    let output = run_aosctl(&["verify-receipt", "--help"]);

    assert!(output.status.success(), "Help should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("verify-receipt") || stdout.contains("Verify"),
        "Help should mention verify-receipt"
    );
    assert!(
        stdout.contains("bundle"),
        "Help should mention bundle parameter"
    );
}
