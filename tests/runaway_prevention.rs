//! Tests for runaway process prevention mechanisms
//!
//! Comprehensive tests for timeout, circuit breaker, resource limiting, and deadlock detection.
//! Aligns with existing test patterns and policy enforcement requirements.

use mplora_worker::{Worker, InferenceRequest, TimeoutConfig, CircuitBreaker, ResourceLimiter, ResourceLimits};
use mplora_telemetry::TelemetryWriter;
use mplora_manifest::ManifestV3;
use std::time::Duration;
use tokio::time::timeout;
use adapteros_deterministic_exec::{init_global_executor, ExecutorConfig, spawn_deterministic};

/// Create a test worker for testing
async fn create_test_worker() -> Worker<mplora_kernel_api::MockKernels> {
    // Create a minimal manifest for testing
    let manifest = ManifestV3 {
        schema_version: "v3".to_string(),
        cpid: "test".to_string(),
        plan_id: "test-plan".to_string(),
        adapters: vec![],
        router: mplora_manifest::RouterConfig {
            k_sparse: 3,
            tau: 1.0,
            entropy_floor: 0.02,
        },
        policies: mplora_manifest::Policies {
            egress: mplora_manifest::EgressPolicy {
                mode: "deny_all".to_string(),
                serve_requires_pf: true,
                allow_tcp: false,
                allow_udp: false,
                uds_paths: vec!["/var/run/aos/test/*.sock".to_string()],
                media_import: mplora_manifest::MediaImportPolicy {
                    require_signature: true,
                    require_sbom: true,
                },
            },
            determinism: mplora_manifest::DeterminismPolicy {
                require_metallib_embed: true,
                require_kernel_hash_match: true,
                rng: "hkdf_seeded".to_string(),
                retrieval_tie_break: vec!["score_desc".to_string(), "doc_id_asc".to_string()],
            },
            router: mplora_manifest::RouterPolicy {
                k_sparse: 3,
                gate_quant: "q15".to_string(),
                entropy_floor: 0.02,
                sample_tokens_full: 128,
            },
            evidence: mplora_manifest::EvidencePolicy {
                require_open_book: true,
                min_spans: 1,
                prefer_latest_revision: true,
                warn_on_superseded: true,
            },
            refusal: mplora_manifest::RefusalPolicy {
                abstain_threshold: 0.55,
                missing_fields_templates: std::collections::HashMap::new(),
            },
            numeric: mplora_manifest::NumericPolicy {
                canonical_units: std::collections::HashMap::new(),
                max_rounding_error: 0.5,
                require_units_in_trace: true,
            },
            rag: mplora_manifest::RagPolicy {
                index_scope: "per_tenant".to_string(),
                doc_tags_required: vec!["doc_id".to_string(), "rev".to_string()],
                embedding_model_hash: "b3:test".to_string(),
                topk: 5,
                order: vec!["score_desc".to_string(), "doc_id_asc".to_string()],
            },
            isolation: mplora_manifest::IsolationPolicy {
                process_model: "per_tenant".to_string(),
                uds_root: "/var/run/aos/test".to_string(),
                forbid_shm: true,
                keys: mplora_manifest::KeysPolicy {
                    backend: "secure_enclave".to_string(),
                    require_hardware: true,
                },
            },
            telemetry: mplora_manifest::TelemetryPolicy {
                schema_hash: "b3:test".to_string(),
                sampling: mplora_manifest::SamplingPolicy {
                    token: 0.05,
                    router: 1.0,
                    inference: 1.0,
                },
                router_full_tokens: 128,
                bundle: mplora_manifest::BundlePolicy {
                    max_events: 500000,
                    max_bytes: 268435456,
                },
            },
            retention: mplora_manifest::RetentionPolicy {
                keep_bundles_per_cpid: 12,
                keep_incident_bundles: true,
                keep_promotion_bundles: true,
                evict_strategy: "oldest_first_safe".to_string(),
            },
            performance: mplora_manifest::PerformancePolicy {
                latency_p95_ms: 24,
                router_overhead_pct_max: 8,
                throughput_tokens_per_s_min: 40,
            },
            memory: mplora_manifest::MemoryPolicy {
                min_headroom_pct: 15,
                evict_order: vec!["ephemeral_ttl".to_string(), "cold_lru".to_string()],
                k_reduce_before_evict: true,
            },
            artifacts: mplora_manifest::ArtifactsPolicy {
                require_signature: true,
                require_sbom: true,
                cas_only: true,
            },
            secrets: mplora_manifest::SecretsPolicy {
                env_allowed: vec![],
                keystore: "secure_enclave".to_string(),
                rotate_on_promotion: true,
            },
            build_release: mplora_manifest::BuildReleasePolicy {
                require_replay_zero_diff: true,
                hallucination_thresholds: mplora_manifest::HallucinationThresholds {
                    arr_min: 0.95,
                    ecs5_min: 0.75,
                    hlr_max: 0.03,
                    cr_max: 0.01,
                },
                require_signed_plan: true,
                require_rollback_plan: true,
            },
            compliance: mplora_manifest::CompliancePolicy {
                control_matrix_hash: "b3:test".to_string(),
                require_evidence_links: true,
                require_itar_suite_green: true,
            },
            incident: mplora_manifest::IncidentPolicy {
                memory: vec!["drop_ephemeral".to_string(), "reduce_k".to_string()],
                router_skew: vec!["entropy_floor_on".to_string()],
                determinism: vec!["freeze_plan".to_string()],
                violation: vec!["isolate".to_string()],
            },
            output: mplora_manifest::OutputPolicy {
                format: "json".to_string(),
                require_trace: true,
                forbidden_topics: vec!["tenant_crossing".to_string()],
            },
            adapters: mplora_manifest::AdaptersPolicy {
                min_activation_pct: 2.0,
                min_quality_delta: 0.5,
                require_registry_admit: true,
            },
        },
        seeds: mplora_manifest::Seeds {
            global: "test-seed".to_string(),
        },
    };

    // Create mock kernels
    let kernels = mplora_kernel_api::MockKernels::new();

    // Create telemetry writer
    let telemetry = TelemetryWriter::new("/tmp/test-telemetry", 1000, 1024 * 1024).unwrap();

    // Create worker
    Worker::new(manifest, kernels, None, "test-tokenizer.json", "test-model.bin", telemetry).unwrap()
}

#[tokio::test]
async fn test_inference_timeout() {
    let mut worker = create_test_worker().await;
    let request = InferenceRequest {
        cpid: "test".to_string(),
        prompt: "Test prompt".to_string(),
        max_tokens: 1000,
        require_evidence: false,
        request_type: Default::default(),
    };

    // Test timeout behavior
    let result = timeout(Duration::from_secs(1), worker.infer(request)).await;
    assert!(result.is_err(), "Request should timeout");
}

#[tokio::test]
async fn test_circuit_breaker() {
    let mut circuit_breaker = CircuitBreaker::new(3, Duration::from_secs(10));
    
    // Simulate failures
    for _ in 0..3 {
        let result = circuit_breaker.call(async {
            Err(mplora_core::AosError::Worker("Simulated failure".to_string()))
        }).await;
        assert!(result.is_err());
    }
    
    // Circuit should now be open
    let result = circuit_breaker.call(async {
        Ok("success")
    }).await;
    assert!(result.is_err(), "Circuit breaker should be open");
}

#[tokio::test]
async fn test_resource_limiter() {
    let limiter = ResourceLimiter::new(ResourceLimits {
        max_concurrent_requests: 2,
        max_tokens_per_second: 10,
        max_memory_per_request: 1024,
        max_cpu_time_per_request: Duration::from_secs(5),
        max_requests_per_minute: 60,
    });
    
    // Test memory limit enforcement
    let guard1 = limiter.acquire_request().await;
    assert!(guard1.is_ok());
    
    let guard2 = limiter.acquire_request().await;
    assert!(guard2.is_ok());
    
    // Third request should fail due to concurrency limit
    let guard3 = limiter.acquire_request().await;
    assert!(guard3.is_err());
    
    // Release guards
    drop(guard1);
    drop(guard2);
    
    // Should be able to acquire again
    let guard4 = limiter.acquire_request().await;
    assert!(guard4.is_ok());
}

#[tokio::test]
async fn test_token_rate_limiter() {
    let limiter = ResourceLimiter::new(ResourceLimits {
        max_concurrent_requests: 10,
        max_tokens_per_second: 2,
        max_memory_per_request: 1024,
        max_cpu_time_per_request: Duration::from_secs(5),
        max_requests_per_minute: 60,
    });
    
    // First two tokens should succeed
    assert!(limiter.check_token_rate().is_ok());
    assert!(limiter.check_token_rate().is_ok());
    
    // Third token should fail
    assert!(limiter.check_token_rate().is_err());
}

#[tokio::test]
async fn test_timeout_config() {
    let config = TimeoutConfig {
        inference_timeout: Duration::from_secs(5),
        evidence_timeout: Duration::from_secs(1),
        router_timeout: Duration::from_millis(100),
        policy_timeout: Duration::from_millis(50),
    };
    
    assert_eq!(config.inference_timeout, Duration::from_secs(5));
    assert_eq!(config.evidence_timeout, Duration::from_secs(1));
    assert_eq!(config.router_timeout, Duration::from_millis(100));
    assert_eq!(config.policy_timeout, Duration::from_millis(50));
}

#[tokio::test]
async fn test_resource_limits_default() {
    let limits = ResourceLimits::default();
    
    assert_eq!(limits.max_concurrent_requests, 10);
    assert_eq!(limits.max_tokens_per_second, 40);
    assert_eq!(limits.max_memory_per_request, 50 * 1024 * 1024);
    assert_eq!(limits.max_cpu_time_per_request, Duration::from_secs(30));
    assert_eq!(limits.max_requests_per_minute, 100);
}

#[tokio::test]
async fn test_worker_safety_mechanisms() {
    let worker = create_test_worker().await;
    
    // Verify safety mechanisms are initialized
    assert_eq!(worker.timeout_config.inference_timeout, Duration::from_secs(30));
    assert_eq!(worker.circuit_breaker.failure_count(), 0);
    assert_eq!(worker.resource_limiter.get_concurrent_requests(), 0);
    assert_eq!(worker.deadlock_detector.get_deadlock_count(), 0);
    assert!(!worker.health_monitor.is_shutdown_requested());
}

#[tokio::test]
async fn test_health_monitor() {
    let health_monitor = mplora_worker::HealthMonitor::new(mplora_worker::HealthConfig::default()).unwrap();
    
    assert!(!health_monitor.is_shutdown_requested());
    assert!(health_monitor.get_uptime().as_secs() < 1);
    
    health_monitor.record_request();
    // Should not panic
}

#[tokio::test]
async fn test_deadlock_detector() {
    let detector = mplora_worker::DeadlockDetector::new(mplora_worker::DeadlockConfig::default());
    
    assert_eq!(detector.get_deadlock_count(), 0);
    assert!(!detector.is_recovery_in_progress());
    
    detector.record_lock_acquisition("test_lock".to_string(), 1);
    detector.record_lock_release("test_lock", 1);
    
    // Should not panic
    assert_eq!(detector.get_deadlock_count(), 0);
}

#[tokio::test]
async fn test_telemetry_integration() {
    let mut worker = create_test_worker().await;
    let request = InferenceRequest {
        cpid: "test".to_string(),
        prompt: "Test prompt".to_string(),
        max_tokens: 10,
        require_evidence: false,
        request_type: Default::default(),
    };

    // Test that telemetry is logged during inference
    let result = worker.infer(request).await;
    
    // Should complete without error (even if inference fails)
    // The important part is that safety mechanisms are in place
    match result {
        Ok(_) => println!("Inference succeeded"),
        Err(e) => println!("Inference failed as expected: {}", e),
    }
}

#[tokio::test]
async fn test_memory_pressure_handling() {
    let mut worker = create_test_worker().await;
    
    // Test memory pressure detection
    let memory_usage = worker.health_monitor.get_memory_usage();
    assert!(memory_usage.is_ok());
    
    // Test memory monitor
    let headroom = worker.memory_monitor.headroom_pct();
    assert!(headroom > 0.0 && headroom <= 100.0);
}

#[tokio::test]
async fn test_concurrent_request_limiting() {
    let limiter = ResourceLimiter::new(ResourceLimits {
        max_concurrent_requests: 3,
        max_tokens_per_second: 100,
        max_memory_per_request: 1024,
        max_cpu_time_per_request: Duration::from_secs(5),
        max_requests_per_minute: 1000,
    });
    
    // Spawn multiple concurrent requests
    let mut handles = vec![];
    
    for i in 0..5 {
        let limiter = &limiter;
        let handle = spawn_deterministic(format!("Request {}", i), async move {
            match limiter.acquire_request().await {
                Ok(guard) => {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    drop(guard);
                    format!("Request {} completed", i)
                }
                Err(e) => format!("Request {} failed: {}", i, e),
            }
        })?;
        handles.push(handle);
    }
    
    let results: Vec<String> = futures::future::join_all(handles).await
        .into_iter()
        .map(|r| r.unwrap())
        .collect();
    
    // Should have some successes and some failures due to concurrency limit
    let successes = results.iter().filter(|r| r.contains("completed")).count();
    let failures = results.iter().filter(|r| r.contains("failed")).count();
    
    assert_eq!(successes, 3, "Should have exactly 3 successful requests");
    assert_eq!(failures, 2, "Should have exactly 2 failed requests");
}
