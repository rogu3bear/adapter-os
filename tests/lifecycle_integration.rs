//! Integration tests for the complete lifecycle system
//!
//! These tests verify the full startup → runtime → shutdown lifecycle
//! of the AdapterOS server with all major subsystems.

#[cfg(test)]
mod lifecycle_tests {
    use adapteros_core::B3Hash;
    use adapteros_deterministic_exec::{init_global_executor, EnforcementMode, ExecutorConfig};
    use std::time::Duration;

    fn init_test_executor() {
        let manifest_hash = B3Hash::hash(b"test-manifest");
        let executor_config = ExecutorConfig {
            global_seed: adapteros_core::derive_seed(&manifest_hash, "test"),
            max_ticks_per_task: 1_000_000,
            enable_event_logging: false,
            replay_mode: false,
            replay_events: Vec::new(),
            agent_id: None,
            enable_thread_pinning: false,
            worker_threads: Some(2),
            enforcement_mode: EnforcementMode::AuditOnly,
        };
        let _ = init_global_executor(executor_config);
    }

    #[tokio::test]
    async fn test_lifecycle_hooks_before_startup() {
        init_test_executor();

        let registry = adapteros_server_api::LifecycleHookRegistry::new();

        // Register a before_startup hook
        let hook = adapteros_server_api::LifecycleHook {
            id: "test-hook-1".to_string(),
            component: "test-component".to_string(),
            phase: adapteros_server_api::LifecyclePhase::BeforeStartup,
            callback: std::sync::Arc::new(|ctx| {
                assert_eq!(
                    ctx.phase,
                    adapteros_server_api::LifecyclePhase::BeforeStartup
                );
            }),
        };
        registry.register(hook);

        // Run hooks for before_startup
        let start_time = std::time::Instant::now();
        let result = registry
            .run_hooks(
                adapteros_server_api::LifecyclePhase::BeforeStartup,
                start_time,
            )
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_lifecycle_hooks_after_startup() {
        init_test_executor();

        let registry = adapteros_server_api::LifecycleHookRegistry::new();

        let hook = adapteros_server_api::LifecycleHook {
            id: "test-hook-2".to_string(),
            component: "test-component".to_string(),
            phase: adapteros_server_api::LifecyclePhase::AfterStartup,
            callback: std::sync::Arc::new(|ctx| {
                assert_eq!(
                    ctx.phase,
                    adapteros_server_api::LifecyclePhase::AfterStartup
                );
            }),
        };
        registry.register(hook);

        let start_time = std::time::Instant::now();
        let result = registry
            .run_hooks(
                adapteros_server_api::LifecyclePhase::AfterStartup,
                start_time,
            )
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_lifecycle_hooks_before_shutdown() {
        init_test_executor();

        let registry = adapteros_server_api::LifecycleHookRegistry::new();

        let hook = adapteros_server_api::LifecycleHook {
            id: "test-hook-3".to_string(),
            component: "test-component".to_string(),
            phase: adapteros_server_api::LifecyclePhase::BeforeShutdown,
            callback: std::sync::Arc::new(|ctx| {
                assert_eq!(
                    ctx.phase,
                    adapteros_server_api::LifecyclePhase::BeforeShutdown
                );
            }),
        };
        registry.register(hook);

        let start_time = std::time::Instant::now();
        let result = registry
            .run_hooks(
                adapteros_server_api::LifecyclePhase::BeforeShutdown,
                start_time,
            )
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_lifecycle_hooks_after_shutdown() {
        init_test_executor();

        let registry = adapteros_server_api::LifecycleHookRegistry::new();

        let hook = adapteros_server_api::LifecycleHook {
            id: "test-hook-4".to_string(),
            component: "test-component".to_string(),
            phase: adapteros_server_api::LifecyclePhase::AfterShutdown,
            callback: std::sync::Arc::new(|ctx| {
                assert_eq!(
                    ctx.phase,
                    adapteros_server_api::LifecyclePhase::AfterShutdown
                );
            }),
        };
        registry.register(hook);

        let start_time = std::time::Instant::now();
        let result = registry
            .run_hooks(
                adapteros_server_api::LifecyclePhase::AfterShutdown,
                start_time,
            )
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_lifecycle_hooks_multiple_phases() {
        init_test_executor();

        let registry = adapteros_server_api::LifecycleHookRegistry::new();

        // Register hooks for all phases
        for phase in &[
            adapteros_server_api::LifecyclePhase::BeforeStartup,
            adapteros_server_api::LifecyclePhase::AfterStartup,
            adapteros_server_api::LifecyclePhase::BeforeShutdown,
            adapteros_server_api::LifecyclePhase::AfterShutdown,
        ] {
            let hook = adapteros_server_api::LifecycleHook {
                id: format!("hook-{:?}", phase),
                component: "test-component".to_string(),
                phase: *phase,
                callback: std::sync::Arc::new(|_| {}),
            };
            registry.register(hook);
        }

        // Run hooks for each phase
        let start_time = std::time::Instant::now();
        for phase in &[
            adapteros_server_api::LifecyclePhase::BeforeStartup,
            adapteros_server_api::LifecyclePhase::AfterStartup,
            adapteros_server_api::LifecyclePhase::BeforeShutdown,
            adapteros_server_api::LifecyclePhase::AfterShutdown,
        ] {
            let result = registry.run_hooks(*phase, start_time).await;
            assert!(result.is_ok(), "Failed to run hooks for phase {:?}", phase);
        }
    }

    #[tokio::test]
    async fn test_shutdown_coordinator_creation() {
        init_test_executor();

        let coordinator = adapteros_server_api::ShutdownCoordinator::new();
        let mut rx = coordinator.subscribe_shutdown();

        // Verify we can subscribe to shutdown signal by calling shutdown()
        // which internally sends the signal via shutdown_tx
        tokio::spawn(async move {
            let _ = coordinator.shutdown().await;
        });

        // Should receive the signal
        let result = tokio::time::timeout(Duration::from_millis(100), rx.recv()).await;
        assert!(result.is_ok(), "Failed to receive shutdown signal");
    }

    #[tokio::test]
    async fn test_shutdown_coordinator_graceful() {
        init_test_executor();

        let mut coordinator = adapteros_server_api::ShutdownCoordinator::new();

        // Register a simple background task
        let handle =
            adapteros_deterministic_exec::spawn_deterministic("test-task".to_string(), async {
                tokio::time::sleep(Duration::from_millis(50)).await;
            })
            .expect("Failed to spawn test task");

        coordinator.register_task(handle);

        // Shutdown should complete successfully
        let result = tokio::time::timeout(Duration::from_secs(5), coordinator.shutdown()).await;

        assert!(result.is_ok(), "Shutdown timed out");
        assert!(result.unwrap().is_ok(), "Shutdown failed");
    }

    #[tokio::test]
    async fn test_shutdown_coordinator_timeout() {
        init_test_executor();

        let config = adapteros_server_api::ShutdownConfig {
            federation_timeout: Duration::from_millis(50),
            ..Default::default()
        };
        let mut coordinator = adapteros_server_api::ShutdownCoordinator::with_config(config);

        // Register a non-critical task that will timeout
        let mock_handle = tokio::spawn(async {
            tokio::time::sleep(Duration::from_secs(1)).await;
        });
        coordinator.set_federation_handle(mock_handle);

        // Shutdown should report partial failure due to timeout
        let result = coordinator.shutdown().await;
        assert!(result.is_err(), "Expected shutdown to fail due to timeout");

        match result.unwrap_err() {
            adapteros_server_api::ShutdownError::PartialFailure { failed_count } => {
                assert_eq!(failed_count, 1, "Expected 1 failed component");
            }
            other => panic!("Expected PartialFailure, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_shutdown_coordinator_critical_failure() {
        init_test_executor();

        let config = adapteros_server_api::ShutdownConfig {
            telemetry_timeout: Duration::from_millis(50),
            ..Default::default()
        };
        let mut coordinator = adapteros_server_api::ShutdownCoordinator::with_config(config);

        // Register telemetry (critical) that will timeout
        let mock_handle = tokio::spawn(async {
            tokio::time::sleep(Duration::from_secs(1)).await;
        });
        coordinator.set_telemetry_handle(mock_handle);

        // Shutdown should report critical failure
        let result = coordinator.shutdown().await;
        assert!(result.is_err(), "Expected shutdown to fail");

        match result.unwrap_err() {
            adapteros_server_api::ShutdownError::CriticalFailure { component } => {
                assert_eq!(component, "telemetry", "Expected telemetry to fail");
            }
            other => panic!("Expected CriticalFailure, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_shutdown_config_defaults() {
        let config = adapteros_server_api::ShutdownConfig::default();
        assert_eq!(config.telemetry_timeout, Duration::from_secs(10));
        assert_eq!(config.federation_timeout, Duration::from_secs(15));
        assert_eq!(config.uds_metrics_timeout, Duration::from_secs(5));
        assert_eq!(config.git_daemon_timeout, Duration::from_secs(10));
        assert_eq!(config.policy_watcher_timeout, Duration::from_secs(5));
        assert_eq!(config.overall_timeout, Duration::from_secs(30));
    }

    #[tokio::test]
    async fn test_lifecycle_hooks_register_and_retrieve() {
        init_test_executor();

        let registry = adapteros_server_api::LifecycleHookRegistry::new();

        // Register multiple hooks for same phase
        for i in 0..3 {
            let hook = adapteros_server_api::LifecycleHook {
                id: format!("hook-{}", i),
                component: "test-component".to_string(),
                phase: adapteros_server_api::LifecyclePhase::BeforeStartup,
                callback: std::sync::Arc::new(|_| {}),
            };
            registry.register(hook);
        }

        // Retrieve hooks for before_startup
        let hooks =
            registry.get_hooks_for_phase(adapteros_server_api::LifecyclePhase::BeforeStartup);
        assert_eq!(hooks.len(), 3, "Expected 3 hooks for BeforeStartup phase");

        // Verify no hooks for other phases
        let hooks =
            registry.get_hooks_for_phase(adapteros_server_api::LifecyclePhase::AfterStartup);
        assert_eq!(hooks.len(), 0, "Expected 0 hooks for AfterStartup phase");
    }

    #[tokio::test]
    async fn test_lifecycle_phases_independence() {
        init_test_executor();

        let registry = adapteros_server_api::LifecycleHookRegistry::new();

        // Register a hook for each phase with unique identifiers
        let phases = vec![
            adapteros_server_api::LifecyclePhase::BeforeStartup,
            adapteros_server_api::LifecyclePhase::AfterStartup,
            adapteros_server_api::LifecyclePhase::BeforeShutdown,
            adapteros_server_api::LifecyclePhase::AfterShutdown,
        ];

        for (idx, phase) in phases.iter().enumerate() {
            let hook = adapteros_server_api::LifecycleHook {
                id: format!("hook-{}", idx),
                component: format!("component-{}", idx),
                phase: *phase,
                callback: std::sync::Arc::new(|_| {}),
            };
            registry.register(hook);
        }

        // Verify each phase has exactly one hook
        for phase in &phases {
            let hooks = registry.get_hooks_for_phase(*phase);
            assert_eq!(hooks.len(), 1, "Expected 1 hook for phase {:?}", phase);
        }
    }
}
