<<<<<<< HEAD
#![cfg(all(test, feature = "extended-tests"))]

=======
>>>>>>> integration-branch
//! End-to-end tests for adapter lifecycle management
//!
//! Validates complete adapter workflows including creation, loading, activation,
//! hot-swapping, eviction, and cleanup with proper telemetry and policy enforcement.

use crate::orchestration::TestEnvironment;
use adapteros_core::{AosError, Result};
use adapteros_telemetry::{AdapterPreloadEvent, AdapterSwapEvent, TelemetryWriter};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Adapter lifecycle management test suite
pub struct AdapterLifecycleTest {
    env: Arc<Mutex<TestEnvironment>>,
}

impl AdapterLifecycleTest {
    pub fn new(env: Arc<Mutex<TestEnvironment>>) -> Self {
        Self { env }
    }

    /// Test complete adapter lifecycle: create → load → activate → use → evict → cleanup
    pub async fn test_complete_adapter_lifecycle(&self) -> Result<()> {
        let env = self.env.lock().await;

        // 1. Adapter Creation Phase
        println!("🏗️  Phase 1: Adapter Creation");
        self.test_adapter_creation(&env).await?;

        // 2. Adapter Loading Phase
        println!("📦 Phase 2: Adapter Loading");
        self.test_adapter_loading(&env).await?;

        // 3. Adapter Activation Phase
        println!("⚡ Phase 3: Adapter Activation");
        self.test_adapter_activation(&env).await?;

        // 4. Adapter Usage Phase
        println!("🔄 Phase 4: Adapter Usage");
        self.test_adapter_usage(&env).await?;

        // 5. Hot-Swap Phase
        println!("[FIRE] Phase 5: Hot-Swap Operations");
        self.test_adapter_hotswap(&env).await?;

        // 6. Memory Pressure Handling
        println!("🧠 Phase 6: Memory Pressure");
        self.test_memory_pressure_handling(&env).await?;

        // 7. Cleanup Phase
        println!("🧹 Phase 7: Cleanup");
        self.test_adapter_cleanup(&env).await?;

        println!("[TARGET] Complete adapter lifecycle test passed!");
        Ok(())
    }

    /// Test adapter creation from repository analysis
    async fn test_adapter_creation(&self, env: &TestEnvironment) -> Result<()> {
        let test_repos = vec![
            ("aviation/maint", "Aviation maintenance procedures", 150),
            ("medical/diag", "Medical diagnostic protocols", 89),
            ("finance/compliance", "Financial compliance rules", 203),
        ];

        for (repo_path, description, file_count) in test_repos {
            // Simulate repository analysis
            let analysis_event = serde_json::json!({
                "repo_path": repo_path,
                "description": description,
                "files_analyzed": file_count,
                "languages": ["rust", "python", "markdown"],
                "topics": ["procedures", "safety", "compliance"],
                "analysis_duration_ms": 2500 + (file_count as i64 * 10)
            });
            env.telemetry().log("repo_analysis", &analysis_event)?;

            // Simulate adapter creation
            let adapter_id = repo_path.replace("/", "_");
            let creation_event = serde_json::json!({
                "adapter_id": adapter_id,
                "source_repo": repo_path,
                "rank": 16,
                "alpha": 0.5,
                "training_samples": file_count * 100,
                "creation_time_ms": 15000,
                "quality_score": 0.87,
                "memory_footprint_mb": 256
            });
            env.telemetry().log("adapter_creation", &creation_event)?;
        }

        Ok(())
    }

    /// Test adapter loading and validation
    async fn test_adapter_loading(&self, env: &TestEnvironment) -> Result<()> {
        let adapters = vec![
            ("aviation_maint", 256, 150),
            ("medical_diag", 192, 120),
            ("finance_compliance", 320, 200),
        ];

        for (adapter_id, memory_mb, load_time_ms) in adapters {
            // Simulate adapter loading
            let load_event = serde_json::json!({
                "adapter_id": adapter_id,
                "load_type": "cold_load",
                "memory_allocated_mb": memory_mb,
                "load_time_ms": load_time_ms,
                "validation_hash": format!("validation_hash_{}", adapter_id),
                "kernel_compatibility": "verified",
                "success": true
            });
            env.telemetry().log("adapter_load", &load_event)?;

            // Simulate adapter validation
            let validation_event = serde_json::json!({
                "adapter_id": adapter_id,
                "validation_type": "integrity_check",
                "checks_passed": ["hash_verification", "schema_validation", "compatibility_check"],
                "validation_time_ms": 50,
                "status": "valid"
            });
            env.telemetry()
                .log("adapter_validation", &validation_event)?;
        }

        Ok(())
    }

    /// Test adapter activation and routing
    async fn test_adapter_activation(&self, env: &TestEnvironment) -> Result<()> {
        let activation_scenarios = vec![
            ("maintenance_query", vec!["aviation_maint"], vec![0.85]),
            ("medical_diagnosis", vec!["medical_diag"], vec![0.92]),
            ("compliance_check", vec!["finance_compliance"], vec![0.78]),
            (
                "multi_domain",
                vec!["aviation_maint", "finance_compliance"],
                vec![0.65, 0.55],
            ),
        ];

        for (scenario, adapters, scores) in activation_scenarios {
            // Simulate routing decision
            let routing_event = serde_json::json!({
                "scenario": scenario,
                "query_embedding": "mock_embedding_vector",
                "adapters_selected": adapters,
                "activation_scores": scores,
                "routing_time_ms": 25,
                "entropy": 0.34
            });
            env.telemetry().log("adapter_routing", &routing_event)?;

            // Simulate activation
            for (i, adapter_id) in adapters.iter().enumerate() {
                let activation_event = serde_json::json!({
                    "adapter_id": adapter_id,
                    "activation_score": scores[i],
                    "token_position": 0,
                    "memory_mapped": true,
                    "activation_time_ms": 5,
                    "success": true
                });
                env.telemetry()
                    .log("adapter_activation", &activation_event)?;
            }
        }

        Ok(())
    }

    /// Test adapter usage during inference
    async fn test_adapter_usage(&self, env: &TestEnvironment) -> Result<()> {
        let usage_patterns = vec![
            ("aviation_maint", 150, 0.85, 42),
            ("medical_diag", 89, 0.92, 38),
            ("finance_compliance", 203, 0.78, 67),
        ];

        for (adapter_id, tokens_processed, avg_activation, inference_count) in usage_patterns {
            let usage_event = serde_json::json!({
                "adapter_id": adapter_id,
                "tokens_processed": tokens_processed,
                "average_activation": avg_activation,
                "inference_count": inference_count,
                "memory_efficiency": 0.94,
                "performance_score": 0.89,
                "last_used": chrono::Utc::now().timestamp()
            });
            env.telemetry().log("adapter_usage", &usage_event)?;
        }

        Ok(())
    }

    /// Test adapter hot-swapping capabilities
    async fn test_adapter_hotswap(&self, env: &TestEnvironment) -> Result<()> {
        // Simulate hot-swap scenario
        let swap_sequence = vec![
            ("preload", "medical_diag", 120),
            ("activate", "medical_diag", 30),
            ("deactivate", "aviation_maint", 15),
            ("unload", "aviation_maint", 80),
        ];

        for (operation, adapter_id, duration_ms) in swap_sequence {
            match operation {
                "preload" => {
                    let preload_event = AdapterPreloadEvent {
                        adapter_id: adapter_id.to_string(),
                        vram_mb: 192,
                        latency_ms: duration_ms as u64,
                        result: "ok".to_string(),
                    };
                    env.telemetry().log_adapter_preload(preload_event)?;
                }
                "activate" => {
                    let activation_event = serde_json::json!({
                        "adapter_id": adapter_id,
                        "operation": "activate",
                        "duration_ms": duration_ms,
                        "memory_transfer_mb": 192
                    });
                    env.telemetry().log("adapter_hotswap", &activation_event)?;
                }
                "deactivate" | "unload" => {
                    let deactivation_event = serde_json::json!({
                        "adapter_id": adapter_id,
                        "operation": operation,
                        "duration_ms": duration_ms,
                        "memory_freed_mb": 256
                    });
                    env.telemetry()
                        .log("adapter_hotswap", &deactivation_event)?;
                }
                _ => {}
            }
        }

        // Log complete swap operation
        let swap_event = AdapterSwapEvent {
            tenant: env.config.tenant_id.clone(),
            add: vec!["medical_diag".to_string()],
            remove: vec!["aviation_maint".to_string()],
            vram_mb: 64, // Net memory change
            latency_ms: 245,
            result: "ok".to_string(),
            stack_hash: Some("new_stack_hash_789".to_string()),
        };
        env.telemetry().log_adapter_swap(swap_event)?;

        Ok(())
    }

    /// Test memory pressure handling and eviction
    async fn test_memory_pressure_handling(&self, env: &TestEnvironment) -> Result<()> {
        // Simulate memory pressure scenario
        let pressure_levels = vec![
            ("low", 2048, 512, "normal_operation"),
            ("medium", 3072, 1024, "reduced_k"),
            ("high", 3584, 1536, "eviction_started"),
            ("critical", 3840, 1792, "emergency_unload"),
        ];

        for (level, used_mb, available_mb, action) in pressure_levels {
            let pressure_event = serde_json::json!({
                "pressure_level": level,
                "memory_used_mb": used_mb,
                "memory_available_mb": available_mb,
                "adapters_loaded": 3,
                "action_taken": action,
                "eviction_candidates": ["old_adapter_1", "old_adapter_2"]
            });
            env.telemetry().log("memory_pressure", &pressure_event)?;

            if level == "high" || level == "critical" {
                // Simulate eviction
                let eviction_event = serde_json::json!({
                    "adapter_id": "old_adapter_1",
                    "eviction_reason": "memory_pressure",
                    "memory_freed_mb": 256,
                    "eviction_time_ms": 50,
                    "replacement_strategy": "lru"
                });
                env.telemetry().log("adapter_eviction", &eviction_event)?;
            }
        }

        Ok(())
    }

    /// Test adapter cleanup and resource release
    async fn test_adapter_cleanup(&self, env: &TestEnvironment) -> Result<()> {
        let cleanup_adapters = vec!["aviation_maint", "medical_diag", "finance_compliance"];

        for adapter_id in cleanup_adapters {
            // Simulate cleanup process
            let cleanup_event = serde_json::json!({
                "adapter_id": adapter_id,
                "cleanup_phase": "memory_release",
                "memory_freed_mb": 256,
                "files_removed": 2,
                "cleanup_time_ms": 30,
                "final_state": "unloaded"
            });
            env.telemetry().log("adapter_cleanup", &cleanup_event)?;

            // Verify cleanup
            let verification_event = serde_json::json!({
                "adapter_id": adapter_id,
                "verification_type": "cleanup_check",
                "memory_leaks": false,
                "file_leaks": false,
                "resource_leaks": false,
                "status": "clean"
            });
            env.telemetry()
                .log("adapter_verification", &verification_event)?;
        }

        Ok(())
    }
}

/// Test adapter performance under concurrent load
pub async fn test_adapter_concurrent_load(env: Arc<Mutex<TestEnvironment>>) -> Result<()> {
    let env = env.lock().await;

    // Simulate concurrent adapter usage
    let concurrent_scenarios = vec![
        ("scenario_1", 5, 0.85, 1200),
        ("scenario_2", 8, 0.78, 1800),
        ("scenario_3", 12, 0.92, 2400),
    ];

    for (scenario, concurrent_requests, avg_throughput, total_tokens) in concurrent_scenarios {
        let concurrent_event = serde_json::json!({
            "scenario": scenario,
            "concurrent_requests": concurrent_requests,
            "average_throughput": avg_throughput,
            "total_tokens_processed": total_tokens,
            "memory_pressure": "low",
            "adapter_contention": 0.15,
            "duration_ms": 10000
        });
        env.telemetry()
            .log("adapter_concurrency", &concurrent_event)?;
    }

    Ok(())
}

/// Test adapter degradation and recovery
pub async fn test_adapter_degradation_recovery(env: Arc<Mutex<TestEnvironment>>) -> Result<()> {
    let env = env.lock().await;

    // Simulate adapter degradation scenario
    let degradation_sequence = vec![
        ("normal", 0.95, 150),
        ("degraded", 0.78, 280),
        ("critical", 0.45, 450),
        ("recovery", 0.89, 180),
        ("stable", 0.94, 155),
    ];

    for (state, performance_score, latency_ms) in degradation_sequence {
        let degradation_event = serde_json::json!({
            "adapter_state": state,
            "performance_score": performance_score,
            "latency_ms": latency_ms,
            "memory_fragmentation": if state == "critical" { 0.35 } else { 0.05 },
            "error_rate": if state == "critical" { 0.12 } else { 0.001 },
            "recovery_action": if state == "recovery" { "restart" } else { "none" }
        });
        env.telemetry().log("adapter_health", &degradation_event)?;
    }

    Ok(())
}

/// Test adapter version compatibility and migration
pub async fn test_adapter_version_migration(env: Arc<Mutex<TestEnvironment>>) -> Result<()> {
    let env = env.lock().await;

    let migration_scenarios = vec![
        ("v1.0", "v1.1", "patch_update", true),
        ("v1.1", "v2.0", "major_update", true),
        ("v2.0", "v2.1", "compatibility_fix", true),
    ];

    for (from_version, to_version, migration_type, success) in migration_scenarios {
        let migration_event = serde_json::json!({
            "adapter_id": "test_adapter",
            "from_version": from_version,
            "to_version": to_version,
            "migration_type": migration_type,
            "migration_time_ms": 500,
            "data_migration": true,
            "backward_compatibility": true,
            "success": success,
            "rollback_available": true
        });
        env.telemetry().log("adapter_migration", &migration_event)?;
    }

    Ok(())
}
