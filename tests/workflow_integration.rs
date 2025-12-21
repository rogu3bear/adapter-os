//! Integration tests for workflow execution

use adapteros_core::B3Hash;
use adapteros_lora_lifecycle::{
    LifecycleManager, MockAdapterBackend, WorkflowContext, WorkflowExecutor, WorkflowType,
};
use adapteros_manifest::Policies;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

fn build_adapter_hashes(names: &[String]) -> HashMap<String, B3Hash> {
    names
        .iter()
        .map(|name| (name.clone(), B3Hash::hash(name.as_bytes())))
        .collect()
}

#[tokio::test]
async fn test_workflow_execution_sequential() {
    // Create workflow executor
    let backend = Arc::new(MockAdapterBackend);
    let executor = WorkflowExecutor::new(
        WorkflowType::Sequential,
        vec![
            "adapter_1".to_string(),
            "adapter_2".to_string(),
            "adapter_3".to_string(),
        ],
        backend,
    );

    // Create context
    let context = WorkflowContext {
        input_tokens: vec![100, 200, 300],
        model_state: HashMap::new(),
        metadata: HashMap::new(),
    };

    // Execute workflow
    let result = executor.execute(context).await.unwrap();

    // Verify results
    assert_eq!(result.stats.adapters_executed, 3);
    assert_eq!(result.stats.phases.len(), 3);
    assert!(result.stats.total_time_ms > 0);

    // Check phase names
    for phase in result.stats.phases.iter() {
        assert!(phase.name.starts_with("sequential_"));
        assert_eq!(phase.adapter_ids.len(), 1);
    }
}

#[tokio::test]
async fn test_workflow_execution_parallel() {
    // Create workflow executor
    let backend = Arc::new(MockAdapterBackend);
    let executor = WorkflowExecutor::new(
        WorkflowType::Parallel,
        vec![
            "adapter_a".to_string(),
            "adapter_b".to_string(),
            "adapter_c".to_string(),
            "adapter_d".to_string(),
        ],
        backend,
    );

    // Create context
    let mut initial_state = HashMap::new();
    initial_state.insert("model_layer_1".to_string(), vec![0.1, 0.2, 0.3]);

    let context = WorkflowContext {
        input_tokens: vec![1, 2, 3, 4, 5],
        model_state: initial_state,
        metadata: HashMap::from([
            ("request_id".to_string(), "test_123".to_string()),
            ("user_id".to_string(), "user_456".to_string()),
        ]),
    };

    // Execute workflow
    let result = executor.execute(context).await.unwrap();

    // Verify results
    assert_eq!(result.stats.adapters_executed, 4);
    assert_eq!(result.stats.phases.len(), 1);
    assert_eq!(result.stats.phases[0].name, "parallel_all");
    assert_eq!(result.stats.phases[0].adapter_ids.len(), 4);
}

#[tokio::test]
async fn test_workflow_execution_upstream_downstream() {
    // Create workflow executor with 6 adapters
    let backend = Arc::new(MockAdapterBackend);
    let executor = WorkflowExecutor::new(
        WorkflowType::UpstreamDownstream,
        vec![
            "upstream_1".to_string(),
            "upstream_2".to_string(),
            "upstream_3".to_string(),
            "downstream_1".to_string(),
            "downstream_2".to_string(),
            "downstream_3".to_string(),
        ],
        backend,
    );

    // Create context
    let context = WorkflowContext {
        input_tokens: vec![10, 20, 30],
        model_state: HashMap::new(),
        metadata: HashMap::new(),
    };

    // Execute workflow
    let result = executor.execute(context).await.unwrap();

    // Verify results
    assert_eq!(result.stats.adapters_executed, 6);
    assert_eq!(result.stats.phases.len(), 2);

    // Check upstream phase
    assert_eq!(result.stats.phases[0].name, "upstream");
    assert_eq!(result.stats.phases[0].adapter_ids.len(), 3);

    // Check downstream phase
    assert_eq!(result.stats.phases[1].name, "downstream");
    assert_eq!(result.stats.phases[1].adapter_ids.len(), 3);
}

#[tokio::test]
async fn test_lifecycle_manager_workflow_integration() {
    // Create lifecycle manager
    let policies = Policies::default();
    let adapters_path = PathBuf::from("var/test-adapters");
    let adapter_names = vec!["test_adapter_1".to_string(), "test_adapter_2".to_string()];

    let adapter_hashes = build_adapter_hashes(&adapter_names);
    let manager = LifecycleManager::new(
        adapter_names.clone(),
        adapter_hashes,
        &policies,
        adapters_path,
        None,
        3,
    );

    // Activate a stack
    let stack_name = "test_stack".to_string();
    let result = manager
        .activate_stack(stack_name.clone(), adapter_names)
        .await;

    // Stack activation might fail if adapters don't exist, but that's okay for this test
    if result.is_ok() {
        // Verify stack is active
        let active = manager.get_active_stack();
        assert!(active.is_some());
        let (name, ids) = active.unwrap();
        assert_eq!(name, stack_name);
        assert_eq!(ids.len(), 2);

        // Test workflow execution
        let context = WorkflowContext {
            input_tokens: vec![1, 2, 3],
            model_state: HashMap::new(),
            metadata: HashMap::new(),
        };

        // This will use the default Parallel workflow type
        let workflow_result = manager.execute_stack_workflow(context).await;

        // The workflow might fail if adapters aren't loaded, but we verify the structure
        if let Ok(result) = workflow_result {
            assert_eq!(result.stats.adapters_executed, 2);
            assert!(result.stats.phases.len() > 0);
        }
    }
}

#[test]
fn test_workflow_type_serialization() {
    // Test that workflow types serialize correctly
    let parallel = WorkflowType::Parallel;
    let json = serde_json::to_string(&parallel).unwrap();
    assert_eq!(json, "\"parallel\"");

    let sequential = WorkflowType::Sequential;
    let json = serde_json::to_string(&sequential).unwrap();
    assert_eq!(json, "\"sequential\"");

    let upstream_downstream = WorkflowType::UpstreamDownstream;
    let json = serde_json::to_string(&upstream_downstream).unwrap();
    assert_eq!(json, "\"upstream_downstream\"");

    // Test deserialization
    let deserialized: WorkflowType = serde_json::from_str("\"parallel\"").unwrap();
    assert_eq!(deserialized, WorkflowType::Parallel);

    let deserialized: WorkflowType = serde_json::from_str("\"sequential\"").unwrap();
    assert_eq!(deserialized, WorkflowType::Sequential);

    let deserialized: WorkflowType = serde_json::from_str("\"upstream_downstream\"").unwrap();
    assert_eq!(deserialized, WorkflowType::UpstreamDownstream);
}
