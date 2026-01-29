//! Replay and determinism tests for inference_core.

use crate::types::{ReplayContext, SamplingParams};
use adapteros_api_types::inference::ReplayGuarantee;

#[test]
fn test_replay_context_structure() {
    // Verify ReplayContext has all required fields
    let ctx = ReplayContext {
        original_inference_id: "test-123".to_string(),
        required_manifest_hash: "abc123".to_string(),
        required_backend: "mlx".to_string(),
        skip_metadata_capture: true,
        original_policy_id: None,
        original_policy_version: None,
    };
    assert!(ctx.skip_metadata_capture);
    assert_eq!(ctx.original_inference_id, "test-123");
    assert_eq!(ctx.required_manifest_hash, "abc123");
    assert_eq!(ctx.required_backend, "mlx");
}

#[test]
fn test_replay_context_for_normal_inference() {
    // Normal inference should not skip metadata capture
    let ctx = ReplayContext {
        original_inference_id: "original-001".to_string(),
        required_manifest_hash: "manifest-hash".to_string(),
        required_backend: "CoreML".to_string(),
        skip_metadata_capture: false,
        original_policy_id: None,
        original_policy_version: None,
    };
    assert!(!ctx.skip_metadata_capture);
}

#[test]
fn test_sampling_params_serialization_includes_run_envelope() {
    let envelope = adapteros_api_types::RunEnvelope {
        run_id: "run-123".to_string(),
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        workspace_id: "tenant-1".to_string(),
        actor: adapteros_api_types::RunActor {
            subject: "user".to_string(),
            roles: vec!["role".to_string()],
            principal_type: Some("user".to_string()),
            auth_mode: Some("bearer".to_string()),
        },
        manifest_hash_b3: Some("hash".to_string()),
        plan_id: Some("plan".to_string()),
        policy_mask_digest_b3: None,
        router_seed: None,
        tick: Some(1),
        worker_id: None,
        reasoning_mode: false,
        determinism_version: "v1".to_string(),
        boot_trace_id: None,
        created_at: chrono::Utc::now(),
    };

    let params = SamplingParams {
        temperature: 0.0,
        top_k: Some(50),
        top_p: Some(0.9),
        max_tokens: 100,
        seed: Some(42),
        error_code: None,
        seed_mode: None,
        backend_profile: None,
        request_seed_hex: None,
        placement: None,
        run_envelope: Some(envelope),
        adapter_hashes_b3: None,
        dataset_hash_b3: None,
    };
    let json = serde_json::to_string(&params).unwrap();
    assert!(json.contains("\"temperature\":0.0"));
    assert!(json.contains("\"seed\":42"));
    assert!(json.contains("\"top_k\":50"));
    assert!(json.contains("\"top_p\":0.9"));
    assert!(json.contains("\"max_tokens\":100"));
    assert!(json.contains("\"run_id\":\"run-123\""));
    let expected_schema = format!(
        "\"schema_version\":\"{}\"",
        adapteros_api_types::API_SCHEMA_VERSION
    );
    assert!(
        json.contains(&expected_schema),
        "expected run_envelope schema_version in replay sampling params"
    );

    // Verify round-trip
    let parsed: SamplingParams = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.temperature, 0.0);
    assert_eq!(parsed.seed, Some(42));
    assert_eq!(parsed.top_k, Some(50));
    assert_eq!(parsed.top_p, Some(0.9));
    assert_eq!(parsed.max_tokens, 100);
}

#[test]
fn test_sampling_params_default_values() {
    // Test that default values work correctly
    let params = SamplingParams {
        temperature: 1.0,
        top_k: None,
        top_p: None,
        max_tokens: 256,
        seed: None,
        error_code: None,
        seed_mode: None,
        backend_profile: None,
        request_seed_hex: None,
        placement: None,
        run_envelope: None,
        adapter_hashes_b3: None,
        dataset_hash_b3: None,
    };
    let json = serde_json::to_string(&params).unwrap();

    // None values should serialize as null
    let parsed: SamplingParams = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.top_k, None);
    assert_eq!(parsed.top_p, None);
    assert_eq!(parsed.seed, None);
}

#[test]
fn test_sampling_params_greedy_decoding() {
    // Temperature 0 means greedy decoding
    let params = SamplingParams {
        temperature: 0.0,
        top_k: None,
        top_p: None,
        max_tokens: 100,
        seed: Some(0), // Seed still matters for tie-breaking
        error_code: None,
        seed_mode: None,
        backend_profile: None,
        request_seed_hex: None,
        placement: None,
        run_envelope: None,
        adapter_hashes_b3: None,
        dataset_hash_b3: None,
    };
    assert_eq!(params.temperature, 0.0);

    let json = serde_json::to_string(&params).unwrap();
    assert!(json.contains("\"temperature\":0.0"));
}

#[test]
fn test_backend_comparison_case_insensitive() {
    // Backend comparison should be case-insensitive
    let required = "CoreML";
    let current = "coreml";
    assert!(current.eq_ignore_ascii_case(required));

    let required = "MLX";
    let current = "mlx";
    assert!(current.eq_ignore_ascii_case(required));
}

#[test]
fn test_replay_guarantee_variants() {
    // Verify ReplayGuarantee enum values
    assert_eq!(format!("{:?}", ReplayGuarantee::Exact), "Exact");
    assert_eq!(format!("{:?}", ReplayGuarantee::Approximate), "Approximate");
    assert_eq!(format!("{:?}", ReplayGuarantee::None), "None");
}
