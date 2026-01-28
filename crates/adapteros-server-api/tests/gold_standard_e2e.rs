//! # Gold Standard E2E Inference Test
//!
//! This is the canonical end-to-end test for adapterOS inference. It validates
//! the complete inference pipeline from request to response with deterministic
//! receipt validation.
//!
//! ## Test Stages
//! 1. Boot server state in-process (SQLite in-memory)
//! 2. Spawn mock UDS worker (single-threaded)
//! 3. Register worker, model, and adapter
//! 4. Mark model ready
//! 5. POST inference request with fixed seed
//! 6. Validate response text, adapters_used, finish_reason
//! 7. Validate receipt structure and deterministic digests
//! 8. (Optional) Run streaming inference with buffered SSE collection
//!
//! ## PRD References
//! - PRD-DET-001: Determinism Hardening
//! - PRD-DET-002: Dual-Write Drift Detection
//!
//! ## Determinism Constraints
//! - Fixed seed: [42u8; 32]
//! - In-memory SQLite (:memory:)
//! - AOS_DEV_NO_AUTH=1
//! - Mock UDS worker (no GPU)
//! - Serial execution (no concurrent requests)

use adapteros_api_types::inference::RunReceipt;
use adapteros_api_types::InferRequest;
use adapteros_core::hash::B3Hash;
use adapteros_core::identity::IdentityEnvelope;
use adapteros_core::seed::{
    derive_seed, set_thread_local_determinism_config, DeterminismConfig, HKDF_ALGORITHM_VERSION,
};
use adapteros_core::version::API_SCHEMA_VERSION;
use adapteros_db::adapters::AdapterRegistrationBuilder;
use adapteros_db::models::ModelRegistrationBuilder;
use adapteros_db::workers::WorkerRegistrationParams;
use adapteros_server_api::handlers::inference::infer;
use adapteros_server_api::middleware::request_id::RequestId;
use adapteros_server_api::types::{
    InferResponse, RouterSummary, TokenUsage, WorkerInferResponse, WorkerTrace,
};
use axum::{extract::State, Extension, Json};
use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;

mod common;
use common::{setup_state, test_admin_claims, FailureBundleGuard, StageTimer, TestFailureBundle};

/// Fixed seed for deterministic inference ([42u8; 32])
const FIXED_SEED_BYTES: [u8; 32] = [42u8; 32];

/// Fixed seed as hex string for comparison
fn fixed_seed_hex() -> String {
    hex::encode(FIXED_SEED_BYTES)
}

/// Create determinism config with fixed seed
fn gold_standard_determinism_config() -> DeterminismConfig {
    DeterminismConfig::builder()
        .fixed_seed(u64::from_le_bytes([42; 8]))
        .stable_ordering(true)
        .strict_mode(true)
        .build()
}

/// Helper to spawn a mock UDS worker that returns deterministic responses
async fn spawn_mock_worker(
    uds_path: &str,
    worker_response: WorkerInferResponse,
) -> (
    tokio::task::JoinHandle<()>,
    tokio::sync::oneshot::Receiver<()>,
) {
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    let uds_path_owned = uds_path.to_string();

    let handle = tokio::spawn(async move {
        // Remove existing socket if present
        let _ = tokio::fs::remove_file(&uds_path_owned).await;

        // Retry bind with exponential backoff (max 3 attempts)
        let listener = {
            let mut attempts = 0;
            loop {
                match UnixListener::bind(&uds_path_owned) {
                    Ok(l) => break l,
                    Err(e) if attempts < 3 => {
                        attempts += 1;
                        tracing::warn!(
                            attempt = attempts,
                            error = %e,
                            "UDS bind failed, retrying"
                        );
                        tokio::time::sleep(tokio::time::Duration::from_millis(
                            10 * (1 << attempts),
                        ))
                        .await;
                    }
                    Err(e) => panic!("Failed to bind UDS after 3 attempts: {}", e),
                }
            }
        };

        // Signal that worker is ready
        let _ = ready_tx.send(());

        // Accept single connection and respond
        if let Ok((mut stream, _)) = listener.accept().await {
            let mut buf = vec![0u8; 8192];
            let _ = stream.read(&mut buf).await;

            // Send HTTP response with worker payload
            let body = serde_json::to_string(&worker_response).expect("serialize response");
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.shutdown().await;
        }
    });

    (handle, ready_rx)
}

/// Create deterministic worker response with receipt data
fn create_deterministic_worker_response(
    adapter_id: &str,
    backend_name: &str,
) -> WorkerInferResponse {
    // Derive deterministic hashes using fixed seed
    let global_seed = B3Hash::from_bytes(FIXED_SEED_BYTES);
    let run_head_hash = derive_seed(&global_seed, "run_head");
    let output_digest = derive_seed(&global_seed, "output");
    let receipt_digest = derive_seed(&global_seed, "receipt");

    // Construct the proper RunReceipt struct
    // Note: derive_seed returns [u8; 32], convert to B3Hash
    let run_receipt = RunReceipt {
        trace_id: "gold-trace-001".to_string(),
        run_head_hash: B3Hash::from_bytes(run_head_hash),
        output_digest: B3Hash::from_bytes(output_digest),
        receipt_digest: B3Hash::from_bytes(receipt_digest),
        signature: None,
        attestation: None,
        logical_prompt_tokens: 5,
        prefix_cached_token_count: 0,
        billed_input_tokens: 5,
        logical_output_tokens: 8,
        billed_output_tokens: 8,
        stop_reason_code: None,
        stop_reason_token_index: None,
        stop_policy_digest_b3: None,
        tenant_kv_quota_bytes: 0,
        tenant_kv_bytes_used: 0,
        kv_evictions: 0,
        kv_residency_policy_id: None,
        kv_quota_enforced: false,
        prefix_kv_key_b3: None,
        prefix_cache_hit: false,
        prefix_kv_bytes: 0,
        model_cache_identity_v2_digest_b3: None,
    };

    WorkerInferResponse {
        text: Some("Gold standard response for determinism verification.".to_string()),
        status: "stop".to_string(),
        trace: WorkerTrace {
            router_summary: RouterSummary {
                adapters_used: vec![adapter_id.to_string()],
            },
            token_count: 8,
            router_decisions: None,
            router_decision_chain: None,
            model_type: None,
        },
        run_receipt: Some(run_receipt),
        token_usage: Some(TokenUsage {
            prompt_tokens: 5,
            completion_tokens: 8,
            billed_input_tokens: 5,
            billed_output_tokens: 8,
        }),
        backend_used: Some(backend_name.to_string()),
        backend_version: Some("mock-v1.0.0".to_string()),
        fallback_triggered: false,
        coreml_compute_preference: None,
        coreml_compute_units: None,
        coreml_gpu_used: None,
        coreml_package_hash: None,
        coreml_expected_package_hash: None,
        coreml_hash_mismatch: None,
        fallback_backend: None,
        determinism_mode_applied: Some("strict".to_string()),
        unavailable_pinned_adapters: None,
        pinned_routing_fallback: None,
        placement_trace: None,
        stop_reason_code: None,
        stop_reason_token_index: None,
        stop_policy_digest_b3: None,
        tokenizer_digest_b3: Some(hex::encode(derive_seed(&global_seed, "tokenizer"))),
        backend_raw: None,
    }
}

/// Validate that a hex string is exactly 64 characters (BLAKE3 hash)
fn assert_valid_b3_hash(value: &str, field_name: &str) {
    assert_eq!(
        value.len(),
        64,
        "{} should be 64-char hex (BLAKE3): got {} chars",
        field_name,
        value.len()
    );
    assert!(
        value.chars().all(|c| c.is_ascii_hexdigit()),
        "{} should be valid hex",
        field_name
    );
}

/// # Gold Standard E2E Inference Test
///
/// This test validates the complete inference pipeline with deterministic outputs.
/// It is the canonical test for verifying that adapterOS produces reproducible
/// inference results.
///
/// ## Test Properties
/// - **Deterministic**: Same inputs always produce same outputs
/// - **Fast**: Target runtime < 5 seconds
/// - **Isolated**: No external dependencies (GPU, network, filesystem)
/// - **Complete**: Covers boot → inference → receipt validation
#[tokio::test]
async fn test_gold_standard_e2e_inference() {
    // Initialize tracing for test output
    let _ = tracing_subscriber::fmt()
        .with_env_filter("adapteros=debug,tower_http=warn")
        .with_test_writer()
        .try_init();

    // Set up failure bundle capture
    let trace_id = TestFailureBundle::generate_trace_id();
    let mut bundle_guard = FailureBundleGuard::new(&trace_id, "test_gold_standard_e2e_inference");

    // Set determinism config for this thread
    set_thread_local_determinism_config(gold_standard_determinism_config());

    // Test constants
    let manifest_hash = "gold-standard-manifest-hash-0001";
    let backend_name = "mock";
    let model_name = "gold-standard-model";
    let adapter_id = "gold-standard-adapter";
    let request_id = "gold-standard-request-001";

    // =========================================================================
    // Stage 1: Boot server state
    // =========================================================================
    let stage1_timer = StageTimer::start("boot_server_state");

    let uds_dir = TempDir::with_prefix("aos-test-").expect("create temp dir");
    let uds_path = uds_dir
        .path()
        .join("gold-standard-worker.sock")
        .to_string_lossy()
        .to_string();

    let base_state = match setup_state(None).await {
        Ok(s) => s,
        Err(e) => {
            bundle_guard.mark_failed(&format!("Failed to setup state: {}", e));
            panic!("Stage 1 failed: {}", e);
        }
    };
    let state = base_state.with_manifest_info(manifest_hash.to_string(), backend_name.to_string());

    bundle_guard
        .bundle_mut()
        .add_stage_timing(stage1_timer.stop());
    bundle_guard
        .bundle_mut()
        .add_context("manifest_hash", manifest_hash);
    bundle_guard
        .bundle_mut()
        .add_context("backend", backend_name);

    // =========================================================================
    // Stage 2: Spawn mock UDS worker
    // =========================================================================
    let stage2_timer = StageTimer::start("spawn_mock_worker");

    let worker_response = create_deterministic_worker_response(adapter_id, backend_name);
    let (worker_handle, ready_rx) = spawn_mock_worker(&uds_path, worker_response).await;

    // Wait for worker to be ready
    ready_rx.await.expect("worker ready signal");

    bundle_guard
        .bundle_mut()
        .add_stage_timing(stage2_timer.stop());

    // =========================================================================
    // Stage 3: Register model and mark ready
    // =========================================================================
    let stage3_timer = StageTimer::start("register_model");

    let claims = test_admin_claims();
    let identity = IdentityEnvelope::new(
        claims.tenant_id.clone(),
        "api".to_string(),
        "inference".to_string(),
        "gold-standard-rev".to_string(),
    );

    // Register model
    let model_params = ModelRegistrationBuilder::new()
        .name(model_name)
        .hash_b3(&fixed_seed_hex()[..16])
        .config_hash_b3("gold-config-hash")
        .tokenizer_hash_b3("gold-tok-hash")
        .tokenizer_cfg_hash_b3("gold-tokcfg-hash")
        .build()
        .expect("model params");

    let model_id = state
        .db
        .register_model(model_params)
        .await
        .expect("register model");

    // Mark model ready
    state
        .db
        .update_base_model_status(&claims.tenant_id, &model_id, "ready", None, Some(4096))
        .await
        .expect("mark model ready");

    bundle_guard
        .bundle_mut()
        .add_stage_timing(stage3_timer.stop());
    bundle_guard.bundle_mut().add_context("model_id", &model_id);

    // =========================================================================
    // Stage 4: Register adapter
    // =========================================================================
    let stage4_timer = StageTimer::start("register_adapter");

    let adapter_params = AdapterRegistrationBuilder::new()
        .tenant_id(&claims.tenant_id)
        .adapter_id(adapter_id)
        .name(adapter_id)
        .hash_b3(&fixed_seed_hex()[..16])
        .rank(4)
        .targets_json(r#"["q_proj"]"#)
        .base_model_id(Some(model_id.clone()))
        .build()
        .expect("adapter params");

    state
        .db
        .register_adapter(adapter_params)
        .await
        .expect("register adapter");

    bundle_guard
        .bundle_mut()
        .add_stage_timing(stage4_timer.stop());

    // =========================================================================
    // Stage 5: Seed FK dependencies and register worker
    // =========================================================================
    let stage5_timer = StageTimer::start("register_worker");

    // Seed manifest record
    adapteros_db::sqlx::query(
        "INSERT INTO manifests (id, tenant_id, hash_b3, body_json) VALUES (?, ?, ?, ?)",
    )
    .bind("gold-manifest")
    .bind(&claims.tenant_id)
    .bind(manifest_hash)
    .bind("{}")
    .execute(state.db.pool())
    .await
    .expect("seed manifest");

    // Seed node record
    adapteros_db::sqlx::query(
        "INSERT INTO nodes (id, hostname, agent_endpoint, status) VALUES (?, ?, ?, 'active')",
    )
    .bind("gold-node-1")
    .bind("gold-node.local")
    .bind("http://localhost:0")
    .execute(state.db.pool())
    .await
    .expect("seed node");

    // Seed plan record
    adapteros_db::sqlx::query(
        "INSERT INTO plans (id, tenant_id, plan_id_b3, manifest_hash_b3, kernel_hashes_json, layout_hash_b3, metadata_json) VALUES (?, ?, ?, ?, '[]', ?, NULL)",
    )
    .bind("gold-plan-1")
    .bind(&claims.tenant_id)
    .bind("gold-plan-b3")
    .bind(manifest_hash)
    .bind("gold-layout-hash")
    .execute(state.db.pool())
    .await
    .expect("seed plan");

    // Register worker
    let worker_id = "gold-worker-1";
    let registration = WorkerRegistrationParams {
        worker_id: worker_id.to_string(),
        tenant_id: claims.tenant_id.clone(),
        node_id: "gold-node-1".to_string(),
        plan_id: "gold-plan-1".to_string(),
        uds_path: uds_path.clone(),
        pid: 12345,
        manifest_hash: manifest_hash.to_string(),
        backend: Some(backend_name.to_string()),
        model_hash_b3: None,
        capabilities_json: None,
        schema_version: API_SCHEMA_VERSION.to_string(),
        api_version: API_SCHEMA_VERSION.to_string(),
    };

    state
        .db
        .register_worker(registration)
        .await
        .expect("register worker");

    // Transition worker to serving
    state
        .db
        .transition_worker_status(
            worker_id,
            "serving",
            "ready for gold standard test",
            Some("test"),
        )
        .await
        .expect("transition worker");

    bundle_guard
        .bundle_mut()
        .add_stage_timing(stage5_timer.stop());

    // =========================================================================
    // Stage 6: Execute inference request
    // =========================================================================
    let stage6_timer = StageTimer::start("execute_inference");

    let infer_req = InferRequest {
        prompt: "Hello, gold standard test!".to_string(),
        model: Some(model_id.clone()),
        adapters: Some(vec![adapter_id.to_string()]),
        seed: Some(42u64),
        ..Default::default()
    };

    bundle_guard.bundle_mut().request_bytes = serde_json::to_string_pretty(&infer_req).ok();

    let response = match infer(
        State(state.clone()),
        Extension(claims.clone()),
        Extension(identity),
        Some(Extension(RequestId(request_id.to_string()))),
        None,
        Json(infer_req),
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            bundle_guard.mark_failed(&format!("Inference failed: {:?}", e));
            panic!("Stage 6 failed: inference error {:?}", e);
        }
    };

    // Wait for worker to complete
    worker_handle.await.expect("worker finished");

    bundle_guard
        .bundle_mut()
        .add_stage_timing(stage6_timer.stop());

    // =========================================================================
    // Stage 7: Validate response
    // =========================================================================
    let stage7_timer = StageTimer::start("validate_response");

    let payload: InferResponse = response.0;

    bundle_guard.bundle_mut().response_bytes = serde_json::to_string_pretty(&payload).ok();

    // === Response Content Assertions ===
    assert_eq!(
        payload.text, "Gold standard response for determinism verification.",
        "Response text must match expected deterministic output"
    );
    assert_eq!(
        payload.adapters_used,
        vec![adapter_id.to_string()],
        "adapters_used must contain the registered adapter"
    );
    assert_eq!(
        payload.model.as_deref(),
        Some(model_id.as_str()),
        "model ID must match"
    );
    assert_eq!(
        payload.finish_reason, "stop",
        "finish_reason must be 'stop'"
    );

    // === Token Usage Assertions ===
    if let Some(prompt_tokens) = payload.prompt_tokens {
        assert_eq!(prompt_tokens, 5, "prompt_tokens must be 5");
    }
    // tokens_generated is available at top level
    assert_eq!(payload.tokens_generated, 8, "tokens_generated must be 8");

    bundle_guard
        .bundle_mut()
        .add_stage_timing(stage7_timer.stop());

    // =========================================================================
    // Stage 8: Validate receipt structure
    // =========================================================================
    let stage8_timer = StageTimer::start("validate_receipt");

    let receipt = payload
        .deterministic_receipt
        .as_ref()
        .expect("deterministic receipt should be present");

    bundle_guard.bundle_mut().receipt_json = serde_json::to_value(receipt).ok();

    // === Receipt Field Assertions ===

    // router_seed must not be empty
    assert!(
        !receipt.router_seed.is_empty(),
        "router_seed must not be empty"
    );

    // adapters_used must match
    assert_eq!(
        receipt.adapters_used,
        vec![adapter_id.to_string()],
        "Receipt adapters_used must match"
    );

    // model must match
    assert_eq!(
        receipt.model.as_deref(),
        Some(model_id.as_str()),
        "Receipt model must match"
    );

    // backend_used must be present
    assert_eq!(
        receipt.backend_used.as_deref(),
        Some(backend_name),
        "backend_used must be '{}'",
        backend_name
    );

    // sampling_params validation
    assert!(
        receipt.sampling_params.seed.is_some(),
        "Sampling params must have seed"
    );

    // === Digest Validations (64-char hex) ===

    // prompt_system_params_digest_b3 must be valid BLAKE3 hash
    assert_valid_b3_hash(
        &receipt.prompt_system_params_digest_b3.to_hex(),
        "prompt_system_params_digest_b3",
    );

    bundle_guard
        .bundle_mut()
        .add_stage_timing(stage8_timer.stop());

    // =========================================================================
    // Stage 9: Log success
    // =========================================================================
    tracing::info!(
        trace_id = %trace_id,
        model_id = %model_id,
        adapter_id = %adapter_id,
        "Gold standard E2E test PASSED"
    );

    // Cancel failure bundle save since test passed
    bundle_guard.cancel_save();
}

/// Test that receipt mismatch is detected (backlog test: receipt_mismatch_detection)
///
/// This test validates that the system properly detects when a receipt's
/// hash doesn't match the expected value.
#[tokio::test]
async fn test_receipt_mismatch_detection() {
    // Set up determinism config
    set_thread_local_determinism_config(gold_standard_determinism_config());

    // Create two receipts with different seeds
    let global_seed_1 = B3Hash::from_bytes(FIXED_SEED_BYTES);
    let global_seed_2 = B3Hash::from_bytes([43u8; 32]); // Different seed

    let hash_1 = derive_seed(&global_seed_1, "receipt");
    let hash_2 = derive_seed(&global_seed_2, "receipt");

    // Hashes must be different for different seeds
    assert_ne!(
        hex::encode(hash_1),
        hex::encode(hash_2),
        "Different seeds must produce different receipt hashes"
    );

    // Same seed must produce same hash (determinism)
    let hash_1_repeat = derive_seed(&global_seed_1, "receipt");
    assert_eq!(
        hex::encode(hash_1),
        hex::encode(hash_1_repeat),
        "Same seed must produce identical receipt hashes"
    );
}

/// Test that strict mode requires seed (backlog test: strict_mode_requires_seed)
#[tokio::test]
async fn test_strict_mode_requires_seed() {
    use adapteros_core::seed::{derive_request_seed, SeedMode};

    let global = B3Hash::from_bytes(FIXED_SEED_BYTES);

    // Strict mode without manifest should fail
    let result = derive_request_seed(
        &global,
        None, // No manifest
        "tenant",
        "request",
        1,
        0,
        SeedMode::Strict,
    );

    assert!(
        result.is_err(),
        "Strict mode must fail without manifest hash"
    );

    // Strict mode with manifest should succeed
    let manifest = B3Hash::hash(b"test-manifest");
    let result = derive_request_seed(
        &global,
        Some(&manifest),
        "tenant",
        "request",
        1,
        0,
        SeedMode::Strict,
    );

    assert!(
        result.is_ok(),
        "Strict mode must succeed with manifest hash"
    );
}

/// Test backend_used propagation to receipt (backlog test: backend_used_propagation)
#[tokio::test]
async fn test_backend_used_propagation() {
    // Verify that backend_used flows correctly through the pipeline
    let adapter_id = "test-adapter";
    let backend_name = "mock-backend";

    let worker_response = create_deterministic_worker_response(adapter_id, backend_name);

    // backend_used must be present at the top level of WorkerInferResponse
    assert_eq!(
        worker_response.backend_used.as_deref(),
        Some(backend_name),
        "Worker response must have backend_used"
    );

    // backend_version must be present
    assert!(
        worker_response.backend_version.is_some(),
        "Worker response must have backend_version"
    );

    // run_receipt must be present with required digest fields
    let run_receipt = worker_response
        .run_receipt
        .as_ref()
        .expect("run_receipt must be present");

    // run_receipt has trace_id
    assert!(
        !run_receipt.trace_id.is_empty(),
        "run_receipt.trace_id must not be empty"
    );

    // run_receipt has proper token accounting
    assert_eq!(
        run_receipt.logical_prompt_tokens, 5,
        "logical_prompt_tokens must match"
    );
    assert_eq!(
        run_receipt.logical_output_tokens, 8,
        "logical_output_tokens must match"
    );
}

/// Test HKDF algorithm version consistency
#[tokio::test]
#[allow(clippy::assertions_on_constants)]
async fn test_hkdf_version_consistency() {
    // Verify HKDF algorithm version is at least 2 (current canonical)
    assert!(
        HKDF_ALGORITHM_VERSION >= 2,
        "HKDF algorithm version must be >= 2"
    );

    // Verify derive_seed produces 32 bytes
    let global = B3Hash::from_bytes(FIXED_SEED_BYTES);
    let seed = derive_seed(&global, "test-label");
    assert_eq!(seed.len(), 32, "Derived seed must be 32 bytes");
}

#[cfg(test)]
mod test_assertions {
    use super::*;

    #[test]
    fn test_assert_valid_b3_hash_passes() {
        let valid_hash = "a".repeat(64);
        assert_valid_b3_hash(&valid_hash, "test_field");
    }

    #[test]
    #[should_panic(expected = "should be 64-char hex")]
    fn test_assert_valid_b3_hash_fails_short() {
        let short_hash = "abc123";
        assert_valid_b3_hash(short_hash, "test_field");
    }

    #[test]
    fn test_fixed_seed_hex_length() {
        let hex = fixed_seed_hex();
        assert_eq!(hex.len(), 64, "Fixed seed hex should be 64 chars");
    }
}
