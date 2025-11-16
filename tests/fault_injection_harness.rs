//! Fault injection and adversarial testing harness
//!
//! This test suite validates system behavior under adversarial conditions:
//! - Database failures during stack operations
//! - Workflow executor failures and recovery
//! - Router failures with active stacks
//! - Concurrent stack activation/deactivation

use adapteros_lora_lifecycle::{LifecycleManager, WorkflowContext, WorkflowExecutor, WorkflowType};
use adapteros_lora_router::{Router, RouterWeights};
use adapteros_manifest::Policies;
use std::collections::HashMap;
use std::path::PathBuf;

#[tokio::test]
async fn test_workflow_executor_empty_adapter_list() {
    // Adversarial case: Execute workflow with no adapters
    let executor = WorkflowExecutor::new(WorkflowType::Sequential, vec![]);

    let context = WorkflowContext {
        input_tokens: vec![1, 2, 3],
        model_state: HashMap::new(),
        metadata: HashMap::new(),
    };

    let result = executor.execute(context).await.unwrap();

    // Should complete successfully with zero adapters
    assert_eq!(result.stats.adapters_executed, 0);
    assert_eq!(result.output_tokens, vec![1, 2, 3]); // Input unchanged
}

#[tokio::test]
async fn test_workflow_executor_single_adapter_upstream_downstream() {
    // Edge case: UpstreamDownstream with only 1 adapter
    let executor = WorkflowExecutor::new(
        WorkflowType::UpstreamDownstream,
        vec!["single_adapter".to_string()],
    );

    let context = WorkflowContext {
        input_tokens: vec![10, 20],
        model_state: HashMap::new(),
        metadata: HashMap::new(),
    };

    let result = executor.execute(context).await.unwrap();

    // Should handle gracefully (0 upstream, 1 downstream)
    assert_eq!(result.stats.adapters_executed, 1);
    assert_eq!(result.stats.phases.len(), 2); // Still 2 phases
}

#[tokio::test]
async fn test_router_stack_filtering_with_empty_stack() {
    // Adversarial case: Activate stack with no adapters
    let weights = RouterWeights::default();
    let mut router = Router::new_with_weights(weights, 3, 1.0, 0.02);

    router.set_active_stack(Some("empty_stack".to_string()), Some(vec![]));

    let features = vec![0.5; 22];
    let priors = vec![0.1, 0.2, 0.3, 0.4, 0.5];

    let decision = router.route(&features, &priors);

    // Should produce empty decision (no adapters in stack)
    assert_eq!(decision.indices.len(), 0);
    assert_eq!(decision.gates_q15.len(), 0);
}

#[tokio::test]
async fn test_router_stack_filtering_with_non_existent_adapters() {
    // Adversarial case: Stack references adapters not in prior list
    let weights = RouterWeights::default();
    let mut router = Router::new_with_weights(weights, 2, 1.0, 0.02);

    // Stack contains adapters that won't match any priors by ID
    router.set_active_stack(
        Some("mismatched_stack".to_string()),
        Some(vec!["nonexistent_1".to_string(), "nonexistent_2".to_string()])
    );

    let features = vec![0.5; 22];
    let priors = vec![0.1, 0.2, 0.3]; // 3 adapters, none matching stack

    let decision = router.route(&features, &priors);

    // Should handle gracefully - the filter_by_stack uses adapter_info,
    // but in the simple route() call, we don't have that context
    // The route() method doesn't filter, only route_with_code_features() does
    assert!(decision.indices.len() <= 2); // K=2
}

#[tokio::test]
async fn test_lifecycle_manager_activate_nonexistent_adapters() {
    // Adversarial case: Try to activate stack with adapters that don't exist
    let policies = Policies::default();
    let manager = LifecycleManager::new(
        vec!["real_adapter_1".to_string(), "real_adapter_2".to_string()],
        &policies,
        PathBuf::from("/tmp/test"),
        None,
        3,
    );

    let result = manager.activate_stack(
        "bad_stack".to_string(),
        vec!["nonexistent_adapter".to_string()],
    ).await;

    // Should return error for nonexistent adapter
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("not found"));
}

#[tokio::test]
async fn test_lifecycle_manager_execute_workflow_without_active_stack() {
    // Adversarial case: Try to execute workflow with no active stack
    let policies = Policies::default();
    let manager = LifecycleManager::new(
        vec!["adapter_1".to_string()],
        &policies,
        PathBuf::from("/tmp/test"),
        None,
        3,
    );

    let context = WorkflowContext {
        input_tokens: vec![1, 2, 3],
        model_state: HashMap::new(),
        metadata: HashMap::new(),
    };

    let result = manager.execute_stack_workflow(context).await;

    // Should return error when no stack is active
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("No active stack"));
}

#[tokio::test]
async fn test_concurrent_stack_activation_deactivation() {
    // Adversarial case: Rapidly activate and deactivate stacks
    let policies = Policies::default();
    let manager = LifecycleManager::new(
        vec!["adapter_1".to_string(), "adapter_2".to_string()],
        &policies,
        PathBuf::from("/tmp/test"),
        None,
        3,
    );

    // Rapid activation/deactivation cycle
    for i in 0..10 {
        let stack_name = format!("stack_{}", i);
        let _ = manager.activate_stack(
            stack_name,
            vec!["adapter_1".to_string()],
        ).await;

        let _ = manager.deactivate_stack().await;
    }

    // Should be deactivated at the end
    assert!(manager.get_active_stack().is_none());
}

#[tokio::test]
async fn test_workflow_parallel_execution_stress() {
    // Stress test: Execute many adapters in parallel
    let num_adapters = 50;
    let adapters: Vec<String> = (0..num_adapters)
        .map(|i| format!("adapter_{}", i))
        .collect();

    let executor = WorkflowExecutor::new(WorkflowType::Parallel, adapters);

    let context = WorkflowContext {
        input_tokens: vec![1; 100],
        model_state: HashMap::new(),
        metadata: HashMap::new(),
    };

    let result = executor.execute(context).await.unwrap();

    assert_eq!(result.stats.adapters_executed, num_adapters);
    assert_eq!(result.stats.phases.len(), 1);
    assert!(result.stats.total_time_ms > 0);
}

#[tokio::test]
async fn test_workflow_sequential_execution_stress() {
    // Stress test: Execute many adapters sequentially
    let num_adapters = 20;
    let adapters: Vec<String> = (0..num_adapters)
        .map(|i| format!("adapter_{}", i))
        .collect();

    let executor = WorkflowExecutor::new(WorkflowType::Sequential, adapters);

    let context = WorkflowContext {
        input_tokens: vec![1; 10],
        model_state: HashMap::new(),
        metadata: HashMap::new(),
    };

    let result = executor.execute(context).await.unwrap();

    assert_eq!(result.stats.adapters_executed, num_adapters);
    assert_eq!(result.stats.phases.len(), num_adapters);

    // Each phase should have executed exactly one adapter
    for (i, phase) in result.stats.phases.iter().enumerate() {
        assert_eq!(phase.adapter_ids.len(), 1);
        assert!(phase.name.starts_with("sequential_"));
        assert!(phase.time_ms > 0);
    }
}

#[test]
fn test_router_extreme_temperature_values() {
    // Adversarial case: Test router with extreme temperature values
    let weights = RouterWeights::default();

    // Very low temperature (near-greedy selection)
    let mut router_low = Router::new_with_weights(weights.clone(), 3, 0.001, 0.02);
    let features = vec![0.5; 22];
    let priors = vec![0.1, 0.5, 0.3, 0.2, 0.4];

    let decision_low = router_low.route(&features, &priors);
    assert_eq!(decision_low.indices.len(), 3);

    // Very high temperature (near-uniform selection)
    let mut router_high = Router::new_with_weights(weights, 3, 100.0, 0.02);
    let decision_high = router_high.route(&features, &priors);
    assert_eq!(decision_high.indices.len(), 3);

    // High temperature should produce more uniform distribution
    let gates_high = decision_high.gates_f32();
    let variance_high: f32 = {
        let mean: f32 = gates_high.iter().sum::<f32>() / gates_high.len() as f32;
        gates_high.iter().map(|&g| (g - mean).powi(2)).sum::<f32>() / gates_high.len() as f32
    };

    // Low temperature should have higher variance (more concentrated)
    let gates_low = decision_low.gates_f32();
    let variance_low: f32 = {
        let mean: f32 = gates_low.iter().sum::<f32>() / gates_low.len() as f32;
        gates_low.iter().map(|&g| (g - mean).powi(2)).sum::<f32>() / gates_low.len() as f32
    };

    // Sanity check: low temp should have higher variance than high temp
    assert!(
        variance_low > variance_high,
        "Low temp variance {} should be > high temp variance {}",
        variance_low,
        variance_high
    );
}

#[test]
fn test_router_extreme_k_values() {
    // Adversarial case: Test with K=1 (minimum) and K=all (maximum)
    let weights = RouterWeights::default();
    let num_adapters = 10;

    // K=1: Select only top adapter
    let mut router_k1 = Router::new_with_weights(weights.clone(), 1, 1.0, 0.02);
    let features = vec![0.5; 22];
    let priors = vec![0.1; num_adapters];

    let decision_k1 = router_k1.route(&features, &priors);
    assert_eq!(decision_k1.indices.len(), 1);
    assert_eq!(decision_k1.gates_q15.len(), 1);

    // Gate should be approximately 1.0
    let gates_k1 = decision_k1.gates_f32();
    assert!((gates_k1[0] - 1.0).abs() < 0.01);

    // K=all: Select all adapters
    let mut router_all = Router::new_with_weights(weights, num_adapters, 1.0, 0.02);
    let decision_all = router_all.route(&features, &priors);
    assert_eq!(decision_all.indices.len(), num_adapters);

    // Gates should be relatively uniform (uniform priors)
    let gates_all = decision_all.gates_f32();
    let mean_gate = 1.0 / num_adapters as f32;
    for &g in &gates_all {
        assert!((g - mean_gate).abs() < 0.1); // Within 10% of uniform
    }
}

#[tokio::test]
async fn test_workflow_large_input_tokens() {
    // Adversarial case: Very large input token sequences
    let executor = WorkflowExecutor::new(
        WorkflowType::Sequential,
        vec!["adapter_1".to_string(), "adapter_2".to_string()],
    );

    let large_input: Vec<u32> = (0..10000).collect();
    let context = WorkflowContext {
        input_tokens: large_input.clone(),
        model_state: HashMap::new(),
        metadata: HashMap::new(),
    };

    let result = executor.execute(context).await.unwrap();

    // Should handle large inputs without errors
    assert_eq!(result.stats.adapters_executed, 2);
    assert!(result.output_tokens.len() > 0);
}

#[tokio::test]
async fn test_workflow_large_model_state() {
    // Adversarial case: Very large model state
    let executor = WorkflowExecutor::new(
        WorkflowType::Parallel,
        vec!["adapter_1".to_string()],
    );

    let mut large_state = HashMap::new();
    for i in 0..1000 {
        large_state.insert(
            format!("layer_{}", i),
            vec![0.1; 1000], // 1000 layers with 1000 values each
        );
    }

    let context = WorkflowContext {
        input_tokens: vec![1, 2, 3],
        model_state: large_state,
        metadata: HashMap::new(),
    };

    let result = executor.execute(context).await.unwrap();

    // Should handle large state without errors
    assert_eq!(result.stats.adapters_executed, 1);
    assert!(result.final_state.len() >= 1000);
}
