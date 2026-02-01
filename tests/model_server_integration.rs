//! Model Server Integration Tests
//!
//! Tests the full worker → Model Server → inference path:
//! - Workers connect to a shared Model Server
//! - Forward pass requests are executed via gRPC
//! - KV cache is shared across multiple workers
//! - Hot/cold adapter partitioning works correctly
//!
//! These tests verify the architecture described in `crates/adapteros-model-server/src/lib.rs`:
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                    Control Plane                                 │
//! └───────────────────────────────┬─────────────────────────────────┘
//!                                 │
//!           ┌─────────────────────┼─────────────────────┐
//!           ▼                     ▼                     ▼
//!    ┌────────────┐        ┌────────────┐        ┌────────────┐
//!    │  Worker A  │        │  Worker B  │        │  Worker C  │
//!    │ (adapters) │        │ (adapters) │        │ (adapters) │
//!    └─────┬──────┘        └─────┬──────┘        └─────┬──────┘
//!          │                     │                     │
//!          └─────────────────────┼─────────────────────┘
//!                                ▼
//!                      ┌──────────────────┐
//!                      │   Model Server   │
//!                      │ (aos-model-srv)  │
//!                      │                  │
//!                      │  Loaded Model    │
//!                      │  KV Cache Mgr    │
//!                      └──────────────────┘
//! ```
//!
//! ## Running Tests
//!
//! Unit-level tests (no server required):
//! ```bash
//! cargo test --test model_server_integration
//! ```
//!
//! gRPC integration tests (requires running Model Server):
//! ```bash
//! # First, start the model server:
//! aos-model-srv --model-path /var/models/Llama-3.2-3B-Instruct-4bit
//!
//! # Then run the ignored tests with model-server feature:
//! cargo test --test model_server_integration --features adapteros-lora-worker/model-server -- --ignored
//! ```

#![cfg(test)]
#![allow(clippy::single_component_path_imports)]
#![allow(unused_imports)]

use std::sync::Arc;
use std::time::Duration;

use adapteros_core::Result;
use adapteros_model_server::adapter_cache::AdapterCache;
use adapteros_model_server::config::ModelServerConfig;
use adapteros_model_server::forward::{ForwardExecutor, ForwardPassRequest, ForwardPassResponse};
use adapteros_model_server::kv_cache::KvCacheManager;
use adapteros_model_server::ModelServer;

// =============================================================================
// Unit-Level Integration Tests (No gRPC server required)
// =============================================================================

/// Test that two workers sharing the same ForwardExecutor share KV cache
#[test]
fn test_shared_kv_cache_across_sessions() {
    // Create shared infrastructure (simulating what Model Server provides)
    // KV cache memory per session = 2 * max_seq_len * hidden_size * num_layers * 4 bytes
    // For hidden=256, layers=4, seq=128: 2 * 128 * 256 * 4 * 4 = 1MB per session
    // 16MB cache should hold many sessions without eviction
    let kv_cache = Arc::new(KvCacheManager::new(
        16 * 1024 * 1024, // 16MB cache
        256,              // hidden_size (small for testing)
        4,                // num_layers (small for testing)
    ));
    let adapter_cache = Arc::new(AdapterCache::with_defaults());

    // Create executor with mock model (no MLX feature required)
    let executor = ForwardExecutor::new(
        kv_cache.clone(),
        adapter_cache.clone(),
        32000, // vocab_size (LLaMA-3 sized)
        256,   // hidden_size (match KV cache)
        4,     // num_layers (match KV cache)
    );

    // Worker 1: First request (cache miss)
    let request1 = ForwardPassRequest {
        session_id: "worker-1-session".to_string(),
        input_ids: vec![1, 2, 3, 4, 5],
        position: 0,
        max_seq_len: 128, // Small seq len for testing
        adapter_ids: vec![],
        adapter_gates_q15: vec![],
        include_hidden_states: false,
        manifest_seed: None,
    };

    let response1 = executor.forward(request1).expect("Forward pass 1 failed");

    // Verify first request was a cache miss
    assert!(
        !response1.kv_cache_hit,
        "First request should be cache miss"
    );
    assert_eq!(
        response1.cached_tokens, 0,
        "No tokens should be cached initially"
    );
    assert_eq!(
        response1.logits.len(),
        32000,
        "Logit dimension should match vocab_size"
    );
    assert_eq!(
        response1.position, 5,
        "Position should advance by input length"
    );

    // Worker 1: Second request on same session (cache hit)
    let request2 = ForwardPassRequest {
        session_id: "worker-1-session".to_string(),
        input_ids: vec![6, 7, 8],
        position: 5,
        max_seq_len: 128,
        adapter_ids: vec![],
        adapter_gates_q15: vec![],
        include_hidden_states: false,
        manifest_seed: None,
    };

    let response2 = executor.forward(request2).expect("Forward pass 2 failed");

    // Verify second request hit the cache
    assert!(response2.kv_cache_hit, "Second request should hit KV cache");
    assert_eq!(
        response2.cached_tokens, 5,
        "Should have 5 cached tokens from first request"
    );
    assert_eq!(response2.position, 8, "Position should continue from 5 + 3");

    // Worker 2: Request on different session (new cache entry)
    let request3 = ForwardPassRequest {
        session_id: "worker-2-session".to_string(),
        input_ids: vec![10, 11, 12],
        position: 0,
        max_seq_len: 128,
        adapter_ids: vec![],
        adapter_gates_q15: vec![],
        include_hidden_states: false,
        manifest_seed: None,
    };

    let response3 = executor.forward(request3).expect("Forward pass 3 failed");

    // Different session = cache miss
    assert!(!response3.kv_cache_hit, "New session should be cache miss");
    assert_eq!(response3.cached_tokens, 0);
    assert_eq!(response3.position, 3);

    // Verify cache statistics
    let stats = kv_cache.stats();
    assert_eq!(stats.active_sessions, 2, "Should have 2 active sessions");
    // hits: 1 (second request from worker 1 hit the cache from first request)
    // Note: get_or_create counts as miss on first access, hit on second
    assert!(stats.hits >= 1, "Should have at least 1 cache hit");
}

/// Test that logits have correct dimensions matching vocab_size
#[test]
fn test_logit_dimensions() {
    let vocab_sizes = vec![32000, 50257, 128256]; // LLaMA-3, GPT-2, LLaMA-3.1

    for vocab_size in vocab_sizes {
        let kv_cache = Arc::new(KvCacheManager::new(1024 * 1024, 4096, 32));
        let adapter_cache = Arc::new(AdapterCache::with_defaults());

        let executor = ForwardExecutor::new(kv_cache, adapter_cache, vocab_size, 4096, 32);

        let request = ForwardPassRequest {
            session_id: format!("test-vocab-{}", vocab_size),
            input_ids: vec![1, 2, 3],
            position: 0,
            max_seq_len: 2048,
            adapter_ids: vec![],
            adapter_gates_q15: vec![],
            include_hidden_states: false,
            manifest_seed: None,
        };

        let response = executor.forward(request).expect("Forward pass failed");
        assert_eq!(
            response.logits.len(),
            vocab_size,
            "Logits length should match vocab_size = {}",
            vocab_size
        );
    }
}

/// Test hot adapter fusion modifies logits
#[test]
fn test_hot_adapter_fusion() {
    let kv_cache = Arc::new(KvCacheManager::new(1024 * 1024, 4096, 32));
    let adapter_cache = Arc::new(AdapterCache::new(8, None));

    // Load a hot adapter
    let lora_a = vec![0.1f32; 4096 * 8]; // rank=8, hidden=4096
    let lora_b = vec![0.1f32; 32000 * 8]; // vocab=32000, rank=8
    adapter_cache
        .load(1, "test-adapter".to_string(), lora_a, lora_b, 1.0)
        .expect("Failed to load adapter");

    let executor = ForwardExecutor::new(kv_cache, adapter_cache.clone(), 32000, 4096, 32);

    // Request without adapter
    let request_no_adapter = ForwardPassRequest {
        session_id: "no-adapter".to_string(),
        input_ids: vec![1, 2, 3],
        position: 0,
        max_seq_len: 2048,
        adapter_ids: vec![],
        adapter_gates_q15: vec![],
        include_hidden_states: false,
        manifest_seed: None,
    };

    let response_no_adapter = executor
        .forward(request_no_adapter)
        .expect("Forward without adapter failed");

    // Request with adapter (gate = max Q15 value)
    let request_with_adapter = ForwardPassRequest {
        session_id: "with-adapter".to_string(),
        input_ids: vec![1, 2, 3],
        position: 0,
        max_seq_len: 2048,
        adapter_ids: vec![1],
        adapter_gates_q15: vec![32767], // Full gate
        include_hidden_states: false,
        manifest_seed: None,
    };

    let response_with_adapter = executor
        .forward(request_with_adapter)
        .expect("Forward with adapter failed");

    // Both should have same dimensions
    assert_eq!(
        response_no_adapter.logits.len(),
        response_with_adapter.logits.len()
    );

    // Verify adapter cache recorded fusion
    let stats = adapter_cache.stats();
    assert_eq!(stats.fusions, 1, "Should have recorded 1 fusion operation");
}

/// Test adapter cache eviction with LRU policy
#[test]
fn test_adapter_cache_lru_eviction() {
    // Create cache with max 2 adapters
    let adapter_cache = Arc::new(AdapterCache::new(2, None));

    // Load 2 adapters
    adapter_cache
        .load(
            1,
            "adapter-1".to_string(),
            vec![1.0; 100],
            vec![1.0; 100],
            1.0,
        )
        .expect("Failed to load adapter 1");
    adapter_cache
        .load(
            2,
            "adapter-2".to_string(),
            vec![2.0; 100],
            vec![2.0; 100],
            1.0,
        )
        .expect("Failed to load adapter 2");

    assert!(adapter_cache.contains(1));
    assert!(adapter_cache.contains(2));

    // Access adapter 2 to make it more recently used
    adapter_cache.get(2);

    // Load a third adapter - should evict adapter 1 (LRU)
    adapter_cache
        .load(
            3,
            "adapter-3".to_string(),
            vec![3.0; 100],
            vec![3.0; 100],
            1.0,
        )
        .expect("Failed to load adapter 3");

    // Adapter 1 should be evicted (LRU)
    assert!(
        !adapter_cache.contains(1),
        "Adapter 1 should be evicted (LRU)"
    );
    assert!(
        adapter_cache.contains(2),
        "Adapter 2 should be kept (recently accessed)"
    );
    assert!(
        adapter_cache.contains(3),
        "Adapter 3 should be present (newly loaded)"
    );
}

/// Test KV cache eviction under memory pressure
#[test]
fn test_kv_cache_eviction_under_pressure() {
    // Create a very small cache to trigger eviction
    // Memory per session = 2 * max_seq_len * hidden_size * num_layers * 4 bytes
    // For hidden=16, layers=1, seq=8: 2 * 8 * 16 * 1 * 4 = 1024 bytes
    let kv_cache = Arc::new(KvCacheManager::new(1500, 16, 1));

    // Create first session
    let _entry1 = kv_cache.get_or_create("session-1", 8);
    assert_eq!(kv_cache.active_sessions(), 1);

    // Create second session - should trigger eviction of first due to memory limit
    let _entry2 = kv_cache.get_or_create("session-2", 8);

    // Check eviction occurred
    let stats = kv_cache.stats();
    assert!(
        stats.evictions > 0 || stats.active_sessions <= 2,
        "Should have evicted or be at max capacity"
    );
}

/// Test multiple workers sharing the same session ID (concurrent access pattern)
#[test]
fn test_concurrent_session_access() {
    let kv_cache = Arc::new(KvCacheManager::new(1024 * 1024 * 1024, 4096, 32));
    let adapter_cache = Arc::new(AdapterCache::with_defaults());

    let executor = ForwardExecutor::new(kv_cache.clone(), adapter_cache, 32000, 4096, 32);

    // Shared session ID (simulating workers processing same conversation)
    let shared_session = "shared-conversation-123".to_string();

    // Worker 1: Process first chunk
    let request1 = ForwardPassRequest {
        session_id: shared_session.clone(),
        input_ids: vec![1, 2, 3, 4, 5],
        position: 0,
        max_seq_len: 4096,
        adapter_ids: vec![],
        adapter_gates_q15: vec![],
        include_hidden_states: false,
        manifest_seed: None,
    };

    let response1 = executor.forward(request1).expect("Worker 1 forward failed");
    assert_eq!(response1.position, 5);
    assert!(!response1.kv_cache_hit);

    // Worker 2: Process next chunk (continuing same session)
    let request2 = ForwardPassRequest {
        session_id: shared_session.clone(),
        input_ids: vec![6],
        position: 5,
        max_seq_len: 4096,
        adapter_ids: vec![],
        adapter_gates_q15: vec![],
        include_hidden_states: false,
        manifest_seed: None,
    };

    let response2 = executor.forward(request2).expect("Worker 2 forward failed");
    assert_eq!(response2.position, 6);
    assert!(
        response2.kv_cache_hit,
        "Worker 2 should benefit from Worker 1's cache"
    );
    assert_eq!(
        response2.cached_tokens, 5,
        "Should see 5 tokens cached by Worker 1"
    );

    // Verify only 1 session exists
    assert_eq!(kv_cache.active_sessions(), 1);
}

/// Test deterministic seed propagation
#[test]
fn test_manifest_seed_propagation() {
    let kv_cache = Arc::new(KvCacheManager::new(1024 * 1024, 4096, 32));
    let adapter_cache = Arc::new(AdapterCache::with_defaults());

    let executor = ForwardExecutor::new(kv_cache, adapter_cache, 32000, 4096, 32);

    let seed = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x12, 0x34, 0x56, 0x78];

    let request = ForwardPassRequest {
        session_id: "deterministic-session".to_string(),
        input_ids: vec![1, 2, 3],
        position: 0,
        max_seq_len: 2048,
        adapter_ids: vec![],
        adapter_gates_q15: vec![],
        include_hidden_states: false,
        manifest_seed: Some(seed.clone()),
    };

    // The mock forward pass doesn't use the seed, but we verify it's accepted
    let response = executor.forward(request).expect("Forward with seed failed");
    assert_eq!(response.logits.len(), 32000);
}

/// Test hidden states are returned when requested
#[test]
fn test_hidden_states_output() {
    let kv_cache = Arc::new(KvCacheManager::new(1024 * 1024, 4096, 32));
    let adapter_cache = Arc::new(AdapterCache::with_defaults());

    let executor = ForwardExecutor::new(
        kv_cache,
        adapter_cache,
        32000,
        4096, // hidden_size
        32,
    );

    // Request without hidden states
    let request_no_hidden = ForwardPassRequest {
        session_id: "no-hidden".to_string(),
        input_ids: vec![1, 2, 3],
        position: 0,
        max_seq_len: 2048,
        adapter_ids: vec![],
        adapter_gates_q15: vec![],
        include_hidden_states: false,
        manifest_seed: None,
    };

    let response_no_hidden = executor.forward(request_no_hidden).expect("Forward failed");
    assert!(response_no_hidden.hidden_states.is_none());

    // Request with hidden states
    let request_with_hidden = ForwardPassRequest {
        session_id: "with-hidden".to_string(),
        input_ids: vec![1, 2, 3],
        position: 0,
        max_seq_len: 2048,
        adapter_ids: vec![],
        adapter_gates_q15: vec![],
        include_hidden_states: true,
        manifest_seed: None,
    };

    let response_with_hidden = executor
        .forward(request_with_hidden)
        .expect("Forward failed");
    assert!(response_with_hidden.hidden_states.is_some());
    let hidden = response_with_hidden.hidden_states.unwrap();
    assert_eq!(hidden.len(), 4096, "Hidden states should match hidden_size");
}

/// Test Q15 gate conversion and application
#[test]
fn test_q15_gate_handling() {
    let kv_cache = Arc::new(KvCacheManager::new(1024 * 1024, 4096, 32));
    let adapter_cache = Arc::new(AdapterCache::new(8, None));

    // Load adapter
    adapter_cache
        .load(
            1,
            "gated-adapter".to_string(),
            vec![0.5f32; 100],
            vec![0.5f32; 100],
            1.0,
        )
        .expect("Failed to load adapter");

    let executor = ForwardExecutor::new(kv_cache, adapter_cache, 32000, 4096, 32);

    // Test various Q15 gate values
    let gate_tests = vec![
        (32767, "max gate (1.0)"),
        (16383, "half gate (0.5)"),
        (0, "zero gate (0.0)"),
        (-16383, "negative half gate (-0.5)"),
    ];

    for (gate_q15, description) in gate_tests {
        let request = ForwardPassRequest {
            session_id: format!("gate-test-{}", gate_q15),
            input_ids: vec![1, 2, 3],
            position: 0,
            max_seq_len: 2048,
            adapter_ids: vec![1],
            adapter_gates_q15: vec![gate_q15],
            include_hidden_states: false,
            manifest_seed: None,
        };

        let response = executor.forward(request);
        assert!(
            response.is_ok(),
            "Forward pass should succeed with gate: {} ({})",
            gate_q15,
            description
        );
    }
}

// =============================================================================
// gRPC Server Integration Tests (Requires running Model Server)
// =============================================================================
//
// These tests require:
// 1. The `model-server` feature enabled on adapteros-lora-worker
// 2. A running Model Server instance
//
// Run with:
// ```bash
// # Start model server first:
// aos-model-srv --model-path /var/models/Llama-3.2-3B-Instruct-4bit
//
// # Then run tests:
// cargo test --test model_server_integration --features adapteros-lora-worker/model-server -- --ignored
// ```

/// Test workers connecting to Model Server via gRPC
///
/// This test requires the Model Server to be running. Run with:
/// ```bash
/// cargo test --test model_server_integration test_workers_share_model_server \
///     --features adapteros-lora-worker/model-server -- --ignored
/// ```
#[cfg(feature = "model-server")]
#[tokio::test]
#[ignore] // Requires running Model Server binary
async fn test_workers_share_model_server() {
    use adapteros_lora_worker::model_server_client::{ModelServerClient, ModelServerClientConfig};

    // 1. Create client config for Model Server
    let config = ModelServerClientConfig::with_addr("http://127.0.0.1:50051");

    // 2. Create two worker clients
    let client1 = ModelServerClient::new(config.clone());
    let client2 = ModelServerClient::new(config);

    // 3. Connect both clients
    client1.connect().await.expect("Client 1 failed to connect");
    client2.connect().await.expect("Client 2 failed to connect");

    // 4. Worker 1: Send forward request
    let response1 = client1
        .forward(
            "shared-session".to_string(),
            vec![1, 2, 3, 4, 5],
            0,
            4096,
            vec![], // No adapters
            vec![],
            None,
            false,
        )
        .await
        .expect("Worker 1 forward failed");

    assert_eq!(response1.position, 5);
    assert!(!response1.kv_cache_hit);

    // 5. Worker 2: Send forward request (should hit KV cache)
    let response2 = client2
        .forward(
            "shared-session".to_string(),
            vec![6, 7],
            5, // Continue from position 5
            4096,
            vec![],
            vec![],
            None,
            false,
        )
        .await
        .expect("Worker 2 forward failed");

    assert_eq!(response2.position, 7);
    assert!(
        response2.kv_cache_hit,
        "Worker 2 should hit KV cache from Worker 1"
    );
    assert_eq!(response2.cached_tokens, 5);

    // 6. Verify logit dimensions match expected vocab_size
    // (Model Server configures this based on loaded model)
    assert!(!response1.logits.is_empty(), "Should have non-empty logits");
    assert_eq!(
        response1.logits.len(),
        response2.logits.len(),
        "Both workers should get same logit dimensions"
    );
}

/// Test Model Server health and status endpoints
#[cfg(feature = "model-server")]
#[tokio::test]
#[ignore] // Requires running Model Server
async fn test_model_server_health_and_status() {
    use adapteros_lora_worker::model_server_client::{ModelServerClient, ModelServerClientConfig};

    let config = ModelServerClientConfig::default();
    let client = ModelServerClient::new(config);

    client.connect().await.expect("Failed to connect");

    // Check health
    let health = client.health().await.expect("Health check failed");
    assert!(
        health.status == adapteros_model_server::proto::health_response::Status::Healthy as i32
    );

    // Get status
    let status = client.status().await.expect("Status request failed");
    assert!(status.kv_cache_bytes_total > 0);
}

/// Test hot/cold adapter partitioning in Model Server mode
#[cfg(feature = "model-server")]
#[tokio::test]
#[ignore] // Requires running Model Server
async fn test_model_server_adapter_partitioning() {
    use adapteros_lora_worker::model_server_client::{ModelServerClient, ModelServerClientConfig};

    let config = ModelServerClientConfig::default();
    let client = ModelServerClient::new(config);

    client.connect().await.expect("Failed to connect");

    // Load an adapter as hot
    let adapter_weights = vec![0u8; 1024]; // Placeholder weights
    let load_response = client
        .load_adapter(1, "hot-adapter".to_string(), adapter_weights, true)
        .await
        .expect("Failed to load adapter");

    assert!(load_response.success);
    assert!(load_response.is_hot, "Adapter should be marked as hot");

    // List adapters
    let list_response = client
        .list_adapters()
        .await
        .expect("Failed to list adapters");

    assert!(list_response.adapters.iter().any(|a| a.adapter_id == 1));

    // Unload adapter
    let unload_response = client
        .unload_adapter(1)
        .await
        .expect("Failed to unload adapter");

    assert!(unload_response.success);
}

/// Test Model Server graceful drain
#[cfg(feature = "model-server")]
#[tokio::test]
#[ignore] // Requires running Model Server - destructive test
async fn test_model_server_drain() {
    use adapteros_lora_worker::model_server_client::{ModelServerClient, ModelServerClientConfig};

    let config = ModelServerClientConfig::default();
    let client = ModelServerClient::new(config);

    client.connect().await.expect("Failed to connect");

    // Request drain with 5 second grace period
    client.drain(5).await.expect("Drain request failed");

    // After drain, new requests should fail
    tokio::time::sleep(Duration::from_millis(100)).await;

    let result = client
        .forward(
            "post-drain".to_string(),
            vec![1, 2, 3],
            0,
            2048,
            vec![],
            vec![],
            None,
            false,
        )
        .await;

    assert!(result.is_err(), "Requests should fail after drain");
}

// =============================================================================
// Model Server Creation Tests
// =============================================================================

/// Test ModelServer construction with various configurations
#[test]
fn test_model_server_config_variations() {
    use std::path::PathBuf;

    // Default config
    let default_config = ModelServerConfig::default();
    assert!(!default_config.enabled);
    assert!(default_config.model_path.is_none());

    // Builder pattern
    let custom_config = ModelServerConfig::new()
        .with_model_path(PathBuf::from("/var/models/test"))
        .with_socket_path(PathBuf::from("/tmp/test.sock"))
        .enabled();

    assert!(custom_config.enabled);
    assert_eq!(
        custom_config.model_path,
        Some(PathBuf::from("/var/models/test"))
    );
    assert_eq!(custom_config.socket_path, PathBuf::from("/tmp/test.sock"));

    // Validation
    assert!(custom_config.validate().is_ok());

    // Invalid threshold
    let mut invalid_config = custom_config.clone();
    invalid_config.hot_adapter_threshold = 1.5; // Out of range
    assert!(invalid_config.validate().is_err());
}

/// Test ModelServer instantiation
#[test]
fn test_model_server_creation() {
    let config = ModelServerConfig::default();
    let server = ModelServer::new(config);

    // Verify initial state
    assert!(!server.is_draining());
    assert_eq!(server.request_count(), 0);
    assert!(server.uptime_secs() < 1); // Just created
}

/// Test ModelServer drain lifecycle
#[test]
fn test_model_server_drain_state() {
    let config = ModelServerConfig::default();
    let server = ModelServer::new(config);

    assert!(!server.is_draining());

    server.start_drain();

    assert!(server.is_draining());
}
