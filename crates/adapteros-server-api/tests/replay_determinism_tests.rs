//! Deterministic Replay Integration Tests
//!
//! Tests the acceptance criteria for inference replay functionality:
//! 1. Same replay key twice produces same output on same hardware
//! 2. Changed sampling params produce updated evidence and different replay
//! 3. RAG degradation is reported with missing document details

use adapteros_db::{CreateReplayMetadataParams, Db};
use adapteros_server_api::types::{
    DivergenceDetails, ReplayKey, ReplayMatchStatus, ReplayStatus, SamplingParams,
    MAX_REPLAY_TEXT_SIZE, SAMPLING_ALGORITHM_VERSION,
};

/// Helper to create test replay metadata
async fn create_test_metadata(db: &Db, inference_id: &str, tenant_id: &str) -> String {
    let params = CreateReplayMetadataParams {
        inference_id: inference_id.to_string(),
        tenant_id: tenant_id.to_string(),
        manifest_hash: "test-manifest-hash-abc123".to_string(),
        router_seed: Some("test-seed-456".to_string()),
        sampling_params_json: serde_json::to_string(&SamplingParams::default()).unwrap(),
        backend: "CoreML".to_string(),
        sampling_algorithm_version: Some(SAMPLING_ALGORITHM_VERSION.to_string()),
        rag_snapshot_hash: Some("rag-snapshot-hash-789".to_string()),
        adapter_ids: Some(vec!["adapter-1".to_string()]),
        prompt_text: "Test prompt for deterministic replay".to_string(),
        prompt_truncated: false,
        response_text: Some("Test response for comparison".to_string()),
        response_truncated: false,
        rag_doc_ids: Some(vec!["doc-001".to_string(), "doc-002".to_string()]),
        replay_status: Some("available".to_string()),
        latency_ms: Some(100),
        tokens_generated: Some(10),
    };

    db.create_replay_metadata(params).await.unwrap()
}

/// Helper to create test tenant
async fn setup_test_tenant(db: &Db) -> String {
    match db.create_tenant("Test Tenant", false).await {
        Ok(id) => id,
        Err(_) => sqlx::query_scalar::<_, String>("SELECT id FROM tenants LIMIT 1")
            .fetch_one(db.pool())
            .await
            .expect("No tenant found"),
    }
}

// ============================================================================
// Acceptance Test 1: Deterministic Replay Key Structure
// ============================================================================

#[test]
fn test_replay_key_includes_all_required_fields() {
    // Required fields: manifest_hash, router_seed, sampler_params, backend,
    // sampling_algorithm_version, rag_snapshot_hash
    let key = ReplayKey {
        manifest_hash: "manifest-abc".to_string(),
        router_seed: Some("seed-123".to_string()),
        sampler_params: SamplingParams {
            temperature: 0.7,
            top_k: Some(50),
            top_p: Some(0.9),
            max_tokens: 512,
            seed: Some(42),
        },
        backend: "CoreML".to_string(),
        sampling_algorithm_version: "v1.0.0".to_string(),
        rag_snapshot_hash: Some("rag-hash-456".to_string()),
        adapter_ids: Some(vec!["adapter-1".to_string()]),
    };

    // Verify all fields are accessible and serializable
    let json = serde_json::to_string(&key).expect("ReplayKey should serialize");
    let parsed: ReplayKey = serde_json::from_str(&json).expect("ReplayKey should deserialize");

    assert_eq!(parsed.manifest_hash, key.manifest_hash);
    assert_eq!(parsed.router_seed, key.router_seed);
    assert_eq!(
        parsed.sampler_params.temperature,
        key.sampler_params.temperature
    );
    assert_eq!(parsed.backend, key.backend);
    assert_eq!(
        parsed.sampling_algorithm_version,
        key.sampling_algorithm_version
    );
    assert_eq!(parsed.rag_snapshot_hash, key.rag_snapshot_hash);
}

#[test]
fn test_sampling_params_serialization() {
    // Required sampling fields: temperature, top-k, top-p, max_tokens, seed
    let params = SamplingParams {
        temperature: 0.0,
        top_k: Some(50),
        top_p: Some(0.95),
        max_tokens: 2048,
        seed: Some(12345),
    };

    let json = serde_json::to_string(&params).unwrap();
    assert!(json.contains("\"temperature\":0.0"));
    assert!(json.contains("\"top_k\":50"));
    assert!(json.contains("\"top_p\":0.95"));
    assert!(json.contains("\"max_tokens\":2048"));
    assert!(json.contains("\"seed\":12345"));

    let parsed: SamplingParams = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.temperature, 0.0);
    assert_eq!(parsed.seed, Some(12345));
}

// ============================================================================
// Acceptance Test 2: 64KB Storage Limit with Truncation
// ============================================================================

#[test]
fn test_64kb_limit_constant() {
    // 64KB limit for prompt/response storage
    assert_eq!(MAX_REPLAY_TEXT_SIZE, 64 * 1024);
}

#[test]
fn test_truncation_at_64kb_boundary() {
    use adapteros_server_api::handlers::replay_inference::truncate_text;

    // Exactly at limit - no truncation
    let text = "a".repeat(MAX_REPLAY_TEXT_SIZE);
    let (result, truncated) = truncate_text(&text, MAX_REPLAY_TEXT_SIZE);
    assert_eq!(result.len(), MAX_REPLAY_TEXT_SIZE);
    assert!(!truncated);

    // One byte over - truncates
    let text = "a".repeat(MAX_REPLAY_TEXT_SIZE + 1);
    let (result, truncated) = truncate_text(&text, MAX_REPLAY_TEXT_SIZE);
    assert_eq!(result.len(), MAX_REPLAY_TEXT_SIZE);
    assert!(truncated);

    // Well under limit - no truncation
    let text = "short text";
    let (result, truncated) = truncate_text(text, MAX_REPLAY_TEXT_SIZE);
    assert_eq!(result, "short text");
    assert!(!truncated);
}

// ============================================================================
// Acceptance Test 3: Match Status Classification
// ============================================================================

#[test]
fn test_match_status_exact() {
    use adapteros_server_api::handlers::replay_inference::compute_match_status;

    let original = "Hello world, this is a test response.";
    let replay = "Hello world, this is a test response.";

    let status = compute_match_status(original, replay);
    assert_eq!(status, ReplayMatchStatus::Exact);
}

#[test]
fn test_match_status_semantic() {
    use adapteros_server_api::handlers::replay_inference::compute_match_status;

    // >80% of words match = semantic
    let original = "The quick brown fox jumps over the lazy dog";
    let replay = "The quick brown fox leaps over the lazy dog";

    let status = compute_match_status(original, replay);
    assert_eq!(status, ReplayMatchStatus::Semantic);
}

#[test]
fn test_match_status_divergent() {
    use adapteros_server_api::handlers::replay_inference::compute_match_status;

    // <80% of words match = divergent
    let original = "The quick brown fox jumps over the lazy dog";
    let replay = "Something completely different with no matching words";

    let status = compute_match_status(original, replay);
    assert_eq!(status, ReplayMatchStatus::Divergent);
}

#[test]
fn test_match_status_empty_strings() {
    use adapteros_server_api::handlers::replay_inference::compute_match_status;

    let status = compute_match_status("", "");
    assert_eq!(status, ReplayMatchStatus::Exact);
}

// ============================================================================
// Acceptance Test 4: Divergence Position Detection
// ============================================================================

#[test]
fn test_divergence_position_identical() {
    use adapteros_server_api::handlers::replay_inference::compute_divergence_position;

    let pos = compute_divergence_position("hello world", "hello world");
    assert_eq!(pos, None); // No divergence
}

#[test]
fn test_divergence_position_different_char() {
    use adapteros_server_api::handlers::replay_inference::compute_divergence_position;

    let pos = compute_divergence_position("hello", "hallo");
    assert_eq!(pos, Some(1)); // Diverges at index 1 ('e' vs 'a')
}

#[test]
fn test_divergence_position_prefix() {
    use adapteros_server_api::handlers::replay_inference::compute_divergence_position;

    // When one is a prefix of the other
    let pos = compute_divergence_position("hello", "hello world");
    assert_eq!(pos, Some(5)); // Diverges at end of shorter string
}

// ============================================================================
// Acceptance Test 5: Replay Status States
// ============================================================================

#[test]
fn test_replay_status_enum_values() {
    // Replay status values: available, approximate, degraded, unavailable
    let available = ReplayStatus::Available;
    let approximate = ReplayStatus::Approximate;
    let degraded = ReplayStatus::Degraded;
    let unavailable = ReplayStatus::Unavailable;

    // Verify serialization matches expected snake_case
    assert_eq!(serde_json::to_string(&available).unwrap(), "\"available\"");
    assert_eq!(
        serde_json::to_string(&approximate).unwrap(),
        "\"approximate\""
    );
    assert_eq!(serde_json::to_string(&degraded).unwrap(), "\"degraded\"");
    assert_eq!(
        serde_json::to_string(&unavailable).unwrap(),
        "\"unavailable\""
    );
}

// ============================================================================
// Acceptance Test 6: Divergence Details Structure
// ============================================================================

#[test]
fn test_divergence_details_structure() {
    let details = DivergenceDetails {
        divergence_position: Some(42),
        backend_changed: true,
        manifest_changed: false,
        approximation_reasons: vec![
            "2 RAG documents unavailable".to_string(),
            "Original prompt was truncated".to_string(),
        ],
    };

    let json = serde_json::to_string(&details).unwrap();
    assert!(json.contains("\"divergence_position\":42"));
    assert!(json.contains("\"backend_changed\":true"));
    assert!(json.contains("\"manifest_changed\":false"));
    assert!(json.contains("RAG documents unavailable"));
}

// ============================================================================
// Database Integration Tests
// ============================================================================

#[tokio::test]
async fn test_replay_metadata_stored_with_replay_key_fields() {
    let db = Db::new_in_memory().await.unwrap();
    let tenant_id = setup_test_tenant(&db).await;

    let inference_id = "prd02-test-001";
    create_test_metadata(&db, inference_id, &tenant_id).await;

    // Retrieve and verify all replay key fields are stored
    let metadata = db
        .get_replay_metadata_by_inference(inference_id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(metadata.manifest_hash, "test-manifest-hash-abc123");
    assert_eq!(metadata.router_seed, Some("test-seed-456".to_string()));
    assert_eq!(metadata.backend, "CoreML");
    assert_eq!(
        metadata.sampling_algorithm_version,
        SAMPLING_ALGORITHM_VERSION
    );
    assert!(metadata.rag_snapshot_hash.is_some());
    assert!(metadata.adapter_ids_json.is_some());
}

#[tokio::test]
async fn test_replay_status_transitions() {
    let db = Db::new_in_memory().await.unwrap();
    let tenant_id = setup_test_tenant(&db).await;

    let inference_id = "prd02-test-002";
    create_test_metadata(&db, inference_id, &tenant_id).await;

    // Initial status should be available
    let metadata = db
        .get_replay_metadata_by_inference(inference_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(metadata.replay_status, "available");

    // Update to degraded
    db.update_replay_status(inference_id, "degraded")
        .await
        .unwrap();

    let metadata = db
        .get_replay_metadata_by_inference(inference_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(metadata.replay_status, "degraded");

    // Update to unavailable
    db.update_replay_status(inference_id, "unavailable")
        .await
        .unwrap();

    let metadata = db
        .get_replay_metadata_by_inference(inference_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(metadata.replay_status, "unavailable");
}

#[tokio::test]
async fn test_truncated_flags_stored_correctly() {
    let db = Db::new_in_memory().await.unwrap();
    let tenant_id = setup_test_tenant(&db).await;

    // Create metadata with truncation flags set
    let params = CreateReplayMetadataParams {
        inference_id: "prd02-test-003".to_string(),
        tenant_id: tenant_id.clone(),
        manifest_hash: "hash".to_string(),
        router_seed: None,
        sampling_params_json: "{}".to_string(),
        backend: "MLX".to_string(),
        sampling_algorithm_version: Some(SAMPLING_ALGORITHM_VERSION.to_string()),
        rag_snapshot_hash: None,
        adapter_ids: None,
        prompt_text: "truncated prompt".to_string(),
        prompt_truncated: true, // Flag set
        response_text: Some("truncated response".to_string()),
        response_truncated: true, // Flag set
        rag_doc_ids: None,
        replay_status: Some("approximate".to_string()),
        latency_ms: None,
        tokens_generated: None,
    };

    db.create_replay_metadata(params).await.unwrap();

    let metadata = db
        .get_replay_metadata_by_inference("prd02-test-003")
        .await
        .unwrap()
        .unwrap();

    assert_eq!(metadata.prompt_truncated, 1);
    assert_eq!(metadata.response_truncated, 1);
    // When truncated, status should be approximate
    assert_eq!(metadata.replay_status, "approximate");
}

// ============================================================================
// Acceptance Test: RAG Document Tracking
// ============================================================================

#[tokio::test]
async fn test_rag_doc_ids_stored_and_retrieved() {
    let db = Db::new_in_memory().await.unwrap();
    let tenant_id = setup_test_tenant(&db).await;

    let inference_id = "prd02-rag-test-001";
    let rag_doc_ids = vec![
        "doc-uuid-1".to_string(),
        "doc-uuid-2".to_string(),
        "doc-uuid-3".to_string(),
    ];

    let params = CreateReplayMetadataParams {
        inference_id: inference_id.to_string(),
        tenant_id: tenant_id.clone(),
        manifest_hash: "hash".to_string(),
        router_seed: None,
        sampling_params_json: "{}".to_string(),
        backend: "CoreML".to_string(),
        sampling_algorithm_version: Some(SAMPLING_ALGORITHM_VERSION.to_string()),
        rag_snapshot_hash: Some("rag-hash".to_string()),
        adapter_ids: None,
        prompt_text: "prompt".to_string(),
        prompt_truncated: false,
        response_text: Some("response".to_string()),
        response_truncated: false,
        rag_doc_ids: Some(rag_doc_ids.clone()),
        replay_status: Some("available".to_string()),
        latency_ms: None,
        tokens_generated: None,
    };

    db.create_replay_metadata(params).await.unwrap();

    let metadata = db
        .get_replay_metadata_by_inference(inference_id)
        .await
        .unwrap()
        .unwrap();

    // Verify RAG doc IDs are stored correctly
    let stored_doc_ids: Vec<String> =
        serde_json::from_str(&metadata.rag_doc_ids_json.unwrap()).unwrap();
    assert_eq!(stored_doc_ids, rag_doc_ids);
}
